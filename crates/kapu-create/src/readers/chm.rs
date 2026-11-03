//! Compiled HTML help (KD10): on Windows `hh.exe -decompile` unpacks it,
//! the contents file (`.hhc`) gives the order, and every page goes through
//! the HTML reader. Elsewhere a CHM file is reported, not read.

use std::path::{Path, PathBuf};

use crate::doc::Document;
use crate::readers::{Ctx, html};
use crate::{Error, Result};

/// A `.chm` file.
///
/// # Errors
/// Off Windows, or when `hh.exe` gives no pages.
pub fn read(bytes: &[u8], key: &str, name: &str, ctx: &Ctx<'_>) -> Result<Vec<Document>> {
    if !cfg!(windows) {
        return Err(Error::new(
            "CHM help files are read on Windows only (with hh.exe)",
        ));
    }
    let _ = ctx;
    let dir = std::env::temp_dir().join(format!(
        "kapu-chm-{}-{}",
        std::process::id(),
        blake3::hash(key.as_bytes())
            .to_hex()
            .get(..12)
            .unwrap_or("x")
    ));
    let out = dir.join("out");
    std::fs::create_dir_all(&out)?;
    let file = dir.join("help.chm");
    std::fs::write(&file, bytes)?;
    let hh = std::env::var_os("SystemRoot").map_or_else(
        || PathBuf::from("hh.exe"),
        |r| PathBuf::from(r).join("hh.exe"),
    );
    let status = std::process::Command::new(&hh)
        .arg("-decompile")
        .arg(&out)
        .arg(&file)
        .status();
    let result = match status {
        Ok(_) => pages(&out, key, name),
        Err(e) => Err(Error::new(format!("hh.exe: {e}"))),
    };
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn pages(out: &Path, key: &str, name: &str) -> Result<Vec<Document>> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect(out, &mut files);
    let rel = |p: &Path| {
        p.strip_prefix(out)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default()
    };
    let order = files
        .iter()
        .find(|p| crate::detect::extension(&p.to_string_lossy()) == "hhc")
        .and_then(|p| std::fs::read(p).ok())
        .map(|b| contents_order(&crate::text::decode(&b)))
        .unwrap_or_default();
    let mut htmls: Vec<String> = files
        .iter()
        .map(|p| rel(p))
        .filter(|r| matches!(crate::detect::extension(r).as_str(), "htm" | "html"))
        .collect();
    htmls.sort_by_key(|r| {
        let pos = order
            .iter()
            .position(|o| o.eq_ignore_ascii_case(r))
            .unwrap_or(usize::MAX);
        (pos, r.to_lowercase())
    });
    let mut docs = Vec::new();
    for r in htmls {
        let Ok(bytes) = std::fs::read(out.join(&r)) else {
            continue;
        };
        let mut doc = html::read(&bytes, &format!("{key}!{r}"), &format!("{name} > {r}"));
        key.clone_into(&mut doc.site);
        doc.reader = "chm".into();
        docs.push(doc);
    }
    if docs.is_empty() {
        return Err(Error::new("hh.exe unpacked no HTML pages"));
    }
    Ok(docs)
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// The page order of a `.hhc` contents file: the `Local` parameters.
fn contents_order(hhc: &str) -> Vec<String> {
    let lower = hhc.to_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = lower.get(from..).and_then(|s| s.find("name=\"local\"")) {
        let at = from + i;
        from = at + 12;
        let Some(v) = lower.get(at..).and_then(|s| s.find("value=\"")) else {
            continue;
        };
        let start = at + v + 7;
        let Some(len) = hhc.get(start..).and_then(|s| s.find('"')) else {
            continue;
        };
        if let Some(value) = hhc.get(start..start + len) {
            let page = value.split('#').next().unwrap_or(value).replace('\\', "/");
            if !out.contains(&page) {
                out.push(page);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_contents_file_gives_the_order() {
        let hhc = r#"<UL><LI><OBJECT type="text/sitemap"><param name="Name" value="Start"><param name="Local" value="html\start.htm"></OBJECT>
            <LI><OBJECT><param name="Local" value="html/b.htm#x"><param name="Name" value="B"></OBJECT>"#;
        assert_eq!(contents_order(hhc), ["html/start.htm", "html/b.htm"]);
    }
}
