//! Plan v3 P1, P2, P4: BM25 over passages instead of doors.
//!
//! A room's text is cut into leaves: its introduction (the text before its
//! first sub-door) and its sub-doors (FR-07). Each leaf is cut into windows of
//! at most `passage_chars` characters, each starting 7/8 of that after the
//! previous one, as the vector windows (D141). BM25 runs over these passages
//! with their own `N`, `df` and mean length, and a door scores its best
//! passage (max-passage). A room door stands for its introduction only, so a room no
//! longer outranks its own sections with their text counted twice (A57).
//!
//! P2: every passage of a leaf also holds the room title, the section title
//! and the room tags (the leaf's head) with the weight `passage_prefix`. The
//! head is stored once per leaf and weighed at query time, so the variants of
//! a measurement share one index.
//!
//! The index is built in memory on the first search and kept per file and
//! window size: a switch for measurement, not stored in the file (P6 would).

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use crate::error::Result;
use crate::header::Hash;
use crate::search::{B, Hit, K1, RankOptions, query_terms_with, room_of};
use crate::sections::Sections;
use crate::terms::{index_tokens, stems};

/// Default window size of a passage, about 512 `o200k_base` tokens of
/// English, the size of the chunk baseline (plan v2 M2).
pub const PASSAGE_CHARS: usize = 2000;

/// One leaf: a room's introduction or one of its sub-doors.
struct Leaf {
    /// Its first passage.
    first: u32,
    /// Number of its passages.
    count: u32,
    /// Weighted length of its head at weight 1.
    head_len: f32,
}

/// The passages of one file.
pub struct PassageIndex {
    /// Per passage: index of its door in `#DOCS`.
    door: Vec<u32>,
    /// Per passage: its leaf.
    leaf: Vec<u32>,
    /// Per passage: the number of index tokens of its text (as `dl`, FR-09/6).
    len: Vec<f32>,
    leaves: Vec<Leaf>,
    /// Mean of `len`.
    mean_len: f64,
    /// Mean over the passages of their leaf's `head_len`.
    mean_head: f64,
    /// Term ids.
    terms: HashMap<String, u32>,
    /// Per term id: (passage, tf) of the passage texts, passages ascending.
    postings: Vec<Vec<(u32, f32)>>,
    /// Per term id: (leaf, tf) of the heads.
    heads: Vec<Vec<(u32, f32)>>,
}

impl PassageIndex {
    /// Number of passages.
    #[must_use]
    pub fn passages(&self) -> usize {
        self.door.len()
    }

    fn term(&mut self, term: String) -> u32 {
        let next = u32::try_from(self.postings.len()).unwrap_or(u32::MAX);
        *self.terms.entry(term).or_insert_with(|| {
            self.postings.push(Vec::new());
            self.heads.push(Vec::new());
            next
        })
    }

    /// The term ids of a word and its stems, remembered per word.
    fn ids_of(&mut self, word: String, memo: &mut HashMap<String, Vec<u32>>) -> Vec<u32> {
        if let Some(ids) = memo.get(&word) {
            return ids.clone();
        }
        let ids: Vec<u32> = stems(&word).into_iter().map(|t| self.term(t)).collect();
        memo.insert(word, ids.clone());
        ids
    }

    fn counts(
        &mut self,
        text: &str,
        memo: &mut HashMap<String, Vec<u32>>,
    ) -> (BTreeMap<u32, f32>, f32) {
        let mut tf: BTreeMap<u32, f32> = BTreeMap::new();
        let mut len = 0f32;
        for word in index_tokens(text) {
            len += 1.0;
            for id in self.ids_of(word, memo) {
                *tf.entry(id).or_insert(0.0) += 1.0;
            }
        }
        (tf, len)
    }

    fn build(s: &Sections<'_>, chars: usize) -> Result<Self> {
        let entries = s.doc_entries()?;
        let map = crate::catalog::parse_map(s.map)?;
        let titles: HashMap<&str, String> = map
            .doors
            .iter()
            .map(|d| (d.id, crate::catalog::unescape_field(d.title)))
            .collect();
        let desc = crate::catalog::parse_desc(s.desc)?;
        let tags: HashMap<&str, String> = desc.iter().map(|d| (d.id, d.tags.join(" "))).collect();
        // a room's text ends its introduction where its first sub-door starts
        let mut first_sub: HashMap<&str, u64> = HashMap::new();
        for e in entries.iter().filter(|e| e.id.contains('/')) {
            let start = first_sub.entry(room_of(e.id)).or_insert(e.span.start);
            *start = (*start).min(e.span.start);
        }
        let mut ix = Self {
            door: Vec::new(),
            leaf: Vec::new(),
            len: Vec::new(),
            leaves: Vec::new(),
            mean_len: 0.0,
            mean_head: 0.0,
            terms: HashMap::new(),
            postings: Vec::new(),
            heads: Vec::new(),
        };
        let mut memo: HashMap<String, Vec<u32>> = HashMap::new();
        for (d, e) in entries.iter().enumerate() {
            let room = room_of(e.id);
            let mut stored = s.stored(e)?;
            if !e.id.contains('/')
                && let Some(start) = first_sub.get(room)
            {
                let intro = start
                    .checked_sub(e.span.start)
                    .and_then(|n| usize::try_from(n).ok());
                stored = intro.and_then(|n| stored.get(..n)).unwrap_or(stored);
            }
            let text = crate::text::indexed_text(&crate::docs::served_text(stored)?);
            if text.trim().is_empty() {
                continue;
            }
            let title = |id: &str| titles.get(id).map_or("", String::as_str);
            let head = if e.id.contains('/') {
                format!(
                    "{}\n{}\n{}",
                    title(room),
                    title(e.id),
                    tags.get(room).map_or("", String::as_str)
                )
            } else {
                format!("{}\n{}", title(room), tags.get(room).map_or("", String::as_str))
            };
            let leaf = u32::try_from(ix.leaves.len()).unwrap_or(u32::MAX);
            let (head_tf, head_len) = ix.counts(&head, &mut memo);
            for (id, tf) in head_tf {
                if let Some(list) = ix.heads.get_mut(id as usize) {
                    list.push((leaf, tf));
                }
            }
            let first = u32::try_from(ix.door.len()).unwrap_or(u32::MAX);
            let door = u32::try_from(d).unwrap_or(u32::MAX);
            for window in crate::embed::windows(&text, chars) {
                let passage = u32::try_from(ix.door.len()).unwrap_or(u32::MAX);
                let (tf, len) = ix.counts(window, &mut memo);
                for (id, tf) in tf {
                    if let Some(list) = ix.postings.get_mut(id as usize) {
                        list.push((passage, tf));
                    }
                }
                ix.door.push(door);
                ix.leaf.push(leaf);
                ix.len.push(len);
            }
            let count = u32::try_from(ix.door.len()).unwrap_or(u32::MAX) - first;
            ix.leaves.push(Leaf {
                first,
                count,
                head_len,
            });
        }
        let passages = ix.door.len().max(1);
        #[allow(clippy::cast_precision_loss)]
        {
            ix.mean_len = ix.len.iter().map(|l| f64::from(*l)).sum::<f64>() / passages as f64;
            ix.mean_head = ix
                .leaf
                .iter()
                .filter_map(|l| ix.leaves.get(*l as usize))
                .map(|l| f64::from(l.head_len))
                .sum::<f64>()
                / passages as f64;
        }
        Ok(ix)
    }
}

/// Indexes kept per (body, window size): the variants of one measurement
/// use at most a few window sizes.
const MEMO_INDEXES: usize = 3;

type MemoKey = (Hash, usize);

static MEMO: Mutex<Vec<(MemoKey, Arc<PassageIndex>)>> = Mutex::new(Vec::new());

/// The passage index of a file, built on first use.
///
/// # Errors
/// A broken `#MAP`, `#DESC` or `#DOCS`, or a door that is not UTF-8.
pub fn index_for(s: &Sections<'_>, chars: usize) -> Result<Arc<PassageIndex>> {
    let key = (s.header.bodyhash, chars.max(1));
    if let Ok(memo) = MEMO.lock()
        && let Some((_, ix)) = memo.iter().find(|(k, _)| *k == key)
    {
        return Ok(Arc::clone(ix));
    }
    let ix = Arc::new(PassageIndex::build(s, key.1)?);
    if let Ok(mut memo) = MEMO.lock() {
        if memo.len() >= MEMO_INDEXES {
            memo.remove(0);
        }
        memo.push((key, Arc::clone(&ix)));
    }
    Ok(ix)
}

/// P1: BM25 over the passages, a door scored by its best passage, best
/// first, ties by id, at most `depth`, only doors `allowed` keeps.
///
/// # Errors
/// As [`index_for`].
pub fn bm25(
    s: &Sections<'_>,
    query: &str,
    allowed: &dyn Fn(&str) -> bool,
    rank: &RankOptions,
    depth: usize,
) -> Result<Vec<Hit>> {
    let ix = index_for(s, rank.passage_chars)?;
    let entries = s.doc_entries()?;
    let w = rank.passage_prefix;
    #[allow(clippy::cast_precision_loss)]
    let n = ix.passages() as f64;
    let avg = (ix.mean_len + w * ix.mean_head).max(f64::MIN_POSITIVE);
    let head_len = |p: u32| {
        ix.leaf
            .get(p as usize)
            .and_then(|l| ix.leaves.get(*l as usize))
            .map_or(0.0, |l| f64::from(l.head_len))
    };

    let mut score: HashMap<u32, f64> = HashMap::new();
    let mut tf: HashMap<u32, f64> = HashMap::new();
    // in byte order of the query terms, so every passage sums them in one order
    for term in query_terms_with(query, rank.stemming) {
        let Some(&id) = ix.terms.get(&term) else {
            continue;
        };
        tf.clear();
        for &(p, f) in ix.postings.get(id as usize).into_iter().flatten() {
            tf.insert(p, f64::from(f));
        }
        if w > 0.0 {
            for &(leaf, f) in ix.heads.get(id as usize).into_iter().flatten() {
                if let Some(l) = ix.leaves.get(leaf as usize) {
                    for p in l.first..l.first + l.count {
                        *tf.entry(p).or_insert(0.0) += w * f64::from(f);
                    }
                }
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let df = tf.len() as f64;
        let idf = libm::log((n - df + 0.5) / (df + 0.5) + 1.0);
        for (&p, &f) in &tf {
            let len = ix.len.get(p as usize).map_or(0.0, |l| f64::from(*l)) + w * head_len(p);
            let part = idf * f * (K1 + 1.0) / (f + K1 * (1.0 - B + B * len / avg));
            *score.entry(p).or_insert(0.0) += part;
        }
    }
    // MaxP: a door scores its best passage
    let mut best: HashMap<u32, f64> = HashMap::new();
    for (p, sc) in score {
        if let Some(&d) = ix.door.get(p as usize) {
            let b = best.entry(d).or_insert(sc);
            *b = b.max(sc);
        }
    }
    let mut hits: Vec<Hit> = best
        .into_iter()
        .filter_map(|(d, score)| {
            entries
                .get(d as usize)
                .filter(|e| allowed(e.id))
                .map(|e| Hit {
                    id: e.id.to_owned(),
                    score,
                })
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.id.as_bytes().cmp(b.id.as_bytes()))
    });
    hits.truncate(depth);
    Ok(hits)
}
