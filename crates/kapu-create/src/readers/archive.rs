//! ZIP and 7z archives (KD8): every member read like a file given by the
//! user, at most `max_depth` levels deep, with size and ratio limits
//! against zip bombs.

use std::io::{Cursor, Read};

use crate::doc::Document;
use crate::readers::{Ctx, read};
use crate::{Error, Result};

/// A member that could not be read: a document without blocks whose
/// warning says why, so the report names it (nothing is skipped silently).
#[must_use]
pub fn failed(key: &str, name: &str, why: &str) -> Document {
    let mut doc = Document::new(key, name, "none");
    doc.warnings.push(why.to_owned());
    doc
}

/// Reads one member and appends its documents (or its failure).
fn member(out: &mut Vec<Document>, bytes: &[u8], outer_key: &str, outer_name: &str, inner: &str, ctx: &Ctx<'_>) {
    let key = format!("{outer_key}!{inner}");
    let name = format!("{outer_name} > {inner}");
    match ctx.deeper().and_then(|deeper| read(bytes, &key, &name, &deeper)) {
        Ok(docs) => out.extend(docs),
        Err(e) => out.push(failed(&key, &name, &e.0)),
    }
}

/// Whether a member name is worth reading (no folders, no OS litter).
fn wanted(name: &str) -> bool {
    let last = name.rsplit('/').next().unwrap_or(name);
    !(name.ends_with('/')
        || name.starts_with("__MACOSX/")
        || last.starts_with("._")
        || last.eq_ignore_ascii_case(".ds_store")
        || last.eq_ignore_ascii_case("thumbs.db")
        || last.eq_ignore_ascii_case("desktop.ini"))
}

/// A ZIP archive.
///
/// # Errors
/// When the archive's directory cannot be read.
pub fn zip(bytes: &[u8], key: &str, name: &str, ctx: &Ctx<'_>) -> Result<Vec<Document>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| Error::new(format!("ZIP: {e}")))?;
    let mut out = Vec::new();
    let mut unpacked = 0u64;
    for i in 0..archive.len() {
        let mut file = match archive.by_index(i) {
            Ok(f) => f,
            Err(e) => {
                out.push(failed(&format!("{key}!#{i}"), &format!("{name} > #{i}"), &format!("ZIP member: {e}")));
                continue;
            }
        };
        let inner = file.name().replace('\\', "/");
        if file.is_dir() || !wanted(&inner) {
            continue;
        }
        let size = file.size();
        let packed = file.compressed_size().max(1);
        if size > ctx.options.max_member || (size > 64 << 20 && size / packed > 200) {
            out.push(failed(
                &format!("{key}!{inner}"),
                &format!("{name} > {inner}"),
                &format!("not unpacked: {size} bytes (size or compression ratio over the limit)"),
            ));
            continue;
        }
        unpacked += size;
        if unpacked > ctx.options.max_unpacked {
            return Err(Error::new(format!(
                "ZIP: more than {} bytes unpacked, stopped",
                ctx.options.max_unpacked
            )));
        }
        let mut data = Vec::with_capacity(usize::try_from(size).unwrap_or(0).min(64 << 20));
        if let Err(e) = (&mut file).take(ctx.options.max_member + 1).read_to_end(&mut data) {
            out.push(failed(&format!("{key}!{inner}"), &format!("{name} > {inner}"), &format!("ZIP member: {e}")));
            continue;
        }
        drop(file);
        member(&mut out, &data, key, name, &inner, ctx);
    }
    Ok(out)
}

/// A 7z archive.
///
/// # Errors
/// When the archive cannot be opened (damaged, encrypted).
pub fn sevenz(bytes: &[u8], key: &str, name: &str, ctx: &Ctx<'_>) -> Result<Vec<Document>> {
    let mut reader = sevenz_rust2::ArchiveReader::new(Cursor::new(bytes), sevenz_rust2::Password::empty())
        .map_err(|e| Error::new(format!("7z: {e}")))?;
    let mut members: Vec<(String, Vec<u8>)> = Vec::new();
    let mut skipped: Vec<(String, String)> = Vec::new();
    let (max_member, max_unpacked) = (ctx.options.max_member, ctx.options.max_unpacked);
    let mut unpacked = 0u64;
    reader
        .for_each_entries(|entry, data| {
            let inner = entry.name().replace('\\', "/");
            if entry.is_directory() || !entry.has_stream() || !wanted(&inner) {
                std::io::copy(data, &mut std::io::sink())?;
                return Ok(true);
            }
            if entry.size() > max_member {
                skipped.push((inner, format!("not unpacked: {} bytes, over the limit", entry.size())));
                std::io::copy(data, &mut std::io::sink())?;
                return Ok(true);
            }
            unpacked += entry.size();
            if unpacked > max_unpacked {
                return Ok(false);
            }
            let mut buf = Vec::new();
            data.take(max_member + 1).read_to_end(&mut buf)?;
            members.push((inner, buf));
            Ok(true)
        })
        .map_err(|e| Error::new(format!("7z: {e}")))?;
    let mut out = Vec::new();
    for (inner, why) in skipped {
        out.push(failed(&format!("{key}!{inner}"), &format!("{name} > {inner}"), &why));
    }
    for (inner, data) in members {
        member(&mut out, &data, key, name, &inner, ctx);
    }
    if unpacked > max_unpacked {
        out.push(failed(key, name, &format!("7z: more than {max_unpacked} bytes unpacked, the rest was left out")));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::readers::Options;

    #[test]
    fn zip_members_are_read_and_nested_ones_too() {
        let inner = {
            let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
            let o = zip::write::SimpleFileOptions::default();
            assert!(w.start_file("deep.md", o).is_ok());
            assert!(w.write_all(b"# Deep\n\ntext").is_ok());
            w.finish().map(Cursor::into_inner).unwrap_or_default()
        };
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let o = zip::write::SimpleFileOptions::default();
        assert!(w.start_file("a/readme.txt", o).is_ok());
        assert!(w.write_all(b"hello").is_ok());
        assert!(w.start_file("inner.zip", o).is_ok());
        assert!(w.write_all(&inner).is_ok());
        assert!(w.start_file("__MACOSX/a/._readme.txt", o).is_ok());
        assert!(w.write_all(b"junk").is_ok());
        let bytes = w.finish().map(Cursor::into_inner).unwrap_or_default();
        let options = Options::default();
        let docs = zip(&bytes, "p.zip", "p.zip", &Ctx { options: &options, depth: 0 }).unwrap_or_default();
        let keys: Vec<&str> = docs.iter().map(|d| d.key.as_str()).collect();
        assert_eq!(keys, ["p.zip!a/readme.txt", "p.zip!inner.zip!deep.md"]);
        assert_eq!(docs.get(1).map(|d| d.name.as_str()), Some("p.zip > inner.zip > deep.md"));
    }
}
