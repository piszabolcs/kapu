//! Text rules of the builder: ids from titles (FR-04/4), the escape (FR-04/2),
//! fenced code (FR-07), captions (FR-06) and links (FR-11/1).
//!
//! Every function here mirrors one function of `tools/build_vectors.py`, the
//! byte-level reference (D56). Where the Python behaviour is subtle, the
//! comment says so; changing either side means rebuilding the vectors.

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::canonical_combining_class;

use crate::docs::strip_comments;

/// Hard limit of a title in characters (FR-06/1).
pub const TITLE_MAX: usize = 200;
/// Hard limit of one tag in characters (FR-06/3).
pub const TAG_MAX: usize = 64;
/// At most this many tags (FR-06/3).
pub const TAGS_MAX: usize = 32;

/// NFC normalisation (FR-02/6).
#[must_use]
pub fn nfc(s: &str) -> String {
    s.nfc().collect()
}

/// FR-04/4 slug: lower-case, NFKD, combining marks dropped, every run of
/// other characters becomes one `-`, trimmed, at most 64 bytes. An empty
/// result becomes `szoba` (D58).
#[must_use]
pub fn slug(title: &str) -> String {
    let lowered = title.to_lowercase();
    let mut out = String::new();
    let mut dash = false;
    for c in lowered
        .nfkd()
        .filter(|c| canonical_combining_class(*c) == 0)
    {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    let cut = trimmed.get(..trimmed.len().min(64)).unwrap_or(trimmed);
    let cut = cut.trim_matches('-');
    if cut.is_empty() {
        "szoba".to_owned()
    } else {
        cut.to_owned()
    }
}

/// FR-04/2 write side: one extra `\` before every line that is zero or more
/// `\` followed by `@@@`.
#[must_use]
pub fn escape(content: &str) -> String {
    let mut out = String::with_capacity(content.len() + 8);
    for (i, line) in content.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if line.trim_start_matches('\\').starts_with("@@@") {
            out.push('\\');
        }
        out.push_str(line);
    }
    out
}

/// For each line: is it inside a fenced code block (three backticks or tildes, the fence
/// lines included)? A fence opens on a line whose left-trimmed text starts
/// with three backticks or tildes and closes on the next line that starts
/// with the same three characters.
#[must_use]
pub fn code_fence_mask(lines: &[&str]) -> Vec<bool> {
    let mut mask = Vec::with_capacity(lines.len());
    let mut fence: Option<&str> = None;
    for line in lines {
        let s = line.trim_start();
        match fence {
            None => {
                if s.starts_with("```") {
                    fence = Some("```");
                    mask.push(true);
                } else if s.starts_with("~~~") {
                    fence = Some("~~~");
                    mask.push(true);
                } else {
                    mask.push(false);
                }
            }
            Some(f) => {
                mask.push(true);
                if s.starts_with(f) {
                    fence = None;
                }
            }
        }
    }
    mask
}

/// FR-06/2 source text: paragraphs outside headings, comments, code, `>`
/// lines and the `tags:` line, line breaks as spaces. With `first_only`
/// only the first paragraph (sub-doors, FR-07/4).
#[must_use]
pub fn paragraphs(text: &str, first_only: bool) -> String {
    let stripped = strip_comments(text);
    let lines: Vec<&str> = stripped.split('\n').collect();
    let mask = code_fence_mask(&lines);
    let mut paras: Vec<String> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for (line, in_code) in lines.iter().zip(mask) {
        let skip = in_code
            || line.starts_with('#')
            || line.starts_with("> ")
            || line.trim() == ">"
            || line.starts_with("tags:");
        if line.trim().is_empty() || skip {
            if !cur.is_empty() {
                paras.push(cur.join(" "));
                cur.clear();
            }
            continue;
        }
        cur.push(line.trim());
    }
    if !cur.is_empty() {
        paras.push(cur.join(" "));
    }
    if first_only {
        paras.into_iter().next().unwrap_or_default()
    } else {
        paras.join(" ")
    }
}

/// Title (FR-06/1): the first `# ` line; if it is empty or missing, the
/// first non-empty line that is not a heading marker, `>` line or `tags:`
/// line (D87). Comments removed first.
#[must_use]
pub fn heading_title(text: &str) -> String {
    let stripped = strip_comments(text);
    let first = stripped
        .split('\n')
        .find_map(|line| line.strip_prefix("# "))
        .map_or("", str::trim);
    if !first.is_empty() {
        return first.to_owned();
    }
    for line in stripped.split('\n') {
        let s = line.trim();
        if !s.is_empty()
            && s != "#"
            && s != ">"
            && !s.starts_with("# ")
            && !s.starts_with("> ")
            && !s.starts_with("tags:")
        {
            return s.to_owned();
        }
    }
    String::new()
}

/// The first non-empty `> ` line (FR-06/2 manual description).
#[must_use]
pub fn manual_description(text: &str) -> String {
    strip_comments(text)
        .split('\n')
        .find_map(|line| {
            line.strip_prefix("> ")
                .map(str::trim)
                .filter(|d| !d.is_empty())
                .map(str::to_owned)
        })
        .unwrap_or_default()
}

/// The `tags:` line split on commas (FR-06/3). `None` if there is no
/// `tags:` line; `Some(empty)` if it is there but empty.
#[must_use]
pub fn manual_tags(text: &str) -> Option<Vec<String>> {
    let stripped = strip_comments(text);
    let line = stripped.split('\n').find(|l| l.starts_with("tags:"))?;
    Some(
        line.get(5..)
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| t.chars().take(TAG_MAX).collect())
            .take(TAGS_MAX)
            .collect(),
    )
}

/// First `max` characters.
#[must_use]
pub fn take_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// `[text](#id)` links outside fenced code (FR-11/1), in order.
#[must_use]
pub fn find_links(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mask = code_fence_mask(&lines);
    let mut out = Vec::new();
    for (line, in_code) in lines.iter().zip(mask) {
        if in_code {
            continue;
        }
        let mut rest = *line;
        while let Some(pos) = rest.find("](#") {
            let after = rest.get(pos + 3..).unwrap_or("");
            let id_len = after
                .bytes()
                .take_while(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-' || *b == b'/'
                })
                .count();
            let id = after.get(..id_len).unwrap_or("");
            let first_ok = id
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
            if first_ok && after.get(id_len..id_len + 1) == Some(")") {
                out.push(id.to_owned());
                rest = after.get(id_len + 1..).unwrap_or("");
            } else {
                rest = after;
            }
        }
    }
    out
}

/// The indexed content (D62): the served text without the `tags:` line.
#[must_use]
pub fn indexed_text(served: &str) -> String {
    served
        .split('\n')
        .filter(|l| !l.starts_with("tags:"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_rules() {
        assert_eq!(slug("Frontend design alapok"), "frontend-design-alapok");
        assert_eq!(slug("Electron leckék"), "electron-leckek");
        assert_eq!(slug("Szobajelölő | escape"), "szobajelolo-escape");
        assert_eq!(slug(""), "szoba");
        assert_eq!(slug("!!!"), "szoba");
        assert_eq!(slug(&"a".repeat(70)).len(), 64);
        assert_eq!(slug(&format!("{}-b", "a".repeat(63))), "a".repeat(63));
    }

    #[test]
    fn escape_rules() {
        assert_eq!(
            escape("@@@ hamis\n\\@@@ x\nok @@@"),
            "\\@@@ hamis\n\\\\@@@ x\nok @@@"
        );
        assert_eq!(
            crate::docs::unescape(&escape("@@@ a\n\\\\@@@ b")),
            "@@@ a\n\\\\@@@ b"
        );
    }

    #[test]
    fn fences() {
        let lines = ["a", "```css", "## x", "```", "~~~", "```", "~~~", "b"];
        assert_eq!(
            code_fence_mask(&lines),
            [false, true, true, true, true, true, true, false]
        );
    }

    #[test]
    fn captions() {
        let text = "# Cím\n> Leírás itt.\ntags: a, b ,, c\n\nElső bekezdés\nfolytatás.\n\n## Al\nMásodik.\n```\nkód\n```\n";
        assert_eq!(heading_title(text), "Cím");
        assert_eq!(manual_description(text), "Leírás itt.");
        assert_eq!(
            manual_tags(text),
            Some(vec!["a".to_owned(), "b".to_owned(), "c".to_owned()])
        );
        assert_eq!(paragraphs(text, false), "Első bekezdés folytatás. Második.");
        assert_eq!(paragraphs(text, true), "Első bekezdés folytatás.");
        assert_eq!(manual_tags("x"), None);
        assert_eq!(heading_title("<!-- c -->\nSima sor\n"), "Sima sor");
    }

    #[test]
    fn links_skip_code_and_bad_ids() {
        let text = "[a](#x-1) [b](#Bad) [c](#y/z)\n```\n[d](#q)\n```\n[e](#)";
        assert_eq!(find_links(text), ["x-1", "y/z"]);
    }
}
