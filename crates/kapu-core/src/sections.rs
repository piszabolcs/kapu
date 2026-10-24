//! Section layout (FR-01) and character rules (FR-02) of a whole file.
//!
//! [`Sections::parse`] takes the raw bytes of a file, parses the header, checks
//! that the five derived sections sit exactly where the header says, back to
//! back, each framed by `#NAME\n` ... `#END\n`, followed by `#BODY\n` and the
//! body. It also runs the character rules of FR-02 over the file. Every slice
//! it hands out has been bounds-checked against the file and against the
//! parser limits (NFR-01, ASSUMPTION-08) first; nothing is allocated whose size
//! comes from the file.

use crate::HEADER_LEN;
use crate::error::{Error, ErrorCode, Result};
use crate::header::{Header, Span};

/// Parser limits (NFR-01). Defaults are ASSUMPTION-08: a section at most
/// 1 GiB, a line at most 1 MiB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Largest accepted section, in bytes.
    pub max_section: u64,
    /// Longest accepted line in a derived section, in bytes (LF excluded).
    pub max_line: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_section: 1 << 30,
            max_line: 1 << 20,
        }
    }
}

/// How to treat a `body=` length that does not reach the end of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyLength {
    /// The header must describe the file exactly (validator).
    Strict,
    /// Accept a body that grew or shrank after the build; the reader then
    /// rebuilds positions from the `@@@` lines (FR-15 edge 2).
    Tolerate,
}

/// The five derived sections and the body of one file, as borrowed slices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sections<'a> {
    /// The parsed header.
    pub header: Header,
    /// Lines of `#MAP` without the `#MAP` and `#END` frame lines.
    pub map: &'a str,
    /// Lines of `#DESC` without the frame.
    pub desc: &'a str,
    /// Lines of `#GRAPH` without the frame.
    pub graph: &'a str,
    /// Lines of `#INDEX` without the frame.
    pub index: &'a str,
    /// Lines of `#DOCS` without the frame.
    pub docs: &'a str,
    /// Stored body: everything after the `#BODY` line, to the end of the file.
    pub body: &'a [u8],
    /// True when `body=` in the header matches the file length exactly.
    pub body_length_matches: bool,
}

const DERIVED: [&str; 5] = ["MAP", "DESC", "GRAPH", "INDEX", "DOCS"];

impl<'a> Sections<'a> {
    /// Parses a whole file with the default limits and a strict body length.
    ///
    /// # Errors
    /// The first violated rule of FR-01, FR-02 or FR-03, with its FR-20 code.
    pub fn parse(file: &'a [u8]) -> Result<Self> {
        Self::parse_with(file, &Limits::default(), BodyLength::Strict)
    }

    /// Parses a whole file with explicit limits and body-length policy.
    ///
    /// Checks, in order: header (see [`Header::parse`]); CR anywhere (E003);
    /// NUL and other C0 controls anywhere except TAB and LF (E004); section
    /// spans contiguous, inside the file and within limits (E005); frame lines
    /// (E005); derived sections valid UTF-8 without bidi or invisible
    /// characters (E004) and without over-long lines (E005); `#BODY` line and
    /// body start (E005); body valid UTF-8 without bidi controls (E004).
    ///
    /// # Errors
    /// The first violated rule, with its FR-20 code.
    pub fn parse_with(file: &'a [u8], limits: &Limits, body_len: BodyLength) -> Result<Self> {
        let header = Header::parse(file)?;
        scan_bytes(file)?;

        let file_len =
            u64::try_from(file.len()).map_err(|_| e005("file is too large to address"))?;
        let spans = [
            header.map,
            header.desc,
            header.graph,
            header.index,
            header.docs,
        ];
        let mut cursor = u64::try_from(HEADER_LEN).unwrap_or(u64::MAX);
        let mut texts: [&str; 5] = [""; 5];
        for ((name, span), slot) in DERIVED.iter().zip(spans).zip(texts.iter_mut()) {
            let text = section_text(file, file_len, name, span, cursor, limits)?;
            check_derived_text(name, text, limits)?;
            *slot = text;
            cursor = span.end().ok_or_else(|| e005("span end overflows"))?;
        }

        // `#BODY\n` directly after #DOCS, then the body content.
        let marker = b"#BODY\n";
        let body_start = cursor
            .checked_add(u64::try_from(marker.len()).unwrap_or(u64::MAX))
            .ok_or_else(|| e005("body start overflows"))?;
        if header.body.start != body_start {
            return Err(e005(format!(
                "header says body starts at {}, but #DOCS ends at {cursor} so it must start at {body_start}",
                header.body.start
            )));
        }
        let marker_at = slice(
            file,
            cursor,
            u64::try_from(marker.len()).unwrap_or(u64::MAX),
        )
        .ok_or_else(|| e005("file ends before the #BODY line"))?;
        if marker_at != marker {
            return Err(e005("expected the `#BODY` line after #DOCS"));
        }
        let body = slice(file, body_start, file_len - body_start).unwrap_or(&[]);
        let declared_end = header
            .body
            .end()
            .ok_or_else(|| e005("body span overflows"))?;
        let body_length_matches = declared_end == file_len;
        if body_len == BodyLength::Strict && !body_length_matches {
            return Err(e005(format!(
                "header says the body ends at {declared_end}, but the file is {file_len} bytes"
            )));
        }
        let body_text = std::str::from_utf8(body).map_err(|e| {
            e004(format!(
                "body is not UTF-8 at body offset {}",
                e.valid_up_to()
            ))
        })?;
        if let Some((pos, c)) = body_text.char_indices().find(|&(_, c)| is_bidi_control(c)) {
            return Err(e004(format!(
                "bidirectional control U+{:04X} at body offset {pos}",
                u32::from(c)
            )));
        }

        let [map, desc, graph, index, docs] = texts;
        Ok(Self {
            header,
            map,
            desc,
            graph,
            index,
            docs,
            body,
            body_length_matches,
        })
    }
}

/// A file that passed the layout and character checks once (D139): later
/// calls slice its sections without scanning the bytes again. The MCP server
/// keeps one per unchanged file, so a search does not re-check 100 MB.
#[derive(Debug)]
pub struct CheckedFile {
    text: String,
    header: Header,
    body_length_matches: bool,
    /// Whether the body length and fingerprint match (set by the first open).
    pub(crate) body_ok: std::sync::OnceLock<bool>,
}

impl CheckedFile {
    /// Runs every check of [`Sections::parse_with`] with the default limits,
    /// accepting a body that grew or shrank after the build, and keeps the file.
    ///
    /// # Errors
    /// The first violated rule, as from the parser.
    pub fn new(bytes: Vec<u8>) -> Result<Self> {
        let s = Sections::parse_with(&bytes, &Limits::default(), BodyLength::Tolerate)?;
        let (header, body_length_matches) = (s.header.clone(), s.body_length_matches);
        let text = String::from_utf8(bytes).map_err(|e| {
            e004(format!(
                "file is not UTF-8 at byte {}",
                e.utf8_error().valid_up_to()
            ))
        })?;
        Ok(Self {
            text,
            header,
            body_length_matches,
            body_ok: std::sync::OnceLock::new(),
        })
    }

    /// The whole file.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.text.as_bytes()
    }

    /// The sections at the positions the checks accepted.
    #[must_use]
    pub fn sections(&self) -> Sections<'_> {
        let at = |n: u64| usize::try_from(n).unwrap_or(usize::MAX);
        // inner text: after the `#NAME` line, before the `#END` line
        let inner = |name: &str, span: Span| {
            let start = at(span.start).saturating_add(name.len() + 2);
            let end = at(span.end().unwrap_or(0)).saturating_sub(5);
            self.text.get(start..end).unwrap_or("")
        };
        let h = &self.header;
        Sections {
            map: inner("MAP", h.map),
            desc: inner("DESC", h.desc),
            graph: inner("GRAPH", h.graph),
            index: inner("INDEX", h.index),
            docs: inner("DOCS", h.docs),
            body: self.bytes().get(at(h.body.start)..).unwrap_or(&[]),
            body_length_matches: self.body_length_matches,
            header: h.clone(),
        }
    }
}

fn e004(detail: impl Into<String>) -> Error {
    Error::new(ErrorCode::E004, detail)
}

fn e005(detail: impl Into<String>) -> Error {
    Error::new(ErrorCode::E005, detail)
}

/// Bounds-checked sub-slice by `u64` start and length.
fn slice(file: &[u8], start: u64, len: u64) -> Option<&[u8]> {
    let start = usize::try_from(start).ok()?;
    let len = usize::try_from(len).ok()?;
    file.get(start..start.checked_add(len)?)
}

/// FR-02 rules 1-3 over the whole file: no CR (E003), no NUL and no C0
/// control other than TAB and LF (E004). Byte-level, so it needs no decoding.
fn scan_bytes(file: &[u8]) -> Result<()> {
    if let Some(pos) = file.iter().position(|&b| b == b'\r') {
        return Err(Error::new(ErrorCode::E003, format!("CR at byte {pos}")));
    }
    if let Some((pos, b)) = file
        .iter()
        .enumerate()
        .find(|&(_, &b)| b < 0x20 && b != b'\t' && b != b'\n' || b == 0x7f)
    {
        return Err(e004(format!("control byte 0x{b:02X} at byte {pos}")));
    }
    Ok(())
}

/// Checks one derived section's span and frame; returns its inner text.
fn section_text<'a>(
    file: &'a [u8],
    file_len: u64,
    name: &str,
    span: Span,
    expected_start: u64,
    limits: &Limits,
) -> Result<&'a str> {
    if span.start != expected_start {
        return Err(e005(format!(
            "#{name} starts at {} but the previous section ends at {expected_start}",
            span.start
        )));
    }
    if span.len > limits.max_section {
        return Err(e005(format!(
            "#{name} is {} bytes, above the {} byte limit",
            span.len, limits.max_section
        )));
    }
    let end = span
        .end()
        .ok_or_else(|| e005(format!("#{name} span overflows")))?;
    if end > file_len {
        return Err(e005(format!(
            "#{name} ends at {end}, past the end of the file ({file_len})"
        )));
    }
    let bytes = slice(file, span.start, span.len)
        .ok_or_else(|| e005(format!("#{name} is not addressable")))?;
    let open = format!("#{name}\n");
    let inner = bytes
        .strip_prefix(open.as_bytes())
        .and_then(|rest| rest.strip_suffix(b"#END\n"))
        .ok_or_else(|| {
            e005(format!(
                "#{name} is not framed by `#{name}` and `#END` lines"
            ))
        })?;
    std::str::from_utf8(inner).map_err(|e| {
        e004(format!(
            "#{name} is not UTF-8 at section offset {}",
            e.valid_up_to()
        ))
    })
}

/// FR-02 rules 4-5 and the line limit on a derived section.
fn check_derived_text(name: &str, text: &str, limits: &Limits) -> Result<()> {
    if let Some((pos, c)) = text
        .char_indices()
        .find(|&(_, c)| is_bidi_control(c) || is_invisible(c))
    {
        return Err(e004(format!(
            "forbidden character U+{:04X} at offset {pos} in #{name}",
            u32::from(c)
        )));
    }
    if let Some((n, _)) = text
        .split('\n')
        .enumerate()
        .find(|(_, line)| line.len() > limits.max_line)
    {
        return Err(e005(format!(
            "line {} of #{name} is longer than {} bytes",
            n + 1,
            limits.max_line
        )));
    }
    Ok(())
}

/// FR-02 rule 4: U+202A..U+202E and U+2066..U+2069.
fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// FR-02 rule 5: U+200B..U+200D, U+2060, U+FEFF.
fn is_invisible(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200D}' | '\u{2060}' | '\u{FEFF}')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FORMAT_VERSION;

    const B3: &str = "b3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262";

    /// Assembles a structurally valid file from section inner texts and a body.
    /// Hashes are not checked here (that is step 3), so they are placeholders.
    fn build(inner: [&str; 5], body: &[u8], patch: impl Fn(&mut Vec<String>)) -> Vec<u8> {
        let sections: Vec<Vec<u8>> = DERIVED
            .iter()
            .zip(inner)
            .map(|(n, t)| format!("#{n}\n{t}#END\n").into_bytes())
            .collect();
        let mut pos = HEADER_LEN;
        let mut lines = vec![
            "eol=LF".to_owned(),
            "enc=UTF-8".to_owned(),
            "tok=o200k_base".to_owned(),
            "emb=none".to_owned(),
            format!("embhash={B3}"),
        ];
        for (n, s) in DERIVED.iter().zip(&sections) {
            lines.push(format!("{}={pos}+{}", n.to_lowercase(), s.len()));
            pos += s.len();
        }
        lines.push(format!("body={}+{}", pos + 6, body.len()));
        lines.extend(["rooms=0", "doors=0", "groups=0"].map(str::to_owned));
        lines.push(format!("bodyhash={B3}"));
        lines.push("guide=std-1".to_owned());
        patch(&mut lines);
        let mut file = format!("%KAPU {FORMAT_VERSION}\n{}\n", lines.join("\n")).into_bytes();
        file.resize(HEADER_LEN - 1, b' ');
        file.push(b'\n');
        for s in sections {
            file.extend(s);
        }
        file.extend(b"#BODY\n");
        file.extend(body);
        file
    }

    fn minimal() -> Vec<u8> {
        build(
            ["G g1 | x | 0 | 0t\n", "", "", "", ""],
            b"@@@ a\nhello\n",
            |_| {},
        )
    }

    fn code(file: &[u8]) -> ErrorCode {
        Sections::parse(file).unwrap_err().code
    }

    #[test]
    fn minimal_file_parses_and_slices_are_right() {
        let file = minimal();
        let s = Sections::parse(&file).unwrap();
        assert_eq!(s.map, "G g1 | x | 0 | 0t\n");
        assert_eq!(s.desc, "");
        assert_eq!(s.body, b"@@@ a\nhello\n");
        assert!(s.body_length_matches);
    }

    #[test]
    fn cr_anywhere_is_e003_and_controls_are_e004() {
        let mut f = minimal();
        let n = f.len();
        f[n - 1] = b'\r';
        assert_eq!(code(&f), ErrorCode::E003);
        let mut f = minimal();
        let n = f.len();
        f[n - 2] = 0;
        assert_eq!(code(&f), ErrorCode::E004);
        f[n - 2] = 0x1b;
        assert_eq!(code(&f), ErrorCode::E004);
        f[n - 2] = b'\t';
        assert!(Sections::parse(&f).is_ok(), "TAB is allowed");
    }

    #[test]
    fn bidi_and_invisible_characters_are_e004() {
        let f = build(["", "a\u{202E}b\n", "", "", ""], b"", |_| {});
        assert_eq!(code(&f), ErrorCode::E004);
        let f = build(["", "a\u{200B}b\n", "", "", ""], b"", |_| {});
        assert_eq!(code(&f), ErrorCode::E004);
        // invisible characters are allowed in the body (the builder only warns), bidi is not
        assert!(
            Sections::parse(&build(
                ["", "", "", "", ""],
                "x\u{200B}y".as_bytes(),
                |_| {}
            ))
            .is_ok()
        );
        assert_eq!(
            code(&build(
                ["", "", "", "", ""],
                "x\u{2066}y".as_bytes(),
                |_| {}
            )),
            ErrorCode::E004
        );
    }

    #[test]
    fn gaps_overlaps_and_overruns_are_e005() {
        let shift = |field: &'static str, delta: i64| {
            move |lines: &mut Vec<String>| {
                for l in lines.iter_mut() {
                    if let Some(rest) = l.strip_prefix(&format!("{field}=")) {
                        let (s, n) = rest.split_once('+').unwrap();
                        let s: i64 = s.parse().unwrap();
                        *l = format!("{field}={}+{n}", s + delta);
                    }
                }
            }
        };
        assert_eq!(
            code(&build(["", "", "", "", ""], b"", shift("desc", 1))),
            ErrorCode::E005
        );
        assert_eq!(
            code(&build(["", "", "", "", ""], b"", shift("desc", -1))),
            ErrorCode::E005
        );
        let grow = |lines: &mut Vec<String>| {
            for l in lines.iter_mut() {
                if l.starts_with("docs=") {
                    l.push('0');
                }
            }
        };
        assert_eq!(
            code(&build(["", "", "", "", ""], b"", grow)),
            ErrorCode::E005
        );
    }

    #[test]
    fn broken_frame_is_e005() {
        let mut f = minimal();
        let pos = f.windows(5).position(|w| w == b"#DESC").unwrap();
        f[pos + 1] = b'X';
        assert_eq!(code(&f), ErrorCode::E005);
    }

    #[test]
    fn body_length_policy() {
        let mut f = minimal();
        f.extend(b"more\n");
        assert_eq!(code(&f), ErrorCode::E005);
        let s = Sections::parse_with(&f, &Limits::default(), BodyLength::Tolerate).unwrap();
        assert!(!s.body_length_matches);
        assert!(s.body.ends_with(b"more\n"));
    }

    #[test]
    fn checked_file_hands_out_the_sections_of_a_full_parse() {
        let file = minimal();
        let parsed = Sections::parse_with(&file, &Limits::default(), BodyLength::Tolerate).unwrap();
        let checked = CheckedFile::new(file.clone()).unwrap();
        assert_eq!(checked.sections(), parsed);
        assert_eq!(checked.bytes(), file.as_slice());
        // a body that grew after the build stays readable, as with the parser
        let mut grown = minimal();
        grown.extend(b"more\n");
        let checked = CheckedFile::new(grown).unwrap();
        assert!(!checked.sections().body_length_matches);
        assert!(checked.sections().body.ends_with(b"more\n"));
    }

    #[test]
    fn checked_file_refuses_what_the_parser_refuses() {
        let mut f = minimal();
        let n = f.len();
        f[n - 2] = 0x1b;
        assert_eq!(CheckedFile::new(f).unwrap_err().code, ErrorCode::E004);
        let f = build(["", "a\u{200B}b\n", "", "", ""], b"", |_| {});
        assert_eq!(CheckedFile::new(f).unwrap_err().code, ErrorCode::E004);
    }

    #[test]
    fn limits_are_enforced() {
        let long_line = "x".repeat(40) + "\n";
        let f = build(["", &long_line, "", "", ""], b"", |_| {});
        let small = Limits {
            max_section: 1 << 30,
            max_line: 32,
        };
        assert_eq!(
            Sections::parse_with(&f, &small, BodyLength::Strict)
                .unwrap_err()
                .code,
            ErrorCode::E005
        );
        let tiny = Limits {
            max_section: 8,
            max_line: 1 << 20,
        };
        assert_eq!(
            Sections::parse_with(&f, &tiny, BodyLength::Strict)
                .unwrap_err()
                .code,
            ErrorCode::E005
        );
        assert!(Sections::parse(&f).is_ok());
    }
}
