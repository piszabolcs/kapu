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
