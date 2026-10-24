//! Keyword search with BM25 over the `#INDEX` section (FR-09/6, FR-16/1-2).
//!
//! The query goes through the same tokenizer and double stemming as the
//! index (D61). Each distinct stemmed entry counts once (D85). Scores use
//! `k1 = 1.2`, `b = 0.75`, the `dl` of `#DOCS` and
//! `idf = ln((N - df + 0.5) / (df + 0.5) + 1)` with `N` the number of doors.
//! Ties are broken by door id (FR-16/4).
//!
//! Every function has a `_in` form on a parsed file, so one call parses and
//! checks the file once (D139); the byte forms parse and call it.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::error::Result;
use crate::header::parse_decimal;
use crate::sections::{BodyLength, Limits, Sections};
use crate::terms::{index_tokens, stems};

/// BM25 `k1` (FR-09/6).
pub const K1: f64 = 1.2;
/// BM25 `b` (FR-09/6).
pub const B: f64 = 0.75;
/// Each ranking keeps its first 50 hits (FR-16/2).
pub const TOP_N: usize = 50;

/// One BM25 hit.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    /// Door id.
    pub id: String,
    /// BM25 score.
    pub score: f64,
}

/// The distinct stemmed entries of a query, in byte order.
#[must_use]
pub fn query_terms(query: &str) -> BTreeSet<String> {
    index_tokens(query).iter().flat_map(|t| stems(t)).collect()
}

fn parse(file: &[u8]) -> Result<Sections<'_>> {
    Sections::parse_with(file, &Limits::default(), BodyLength::Tolerate)
}

/// The `#INDEX` line of `term`, without its LF, by binary search: the lines
/// are in byte order of their term (FR-09/5, D139).
fn index_line<'a>(index: &'a str, term: &str) -> Option<&'a str> {
    let b = index.as_bytes();
    let (mut lo, mut hi) = (0, b.len()); // `lo` and `hi` are line starts (or the end)
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let start = b
            .get(lo..mid)?
            .iter()
            .rposition(|&c| c == b'\n')
            .map_or(lo, |p| lo + p + 1);
        let end = b
            .get(start..hi)?
            .iter()
            .position(|&c| c == b'\n')
            .map_or(hi, |p| start + p);
        let line = index.get(start..end)?;
        let key = line.split_once('\t').map_or(line, |(k, _)| k);
        match key.as_bytes().cmp(term.as_bytes()) {
            Ordering::Equal => return Some(line),
            Ordering::Less => lo = end + 1,
            Ordering::Greater => hi = start,
        }
    }
    None
}

/// BM25 ranking of all doors for `query`, best first, at most [`TOP_N`].
///
/// # Errors
/// A broken header, layout or `#DOCS`.
pub fn bm25(file: &[u8], query: &str) -> Result<Vec<Hit>> {
    bm25_in(&parse(file)?, query)
}

/// [`bm25`] on a parsed file.
///
/// # Errors
/// A broken `#DOCS`.
pub fn bm25_in(s: &Sections<'_>, query: &str) -> Result<Vec<Hit>> {
    let docs = s.doc_entries()?;
    let dl: BTreeMap<&str, u64> = docs.iter().map(|d| (d.id, d.dl)).collect();
    #[allow(clippy::cast_precision_loss)]
    let n = docs.len() as f64;
    if docs.is_empty() {
        return Ok(Vec::new());
    }
    #[allow(clippy::cast_precision_loss)]
    let avgdl = (docs.iter().map(|d| d.dl).sum::<u64>() as f64 / n).max(f64::MIN_POSITIVE);
    let wanted = query_terms(query);

    let mut scores: BTreeMap<&str, f64> = BTreeMap::new();
    // in byte order of the entries, as the index lines, so the sums are the same
    for term in &wanted {
        let Some(line) = index_line(s.index, term) else {
            continue;
        };
        let mut f = line.splitn(3, '\t');
        let (Some(_), Some(df), Some(list)) = (f.next(), f.next(), f.next()) else {
            continue;
        };
        #[allow(clippy::cast_precision_loss)]
        let df = parse_decimal(df).unwrap_or(0) as f64;
        let idf = libm::log((n - df + 0.5) / (df + 0.5) + 1.0);
        let Ok(list) = crate::docs::postings(list, &docs, &s.header) else {
            continue;
        };
        for (id, tf) in list {
            #[allow(clippy::cast_precision_loss)]
            let tf = tf as f64;
            #[allow(clippy::cast_precision_loss)]
            let len = dl.get(id).copied().unwrap_or(0) as f64;
            let part = idf * tf * (K1 + 1.0) / (tf + K1 * (1.0 - B + B * len / avgdl));
            *scores.entry(id).or_insert(0.0) += part;
        }
    }
    let mut hits: Vec<Hit> = scores
        .into_iter()
        .map(|(id, score)| Hit {
            id: id.to_owned(),
            score,
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.id.as_bytes().cmp(b.id.as_bytes()))
    });
    hits.truncate(TOP_N);
    Ok(hits)
}

// ---------------------------------------------------------------------------
// Hybrid search (FR-16, FR-10/4)
// ---------------------------------------------------------------------------

/// Reciprocal Rank Fusion constant (FR-16/3).
pub const RRF_K: f64 = 60.0;

/// Where a hit came from (FR-15 `kapu_search`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Only the keyword ranking.
    Bm25,
    /// Only the vector ranking.
    Vec,
    /// Both rankings.
    Both,
}

impl Source {
    /// `bm25`, `vec` or `both`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bm25 => "bm25",
            Self::Vec => "vec",
            Self::Both => "both",
        }
    }
}

/// Whether the vector ranking took part (FR-10/4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VectorStatus {
    /// The reader's model matches the file's `embhash`.
    Available,
    /// No model, or a different one; only BM25 ran.
    Unavailable(String),
}

/// One hybrid search hit.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    /// Door id.
    pub id: String,
    /// Title from `#MAP`.
    pub title: String,
    /// Price in tokens.
    pub tokens: u64,
    /// RRF score (or the BM25 score when vectors are unavailable).
    pub score: f64,
    /// Which ranking found it.
    pub source: Source,
}

/// A search response.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult {
    /// Best first, at most `k`.
    pub hits: Vec<SearchHit>,
    /// Whether vectors were used.
    pub vector: VectorStatus,
}

/// Search options (FR-15 `kapu_search`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchOptions {
    /// Number of hits (default 10).
    pub k: usize,
    /// Doors priced above this are left out of both rankings (D96).
    pub max_tokens: Option<u64>,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            k: 10,
            max_tokens: None,
        }
    }
}

/// Hybrid search (FR-16): BM25 and vector rankings, each its first 50,
/// merged with RRF (`k = 60`), ties by id. Without a matching model only
/// BM25 runs and the result says `vector: unavailable` (FR-10/4).
///
/// # Errors
/// A broken file or a tokenizer failure.
pub fn search(
    file: &[u8],
    query: &str,
    options: SearchOptions,
    cache: Option<&crate::embed::VectorCache<'_>>,
) -> Result<SearchResult> {
    search_in(&parse(file)?, query, options, cache)
}

/// [`search`] on a parsed file.
///
/// # Errors
/// A broken section or a tokenizer failure.
pub fn search_in(
    s: &Sections<'_>,
    query: &str,
    options: SearchOptions,
    cache: Option<&crate::embed::VectorCache<'_>>,
) -> Result<SearchResult> {
    use crate::catalog::{parse_desc, parse_map, unescape_field};

    let map = parse_map(s.map)?;
    let price = |id: &str| map.door(id).map_or(0, |d| d.tokens);
    let allowed = |id: &str| options.max_tokens.is_none_or(|m| price(id) <= m);
    let title = |id: &str| {
        map.door(id)
            .map(|d| unescape_field(d.title))
            .unwrap_or_default()
    };

    let keyword: Vec<Hit> = bm25_in(s, query)?
        .into_iter()
        .filter(|h| allowed(&h.id))
        .collect();

    let vector_status = match cache.map(crate::embed::VectorCache::model) {
        None => VectorStatus::Unavailable("no vector model".to_owned()),
        Some(m) if m.name() != s.header.emb || m.hash() != s.header.embhash => {
            VectorStatus::Unavailable(format!(
                "the file was built with `{}`, the reader has `{}`",
                s.header.emb,
                m.name()
            ))
        }
        Some(_) => VectorStatus::Available,
    };
    let (Some(cache), VectorStatus::Available) = (cache, &vector_status) else {
        let hits = keyword
            .into_iter()
            .take(options.k)
            .map(|h| SearchHit {
                title: title(&h.id),
                tokens: price(&h.id),
                score: h.score,
                source: Source::Bm25,
                id: h.id,
            })
            .collect();
        return Ok(SearchResult {
            hits,
            vector: vector_status,
        });
    };

    let desc = parse_desc(s.desc)?;
    let semantic = vector_ranking(s, &desc, &allowed, cache, query)?;
    let fused = rrf(&keyword, &semantic);
    let mut hits: Vec<SearchHit> = fused
        .into_iter()
        .map(|(id, (score, kw, vec))| SearchHit {
            title: title(&id),
            tokens: price(&id),
            score,
            source: match (kw, vec) {
                (true, true) => Source::Both,
                (true, false) => Source::Bm25,
                _ => Source::Vec,
            },
            id,
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.id.as_bytes().cmp(b.id.as_bytes()))
    });
    hits.truncate(options.k);
    Ok(SearchResult {
        hits,
        vector: VectorStatus::Available,
    })
}

/// The vector ranking (FR-16/2): per allowed door the best cosine of the
/// query vector with one of its window vectors (D141), best first, ties by
/// id, at most [`TOP_N`].
fn vector_ranking(
    s: &Sections<'_>,
    desc: &[crate::catalog::DescLine<'_>],
    allowed: &dyn Fn(&str) -> bool,
    cache: &crate::embed::VectorCache<'_>,
    query: &str,
) -> Result<Vec<(f64, String)>> {
    let q = cache.model().embed(query)?;
    let entries = s.doc_entries()?;
    let vectors = door_vectors(s, &entries, desc, cache)?;
    let mut semantic: Vec<(f64, String)> = entries
        .iter()
        .zip(vectors.iter())
        .filter(|(entry, _)| allowed(entry.id))
        .map(|(entry, windows)| {
            let best = windows
                .iter()
                .map(|v| crate::embed::cosine(&q, v))
                .fold(None, |b: Option<f64>, c| Some(b.map_or(c, |b| b.max(c))));
            (best.unwrap_or(0.0), entry.id.to_owned())
        })
        .collect();
    semantic.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| a.1.as_bytes().cmp(b.1.as_bytes()))
    });
    semantic.truncate(TOP_N);
    Ok(semantic)
}

/// The window vectors of every door in `#DOCS` order (D141): from the cache,
/// or computed from title, description and served text and cached.
fn door_vectors(
    s: &Sections<'_>,
    entries: &[crate::DocEntry<'_>],
    desc: &[crate::catalog::DescLine<'_>],
    cache: &crate::embed::VectorCache<'_>,
) -> Result<crate::embed::Vectors> {
    let texts = || -> Result<Vec<String>> {
        let titles: BTreeMap<&str, String> = crate::catalog::parse_map(s.map)?
            .doors
            .iter()
            .map(|d| (d.id, crate::catalog::unescape_field(d.title)))
            .collect();
        let descriptions: BTreeMap<&str, &str> =
            desc.iter().map(|d| (d.id, d.description)).collect();
        entries
            .iter()
            .map(|entry| {
                let served = s
                    .stored(entry)
                    .map_or_else(|_| Ok(String::new()), crate::docs::served_text)?;
                let description = descriptions.get(entry.id).copied().unwrap_or("");
                let title = titles.get(entry.id).map_or("", String::as_str);
                Ok(crate::embed::door_text(title, description, &served))
            })
            .collect()
    };
    cache.door_vectors(&s.header.bodyhash, entries.len(), texts)
}

/// Computes and caches the window vectors of every door (D141), so the first
/// search after a build does not wait for them.
///
/// # Errors
/// A broken section or a tokenizer failure.
pub fn warm_in(s: &Sections<'_>, cache: &crate::embed::VectorCache<'_>) -> Result<()> {
    let desc = crate::catalog::parse_desc(s.desc)?;
    door_vectors(s, &s.doc_entries()?, &desc, cache).map(|_| ())
}

/// RRF (FR-16/3): `sum 1 / (60 + rank)`, ranks from 1. Per id: score, found
/// by BM25, found by vectors.
fn rrf(keyword: &[Hit], semantic: &[(f64, String)]) -> BTreeMap<String, (f64, bool, bool)> {
    let mut fused: BTreeMap<String, (f64, bool, bool)> = BTreeMap::new();
    for (rank, h) in keyword.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let part = 1.0 / (RRF_K + (rank + 1) as f64);
        let e = fused.entry(h.id.clone()).or_insert((0.0, false, false));
        e.0 += part;
        e.1 = true;
    }
    for (rank, (_, id)) in semantic.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let part = 1.0 / (RRF_K + (rank + 1) as f64);
        let e = fused.entry(id.clone()).or_insert((0.0, false, false));
        e.0 += part;
        e.2 = true;
    }
    fused
}

/// Longest excerpt shown under a search hit, in characters (D124).
pub const EXCERPT_CHARS: usize = 160;

/// D124: for each door id, the line of its served text that shares the most
/// query terms (the first such line on a tie), or its first prose line when
/// no line shares one: one plain line of at most [`EXCERPT_CHARS`]
/// characters, without Markdown markers. An unknown id gets `""`.
///
/// # Errors
/// A broken file, or a door whose fingerprint does not match (E007).
pub fn excerpts(file: &[u8], ids: &[&str], query: &str) -> Result<Vec<String>> {
    excerpts_in(&parse(file)?, ids, query)
}

/// [`excerpts`] on a parsed file.
///
/// # Errors
/// A broken section, or a door whose fingerprint does not match (E007).
pub fn excerpts_in(s: &Sections<'_>, ids: &[&str], query: &str) -> Result<Vec<String>> {
    let entries = s.doc_entries()?;
    let weights = idf_of(s, entries.len(), &query_terms(query));
    ids.iter()
        .map(|id| match entries.iter().find(|e| e.id == *id) {
            Some(entry) => Ok(best_line(
                &crate::docs::served_text(s.stored(entry)?)?,
                &weights,
            )),
            None => Ok(String::new()),
        })
        .collect()
}

/// A39 (D132): the BM25 `idf` of each query entry from `#INDEX`; an entry
/// the index does not know weighs as a word of a single door.
fn idf_of(s: &Sections<'_>, doors: usize, wanted: &BTreeSet<String>) -> BTreeMap<String, f64> {
    #[allow(clippy::cast_precision_loss)]
    let n = doors as f64;
    let idf = |df: f64| libm::log((n - df + 0.5) / (df + 0.5) + 1.0);
    wanted
        .iter()
        .map(|t| {
            let df = index_line(s.index, t).map_or(1, |line| {
                line.split('\t')
                    .nth(1)
                    .map_or(0, |df| parse_decimal(df).unwrap_or(0))
            });
            #[allow(clippy::cast_precision_loss)]
            let df = df as f64;
            (t.clone(), idf(df))
        })
        .collect()
}

/// D124 with D132: the line whose shared query entries weigh the most (a
/// rarer entry weighs more), the first on a tie; without one the first
/// prose line.
fn best_line(text: &str, weights: &BTreeMap<String, f64>) -> String {
    let mut best: Option<(f64, &str)> = None;
    let mut first_prose: Option<&str> = None;
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with("```") || line.starts_with("tags:") {
            continue;
        }
        if first_prose.is_none() && !line.starts_with('#') {
            first_prose = Some(line);
        }
        let weight: f64 = query_terms(line)
            .iter()
            .filter_map(|t| weights.get(t))
            .sum();
        if weight > 0.0 && best.is_none_or(|(w, _)| weight > w + 1e-9) {
            best = Some((weight, line));
        }
    }
    one_line(best.map(|(_, l)| l).or(first_prose).unwrap_or(""))
}

/// Markdown markers off, whitespace collapsed, cut at a word before
/// [`EXCERPT_CHARS`]; `kapu-room` is broken up so an excerpt can never look
/// like a frame (FR-15/2).
fn one_line(line: &str) -> String {
    let text = line
        .trim_start_matches(['#', '>', '-', '*', ' '])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("kapu-room", "kapu room");
    if text.chars().count() <= EXCERPT_CHARS {
        return text;
    }
    let cut: String = text.chars().take(EXCERPT_CHARS - 1).collect();
    let head = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{head}…")
}

/// The three rankings of one query, door ids best first (for `kapu bench`):
/// BM25 (top 50), vectors (top 50, `None` without a matching model) and the
/// RRF fusion (up to 100).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rankings {
    /// Keyword ranking.
    pub bm25: Vec<String>,
    /// Vector ranking, if available.
    pub vector: Option<Vec<String>>,
    /// Fused ranking (equals `bm25` without vectors).
    pub hybrid: Vec<String>,
}

/// All three rankings of `query` (D49 measurement).
///
/// # Errors
/// A broken file or a tokenizer failure.
pub fn rankings(
    file: &[u8],
    query: &str,
    cache: Option<&crate::embed::VectorCache<'_>>,
) -> Result<Rankings> {
    rankings_in(&parse(file)?, query, cache)
}

/// [`rankings`] on a parsed file.
///
/// # Errors
/// A broken section or a tokenizer failure.
pub fn rankings_in(
    s: &Sections<'_>,
    query: &str,
    cache: Option<&crate::embed::VectorCache<'_>>,
) -> Result<Rankings> {
    use crate::catalog::parse_desc;
    let bm25_ids: Vec<String> = bm25_in(s, query)?.into_iter().map(|h| h.id).collect();
    let options = SearchOptions {
        k: 2 * TOP_N,
        max_tokens: None,
    };
    let result = search_in(s, query, options, cache)?;
    let hybrid: Vec<String> = result.hits.iter().map(|h| h.id.clone()).collect();
    let vector = match (cache, &result.vector) {
        (Some(cache), VectorStatus::Available) => {
            let desc = parse_desc(s.desc)?;
            let ranked = vector_ranking(s, &desc, &|_| true, cache, query)?;
            Some(ranked.into_iter().map(|(_, id)| id).collect())
        }
        _ => None,
    };
    Ok(Rankings {
        bm25: bm25_ids,
        vector,
        hybrid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_line_finds_each_term_by_binary_search() {
        let long = format!("b\t1\t{}x:1", "x:1,".repeat(5000));
        let lines = [
            "a\t1\tr:1",
            &long,
            "bb\t2\tr:1,s:1",
            "c\t1\ts:2",
            "gomb\t1\tr:1",
            "gombok\t1\ts:1",
            "új\t1\tr:1",
        ];
        for index in [lines.join("\n"), lines.join("\n") + "\n"] {
            for line in lines {
                let term = line.split('\t').next().unwrap();
                assert_eq!(index_line(&index, term), Some(line), "{term}");
            }
            for missing in ["", "0", "ab", "ba", "d", "gombo", "zz", "ú", "űr"] {
                assert_eq!(index_line(&index, missing), None, "{missing}");
            }
        }
        assert_eq!(index_line("", "a"), None);
        assert_eq!(index_line("a\t1\tr:1\n", "a"), Some("a\t1\tr:1"));
    }
}
