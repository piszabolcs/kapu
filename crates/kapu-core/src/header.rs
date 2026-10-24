//! The 512-byte header (FR-03): parsing and structural checks.
//!
//! Layout: `%KAPU 1.1\n` (or `1.0`, D140), then `key=value\n` lines, then space padding, and an
//! LF at byte 511. The parser only reads those 512 bytes; it never trusts a
//! position before it has been checked (NFR-01), and it allocates nothing whose
//! size comes from the file.

use std::collections::BTreeMap;

use crate::error::{Error, ErrorCode, Result};
use crate::{FORMAT_VERSION, HEADER_LEN, READ_VERSIONS};

/// An absolute byte range in the file, written as `start+len` (FR-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// First byte, counted from the start of the file.
    pub start: u64,
    /// Number of bytes.
    pub len: u64,
}

impl Span {
    /// One past the last byte. `None` if `start + len` overflows `u64`.
    #[must_use]
    pub fn end(self) -> Option<u64> {
        self.start.checked_add(self.len)
    }
}

/// A 256-bit BLAKE3 fingerprint, written as `b3:<64 lowercase hex>` (D16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hash(pub [u8; 32]);

/// The parsed header. Unknown fields are kept in `extra` and otherwise ignored
/// (FR-03 rule: readers ignore unknown fields).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Format version from the first line, one of [`READ_VERSIONS`] (D140).
    pub version: &'static str,
    /// Name of the vector model used by the build (`emb=`).
    pub emb: String,
    /// Fingerprint of that model (`embhash=`).
    pub embhash: Hash,
    /// `#MAP` section span.
    pub map: Span,
    /// `#DESC` section span.
    pub desc: Span,
    /// `#GRAPH` section span.
    pub graph: Span,
    /// `#INDEX` section span.
    pub index: Span,
    /// `#DOCS` section span.
    pub docs: Span,
    /// Body content after the `#BODY` line, up to the end of the file.
    pub body: Span,
    /// Number of rooms.
    pub rooms: u64,
    /// Number of doors (rooms and sub-doors).
    pub doors: u64,
    /// Number of groups.
    pub groups: u64,
    /// BLAKE3 of the whole stored body (D37).
    pub bodyhash: Hash,
    /// Which standard guide the reader serves (`std-1`).
    pub guide: String,
    /// Fields this version does not know, in key order.
    pub extra: BTreeMap<String, String>,
}

/// Fields whose value is fixed by the spec.
const FIXED: [(&str, &str); 3] = [("eol", "LF"), ("enc", "UTF-8"), ("tok", "o200k_base")];

impl Header {
    /// Parses and checks the header at the start of `file`.
    ///
    /// Only the first 512 bytes are read. Checks done here: size and final LF
    /// (E002), CR (E003), UTF-8 and control characters (E004), magic line
    /// (E001), field syntax and required fields (E002), span syntax (E005),
    /// hash prefix (E011). Whether the spans fit the file is checked by the
    /// section reader, which knows the file length.
    ///
    /// # Errors
    /// The first violated rule, with its FR-20 code.
    pub fn parse(file: &[u8]) -> Result<Self> {
        let raw = file.get(..HEADER_LEN).ok_or_else(|| {
            err(
                ErrorCode::E002,
                format!(
                    "file is {} bytes, shorter than the {HEADER_LEN}-byte header",
                    file.len()
                ),
            )
        })?;
        if raw.last() != Some(&b'\n') {
            return Err(err(ErrorCode::E002, "byte 511 of the header is not LF"));
        }
        if let Some(pos) = raw.iter().position(|&b| b == b'\r') {
            return Err(err(
                ErrorCode::E003,
                format!("CR at byte {pos} in the header"),
            ));
        }
        let text = std::str::from_utf8(raw).map_err(|e| {
            err(
                ErrorCode::E004,
                format!("header is not UTF-8 at byte {}", e.valid_up_to()),
            )
        })?;
        if let Some((pos, c)) = text
            .char_indices()
            .find(|&(_, c)| c.is_control() && c != '\n')
        {
            return Err(err(
                ErrorCode::E004,
                format!(
                    "control character U+{:04X} at byte {pos} in the header",
                    u32::from(c)
                ),
            ));
        }

        // Layout: lines, each ending in LF, then only spaces up to byte 510, then LF.
        // Cut the final LF, then the padding: what is left must end with the LF
        // of the last line. Any other byte in the padding stops the trim early
        // and lands here as "does not end with LF".
        let without_final_lf = text.strip_suffix('\n').unwrap_or(text);
        let lines = without_final_lf.trim_end_matches(' ');
        let Some(lines) = lines.strip_suffix('\n') else {
            let first = lines.split('\n').next().unwrap_or("");
            version_of(first)?;
            return Err(err(
                ErrorCode::E002,
                "header padding contains something other than spaces",
            ));
        };
        let (magic, fields) = lines.split_once('\n').unwrap_or((lines, ""));
        let version = version_of(magic)?;

        let mut kv: BTreeMap<String, String> = BTreeMap::new();
        for line in fields.split('\n') {
            let (key, value) = line.split_once('=').ok_or_else(|| {
                err(
                    ErrorCode::E002,
                    format!("header line without `=`: {line:?}"),
                )
            })?;
            if key.is_empty() {
                return Err(err(
                    ErrorCode::E002,
                    format!("header line with empty key: {line:?}"),
                ));
            }
            if kv.insert(key.to_owned(), value.to_owned()).is_some() {
                return Err(err(
                    ErrorCode::E002,
                    format!("header field `{key}` appears twice"),
                ));
            }
        }

        for (key, want) in FIXED {
            let got = take(&mut kv, key)?;
            if got != want {
                return Err(err(
                    ErrorCode::E002,
                    format!("`{key}={got}`, expected `{key}={want}`"),
                ));
            }
        }
        let header = Self {
            version,
            emb: take(&mut kv, "emb")?,
            embhash: parse_hash("embhash", &take(&mut kv, "embhash")?, ErrorCode::E002)?,
            map: parse_span("map", &take_span(&mut kv, "map")?)?,
            desc: parse_span("desc", &take_span(&mut kv, "desc")?)?,
            graph: parse_span("graph", &take_span(&mut kv, "graph")?)?,
            index: parse_span("index", &take_span(&mut kv, "index")?)?,
            docs: parse_span("docs", &take_span(&mut kv, "docs")?)?,
            body: parse_span("body", &take_span(&mut kv, "body")?)?,
            rooms: parse_count("rooms", &take(&mut kv, "rooms")?)?,
            doors: parse_count("doors", &take(&mut kv, "doors")?)?,
            groups: parse_count("groups", &take(&mut kv, "groups")?)?,
            bodyhash: parse_hash("bodyhash", &take(&mut kv, "bodyhash")?, ErrorCode::E002)?,
            guide: take(&mut kv, "guide")?,
            extra: kv,
        };
        Ok(header)
    }
}

fn err(code: ErrorCode, detail: impl Into<String>) -> Error {
    Error::new(code, detail)
}

/// The version of a `%KAPU <version>` line, if the reader knows it (E001).
fn version_of(magic: &str) -> Result<&'static str> {
    magic
        .strip_prefix("%KAPU ")
        .and_then(|v| READ_VERSIONS.iter().find(|&&known| known == v))
        .copied()
        .ok_or_else(|| {
            err(
                ErrorCode::E001,
                format!("first line is {magic:?}, expected \"%KAPU {FORMAT_VERSION}\""),
            )
        })
}

/// Removes a required field; a missing one is a malformed header (E002).
fn take(kv: &mut BTreeMap<String, String>, key: &str) -> Result<String> {
    kv.remove(key).ok_or_else(|| {
        err(
            ErrorCode::E002,
            format!("required header field `{key}` is missing"),
        )
    })
}

/// Like [`take`], but a missing section span is a missing section (E005).
fn take_span(kv: &mut BTreeMap<String, String>, key: &str) -> Result<String> {
    kv.remove(key).ok_or_else(|| {
        err(
            ErrorCode::E005,
            format!("section span `{key}=` is missing from the header"),
        )
    })
}

/// Strict decimal: ASCII digits only, no sign, no leading zero (except `0`).
pub(crate) fn parse_decimal(text: &str) -> Option<u64> {
    let canonical = !text.is_empty()
        && text.bytes().all(|b| b.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    if canonical { text.parse().ok() } else { None }
}

fn parse_span(key: &str, value: &str) -> Result<Span> {
    let bad = || {
        err(
            ErrorCode::E005,
            format!("`{key}={value}` is not a `start+len` span"),
        )
    };
    let (start, len) = value.split_once('+').ok_or_else(bad)?;
    let span = Span {
        start: parse_decimal(start).ok_or_else(bad)?,
        len: parse_decimal(len).ok_or_else(bad)?,
    };
    span.end().ok_or_else(bad)?;
    Ok(span)
}

fn parse_count(key: &str, value: &str) -> Result<u64> {
    parse_decimal(value)
        .ok_or_else(|| err(ErrorCode::E002, format!("`{key}={value}` is not a count")))
}

/// Parses `b3:<64 lowercase hex>`. Another prefix is E011, a bad digest E002.
/// `malformed` is the code for a bad digest: E002 in the header, E005 in `#DOCS`.
pub(crate) fn parse_hash(key: &str, value: &str, malformed: ErrorCode) -> Result<Hash> {
    let (prefix, hex) = value.split_once(':').ok_or_else(|| {
        err(
            ErrorCode::E011,
            format!("`{key}={value}` has no hash prefix"),
        )
    })?;
    if prefix != "b3" {
        return Err(err(
            ErrorCode::E011,
            format!("`{key}` uses unknown hash prefix `{prefix}:`"),
        ));
    }
    let bad = || err(malformed, format!("`{key}` is not 64 lowercase hex digits"));
    if hex.len() != 64 {
        return Err(bad());
    }
    let mut out = [0u8; 32];
    let (pairs, _) = hex.as_bytes().as_chunks::<2>();
    for (byte, [hi, lo]) in out.iter_mut().zip(pairs) {
        let hi = hex_digit(*hi).ok_or_else(bad)?;
        let lo = hex_digit(*lo).ok_or_else(bad)?;
        *byte = (hi << 4) | lo;
    }
    Ok(Hash(out))
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a 512-byte header from field lines, padded like the builder does.
    fn header_with(lines: &[&str]) -> Vec<u8> {
        let mut text = format!("%KAPU {FORMAT_VERSION}\n");
        for l in lines {
            text.push_str(l);
            text.push('\n');
        }
        let mut bytes = text.into_bytes();
        bytes.resize(HEADER_LEN - 1, b' ');
        bytes.push(b'\n');
        bytes
    }

    const B3: &str = "b3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262";

    fn valid_lines() -> Vec<String> {
        [
            "eol=LF",
            "enc=UTF-8",
            "tok=o200k_base",
            "emb=none",
            &format!("embhash={B3}"),
            "map=512+10",
            "desc=522+10",
            "graph=532+10",
            "index=542+10",
            "docs=552+10",
            "body=568+0",
            "rooms=0",
            "doors=0",
            "groups=0",
            &format!("bodyhash={B3}"),
            "guide=std-1",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
    }

    fn parse_lines(lines: &[String]) -> Result<Header> {
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        Header::parse(&header_with(&refs))
    }

    fn code_of(lines: &[String]) -> ErrorCode {
        parse_lines(lines).unwrap_err().code
    }

    fn replace(field: &str, new: &str) -> Vec<String> {
        valid_lines()
            .into_iter()
            .map(|l| {
                if l.starts_with(&format!("{field}=")) {
                    new.to_owned()
                } else {
                    l
                }
            })
            .collect()
    }

    #[test]
    fn valid_header_parses() {
        let h = parse_lines(&valid_lines()).unwrap();
        assert_eq!(
            h.map,
            Span {
                start: 512,
                len: 10
            }
        );
        assert_eq!(h.emb, "none");
        assert_eq!(h.embhash.0[0], 0xaf);
        assert!(h.extra.is_empty());
    }

    #[test]
    fn unknown_field_is_kept_and_ignored() {
        let mut lines = valid_lines();
        lines.push("future=42".to_owned());
        assert_eq!(
            parse_lines(&lines)
                .unwrap()
                .extra
                .get("future")
                .map(String::as_str),
            Some("42")
        );
    }

    #[test]
    fn short_file_is_e002() {
        assert_eq!(
            Header::parse(b"%KAPU 1.0\n").unwrap_err().code,
            ErrorCode::E002
        );
    }

    #[test]
    fn wrong_version_is_e001() {
        let mut bytes = header_with(&[]);
        bytes[8] = b'2';
        assert_eq!(Header::parse(&bytes).unwrap_err().code, ErrorCode::E001);
    }

    #[test]
    fn version_1_0_is_still_read() {
        // D140: the builder writes 1.1; a 1.0 file stays readable until rebuilt
        let refs: Vec<String> = valid_lines();
        let refs: Vec<&str> = refs.iter().map(String::as_str).collect();
        let mut bytes = header_with(&refs);
        assert_eq!(Header::parse(&bytes).unwrap().version, "1.1");
        bytes[8] = b'0';
        assert_eq!(Header::parse(&bytes).unwrap().version, "1.0");
        bytes[8] = b'2';
        assert_eq!(Header::parse(&bytes).unwrap_err().code, ErrorCode::E001);
    }

    #[test]
    fn duplicate_or_missing_field_is_e002() {
        let mut dup = valid_lines();
        dup.push("rooms=1".to_owned());
        assert_eq!(code_of(&dup), ErrorCode::E002);
        let missing: Vec<String> = valid_lines()
            .into_iter()
            .filter(|l| !l.starts_with("guide="))
            .collect();
        assert_eq!(code_of(&missing), ErrorCode::E002);
    }

    #[test]
    fn missing_or_bad_span_is_e005() {
        let missing: Vec<String> = valid_lines()
            .into_iter()
            .filter(|l| !l.starts_with("docs="))
            .collect();
        assert_eq!(code_of(&missing), ErrorCode::E005);
        for bad in [
            "map=512",
            "map=+10",
            "map=0512+10",
            "map=-1+3",
            "map=18446744073709551615+1",
        ] {
            assert_eq!(code_of(&replace("map", bad)), ErrorCode::E005, "{bad}");
        }
    }

    #[test]
    fn hash_rules() {
        assert_eq!(
            code_of(&replace("bodyhash", "bodyhash=sha256:00")),
            ErrorCode::E011
        );
        assert_eq!(
            code_of(&replace("bodyhash", "bodyhash=abc")),
            ErrorCode::E011
        );
        let upper = format!("bodyhash=b3:{}", "A".repeat(64));
        assert_eq!(code_of(&replace("bodyhash", &upper)), ErrorCode::E002);
    }

    #[test]
    fn control_character_is_e004_and_cr_is_e003() {
        assert_eq!(code_of(&replace("emb", "emb=a\u{1b}b")), ErrorCode::E004);
        assert_eq!(code_of(&replace("emb", "emb=a\rb")), ErrorCode::E003);
    }

    #[test]
    fn non_space_padding_is_e002() {
        let mut bytes = header_with(&valid_lines().iter().map(String::as_str).collect::<Vec<_>>());
        bytes[HEADER_LEN - 3] = b'x';
        assert_eq!(Header::parse(&bytes).unwrap_err().code, ErrorCode::E002);
    }
}
