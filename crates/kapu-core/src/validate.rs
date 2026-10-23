//! The validator (FR-20): header, derived sections and their cross-references,
//! and with `--full` the body fingerprints too.
//!
//! Order of the checks, and why: structure first (header, sections, `#DOCS`
//! lines), then identifiers (E008), then every reference against the set of
//! known doors (E009), then caption limits (E010), then ordering rules of the
//! sections (E005), and only in full mode the fingerprints (E006, E007) and the
//! agreement of `#DOCS` with the `@@@` lines of the body. Each test vector
//! breaks exactly one rule, and this order makes sure that rule is the one
//! reported.

use std::collections::BTreeSet;

use crate::catalog::{DescLine, Map, parse_desc, parse_map, unescape_field};
use crate::docs::DocEntry;
use crate::error::{Error, ErrorCode, Result};
use crate::header::parse_decimal;
use crate::sections::Sections;

/// How much the validator checks (spec FR-20 rules).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Header and derived sections (the default of `kapu validate`).
    Basic,
    /// Also the body: `bodyhash`, every door fingerprint, room markers, and
    /// whether a rebuild gives the same bytes (D89).
    Full,
}

/// Hard limits of captions (FR-06).
pub const TITLE_MAX_CHARS: usize = 200;
/// At most this many tags per door (FR-06/3).
pub const TAGS_MAX: usize = 32;
/// At most this many characters per tag (FR-06/3).
pub const TAG_MAX_CHARS: usize = 64;

/// What a successful validation found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Number of rooms.
    pub rooms: usize,
    /// Number of doors (rooms and sub-doors).
    pub doors: usize,
    /// Whether the body checks of [`Level::Full`] ran.
    pub full: bool,
    /// Whether the rebuild comparison ran. It is skipped, with warning W001,
    /// when the file was built with a model this reader does not have (D50).
    pub rebuild_checked: bool,
}

/// FR-04/3: `[a-z0-9][a-z0-9-]{0,63}`, and for a sub-door `<room>/<sub>` with
/// the same grammar on both sides.
#[must_use]
pub fn is_valid_id(id: &str) -> bool {
    fn part(p: &str) -> bool {
        let b = p.as_bytes();
        !b.is_empty()
            && b.len() <= 64
            && b.first()
                .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            && b.iter()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
    }
    match id.split_once('/') {
        None => part(id),
        Some((room, sub)) => part(room) && part(sub),
    }
}

fn err(code: ErrorCode, detail: impl Into<String>) -> Error {
    Error::new(code, detail)
}

/// Validates a whole file without a vector model: the rebuild comparison of
/// [`Level::Full`] runs only for files built without one (`emb=none`).
///
/// # Errors
/// The first violated rule, with its FR-20 code.
pub fn validate(file: &[u8], level: Level) -> Result<Report> {
    validate_with(file, level, None)
}

/// Validates a whole file; with `model`, a file built with that same model
/// (name and `embhash` equal) also gets the rebuild comparison (D89, D50).
///
/// # Errors
/// The first violated rule, with its FR-20 code.
pub fn validate_with(
    file: &[u8],
    level: Level,
    model: Option<&crate::embed::Model>,
) -> Result<Report> {
    let s = Sections::parse(file)?;
    let entries = s.doc_entries()?;
    let ids = check_ids(&entries)?;
    let rooms: BTreeSet<&str> = ids.iter().copied().filter(|id| !id.contains('/')).collect();

    // E009 for sub-door parents, then every section's references.
    for id in &ids {
        if let Some((room, _)) = id.split_once('/') {
            known(&ids, room, &format!("sub-door `{id}`"))?;
        }
    }
    let map = parse_map(s.map)?;
    for line in &map.doors {
        known(&ids, line.id, "#MAP")?;
    }
    let desc = parse_desc(s.desc)?;
    for line in &desc {
        known(&ids, line.id, "#DESC")?;
    }
    check_graph(s.graph, &ids)?;
    let terms = check_index(s.index, &ids)?;

    check_captions(&map, &desc)?;
    check_counts_and_order(&s, &entries, &terms, &map, &desc, rooms.len(), ids.len())?;

    if level == Level::Full {
        s.check_bodyhash()?;
        for e in &entries {
            s.stored(e)?;
        }
        check_room_markers(&s, &entries)?;
    }
    let matching_model = model.filter(|m| m.name() == s.header.emb && m.hash() == s.header.embhash);
    let rebuild_checked = level == Level::Full
        && (s.header.emb == crate::build::NO_MODEL || matching_model.is_some());
    if rebuild_checked {
        let rebuilt = crate::build::rebuild_with(file, matching_model)?;
        if rebuilt.bytes != file {
            return Err(err(
                ErrorCode::E005,
                "the derived sections differ from what a build of this body gives (run `kapu build`)",
            ));
        }
    }

    Ok(Report {
        rooms: rooms.len(),
        doors: ids.len(),
        full: level == Level::Full,
        rebuild_checked,
    })
}

/// E008: grammar and uniqueness of the ids in `#DOCS`.
fn check_ids<'a>(entries: &[DocEntry<'a>]) -> Result<BTreeSet<&'a str>> {
    let mut ids = BTreeSet::new();
    for e in entries {
        if !is_valid_id(e.id) {
            return Err(err(
                ErrorCode::E008,
                format!("`{}` is not a valid door id", e.id),
            ));
        }
        if !ids.insert(e.id) {
            return Err(err(
                ErrorCode::E008,
                format!("door id `{}` appears twice in #DOCS", e.id),
            ));
        }
    }
    Ok(ids)
}

/// E009 unless `id` is a known door.
fn known(ids: &BTreeSet<&str>, id: &str, where_: &str) -> Result<()> {
    if ids.contains(id) {
        Ok(())
    } else {
        Err(err(
            ErrorCode::E009,
            format!("{where_} refers to unknown door `{id}`"),
        ))
    }
}

/// `#GRAPH` lines: endpoints known (E009), type, weight and no self-loop (E005).
fn check_graph(text: &str, ids: &BTreeSet<&str>) -> Result<()> {
    for (n, line) in text.lines().enumerate() {
        let bad = |what: String| err(ErrorCode::E005, format!("#GRAPH line {}: {what}", n + 1));
        let f: Vec<&str> = line.split('\t').collect();
        let [from, to, kind, weight] = f.as_slice() else {
            return Err(bad("expected 4 fields".to_owned()));
        };
        known(ids, from, "#GRAPH")?;
        known(ids, to, "#GRAPH")?;
        if !matches!(*kind, "link" | "term" | "sim") {
            return Err(bad(format!("unknown edge type `{kind}`")));
        }
        let w: f64 = weight
            .parse()
            .map_err(|_| bad(format!("bad weight `{weight}`")))?;
        if !(0.0..=1.0).contains(&w) {
            return Err(bad(format!("weight {w} outside 0..1")));
        }
        if from == to {
            return Err(bad(format!("self-loop on `{from}`")));
        }
    }
    Ok(())
}

/// `#INDEX` lines: postings name known doors (E009), `df` equals the number
/// of postings (E005). Returns the terms in file order.
fn check_index<'a>(text: &'a str, ids: &BTreeSet<&str>) -> Result<Vec<&'a str>> {
    let mut terms = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let bad = |what: &str| err(ErrorCode::E005, format!("#INDEX line {}: {what}", n + 1));
        let f: Vec<&str> = line.split('\t').collect();
        let [term, df, postings] = f.as_slice() else {
            return Err(bad("expected 3 fields"));
        };
        let df = parse_decimal(df).ok_or_else(|| bad("bad df"))?;
        let mut count = 0u64;
        for posting in postings.split(',') {
            let (id, tf) = posting
                .rsplit_once(':')
                .ok_or_else(|| bad("posting is not `id:tf`"))?;
            known(ids, id, "#INDEX")?;
            parse_decimal(tf).ok_or_else(|| bad("bad tf"))?;
            count += 1;
        }
        if count != df {
            return Err(bad(&format!("df is {df} but there are {count} postings")));
        }
        terms.push(*term);
    }
    Ok(terms)
}

/// E010: title length in `#MAP`, tag count and tag length in `#DESC`.
fn check_captions(map: &Map<'_>, desc: &[DescLine<'_>]) -> Result<()> {
    for line in &map.doors {
        let chars = unescape_field(line.title).chars().count();
        if chars > TITLE_MAX_CHARS {
            return Err(err(
                ErrorCode::E010,
                format!(
                    "title of `{}` is {chars} characters, the limit is {TITLE_MAX_CHARS}",
                    line.id
                ),
            ));
        }
    }
    for line in desc {
        let tokens = crate::terms::tokens(line.description)?;
        if tokens > crate::build::DESCRIPTION_MAX_TOKENS {
            return Err(err(
                ErrorCode::E010,
                format!(
                    "description of `{}` is {tokens} tokens, the limit is {}",
                    line.id,
                    crate::build::DESCRIPTION_MAX_TOKENS
                ),
            ));
        }
        if line.tags.len() > TAGS_MAX {
            return Err(err(
                ErrorCode::E010,
                format!(
                    "`{}` has {} tags, the limit is {TAGS_MAX}",
                    line.id,
                    line.tags.len()
                ),
            ));
        }
        if let Some(tag) = line.tags.iter().find(|t| t.chars().count() > TAG_MAX_CHARS) {
            return Err(err(
                ErrorCode::E010,
                format!(
                    "a tag of `{}` is longer than {TAG_MAX_CHARS} characters: {tag:?}",
                    line.id
                ),
            ));
        }
    }
    Ok(())
}

/// E005: header counts, sort order of `#DOCS` and `#INDEX`, and every door
/// listed once in `#MAP` and `#DESC` (FR-03, FR-09/5, FR-13, FR-14/3).
fn check_counts_and_order(
    s: &Sections<'_>,
    entries: &[DocEntry<'_>],
    terms: &[&str],
    map: &Map<'_>,
    desc: &[DescLine<'_>],
    rooms: usize,
    doors: usize,
) -> Result<()> {
    let counts = [
        ("rooms", s.header.rooms, rooms),
        ("doors", s.header.doors, doors),
        ("groups", s.header.groups, map.groups),
    ];
    for (name, declared, actual) in counts {
        if usize::try_from(declared).ok() != Some(actual) {
            return Err(err(
                ErrorCode::E005,
                format!("header says {name}={declared}, the sections have {actual}"),
            ));
        }
    }
    if !entries
        .windows(2)
        .all(|w| matches!(w, [a, b] if a.id.as_bytes() < b.id.as_bytes()))
    {
        return Err(err(ErrorCode::E005, "#DOCS is not sorted by id"));
    }
    if !terms
        .windows(2)
        .all(|w| matches!(w, [a, b] if a.as_bytes() < b.as_bytes()))
    {
        return Err(err(ErrorCode::E005, "#INDEX is not sorted by term"));
    }
    if map.doors.len() != doors || desc.len() != doors {
        return Err(err(
            ErrorCode::E005,
            "#MAP and #DESC must list every door exactly once",
        ));
    }
    Ok(())
}

/// Every `@@@ <id>` line of the body must sit right before its room's span,
/// and the rooms must be exactly the ones listed in `#DOCS` (FR-04/1, D36).
fn check_room_markers(s: &Sections<'_>, entries: &[DocEntry<'_>]) -> Result<()> {
    let mut markers: Vec<(u64, &str)> = Vec::new();
    let mut offset = 0u64;
    for line in s.body.split(|&b| b == b'\n') {
        if let Some(rest) = line.strip_prefix(b"@@@ ") {
            let id = std::str::from_utf8(rest).unwrap_or("");
            let start = offset + u64::try_from(line.len()).unwrap_or(0) + 1;
            markers.push((start, id));
        }
        offset += u64::try_from(line.len()).unwrap_or(0) + 1;
    }
    let mut rooms: Vec<(u64, &str)> = entries
        .iter()
        .filter(|e| !e.id.contains('/'))
        .map(|e| (e.span.start, e.id))
        .collect();
    rooms.sort_unstable();
    if markers != rooms {
        return Err(err(
            ErrorCode::E005,
            "the `@@@` lines of the body do not match the rooms in #DOCS",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_grammar() {
        for ok in [
            "a",
            "0",
            "frontend-design-alapok",
            "room/sub-2",
            &"a".repeat(64),
        ] {
            assert!(is_valid_id(ok), "{ok}");
        }
        for bad in [
            "",
            "Szoba",
            "-a",
            "a_b",
            "a/",
            "/a",
            "a/b/c",
            "ü",
            &"a".repeat(65),
        ] {
            assert!(!is_valid_id(bad), "{bad}");
        }
    }
}
