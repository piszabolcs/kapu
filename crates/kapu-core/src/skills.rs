//! Skill import (K-10, D147): a folder of agent skills as rooms.
//!
//! A skill is a folder with a `SKILL.md` (YAML front matter with `name` and
//! `description`, then Markdown), optional scripts under `scripts/` and
//! optional reference Markdown files. Each skill becomes a room: its `name`
//! is the title, its `description` the `>` line, then `kind: skill`, and
//! `has_scripts: yes` when it has scripts; then the body of `SKILL.md` as it
//! is, a `## References` section that links the reference rooms and a
//! `## Scripts` section with every script as a fenced code block. Every other
//! Markdown file of the skill (outside `scripts/`, `assets/` and the like) is
//! a room of its own, `kind: skill` too. Nothing is run (D2): a script is
//! text in the file, the agent decides what to do with it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::build::{
    Built, RoomSource, Warning, build, normalize_newlines, strip_controls, unique_id,
};
use crate::error::{Error, ErrorCode, Result};
use crate::text::slug;

/// Folders of a skill that hold no reference text.
const SKIP_DIRS: [&str; 7] = [
    "scripts",
    "assets",
    "templates",
    "node_modules",
    ".git",
    "evals",
    "tests",
];
/// Largest script that goes into a room, in bytes; a larger one is listed.
pub const MAX_SCRIPT_BYTES: u64 = 256 * 1024;
/// How deep below the import folder `SKILL.md` files are looked for.
const MAX_DEPTH: usize = 6;

fn io_err(path: &Path, e: &std::io::Error) -> Error {
    Error::new(ErrorCode::E012, format!("{}: {e}", path.display()))
}

/// Entries of a folder in file-name byte order (as `kapu import`).
fn sorted_entries(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| io_err(dir, &e))?
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .collect();
    paths.sort_by(|a, b| {
        a.file_name()
            .map(std::ffi::OsStr::as_encoded_bytes)
            .cmp(&b.file_name().map(std::ffi::OsStr::as_encoded_bytes))
    });
    Ok(paths)
}

/// Skill folders under `dir`: a folder with a `SKILL.md` is a skill and is
/// not searched further.
fn find_skills(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) -> Result<()> {
    if dir.join("SKILL.md").is_file() {
        out.push(dir.to_path_buf());
        return Ok(());
    }
    if depth == 0 {
        return Ok(());
    }
    for path in sorted_entries(dir)? {
        let hidden = path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.') || n == "node_modules");
        if path.is_dir() && !hidden {
            find_skills(&path, depth - 1, out)?;
        }
    }
    Ok(())
}

/// Every file under `dir`, depth first, in file-name byte order.
fn files_under(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for path in sorted_entries(dir)? {
        if path.is_dir() {
            files_under(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

/// The YAML front matter's `key: value` lines (a `>` or `|` block joined into
/// one line, quotes removed) and the text after it.
fn front_matter(text: &str) -> (BTreeMap<String, String>, &str) {
    let mut meta = BTreeMap::new();
    let Some(rest) = text.strip_prefix("---\n") else {
        return (meta, text);
    };
    let Some(end) = rest.find("\n---") else {
        return (meta, text);
    };
    let body = rest
        .get(end + 4..)
        .unwrap_or("")
        .trim_start_matches(|c| c != '\n')
        .trim_start_matches('\n');
    let mut lines = rest.get(..end).unwrap_or("").lines().peekable();
    while let Some(line) = lines.next() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if key.is_empty() || key.starts_with([' ', '\t', '-', '#']) {
            continue;
        }
        let mut value = value.trim().to_owned();
        if matches!(value.as_str(), ">" | "|" | ">-" | "|-" | ">+" | "|+") {
            let mut block = Vec::new();
            while let Some(next) = lines.next_if(|l| l.starts_with([' ', '\t']) || l.is_empty()) {
                block.push(next.trim());
            }
            value = block.join(" ");
        }
        let unquoted = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .map(str::to_owned);
        meta.insert(key.trim().to_lowercase(), unquoted.unwrap_or(value));
    }
    (meta, body)
}

/// One line: whitespace runs as one space.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A code fence longer than every backtick run in `text`.
fn fence_for(text: &str) -> String {
    let mut longest = 0usize;
    let mut run = 0usize;
    for c in text.chars() {
        if c == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    "`".repeat(longest.max(2) + 1)
}

/// The info string of a script's code block, by file extension.
fn language(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "py" => "python",
        "js" | "mjs" | "cjs" => "javascript",
        "ts" => "typescript",
        "sh" | "bash" => "bash",
        "ps1" => "powershell",
        "rb" => "ruby",
        "go" => "go",
        "rs" => "rust",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "html" => "html",
        "css" => "css",
        "sql" => "sql",
        _ => "",
    }
}

/// `a/b.md` with forward slashes, relative to `base`.
fn relative(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// A free id from `base` (FR-04/4 slug, `-2`, `-3`... on collision).
fn free_id(base: &str, used: &mut BTreeSet<String>) -> String {
    let id = unique_id(&slug(base), used);
    used.insert(id.clone());
    id
}

/// A UTF-8 text file, or a W009 warning and `None`.
fn read_text(path: &Path, shown: &str, warnings: &mut Vec<Warning>) -> Result<Option<String>> {
    let bytes = std::fs::read(path).map_err(|e| io_err(path, &e))?;
    if let Ok(text) = String::from_utf8(bytes) {
        let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text).to_owned();
        let text = normalize_newlines(shown, &text, warnings);
        Ok(Some(strip_controls(shown, &text, warnings)))
    } else {
        warnings.push(Warning {
            code: "W009",
            message: format!("{shown} left out: not UTF-8 text"),
        });
        Ok(None)
    }
}

/// The `## Scripts` section: every script as a fenced code block, a too large
/// or binary one by name only.
fn scripts_section(
    scripts: &[PathBuf],
    skill: &Path,
    dir: &Path,
    warnings: &mut Vec<Warning>,
) -> Result<String> {
    let mut out =
        String::from("\n## Scripts\n\nThe skill's scripts as text; KAPU does not run them.\n");
    for path in scripts {
        let rel = relative(path, skill);
        let size = std::fs::metadata(path).map_or(0, |m| m.len());
        if size > MAX_SCRIPT_BYTES {
            let _ = write!(out, "\n### {rel}\n\n({size} bytes, too large to include)\n");
            continue;
        }
        let bytes = std::fs::read(path).map_err(|e| io_err(path, &e))?;
        let Ok(code) = String::from_utf8(bytes) else {
            let _ = write!(out, "\n### {rel}\n\n(binary file, not included)\n");
            continue;
        };
        let code = normalize_newlines(&relative(path, dir), &code, warnings);
        let code = strip_controls(&relative(path, dir), &code, warnings);
        let fence = fence_for(&code);
        let _ = write!(
            out,
            "\n### {rel}\n\n{fence}{}\n{}\n{fence}\n",
            language(path),
            code.trim_end_matches('\n')
        );
    }
    Ok(out)
}

/// One skill: its room, then its reference rooms.
fn skill_rooms(
    skill: &Path,
    dir: &Path,
    used: &mut BTreeSet<String>,
    warnings: &mut Vec<Warning>,
) -> Result<Vec<RoomSource>> {
    let shown = relative(&skill.join("SKILL.md"), dir);
    let Some(text) = read_text(&skill.join("SKILL.md"), &shown, warnings)? else {
        return Ok(Vec::new());
    };
    let (meta, body) = front_matter(&text);
    let folder = skill
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = meta
        .get("name")
        .filter(|n| !n.is_empty())
        .cloned()
        .unwrap_or(folder);
    let id = free_id(&name, used);

    let mut files = Vec::new();
    files_under(skill, &mut files)?;
    let mut references: Vec<(String, String)> = Vec::new(); // (id, title)
    let mut rooms = vec![RoomSource {
        id: id.clone(),
        content: String::new(),
    }];
    let mut scripts: Vec<PathBuf> = Vec::new();
    for path in &files {
        let rel = relative(path, skill);
        let parts: Vec<&str> = rel.split('/').collect();
        if parts.first() == Some(&"scripts") {
            scripts.push(path.clone());
            continue;
        }
        let in_skip = parts
            .iter()
            .take(parts.len().saturating_sub(1))
            .any(|p| SKIP_DIRS.contains(p));
        let is_md = path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("md"));
        if in_skip || !is_md || rel == "SKILL.md" || rel.eq_ignore_ascii_case("LICENSE.md") {
            continue;
        }
        let Some(doc) = read_text(path, &relative(path, dir), warnings)? else {
            continue;
        };
        let (_, doc_body) = front_matter(&doc);
        let title = doc_body
            .lines()
            .find_map(|l| l.strip_prefix("# "))
            .map_or_else(|| rel.clone(), |t| t.trim().to_owned());
        let ref_id = free_id(&format!("{id}-{}", rel.trim_end_matches(".md")), used);
        references.push((ref_id.clone(), title.clone()));
        rooms.push(RoomSource {
            id: ref_id,
            content: format!(
                "# {title}\n> Reference of the {name} skill: {rel}.\nkind: skill\n\n{}\n",
                doc_body.trim_end()
            ),
        });
    }

    let description = one_line(meta.get("description").map_or("", String::as_str));
    let mut content = format!("# {name}\n> {description}\nkind: skill\n");
    if !scripts.is_empty() {
        content.push_str("has_scripts: yes\n");
    }
    let _ = write!(content, "\n{}\n", body.trim_end());
    if !references.is_empty() {
        content.push_str("\n## References\n\n");
        for (ref_id, title) in &references {
            let _ = writeln!(content, "- [{title}](#{ref_id})");
        }
    }
    if !scripts.is_empty() {
        content.push_str(&scripts_section(&scripts, skill, dir, warnings)?);
    }
    if let Some(room) = rooms.first_mut() {
        room.content = content;
    }
    Ok(rooms)
}

/// K-10 (D147): every skill under `dir` (`dir` itself may be one skill) as a
/// room, its reference Markdown files as rooms of their own, in path order.
///
/// # Errors
/// E012 for an unreadable folder or file, E008 if no skill is found, and any
/// [`build`] error.
pub fn import_skills(dir: &Path) -> Result<Built> {
    let mut skills = Vec::new();
    find_skills(dir, MAX_DEPTH, &mut skills)?;
    if skills.is_empty() {
        return Err(Error::new(
            ErrorCode::E012,
            format!("no SKILL.md under {}", dir.display()),
        ));
    }
    let mut warnings: Vec<Warning> = Vec::new();
    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut rooms: Vec<RoomSource> = Vec::new();
    for skill in &skills {
        rooms.extend(skill_rooms(skill, dir, &mut used, &mut warnings)?);
    }
    let mut built = build(&rooms)?;
    warnings.append(&mut built.warnings);
    built.warnings = warnings;
    Ok(built)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_matter_name_description_and_blocks() {
        let (meta, body) = front_matter(
            "---\nname: frontend-design\ndescription: >\n  Builds pages\n  that look good.\nlicense: \"MIT\"\n---\n# Title\ntext\n",
        );
        assert_eq!(
            meta.get("name").map(String::as_str),
            Some("frontend-design")
        );
        assert_eq!(
            meta.get("description").map(String::as_str),
            Some("Builds pages that look good.")
        );
        assert_eq!(meta.get("license").map(String::as_str), Some("MIT"));
        assert_eq!(body, "# Title\ntext\n");
        let (none, all) = front_matter("# No front matter\n");
        assert!(none.is_empty());
        assert_eq!(all, "# No front matter\n");
    }

    #[test]
    fn fences_outgrow_the_backticks_inside() {
        assert_eq!(fence_for("plain"), "```");
        assert_eq!(fence_for("a ``` b"), "````");
        assert_eq!(fence_for("`````"), "``````");
    }

    #[test]
    fn a_skill_folder_becomes_rooms() {
        // K-10: name as title, description as the `>` line, kind: skill,
        // has_scripts: yes, scripts as code blocks, references as linked rooms
        let root = std::env::temp_dir().join(format!("kapu-skills-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let skill = root.join("plugin").join("pdf-tools");
        std::fs::create_dir_all(skill.join("scripts")).unwrap();
        std::fs::create_dir_all(skill.join("reference")).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: pdf-tools\ndescription: Merge and split PDF files.\n---\n# PDF\n\nUse the script.\n",
        )
        .unwrap();
        std::fs::write(skill.join("scripts").join("merge.py"), "print('```')\n").unwrap();
        std::fs::write(
            skill.join("reference").join("forms.md"),
            "# Forms\nFill forms.\n",
        )
        .unwrap();
        let built = import_skills(&root).unwrap();
        crate::validate(&built.bytes, crate::Level::Full).unwrap();
        let rooms = crate::edit::rooms_of(&built.bytes).unwrap();
        assert_eq!(rooms.len(), 2, "{rooms:?}");
        let main = &rooms[0];
        assert_eq!(main.id, "pdf-tools");
        assert!(main.content.starts_with(
            "# pdf-tools\n> Merge and split PDF files.\nkind: skill\nhas_scripts: yes\n"
        ));
        assert!(
            main.content
                .contains("### scripts/merge.py\n\n````python\nprint('```')\n````\n")
        );
        assert!(
            main.content
                .contains("- [Forms](#pdf-tools-reference-forms)")
        );
        assert_eq!(rooms[1].id, "pdf-tools-reference-forms");
        let s = crate::Sections::parse(&built.bytes).unwrap();
        let map = crate::catalog::parse_map(s.map).unwrap();
        assert_eq!(map.door("pdf-tools").map(|d| d.kind), Some("skill"));
        assert_eq!(
            map.door("pdf-tools-reference-forms").map(|d| d.kind),
            Some("skill")
        );
        assert!(
            s.graph
                .contains("pdf-tools/references\tpdf-tools-reference-forms\tlink"),
            "{}",
            s.graph
        );
        // D154: the catalog lists the skill once, its reference folded in
        let catalog = crate::view::kind_catalog_in(&s, "skill").unwrap();
        assert_eq!(catalog.len(), 1, "{catalog:?}");
        assert_eq!(catalog[0].folded, ["pdf-tools-reference-forms"]);
        assert_eq!(
            catalog[0].line(),
            format!(
                "pdf-tools | {}t | Merge and split PDF files. | +1 room",
                catalog[0].tokens
            )
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
