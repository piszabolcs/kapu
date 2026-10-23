//! Keyword search with BM25 over the `#INDEX` section (FR-09/6, FR-16/1-2).
//!
//! The query goes through the same tokenizer and double stemming as the
//! index (D61). Each distinct stemmed entry counts once (D85). Scores use
//! `k1 = 1.2`, `b = 0.75`, the `dl` of `#DOCS` and
//! `idf = ln((N - df + 0.5) / (df + 0.5) + 1)` with `N` the number of doors.
//! Ties are broken by door id (FR-16/4).

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

/// BM25 ranking of all doors for `query`, best first, at most [`TOP_N`].
///
/// # Errors
/// A broken header, layout or `#DOCS`.
pub fn bm25(file: &[u8], query: &str) -> Result<Vec<Hit>> {
    let s = Sections::parse_with(file, &Limits::default(), BodyLength::Tolerate)?;
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
    for line in s.index.lines() {
        let mut f = line.splitn(3, '\t');
        let (Some(term), Some(df), Some(postings)) = (f.next(), f.next(), f.next()) else {
            continue;
        };
        if !wanted.contains(term) {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let df = parse_decimal(df).unwrap_or(0) as f64;
        let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
        for posting in postings.split(',') {
            let Some((id, tf)) = posting.rsplit_once(':') else {
                continue;
            };
            #[allow(clippy::cast_precision_loss)]
            let tf = parse_decimal(tf).unwrap_or(0) as f64;
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
    use crate::catalog::{parse_desc, parse_map, unescape_field};

    let s = Sections::parse_with(file, &Limits::default(), BodyLength::Tolerate)?;
    let map = parse_map(s.map)?;
    let price = |id: &str| map.door(id).map_or(0, |d| d.tokens);
    let allowed = |id: &str| options.max_tokens.is_none_or(|m| price(id) <= m);
    let title = |id: &str| {
        map.door(id)
            .map(|d| unescape_field(d.title))
            .unwrap_or_default()
    };

    let keyword: Vec<Hit> = bm25(file, query)?
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
    let semantic = vector_ranking(&s, &desc, &title, &allowed, cache, query)?;
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

/// The vector ranking (FR-16/2): cosine of the query vector with every
/// allowed door's vector, best first, ties by id, at most [`TOP_N`].
fn vector_ranking(
    s: &Sections<'_>,
    desc: &[crate::catalog::DescLine<'_>],
    title: &dyn Fn(&str) -> String,
    allowed: &dyn Fn(&str) -> bool,
    cache: &crate::embed::VectorCache<'_>,
    query: &str,
) -> Result<Vec<(f64, String)>> {
    let q = cache.model().embed(query)?;
    let entries = s.doc_entries()?;
    let texts = || -> Result<Vec<String>> {
        entries
            .iter()
            .map(|entry| {
                let served = s
                    .stored(entry)
                    .map_or_else(|_| Ok(String::new()), crate::docs::served_text)?;
                let description = desc
                    .iter()
                    .find(|d| d.id == entry.id)
                    .map_or("", |d| d.description);
                Ok(crate::embed::door_text(
                    &title(entry.id),
                    description,
                    &served,
                ))
            })
            .collect()
    };
    let vectors = cache.door_vectors(&s.header.bodyhash, entries.len(), texts)?;
    let mut semantic: Vec<(f64, String)> = entries
        .iter()
        .zip(&vectors)
        .filter(|(entry, _)| allowed(entry.id))
        .map(|(entry, v)| (crate::embed::cosine(&q, v), entry.id.to_owned()))
        .collect();
    semantic.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| a.1.as_bytes().cmp(b.1.as_bytes()))
    });
    semantic.truncate(TOP_N);
    Ok(semantic)
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
