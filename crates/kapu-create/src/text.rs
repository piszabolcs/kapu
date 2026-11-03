//! Text helpers shared by the readers and the room planner: the L2
//! normalisation, decoding, token counts, slugs and section numbers.

use unicode_normalization::UnicodeNormalization;

/// L2 normalisation of every text a reader produces: LF line ends, NFC, no
/// characters the KAPU body forbids or warns about (FR-02/1-5), ligatures
/// spelled out and soft hyphens dropped, so the index sees the plain words.
#[must_use]
pub fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() != Some(&'\n') {
                    out.push('\n');
                }
            }
            '\n' | '\t' => out.push(c),
            '\u{00A0}' | '\u{2007}' | '\u{202F}' => out.push(' '),
            '\u{FB00}' => out.push_str("ff"),
            '\u{FB01}' => out.push_str("fi"),
            '\u{FB02}' => out.push_str("fl"),
            '\u{FB03}' => out.push_str("ffi"),
            '\u{FB04}' => out.push_str("ffl"),
            '\u{FB05}' | '\u{FB06}' => out.push_str("st"),
            c if forbidden(c) => {}
            c => out.push(c),
        }
    }
    if out.is_ascii() {
        out
    } else {
        out.nfc().collect()
    }
}

/// Characters that never reach a room: C0 and C1 controls (but TAB and LF),
/// bidirectional controls, invisible characters and the soft hyphen.
fn forbidden(c: char) -> bool {
    matches!(c,
        '\u{0}'..='\u{8}' | '\u{B}'..='\u{1F}' | '\u{7F}'..='\u{9F}'
        | '\u{AD}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}'
        | '\u{FFFE}' | '\u{FFFF}')
}

/// Bytes as text: UTF-8 (with or without BOM), UTF-16 with a BOM, otherwise
/// the encoding guessed by `chardetng` (old Windows and ISO code pages).
#[must_use]
pub fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return encoding_rs::UTF_16LE.decode(bytes).0.into_owned();
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return encoding_rs::UTF_16BE.decode(bytes).0.into_owned();
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_owned();
    }
    let mut detector = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Allow);
    detector.feed(bytes, true);
    let encoding = detector.guess(None, chardetng::Utf8Detection::Allow);
    encoding.decode(bytes).0.into_owned()
}

/// Whether `bytes` look like text: no NUL in the first 8 KiB and mostly
/// printable.
#[must_use]
pub fn looks_like_text(bytes: &[u8]) -> bool {
    let head = bytes.get(..bytes.len().min(8192)).unwrap_or(bytes);
    if head.starts_with(&[0xFF, 0xFE]) || head.starts_with(&[0xFE, 0xFF]) {
        return true;
    }
    if head.contains(&0) {
        return false;
    }
    let bad = head
        .iter()
        .filter(|&&b| b < 0x20 && !matches!(b, b'\n' | b'\r' | b'\t' | 0x0C))
        .count();
    bad * 100 <= head.len()
}

/// The `o200k_base` token count the KAPU map shows as a price (FR-08), so
/// the planner's sizes are the prices the agent sees.
#[must_use]
pub fn tokens(text: &str) -> usize {
    kapu_core::terms::tokens(text)
        .ok()
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(text.len() / 4)
}

/// FR-04/4 slug of `text`, at most `max` characters, cut at a hyphen when
/// it can be.
#[must_use]
pub fn slug(text: &str, max: usize) -> String {
    let full = kapu_core::text::slug(text);
    if full.len() <= max {
        return full;
    }
    let cut = full.get(..max).unwrap_or(&full);
    let cut = match cut.rfind('-') {
        Some(i) if i >= max / 2 => cut.get(..i).unwrap_or(cut),
        _ => cut,
    };
    cut.trim_matches('-').to_owned()
}

/// The section number at the start of a heading (`3.2.1 Encoder`, `A.4`,
/// `12)`), as an id part (`3-2-1`, `a-4`, `12`), or `None`.
#[must_use]
pub fn numbering(title: &str) -> Option<String> {
    let first = title.split_whitespace().next()?;
    let first = first.trim_end_matches(['.', ')', ':']);
    if first.is_empty() || first.len() > 16 {
        return None;
    }
    let parts: Vec<&str> = first.split('.').collect();
    let mut digits = false;
    for (i, part) in parts.iter().enumerate() {
        let numeric =
            !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()) && part.len() <= 3;
        let letter = i == 0 && part.len() == 1 && part.chars().all(|c| c.is_ascii_uppercase());
        if !numeric && !letter {
            return None;
        }
        digits |= numeric;
    }
    // A lone letter or a lone year-like number is not a section number.
    if !digits || (parts.len() == 1 && (first.len() > 2 || title.split_whitespace().count() < 2)) {
        return None;
    }
    Some(parts.join("-").to_lowercase())
}

/// Whitespace collapsed to single spaces, for comparing texts.
#[must_use]
pub fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// At most `max` characters of `text`, cut at a word boundary, with `…`
/// when cut.
#[must_use]
pub fn shorten(text: &str, max: usize) -> String {
    let text = squash(text);
    if text.chars().count() <= max {
        return text;
    }
    let mut out = String::new();
    for word in text.split(' ') {
        if out.chars().count() + word.chars().count() + 1 > max {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    if out.is_empty() {
        out = text.chars().take(max).collect();
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_drops_what_the_body_forbids() {
        assert_eq!(
            clean("a\r\nb\rc\u{1B}d\u{202E}e\u{AD}f\u{FB01}g"),
            "a\nb\ncdefg".replace("fg", "ffig")
        );
        assert_eq!(clean("e\u{301}"), "\u{e9}");
    }

    #[test]
    fn numbering_takes_section_numbers_only() {
        assert_eq!(
            numbering("3.2.1 Encoder settings").as_deref(),
            Some("3-2-1")
        );
        assert_eq!(numbering("A.4 Appendix").as_deref(), Some("a-4"));
        assert_eq!(numbering("12 Parameters").as_deref(), Some("12"));
        assert_eq!(numbering("2026 was a year"), None);
        assert_eq!(numbering("Overview"), None);
        assert_eq!(numbering("A Guide"), None);
    }

    #[test]
    fn slug_is_cut_at_a_hyphen() {
        assert_eq!(
            slug("IndraDrive Drive Controllers, Control Sections", 24),
            "indradrive-drive"
        );
        assert_eq!(slug("short", 24), "short");
    }

    #[test]
    fn decode_guesses_old_code_pages() {
        let latin2 = [0x41, 0xE1, 0x72, 0x76, 0xED, 0x7A, 0x74, 0xFB, 0x72, 0xF5];
        let text = decode(&latin2);
        assert!(text.starts_with('A'));
        assert_eq!(text.chars().count(), 10);
    }
}
