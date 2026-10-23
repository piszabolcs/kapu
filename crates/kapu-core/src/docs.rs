//! Door positions and fingerprints (FR-14), escape (FR-04/2) and the served
//! text of a door (FR-15/2).
//!
//! `#DOCS` has one line per door:
//! `<id>\t@<start>+<len>\tb3:<64 hex>\tdl=<n>\tc=<n>`, where the span is
//! relative to the start of the body and points at the stored (escaped)
//! content without its `@@@` line (D36). Reading a door means: slice those
//! bytes out of the body, check their BLAKE3 fingerprint, undo the escape and
//! drop the `<!-- -->` comments.

use crate::error::{Error, ErrorCode, Result};
use crate::header::{Hash, Span, parse_decimal, parse_hash};
use crate::sections::Sections;

/// One line of `#DOCS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocEntry<'a> {
    /// Door id (`room` or `room/sub`). The grammar is checked by the validator.
    pub id: &'a str,
    /// Stored content, relative to the start of the body.
    pub span: Span,
    /// BLAKE3 of the stored bytes.
    pub hash: Hash,
    /// Document length used by BM25 (FR-09/6).
    pub dl: u64,
    /// Character count of the served text (FR-08).
    pub chars: u64,
}

fn e005(detail: impl Into<String>) -> Error {
    Error::new(ErrorCode::E005, detail)
}

/// BLAKE3 of `bytes` as a [`Hash`].
#[must_use]
pub fn blake3_of(bytes: &[u8]) -> Hash {
    Hash(*blake3::hash(bytes).as_bytes())
}

/// Parses the inner text of `#DOCS` (without the frame lines).
///
/// A line that does not have the five fields in the right shape is E005
/// (malformed section, D72); a hash with another prefix is E011.
///
/// # Errors
/// The first malformed line, with its 1-based line number.
pub fn parse_docs(text: &str) -> Result<Vec<DocEntry<'_>>> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let n = n + 1;
        let bad = |what: &str| e005(format!("#DOCS line {n}: {what}"));
        let mut fields = line.split('\t');
        let (Some(id), Some(span), Some(hash), Some(dl), Some(chars), None) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) else {
            return Err(bad("expected 5 tab-separated fields"));
        };
        if id.is_empty() {
            return Err(bad("empty id"));
        }
        let (start, len) = span
            .strip_prefix('@')
            .and_then(|s| s.split_once('+'))
            .ok_or_else(|| bad("position is not `@start+len`"))?;
        let span = Span {
            start: parse_decimal(start).ok_or_else(|| bad("bad start"))?,
            len: parse_decimal(len).ok_or_else(|| bad("bad length"))?,
        };
        span.end().ok_or_else(|| bad("position overflows"))?;
        let hash = parse_hash(&format!("#DOCS line {n}"), hash, ErrorCode::E005)?;
        let number = |field: &str, key: &str| {
            field
                .strip_prefix(key)
                .and_then(parse_decimal)
                .ok_or_else(|| bad(&format!("expected `{key}<n>`")))
        };
        out.push(DocEntry {
            id,
            span,
            hash,
            dl: number(dl, "dl=")?,
            chars: number(chars, "c=")?,
        });
    }
    Ok(out)
}

impl<'a> Sections<'a> {
    /// The `#DOCS` entries of this file.
    ///
    /// # Errors
    /// See [`parse_docs`].
    pub fn doc_entries(&self) -> Result<Vec<DocEntry<'a>>> {
        parse_docs(self.docs)
    }

    /// Checks `bodyhash` against the stored body (D37).
    ///
    /// # Errors
    /// E006 if the fingerprint differs.
    pub fn check_bodyhash(&self) -> Result<()> {
        if blake3_of(self.body) == self.header.bodyhash {
            Ok(())
        } else {
            Err(Error::new(
                ErrorCode::E006,
                "bodyhash does not match the body",
            ))
        }
    }

    /// The stored bytes of one door, after checking the position (E005) and
    /// the fingerprint (E007).
    ///
    /// # Errors
    /// E005 if the span leaves the body, E007 if the BLAKE3 differs.
    pub fn stored(&self, entry: &DocEntry<'_>) -> Result<&'a [u8]> {
        let bytes = usize::try_from(entry.span.start)
            .ok()
            .zip(usize::try_from(entry.span.len).ok())
            .and_then(|(s, l)| self.body.get(s..s.checked_add(l)?))
            .ok_or_else(|| {
                e005(format!(
                    "door `{}` at @{}+{} lies outside the body ({} bytes)",
                    entry.id,
                    entry.span.start,
                    entry.span.len,
                    self.body.len()
                ))
            })?;
        if blake3_of(bytes) != entry.hash {
            return Err(Error::new(
                ErrorCode::E007,
                format!(
                    "fingerprint of door `{}` does not match its bytes",
                    entry.id
                ),
            ));
        }
        Ok(bytes)
    }
}

/// FR-04/2 read side: a line that is one or more `\` followed by `@@@`
/// loses one `\`. Every other line is unchanged.
#[must_use]
pub fn unescape(stored: &str) -> String {
    let mut out = String::with_capacity(stored.len());
    for (i, line) in stored.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let after = line.trim_start_matches('\\');
        if after.len() < line.len() && after.starts_with("@@@") {
            out.push_str(line.get(1..).unwrap_or(""));
        } else {
            out.push_str(line);
        }
    }
    out
}

/// D59/D108: removes every `<!-- ... -->` span, also across lines. A line of
/// the result that had a comment removed and is now blank is dropped with its
/// LF; every other line stays as it is. An unclosed `<!--` is kept.
#[must_use]
pub fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut touched: Vec<usize> = Vec::new();
    let mut line = 0usize;
    let mut rest = text;
    loop {
        let Some(open) = rest.find("<!--") else {
            out.push_str(rest);
            break;
        };
        let before = rest.get(..open).unwrap_or("");
        line += before.matches('\n').count();
        out.push_str(before);
        let after_open = rest.get(open + 4..).unwrap_or("");
        let Some(close) = after_open.find("-->") else {
            out.push_str(rest.get(open..).unwrap_or(""));
            break;
        };
        touched.push(line);
        rest = after_open.get(close + 3..).unwrap_or("");
    }
    out.split('\n')
        .enumerate()
        .filter(|(k, l)| !(touched.contains(k) && l.trim().is_empty()))
        .map(|(_, l)| l)
        .collect::<Vec<_>>()
        .join("\n")
}

// The text a reader serves for stored door bytes: escape undone, comments
/// removed (FR-15/2).
///
/// # Errors
/// E004 if the bytes are not UTF-8 (a span that cuts a character in two).
pub fn served_text(stored: &[u8]) -> Result<String> {
    let text = std::str::from_utf8(stored).map_err(|e| {
        Error::new(
            ErrorCode::E004,
            format!("door content is not UTF-8 at offset {}", e.valid_up_to()),
        )
    })?;
    Ok(strip_comments(&unescape(text)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "b3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262";

    #[test]
    fn docs_line_parses() {
        let text = format!("room/sub\t@27+687\t{H}\tdl=147\tc=664\n");
        let e = parse_docs(&text).unwrap();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].id, "room/sub");
        assert_eq!(
            e[0].span,
            Span {
                start: 27,
                len: 687
            }
        );
        assert_eq!((e[0].dl, e[0].chars), (147, 664));
        assert_eq!(e[0].hash, blake3_of(b""));
    }

    #[test]
    fn malformed_docs_lines() {
        for bad in [
            format!("a\t27+1\t{H}\tdl=1\tc=1"),
            format!("a\t@27+1\t{H}\tdl=1"),
            format!("a\t@27+1\t{H}\tdl=1\tc=1\textra"),
            format!("a\t@27+1\t{H}\tdl=01\tc=1"),
            format!("\t@27+1\t{H}\tdl=1\tc=1"),
            "a\t@27+1\tb3:xyz\tdl=1\tc=1".to_owned(),
        ] {
            assert_eq!(parse_docs(&bad).unwrap_err().code, ErrorCode::E005, "{bad}");
        }
        let sha = "a\t@27+1\tsha256:00\tdl=1\tc=1";
        assert_eq!(parse_docs(sha).unwrap_err().code, ErrorCode::E011);
    }

    #[test]
    fn blake3_matches_the_reference_vector() {
        // BLAKE3 of the empty input, from the official test vectors; the same
        // value the Python reference (tools/blake3_ref.py) asserts.
        assert_eq!(blake3::hash(b"").to_hex().as_str(), &H[3..]);
    }

    #[test]
    fn unescape_removes_exactly_one_backslash() {
        assert_eq!(
            unescape("\\@@@ a\n\\\\@@@ b\nx\\@@@ c\n\\x"),
            "@@@ a\n\\@@@ b\nx\\@@@ c\n\\x"
        );
    }

    #[test]
    fn strip_comments_rules() {
        // comment-only line disappears with its LF; empty lines stay
        assert_eq!(strip_comments("a\n<!-- c -->\n\nb"), "a\n\nb");
        // inline comment leaves the rest of the line
        assert_eq!(strip_comments("x <!-- c --> y"), "x  y");
        // unclosed comment stays
        assert_eq!(strip_comments("a <!-- open"), "a <!-- open");
        // D108: a multi-line comment is removed like a single-line one; the
        // line it leaves blank goes, untouched blank lines stay
        assert_eq!(strip_comments("a\n<!-- 1\n2 -->\n  \n\nb"), "a\n  \n\nb");
        assert_eq!(strip_comments("x <!-- 1\n2 --> y\nz"), "x  y\nz");
        assert_eq!(strip_comments("a\n<!-- c -->"), "a");
        assert_eq!(strip_comments("<!-- a -->\n<!-- b -->\nc"), "c");
        assert_eq!(strip_comments("  <!-- c -->  \n   \nd"), "   \nd");
    }
}
