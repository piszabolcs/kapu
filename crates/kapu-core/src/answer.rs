//! Plan v4 (D175): answer passages. A door's text is cut into passages of
//! consecutive lines (a heading starts a new one; about 700 to 1100
//! characters), each remembered with its door and its line range in the
//! door's served text (the text `kapu_open` returns). A query scores the
//! passages by BM25 over the passages (index tokens and their stems, the
//! door title counted with the text), fused by RRF with the rank of the
//! passage's room in the hybrid search: the answer is the best passage of
//! each of the best rooms, not the start of each room. The index is built
//! on the first ask and kept per body, like the P1 passage index.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

use crate::error::Result;
use crate::header::Hash;
use crate::search::{B, K1, room_of};
use crate::sections::Sections;
use crate::terms::{index_tokens, stems};

/// A passage ends at the first blank line after this many characters...
const SOFT_CHARS: usize = 700;
/// ...or, without one, at this many.
const HARD_CHARS: usize = 1100;
/// RRF constant of the fusion (as the search).
const RRF_K: f64 = 60.0;
/// Passages taken from BM25 alone, besides those of the given rooms.
const BM25_CANDIDATES: usize = 100;

/// How the passages are picked.
#[derive(Debug, Clone)]
pub struct AnswerOptions {
    /// The weight of the room's rank in the fusion (the passage's own BM25
    /// rank weighs 1). Measured on the dev part of four sets (plan v4).
    pub room_weight: f64,
    /// At most this many passages of one room.
    pub per_room: usize,
    /// The rooms of the hybrid ranking that count, in order.
    pub rooms: usize,
}

impl Default for AnswerOptions {
    fn default() -> Self {
        Self {
            room_weight: 4.0,
            per_room: 1,
            rooms: 20,
        }
    }
}

/// One answer passage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passage {
    /// Its door.
    pub door: String,
    /// The door's title.
    pub title: String,
    /// Its first and last line in the door's served text, from 1.
    pub lines: (u32, u32),
    /// Its text.
    pub text: String,
}

struct Meta {
    door: u32,
    lines: (u32, u32),
    text: Arc<str>,
    span: (usize, usize),
    len: f32,
}

/// The passages of one file and their BM25 postings.
pub struct AnswerIndex {
    doors: Vec<(String, String)>,
    passages: Vec<Meta>,
    terms: HashMap<String, u32>,
    postings: Vec<Vec<(u32, f32)>>,
    mean_len: f64,
    by_room: HashMap<String, Vec<u32>>,
}

/// The line blocks of a text: (first line, last line, byte start, byte end),
/// lines from 1. A heading line outside code starts a block; a block ends at a
/// blank line once it holds [`SOFT_CHARS`], or at [`HARD_CHARS`].
fn blocks(text: &str) -> Vec<(u32, u32, usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<(u32, usize)> = None;
    let mut size = 0usize;
    let mut in_code = false;
    let mut pos = 0usize;
    let mut last = (0u32, 0usize);
    for (i, line) in text.split_inclusive('\n').enumerate() {
        let n = u32::try_from(i + 1).unwrap_or(u32::MAX);
        let fence = line.trim_start().starts_with("```");
        let heading = !in_code && line.starts_with('#');
        let blank = line.trim().is_empty();
        if let Some((a, at)) = start
            && (heading || (size >= SOFT_CHARS && blank && !in_code) || size >= HARD_CHARS)
        {
            if text.get(at..pos).is_some_and(|t| !t.trim().is_empty()) {
                out.push((a, last.0, at, pos));
            }
            start = None;
            size = 0;
        }
        if fence {
            in_code = !in_code;
        }
        if start.is_none() {
            start = Some((n, pos));
        }
        size += line.len();
        pos += line.len();
        last = (n, pos);
    }
    if let Some((a, at)) = start
        && text.get(at..pos).is_some_and(|t| !t.trim().is_empty())
    {
        out.push((a, last.0, at, pos));
    }
    out
}

impl AnswerIndex {
    fn term(&mut self, term: String) -> u32 {
        let next = u32::try_from(self.postings.len()).unwrap_or(u32::MAX);
        *self.terms.entry(term).or_insert_with(|| {
            self.postings.push(Vec::new());
            next
        })
    }

    fn build(s: &Sections<'_>) -> Result<Self> {
        let entries = s.doc_entries()?;
        let map = crate::catalog::parse_map(s.map)?;
        let titles: HashMap<&str, String> = map
            .doors
            .iter()
            .map(|d| (d.id, crate::catalog::unescape_field(d.title)))
            .collect();
        // a room's own text ends where its first sub-door starts (as P1)
        let mut first_sub: HashMap<&str, u64> = HashMap::new();
        for e in entries.iter().filter(|e| e.id.contains('/')) {
            let start = first_sub.entry(room_of(e.id)).or_insert(e.span.start);
            *start = (*start).min(e.span.start);
        }
        let mut ix = Self {
            doors: Vec::new(),
            passages: Vec::new(),
            terms: HashMap::new(),
            postings: Vec::new(),
            mean_len: 0.0,
            by_room: HashMap::new(),
        };
        let mut memo: HashMap<String, Vec<u32>> = HashMap::new();
        for e in &entries {
            let mut stored = s.stored(e)?;
            if !e.id.contains('/')
                && let Some(start) = first_sub.get(room_of(e.id))
            {
                let intro = start
                    .checked_sub(e.span.start)
                    .and_then(|n| usize::try_from(n).ok());
                stored = intro.and_then(|n| stored.get(..n)).unwrap_or(stored);
            }
            let text: Arc<str> = Arc::from(crate::docs::served_text(stored)?);
            if text.trim().is_empty() {
                continue;
            }
            let title = titles.get(e.id).cloned().unwrap_or_default();
            let door = u32::try_from(ix.doors.len()).unwrap_or(u32::MAX);
            for (a, b, from, to) in blocks(&text) {
                let passage = u32::try_from(ix.passages.len()).unwrap_or(u32::MAX);
                let body = text.get(from..to).unwrap_or("");
                let mut tf: BTreeMap<u32, f32> = BTreeMap::new();
                let mut len = 0f32;
                for word in index_tokens(&format!("{title}\n{body}")) {
                    len += 1.0;
                    let ids = if let Some(ids) = memo.get(&word) {
                        ids.clone()
                    } else {
                        let ids: Vec<u32> = stems(&word).into_iter().map(|t| ix.term(t)).collect();
                        memo.insert(word, ids.clone());
                        ids
                    };
                    for id in ids {
                        *tf.entry(id).or_insert(0.0) += 1.0;
                    }
                }
                for (id, n) in tf {
                    if let Some(list) = ix.postings.get_mut(id as usize) {
                        list.push((passage, n));
                    }
                }
                ix.by_room
                    .entry(room_of(e.id).to_owned())
                    .or_default()
                    .push(passage);
                ix.passages.push(Meta {
                    door,
                    lines: (a, b),
                    text: Arc::clone(&text),
                    span: (from, to),
                    len,
                });
            }
            ix.doors.push((e.id.to_owned(), title));
        }
        #[allow(clippy::cast_precision_loss)]
        {
            let n = ix.passages.len().max(1) as f64;
            ix.mean_len = ix.passages.iter().map(|p| f64::from(p.len)).sum::<f64>() / n;
        }
        Ok(ix)
    }

    /// Number of passages.
    #[must_use]
    pub fn len(&self) -> usize {
        self.passages.len()
    }

    /// Whether the file has no passage.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.passages.is_empty()
    }

    /// BM25 of every passage that holds a query term.
    fn bm25(&self, query: &str) -> HashMap<u32, f64> {
        #[allow(clippy::cast_precision_loss)]
        let n = self.passages.len() as f64;
        let mut ids: Vec<u32> = index_tokens(query)
            .iter()
            .flat_map(|w| stems(w))
            .filter_map(|t| self.terms.get(&t).copied())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        let mut score: HashMap<u32, f64> = HashMap::new();
        for id in ids {
            let Some(list) = self.postings.get(id as usize) else {
                continue;
            };
            #[allow(clippy::cast_precision_loss)]
            let df = list.len() as f64;
            let idf = (1.0 + (n - df + 0.5) / (df + 0.5)).ln();
            for (p, tf) in list {
                let len = self
                    .passages
                    .get(*p as usize)
                    .map_or(0.0, |m| f64::from(m.len));
                let tf = f64::from(*tf);
                *score.entry(*p).or_insert(0.0) += idf * tf * (K1 + 1.0)
                    / (tf + K1 * (1.0 - B + B * len / self.mean_len.max(1e-9)));
            }
        }
        score
    }

    fn passage(&self, p: u32) -> Option<Passage> {
        let m = self.passages.get(p as usize)?;
        let (door, title) = self.doors.get(m.door as usize)?;
        Some(Passage {
            door: door.clone(),
            title: title.clone(),
            lines: m.lines,
            text: m.text.get(m.span.0..m.span.1).unwrap_or("").to_owned(),
        })
    }

    /// The passages for `query`, best first: the passages of the first
    /// `options.rooms` of `rooms` (the hybrid ranking's rooms, best first) and
    /// the best by BM25, scored `1 / (k + bm25 rank) + room_weight / (k + room
    /// rank)`, at most `options.per_room` of a room.
    #[must_use]
    pub fn answer(&self, query: &str, rooms: &[&str], options: &AnswerOptions) -> Vec<Passage> {
        let bm25 = self.bm25(query);
        let mut by_score: Vec<(u32, f64)> = bm25.iter().map(|(p, s)| (*p, *s)).collect();
        by_score.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let room_rank: HashMap<&str, usize> = rooms
            .iter()
            .take(options.rooms)
            .enumerate()
            .map(|(i, r)| (*r, i + 1))
            .collect();
        let mut candidates: Vec<u32> = by_score
            .iter()
            .take(BM25_CANDIDATES)
            .map(|(p, _)| *p)
            .collect();
        for room in rooms.iter().take(options.rooms) {
            candidates.extend(self.by_room.get(*room).into_iter().flatten().copied());
        }
        candidates.sort_unstable();
        candidates.dedup();
        let mut ranked: Vec<u32> = candidates.clone();
        ranked.sort_by(|a, b| {
            let (x, y) = (
                bm25.get(a).copied().unwrap_or(0.0),
                bm25.get(b).copied().unwrap_or(0.0),
            );
            y.total_cmp(&x).then(a.cmp(b))
        });
        let bm25_rank: HashMap<u32, usize> = ranked
            .iter()
            .enumerate()
            .map(|(i, p)| (*p, i + 1))
            .collect();
        let mut fused: Vec<(u32, f64)> = candidates
            .iter()
            .map(|p| {
                let mut s = 0.0;
                if bm25.get(p).is_some_and(|v| *v > 0.0) {
                    #[allow(clippy::cast_precision_loss)]
                    let r = bm25_rank.get(p).copied().unwrap_or(usize::MAX / 2) as f64;
                    s += 1.0 / (RRF_K + r);
                }
                let room = self
                    .passages
                    .get(*p as usize)
                    .and_then(|m| self.doors.get(m.door as usize))
                    .map_or("", |(d, _)| room_of(d));
                if let Some(r) = room_rank.get(room) {
                    #[allow(clippy::cast_precision_loss)]
                    let r = *r as f64;
                    s += options.room_weight / (RRF_K + r);
                }
                (*p, s)
            })
            .collect();
        fused.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut per_room: HashMap<String, usize> = HashMap::new();
        let mut out = Vec::new();
        for (p, _) in fused {
            let Some(passage) = self.passage(p) else {
                continue;
            };
            let n = per_room
                .entry(room_of(&passage.door).to_owned())
                .or_insert(0);
            if *n >= options.per_room {
                continue;
            }
            *n += 1;
            out.push(passage);
        }
        out
    }
}

type MemoKey = Hash;

static MEMO: Mutex<Vec<(MemoKey, Arc<AnswerIndex>)>> = Mutex::new(Vec::new());

/// Indexes kept at once (one per open file of a server).
const MEMO_INDEXES: usize = 4;

/// The answer index of a file, built on first use.
///
/// # Errors
/// A broken `#MAP` or `#DOCS`, or a door that is not UTF-8.
pub fn index_for(s: &Sections<'_>) -> Result<Arc<AnswerIndex>> {
    let key = s.header.bodyhash;
    if let Ok(memo) = MEMO.lock()
        && let Some((_, ix)) = memo.iter().find(|(k, _)| *k == key)
    {
        return Ok(Arc::clone(ix));
    }
    let ix = Arc::new(AnswerIndex::build(s)?);
    if let Ok(mut memo) = MEMO.lock() {
        if memo.len() >= MEMO_INDEXES {
            memo.remove(0);
        }
        memo.push((key, Arc::clone(&ix)));
    }
    Ok(ix)
}

/// The passages as one framed block (D106: one fresh boundary for the whole
/// reply, so a closing tag inside a passage cannot end it): each passage
/// under a line `[door Lfirst-last] title`.
#[must_use]
pub fn frame_passages(file_name: &str, passages: &[Passage]) -> String {
    let file = file_name.replace('&', "&amp;").replace('"', "&quot;");
    let first = passages.first().map_or("", |p| p.door.as_str());
    let boundary = crate::reader::new_boundary(first);
    let mut out = format!("<kapu-passages file=\"{file}\" boundary=\"{boundary}\">\n");
    for p in passages {
        let _ = writeln!(out, "[{} L{}-{}] {}", p.door, p.lines.0, p.lines.1, p.title);
        out.push_str(&p.text);
        if !p.text.ends_with('\n') {
            out.push('\n');
        }
    }
    let _ = writeln!(out, "</kapu-passages boundary=\"{boundary}\">");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_start_at_headings_and_end_after_blank_lines() {
        let text = "# A\nintro\n## B\none\n\ntwo\n";
        let b = blocks(text);
        assert_eq!(b.len(), 2);
        assert_eq!((b[0].0, b[0].1), (1, 2));
        assert_eq!((b[1].0, b[1].1), (3, 6));
        assert_eq!(&text[b[1].2..b[1].3], "## B\none\n\ntwo\n");
        let long = format!("{}\n\n{}\n", "x".repeat(800), "y".repeat(10));
        let b = blocks(&long);
        assert_eq!(b.len(), 2, "{b:?}");
        assert_eq!((b[1].0, b[1].1), (2, 3));
    }

    #[test]
    fn code_fences_hide_headings() {
        let text = "```\n# not a heading\n```\nafter\n";
        assert_eq!(blocks(text).len(), 1);
    }
}
