//! Index terms (FR-09, D61) and prices (FR-08).
//!
//! Mirrors `index_tokens`, `split_subtokens`, `stems` and `tokens` of
//! `tools/build_vectors.py`. The reference uses Python regular expressions;
//! here the same character classes are written out by hand:
//!
//! * identifier: a run of word characters (`\w`: letters, digits, `_`), with
//!   single `-` or `_` joining two such runs, trimmed of `-` and `_`;
//! * sub-token split on every character outside
//!   `[0-9A-Za-zÀ-ɏͰ-ϿЀ-ӿ]`;
//! * camelCase split between `[a-zß-ÿ]` and `[A-ZÀ-Þ]`,
//!   and letter/digit splits between `[A-Za-zÀ-ɏ]` and `[0-9]`.

use std::sync::OnceLock;

use rust_stemmers::{Algorithm, Stemmer};
use tiktoken_rs::CoreBPE;

use crate::error::{Error, ErrorCode, Result};
use crate::text::nfc;

/// Python `\w` for one character: alphanumeric or `_`.
fn is_word(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// Identifiers of a text, in order (D61).
#[must_use]
pub fn identifiers(text: &str) -> Vec<&str> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(&(start, c)) = chars.get(i) {
        if !is_word(c) {
            i += 1;
            continue;
        }
        let mut j = i;
        while chars.get(j).is_some_and(|&(_, c)| is_word(c)) {
            j += 1;
        }
        // `[-_]` joins two word runs; `_` is already a word character
        while chars.get(j).is_some_and(|&(_, c)| c == '-')
            && chars.get(j + 1).is_some_and(|&(_, c)| is_word(c))
        {
            j += 1;
            while chars.get(j).is_some_and(|&(_, c)| is_word(c)) {
                j += 1;
            }
        }
        let end = chars.get(j).map_or(text.len(), |&(p, _)| p);
        let ident = text
            .get(start..end)
            .unwrap_or("")
            .trim_matches(|c| c == '-' || c == '_');
        if !ident.is_empty() {
            out.push(ident);
        }
        i = j;
    }
    out
}

fn in_subtoken_class(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || ('\u{00C0}'..='\u{024F}').contains(&c)
        || ('\u{0370}'..='\u{03FF}').contains(&c)
        || ('\u{0400}'..='\u{04FF}').contains(&c)
}

fn camel_lower(c: char) -> bool {
    c.is_ascii_lowercase() || ('\u{00DF}'..='\u{00FF}').contains(&c)
}

fn camel_upper(c: char) -> bool {
    c.is_ascii_uppercase() || ('\u{00C0}'..='\u{00DE}').contains(&c)
}

fn letter(c: char) -> bool {
    c.is_ascii_alphabetic() || ('\u{00C0}'..='\u{024F}').contains(&c)
}

/// Inserts a space between every adjacent pair `(a, b)` with `pred(a, b)`.
fn split_pairs(s: &str, pred: impl Fn(char, char) -> bool) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let mut last: Option<char> = None;
    for c in s.chars() {
        if last.is_some_and(|p| pred(p, c)) {
            out.push(' ');
        }
        out.push(c);
        last = Some(c);
    }
    out
}

/// Sub-tokens of one identifier (not yet lower-cased).
#[must_use]
pub fn split_subtokens(ident: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in ident
        .split(|c| !in_subtoken_class(c))
        .filter(|p| !p.is_empty())
    {
        let p = split_pairs(part, |a, b| camel_lower(a) && camel_upper(b));
        let p = split_pairs(&p, |a, b| letter(a) && b.is_ascii_digit());
        let p = split_pairs(&p, |a, b| a.is_ascii_digit() && letter(b));
        out.extend(p.split_whitespace().map(str::to_owned));
    }
    out
}

/// D61: lower-cased NFC sub-tokens, plus the whole identifier when it differs.
#[must_use]
pub fn index_tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for ident in identifiers(text) {
        let subs: Vec<String> = split_subtokens(ident)
            .iter()
            .map(|s| nfc(&s.to_lowercase()))
            .collect();
        let whole = nfc(&ident.to_lowercase());
        let differs = subs.len() != 1 || subs.first() != Some(&whole);
        out.extend(subs);
        if differs {
            out.push(whole);
        }
    }
    out
}

fn stemmers() -> &'static (Stemmer, Stemmer) {
    static STEMMERS: OnceLock<(Stemmer, Stemmer)> = OnceLock::new();
    STEMMERS.get_or_init(|| {
        (
            Stemmer::create(Algorithm::Hungarian),
            Stemmer::create(Algorithm::English),
        )
    })
}

/// D61: the term, its Hungarian and its English Snowball stem, deduplicated,
/// in that order (D80: the Snowball generation of `rust-stemmers` 1.2).
#[must_use]
pub fn stems(term: &str) -> Vec<String> {
    stems_of(term, true, true)
}

/// [`stems`] with only the stemmers asked for: the term itself always, then
/// the Hungarian and the English stem if `hu` and `en` (the M0 stemming axis).
#[must_use]
pub fn stems_of(term: &str, hu: bool, en: bool) -> Vec<String> {
    let (hu_stemmer, en_stemmer) = stemmers();
    let mut out: Vec<String> = Vec::with_capacity(3);
    let mut add = |t: String| {
        if !t.is_empty() && !out.contains(&t) {
            out.push(t);
        }
    };
    add(term.to_owned());
    if hu {
        add(hu_stemmer.stem(term).into_owned());
    }
    if en {
        add(en_stemmer.stem(term).into_owned());
    }
    out
}

fn bpe() -> Option<&'static CoreBPE> {
    static BPE: OnceLock<Option<CoreBPE>> = OnceLock::new();
    BPE.get_or_init(|| tiktoken_rs::o200k_base().ok()).as_ref()
}

/// FR-08: number of `o200k_base` tokens; special-token text counts as
/// ordinary text.
///
/// # Errors
/// E012 if the embedded vocabulary cannot be loaded (never expected).
pub fn tokens(text: &str) -> Result<u64> {
    let bpe =
        bpe().ok_or_else(|| Error::new(ErrorCode::E012, "o200k_base tokenizer unavailable"))?;
    Ok(u64::try_from(bpe.encode_ordinary(text).len()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_and_subtokens() {
        assert_eq!(
            identifiers("a btn-glass, --turkiz _x_ backdropFilter12px"),
            ["a", "btn-glass", "turkiz", "x", "backdropFilter12px"]
        );
        assert_eq!(
            split_subtokens("backdropFilter12px"),
            ["backdrop", "Filter", "12", "px"]
        );
        assert_eq!(split_subtokens("btn-glass"), ["btn", "glass"]);
    }

    #[test]
    fn index_tokens_add_the_whole_identifier_when_split() {
        assert_eq!(
            index_tokens("btn-glass Gomb"),
            ["btn", "glass", "btn-glass", "gomb"]
        );
        assert_eq!(index_tokens("3fb6a8"), ["3", "fb", "6", "a", "8", "3fb6a8"]);
    }

    #[test]
    fn stems_are_deduplicated() {
        assert_eq!(stems("gombok"), ["gombok", "gomb"]);
        assert_eq!(stems("buttons"), ["buttons", "button"]);
        assert_eq!(stems("a"), ["a"]);
    }

    #[test]
    fn o200k_price() {
        assert_eq!(tokens("Sötét téma, glass gomb").unwrap(), 9);
        assert_eq!(tokens("").unwrap(), 0);
    }
}
