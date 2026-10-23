//! File-level operations of the writer (spec chapters 7 and 8): `new`,
//! `import`, `export`, `add`, `rm`. Each returns a freshly built file; the
//! caller writes it with [`crate::build::write_atomic`].

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::build::{
    Built, RoomSource, TEMPLATE, Warning, build, new_room_id, normalize_newlines, parse_body,
};
use crate::error::{Error, ErrorCode, Result};
use crate::sections::{BodyLength, Limits, Sections};
use crate::text::slug;

fn io_err(path: &Path, e: &std::io::Error) -> Error {
    Error::new(ErrorCode::E012, format!("{}: {e}", path.display()))
}

/// Rooms of an existing file, read tolerantly (a hand-edited file is fine).
///
/// # Errors
/// A broken header or layout, or a body that does not start with `@@@`.
pub fn rooms_of(file: &[u8]) -> Result<Vec<RoomSource>> {
    let s = Sections::parse_with(file, &Limits::default(), BodyLength::Tolerate)?;
    parse_body(s.body)
}

/// `kapu new`: one room `szoba` with the FR-05 template (FR-04 edge case).
///
/// # Errors
/// Only a builder error, which the template never triggers.
pub fn new_file() -> Result<Built> {
    build(&[RoomSource {
        id: "szoba".to_owned(),
        content: TEMPLATE.to_owned(),
    }])
}

/// `kapu import <dir>` (spec 7.3): every `*.md` file of `dir` (not
/// recursive) is a room, in file-name byte order; the id is the FR-04/4 slug
/// of the file name without extension, `-2`, `-3`... on collision (D90).
/// CRLF is converted to LF with a W004 warning (D40).
///
/// # Errors
/// I/O errors (E012), a file that is not UTF-8 (E004), or a builder error.
pub fn import_dir(dir: &Path) -> Result<Built> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| io_err(dir, &e))?
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("md")))
        .collect();
    paths.sort_by(|a, b| {
        a.file_name()
            .map(std::ffi::OsStr::as_encoded_bytes)
            .cmp(&b.file_name().map(std::ffi::OsStr::as_encoded_bytes))
    });

    let mut warnings: Vec<Warning> = Vec::new();
    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut rooms = Vec::with_capacity(paths.len());
    for path in &paths {
        let bytes = std::fs::read(path).map_err(|e| io_err(path, &e))?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let text = String::from_utf8(bytes).map_err(|e| {
            Error::new(
                ErrorCode::E004,
                format!(
                    "{name} is not UTF-8 at byte {}",
                    e.utf8_error().valid_up_to()
                ),
            )
        })?;
        let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text);
        let content = normalize_newlines(&name, text, &mut warnings);
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let base = slug(&stem);
        let mut id = base.clone();
        let mut n = 2;
        while used.contains(&id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        used.insert(id.clone());
        rooms.push(RoomSource { id, content });
    }
    let mut built = build(&rooms)?;
    warnings.append(&mut built.warnings);
    built.warnings = warnings;
    Ok(built)
}

/// `kapu export <file> <dir>` (spec 7.4): every room as `<dir>/<id>.md`
/// (escape undone, comments kept). The id grammar keeps every path inside
/// `dir`. Returns the written paths in body order.
///
/// # Errors
/// A broken file or an I/O error (E012).
pub fn export_dir(file: &[u8], dir: &Path) -> Result<Vec<PathBuf>> {
    let rooms = rooms_of(file)?;
    std::fs::create_dir_all(dir).map_err(|e| io_err(dir, &e))?;
    let mut written = Vec::with_capacity(rooms.len());
    for room in rooms {
        if !crate::is_valid_id(&room.id) || room.id.contains('/') {
            return Err(Error::new(
                ErrorCode::E008,
                format!("`{}` is not a valid room id", room.id),
            ));
        }
        let path = dir.join(format!("{}.md", room.id));
        std::fs::write(&path, room.content.as_bytes()).map_err(|e| io_err(&path, &e))?;
        written.push(path);
    }
    Ok(written)
}

/// `kapu add <file> <md> [--id <id>]`: replaces the room `id` if it exists,
/// otherwise appends a new room (id from `--id` or from the title, FR-04/4).
/// Returns the build and the room id.
///
/// # Errors
/// A broken file, an invalid `--id` (E008) or a builder error.
pub fn add_room(
    file: &[u8],
    name: &str,
    markdown: &str,
    id: Option<&str>,
) -> Result<(Built, String)> {
    let mut rooms = rooms_of(file)?;
    let mut warnings = Vec::new();
    let content = normalize_newlines(
        name,
        markdown.strip_prefix('\u{FEFF}').unwrap_or(markdown),
        &mut warnings,
    );
    let id = id.map_or_else(
        || {
            let used: BTreeSet<String> = rooms.iter().map(|r| r.id.clone()).collect();
            new_room_id(&content, &used)
        },
        str::to_owned,
    );
    match rooms.iter_mut().find(|r| r.id == id) {
        Some(room) => room.content = content,
        None => rooms.push(RoomSource {
            id: id.clone(),
            content,
        }),
    }
    let mut built = build(&rooms)?;
    warnings.append(&mut built.warnings);
    built.warnings = warnings;
    Ok((built, id))
}

/// `kapu rm <file> <id>` (D45): removes one room with its sub-doors.
///
/// # Errors
/// E009 if there is no such room; a broken file or a builder error.
pub fn remove_room(file: &[u8], id: &str) -> Result<Built> {
    let mut rooms = rooms_of(file)?;
    let before = rooms.len();
    rooms.retain(|r| r.id != id);
    if rooms.len() == before {
        return Err(Error::new(ErrorCode::E009, format!("no room `{id}`")));
    }
    build(&rooms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kapu-edit-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn new_file_has_the_template_room() {
        let built = new_file().unwrap();
        crate::validate(&built.bytes, crate::Level::Full).unwrap();
        assert_eq!(rooms_of(&built.bytes).unwrap()[0].content, TEMPLATE);
    }

    #[test]
    fn import_then_export_is_byte_identical() {
        let src = temp_dir("src");
        std::fs::write(src.join("b-masodik.md"), "# Második\nszöveg\n").unwrap();
        std::fs::write(
            src.join("A Első.md"),
            "# Első\n@@@ trükk\n\\@@@ is\nnincs záró LF",
        )
        .unwrap();
        std::fs::write(src.join("crlf.md"), "# CRLF\r\nsor\r\n").unwrap();
        std::fs::write(src.join("skip.txt"), "nem md").unwrap();
        let built = import_dir(&src).unwrap();
        assert!(built.warnings.iter().any(|w| w.code == "W004"));
        let ids: Vec<String> = rooms_of(&built.bytes)
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(
            ids,
            ["a-elso", "b-masodik", "crlf"],
            "file-name byte order, slug ids"
        );

        let out = temp_dir("out");
        export_dir(&built.bytes, &out).unwrap();
        assert_eq!(
            std::fs::read(out.join("a-elso.md")).unwrap(),
            std::fs::read(src.join("A Első.md")).unwrap()
        );
        assert_eq!(
            std::fs::read(out.join("b-masodik.md")).unwrap(),
            std::fs::read(src.join("b-masodik.md")).unwrap()
        );
        assert_eq!(
            std::fs::read(out.join("crlf.md")).unwrap(),
            b"# CRLF\nsor\n"
        );
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn add_replace_and_remove() {
        let file = new_file().unwrap().bytes;
        let (built, id) = add_room(&file, "x.md", "# Glass gomb\nüveg\n", None).unwrap();
        assert_eq!(id, "glass-gomb");
        let (built, id2) = add_room(&built.bytes, "x.md", "# Glass gomb\nmásik\n", None).unwrap();
        assert_eq!(
            id2, "glass-gomb-2",
            "a new title-derived id never overwrites"
        );
        let (built, _) = add_room(&built.bytes, "x.md", "# Csere\n", Some("glass-gomb")).unwrap();
        let rooms = rooms_of(&built.bytes).unwrap();
        assert_eq!(rooms.len(), 3);
        assert_eq!(rooms[1].content, "# Csere\n");
        let built = remove_room(&built.bytes, "glass-gomb-2").unwrap();
        assert_eq!(rooms_of(&built.bytes).unwrap().len(), 2);
        assert_eq!(
            remove_room(&built.bytes, "nincs").unwrap_err().code,
            ErrorCode::E009
        );
        assert_eq!(
            add_room(&built.bytes, "x.md", "x", Some("Rossz"))
                .unwrap_err()
                .code,
            ErrorCode::E008
        );
    }
}
