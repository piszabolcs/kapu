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
use crate::terms::{index_tokens, stems_of};

/// BM25 `k1` (FR-09/6).
pub const K1: f64 = 1.2;
/// BM25 `b` (FR-09/6).
pub const B: f64 = 0.75;
/// Each ranking keeps its first 50 hits (FR-16/2).
pub const TOP_N: usize = 50;
/// Depth of the rankings `kapu bench` reports (plan v2 M0, M2: recall@100).
pub const BENCH_DEPTH: usize = 100;

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
    query_terms_with(query, Stemming::Both)
}

/// [`query_terms`] with only the stems `stemming` asks for.
#[must_use]
pub fn query_terms_with(query: &str, stemming: Stemming) -> BTreeSet<String> {
    let (hu, en) = stemming.stemmers();
    index_tokens(query)
        .iter()
        .flat_map(|t| stems_of(t, hu, en))
        .collect()
}

// ---------------------------------------------------------------------------
// Ranking switches (plan v2 M0)
// ---------------------------------------------------------------------------

/// Which forms of a query word are looked up. The index keeps the word and
/// both stems (FR-09/3), so leaving a stemmer out at query time measures an
/// index without it (M0 stemming axis, J14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stemming {
    /// The word, its Hungarian and its English stem (FR-09/3).
    #[default]
    Both,
    /// Only the word.
    Surface,
    /// The word and its English stem.
    English,
    /// The word and its Hungarian stem.
    Hungarian,
}

impl Stemming {
    /// (Hungarian, English) stemmer on.
    #[must_use]
    pub const fn stemmers(self) -> (bool, bool) {
        match self {
            Self::Both => (true, true),
            Self::Surface => (false, false),
            Self::English => (false, true),
            Self::Hungarian => (true, false),
        }
    }

    /// `both`, `surface`, `en` or `hu`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::Surface => "surface",
            Self::English => "en",
            Self::Hungarian => "hu",
        }
    }
}

/// How the BM25 and the vector ranking are merged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fusion {
    /// Reciprocal Rank Fusion, `w / (k + rank)` (FR-16/3).
    #[default]
    Rrf,
    /// Min-max normalised scores, `w_bm25 * bm25 + w_vec * cosine` (J4d).
    Convex,
}

/// The ranking switches of plan v2. The default is the ranking of 0.2.1,
/// bit for bit (rule 5); `kapu bench --config` sets them for the ablation
/// (M0).
#[derive(Debug, Clone, PartialEq)]
pub struct RankOptions {
    /// Stems looked up for each query word.
    pub stemming: Stemming,
    /// J2a: rooms and sub-doors are normalised by the average length of
    /// their own kind of door instead of one average.
    pub split_avgdl: bool,
    /// J2b: a sub-door's BM25 score gets this share of its room's score
    /// (0 = off).
    pub parent: f64,
    /// J4a: the first this many BM25 hits always get a float vector score,
    /// besides the candidates of the sign bits (0 = off).
    pub rescore_bm25: usize,
    /// The fusion.
    pub fusion: Fusion,
    /// J4b: weight of the BM25 ranking.
    pub w_bm25: f64,
    /// J4b: weight of the vector ranking.
    pub w_vec: f64,
    /// J4b: the RRF constant (60 in FR-16/3).
    pub rrf_k: f64,
    /// J4c: the BM25 weight is multiplied by this for a query with a code
    /// word, the vector weight for a query with mostly unknown words
    /// (1 = off).
    pub adaptive: f64,
    /// J1: at most this many doors of one room among the hits, the others
    /// listed under the room's first hit (0 = off).
    pub collapse: usize,
    /// J1 with `collapse = 1`: a room scores its best door plus this share
    /// of its other matching doors (0 = the best door alone).
    pub room_beta: f64,
    /// J10: sub-doors priced below this many tokens are no candidates of
    /// their own; their text is in their room (0 = off).
    pub min_door_tokens: u64,
    /// How many hits of each ranking the fusion takes (50 in FR-16/2).
    pub depth: usize,
}

impl Default for RankOptions {
    fn default() -> Self {
        Self {
            stemming: Stemming::Both,
            split_avgdl: false,
            parent: 0.0,
            rescore_bm25: 0,
            fusion: Fusion::Rrf,
            w_bm25: 1.0,
            w_vec: 1.0,
            rrf_k: RRF_K,
            adaptive: 1.0,
            collapse: 0,
            room_beta: 0.0,
            min_door_tokens: 0,
            depth: TOP_N,
        }
    }
}

impl RankOptions {
    /// From one configuration object of `kapu bench --config` (M0). Every
    /// key is optional; an unknown key or a wrong value is an error, so a
    /// typo cannot pass as the default. `name` is the caller's.
    ///
    /// # Errors
    /// E012 naming the key.
    pub fn from_json(v: &serde_json::Value) -> Result<Self> {
        let bad = |key: &str| {
            crate::Error::new(
                crate::ErrorCode::E012,
                format!("ranking option `{key}`: unknown key or wrong value"),
            )
        };
        let obj = v.as_object().ok_or_else(|| bad("(not an object)"))?;
        let mut r = Self::default();
        for (key, value) in obj {
            let num = || value.as_f64().filter(|x| x.is_finite() && *x >= 0.0);
            let count = || value.as_u64();
            let ok = match key.as_str() {
                "name" => value.is_string(),
                "stemming" => match value.as_str() {
                    Some("both") => Some(Stemming::Both),
                    Some("surface") => Some(Stemming::Surface),
                    Some("en") => Some(Stemming::English),
                    Some("hu") => Some(Stemming::Hungarian),
                    _ => None,
                }
                .map(|x| r.stemming = x)
                .is_some(),
                "fusion" => match value.as_str() {
                    Some("rrf") => Some(Fusion::Rrf),
                    Some("convex") => Some(Fusion::Convex),
                    _ => None,
                }
                .map(|x| r.fusion = x)
                .is_some(),
                "split_avgdl" => value.as_bool().map(|x| r.split_avgdl = x).is_some(),
                "parent" => num().map(|x| r.parent = x).is_some(),
                "w_bm25" => num().map(|x| r.w_bm25 = x).is_some(),
                "w_vec" => num().map(|x| r.w_vec = x).is_some(),
                "rrf_k" => num().filter(|x| *x > 0.0).map(|x| r.rrf_k = x).is_some(),
                "adaptive" => num()
                    .filter(|x| *x >= 1.0)
                    .map(|x| r.adaptive = x)
                    .is_some(),
                "room_beta" => num().map(|x| r.room_beta = x).is_some(),
                "rescore_bm25" => count()
                    .and_then(|x| usize::try_from(x).ok())
                    .map(|x| r.rescore_bm25 = x)
                    .is_some(),
                "collapse" => count()
                    .and_then(|x| usize::try_from(x).ok())
                    .map(|x| r.collapse = x)
                    .is_some(),
                "depth" => count()
                    .and_then(|x| usize::try_from(x).ok())
                    .filter(|x| *x > 0)
                    .map(|x| r.depth = x)
                    .is_some(),
                "min_door_tokens" => count().map(|x| r.min_door_tokens = x).is_some(),
                _ => false,
            };
            if !ok {
                return Err(bad(key));
            }
        }
        Ok(r)
    }

    /// Every switch as a JSON object (the metadata of a bench run).
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "stemming": self.stemming.as_str(),
            "split_avgdl": self.split_avgdl,
            "parent": self.parent,
            "rescore_bm25": self.rescore_bm25,
            "fusion": match self.fusion { Fusion::Rrf => "rrf", Fusion::Convex => "convex" },
            "w_bm25": self.w_bm25,
            "w_vec": self.w_vec,
            "rrf_k": self.rrf_k,
            "adaptive": self.adaptive,
            "collapse": self.collapse,
            "room_beta": self.room_beta,
            "min_door_tokens": self.min_door_tokens,
            "depth": self.depth,
        })
    }
}

/// The room of a door id: the part before the first `/`.
#[must_use]
pub fn room_of(id: &str) -> &str {
    id.split('/').next().unwrap_or(id)
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
    bm25_where(s, query, &|_| true, &RankOptions::default(), TOP_N)
}

/// Mean `dl` of `lens`, at least the smallest positive number.
fn mean_length(lens: impl Iterator<Item = u64>) -> f64 {
    let (sum, count) = lens.fold((0u64, 0u64), |(s, c), l| (s + l, c + 1));
    #[allow(clippy::cast_precision_loss)]
    let mean = sum as f64 / count.max(1) as f64;
    mean.max(f64::MIN_POSITIVE)
}

/// [`bm25_in`] over the doors `allowed` keeps: the filter runs before the
/// first `depth` are taken (D96, D144).
fn bm25_where(
    s: &Sections<'_>,
    query: &str,
    allowed: &dyn Fn(&str) -> bool,
    rank: &RankOptions,
    depth: usize,
) -> Result<Vec<Hit>> {
    let docs = s.doc_entries()?;
    let dl: BTreeMap<&str, u64> = docs.iter().map(|d| (d.id, d.dl)).collect();
    #[allow(clippy::cast_precision_loss)]
    let n = docs.len() as f64;
    if docs.is_empty() {
        return Ok(Vec::new());
    }
    #[allow(clippy::cast_precision_loss)]
    let avgdl = (docs.iter().map(|d| d.dl).sum::<u64>() as f64 / n).max(f64::MIN_POSITIVE);
    // J2a: a room's text holds its sections again, so one mean mixes two kinds
    let (room_avgdl, sub_avgdl) = if rank.split_avgdl {
        let is_sub = |d: &&crate::DocEntry<'_>| d.id.contains('/');
        let subs = mean_length(docs.iter().filter(is_sub).map(|d| d.dl));
        let rooms = mean_length(docs.iter().filter(|d| !is_sub(d)).map(|d| d.dl));
        (rooms, subs)
    } else {
        (avgdl, avgdl)
    };
    let wanted = query_terms_with(query, rank.stemming);

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
            let avg = if id.contains('/') {
                sub_avgdl
            } else {
                room_avgdl
            };
            let part = idf * tf * (K1 + 1.0) / (tf + K1 * (1.0 - B + B * len / avg));
            *scores.entry(id).or_insert(0.0) += part;
        }
    }
    // J2b: a section scores a share of its room, which it belongs to
    if rank.parent > 0.0 {
        let rooms: BTreeMap<&str, f64> = scores
            .iter()
            .filter(|(id, _)| !id.contains('/'))
            .map(|(id, s)| (*id, *s))
            .collect();
        for (id, score) in &mut scores {
            if id.contains('/')
                && let Some(room) = rooms.get(room_of(id))
            {
                *score += rank.parent * room;
            }
        }
    }
    let mut hits: Vec<Hit> = scores
        .into_iter()
        .filter(|(id, _)| allowed(id))
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
    hits.truncate(depth);
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
    /// J1: the other matching doors of the same room, best first (empty
    /// unless `collapse` is on).
    pub also: Vec<String>,
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
#[derive(Debug, Clone, PartialEq)]
pub struct SearchOptions {
    /// Number of hits (default 10).
    pub k: usize,
    /// Doors priced above this are left out of both rankings (D96; called
    /// `max_tokens` before v1.2, D145).
    pub max_price: Option<u64>,
    /// Only doors of rooms of this kind, sub-doors by their room (D144).
    pub kind: Option<String>,
    /// The ranking switches (plan v2 M0).
    pub rank: RankOptions,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            k: 10,
            max_price: None,
            kind: None,
            rank: RankOptions::default(),
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
    options: &SearchOptions,
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
    options: &SearchOptions,
    cache: Option<&crate::embed::VectorCache<'_>>,
) -> Result<SearchResult> {
    let ranked = ranked_in(s, query, options, cache, options.k)?;
    Ok(SearchResult {
        hits: ranked.hits,
        vector: ranked.status,
    })
}

/// The rankings behind one search (M0): BM25 to at least [`BENCH_DEPTH`],
/// every door the vectors scored, and the fused hits, at most `limit`.
struct Ranked {
    bm25: Vec<Hit>,
    vector: Option<Vec<(f64, String)>>,
    hits: Vec<SearchHit>,
    status: VectorStatus,
}

fn ranked_in(
    s: &Sections<'_>,
    query: &str,
    options: &SearchOptions,
    cache: Option<&crate::embed::VectorCache<'_>>,
    limit: usize,
) -> Result<Ranked> {
    use crate::catalog::{parse_desc, parse_map, unescape_field};

    let rank = &options.rank;
    let map = parse_map(s.map)?;
    let price = |id: &str| map.door(id).map_or(0, |d| d.tokens);
    // the filters look at every door of the vector ranking: one lookup table
    let filtered =
        options.max_price.is_some() || options.kind.is_some() || rank.min_door_tokens > 0;
    let doors: BTreeMap<&str, (u64, &str)> = if filtered {
        map.doors
            .iter()
            .map(|d| (d.id, (d.tokens, d.kind)))
            .collect()
    } else {
        BTreeMap::new()
    };
    let allowed = |id: &str| {
        let room = room_of(id);
        options
            .max_price
            .is_none_or(|m| doors.get(id).is_some_and(|d| d.0 <= m))
            && options
                .kind
                .as_deref()
                .is_none_or(|k| doors.get(room).is_some_and(|d| d.1 == k))
            // J10: a short section is found through its room
            && (rank.min_door_tokens == 0
                || !id.contains('/')
                || doors.get(id).is_some_and(|d| d.0 >= rank.min_door_tokens))
    };
    let title = |id: &str| {
        map.door(id)
            .map(|d| unescape_field(d.title))
            .unwrap_or_default()
    };

    let keyword: Vec<Hit> = bm25_where(s, query, &allowed, rank, rank.depth.max(BENCH_DEPTH))?;

    let vector_status = vector_status(s, cache);
    let (Some(cache), VectorStatus::Available) = (cache, &vector_status) else {
        let hits = keyword
            .iter()
            .map(|h| SearchHit {
                title: title(&h.id),
                tokens: price(&h.id),
                score: h.score,
                source: Source::Bm25,
                id: h.id.clone(),
                also: Vec::new(),
            })
            .collect();
        return Ok(Ranked {
            bm25: keyword,
            vector: None,
            hits: collapse(hits, rank, limit),
            status: vector_status,
        });
    };

    let desc = parse_desc(s.desc)?;
    let extra: Vec<&str> = keyword
        .iter()
        .take(rank.rescore_bm25)
        .map(|h| h.id.as_str())
        .collect();
    let semantic = vector_ranking(s, &desc, &allowed, cache, query, &extra)?;
    let (w_bm25, w_vec) = query_weights(s, query, rank);
    let kw = keyword.get(..rank.depth).unwrap_or(&keyword);
    let sem = semantic.get(..rank.depth).unwrap_or(&semantic);
    let fused = match rank.fusion {
        Fusion::Rrf => rrf(kw, sem, w_bm25, w_vec, rank.rrf_k),
        Fusion::Convex => convex(kw, sem, w_bm25, w_vec),
    };
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
            also: Vec::new(),
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.id.as_bytes().cmp(b.id.as_bytes()))
    });
    Ok(Ranked {
        bm25: keyword,
        vector: Some(semantic),
        hits: collapse(hits, rank, limit),
        status: VectorStatus::Available,
    })
}

/// FR-10/4: whether the reader's model matches the file's `embhash`.
fn vector_status(s: &Sections<'_>, cache: Option<&crate::embed::VectorCache<'_>>) -> VectorStatus {
    match cache.map(crate::embed::VectorCache::model) {
        None => VectorStatus::Unavailable("no vector model".to_owned()),
        Some(m) if m.name() != s.header.emb || m.hash() != s.header.embhash => {
            VectorStatus::Unavailable(format!(
                "the file was built with `{}`, the reader has `{}`",
                s.header.emb,
                m.name()
            ))
        }
        Some(_) => VectorStatus::Available,
    }
}

/// J4c: the weights of the two rankings for this query. A code word
/// (`snake_case`, `camelCase`, `a.b`, `f()`, `a::b`) favours BM25; a query whose
/// words are mostly not in `#INDEX` (another language) favours the vectors.
fn query_weights(s: &Sections<'_>, query: &str, rank: &RankOptions) -> (f64, f64) {
    let (mut w_bm25, mut w_vec) = (rank.w_bm25, rank.w_vec);
    if rank.adaptive > 1.0 {
        if query.split_whitespace().any(is_code_word) {
            w_bm25 *= rank.adaptive;
        }
        let words = index_tokens(query);
        let unknown = words
            .iter()
            .filter(|w| index_line(s.index, w).is_none())
            .count();
        if !words.is_empty() && 2 * unknown >= words.len() {
            w_vec *= rank.adaptive;
        }
    }
    (w_bm25, w_vec)
}

/// J4c: a word that looks like code.
fn is_code_word(word: &str) -> bool {
    let chars: Vec<char> = word.chars().collect();
    let between = |c: char| {
        chars.windows(3).any(
            |w| matches!(w, [a, m, b] if *m == c && a.is_alphanumeric() && b.is_alphanumeric()),
        )
    };
    let camel = chars
        .windows(2)
        .any(|w| matches!(w, [a, b] if a.is_lowercase() && b.is_uppercase()));
    between('_') || between('.') || camel || word.contains("()") || word.contains("::")
}

/// J1: at most `rank.collapse` doors per room. With 1, a room is one hit, its
/// best door, and its other doors go to `also`; with `room_beta` the rooms
/// are ordered by their best door plus that share of the others. With more,
/// a room's doors after the first `collapse` go to the `also` of its first.
fn collapse(hits: Vec<SearchHit>, rank: &RankOptions, limit: usize) -> Vec<SearchHit> {
    if rank.collapse == 0 {
        let mut hits = hits;
        hits.truncate(limit);
        return hits;
    }
    let mut out: Vec<SearchHit> = Vec::new();
    // room -> (index in `out` of its first hit, doors kept, sum of the rest)
    let mut rooms: BTreeMap<String, (usize, usize, f64)> = BTreeMap::new();
    for hit in hits {
        let room = room_of(&hit.id).to_owned();
        match rooms.get_mut(&room) {
            Some((_, kept, _)) if *kept < rank.collapse => {
                *kept += 1;
                out.push(hit);
            }
            Some((first, _, rest)) => {
                *rest += hit.score;
                if let Some(head) = out.get_mut(*first) {
                    head.also.push(hit.id);
                }
            }
            None => {
                rooms.insert(room, (out.len(), 1, 0.0));
                out.push(hit);
            }
        }
    }
    if rank.collapse == 1 && rank.room_beta > 0.0 {
        for (first, _, rest) in rooms.values() {
            if let Some(head) = out.get_mut(*first) {
                head.score += rank.room_beta * rest;
            }
        }
        // stable: a tie keeps the order of the best doors
        out.sort_by(|a, b| b.score.total_cmp(&a.score));
    }
    out.truncate(limit);
    out
}

/// The vector ranking (FR-16/2): per allowed door the best cosine of the
/// query vector with one of its window vectors (D141), best first, ties by
/// id, at most [`RESCORE`] plus `extra` (the fusion takes the first
/// `depth`). The windows come from the file's `#VEC` section when it has one
/// (D143), otherwise from the vector cache. J4a: the `extra` doors (the first
/// BM25 hits) get a float score even if the sign bits did not pick them.
fn vector_ranking(
    s: &Sections<'_>,
    desc: &[crate::catalog::DescLine<'_>],
    allowed: &dyn Fn(&str) -> bool,
    cache: &crate::embed::VectorCache<'_>,
    query: &str,
    extra: &[&str],
) -> Result<Vec<(f64, String)>> {
    let q = cache.model().embed(query)?;
    let entries = s.doc_entries()?;
    let best = |windows: &[Vec<f32>]| {
        windows
            .iter()
            .map(|v| crate::embed::cosine(&q, v))
            .fold(None, |b: Option<f64>, c| Some(b.map_or(c, |b| b.max(c))))
            .unwrap_or(0.0)
    };
    let mut semantic: Vec<(f64, String)> = if let Some(text) = s.vec {
        let signs = crate::embed::file_vectors(text, entries.len())?;
        if signs.dim() != q.len() {
            return Err(crate::Error::new(
                crate::ErrorCode::E005,
                format!("#VEC has {} components, the model {}", signs.dim(), q.len()),
            ));
        }
        // D143: the sign bits pick the candidates, the model's float windows
        // of the best RESCORE allowed doors give their order
        let mut coarse: Vec<(f64, &crate::DocEntry<'_>)> = entries
            .iter()
            .zip(signs.best_scores(&q))
            .filter(|(entry, _)| allowed(entry.id))
            .map(|(entry, score)| (score, entry))
            .collect();
        coarse.sort_by(|a, b| {
            b.0.total_cmp(&a.0)
                .then_with(|| a.1.id.as_bytes().cmp(b.1.id.as_bytes()))
        });
        coarse.truncate(RESCORE);
        if !extra.is_empty() {
            let have: BTreeSet<&str> = coarse.iter().map(|(_, e)| e.id).collect();
            for id in extra {
                // #DOCS is in byte order of the ids (FR-09/4)
                let at = entries.binary_search_by(|e| e.id.as_bytes().cmp(id.as_bytes()));
                if let Some(entry) = at.ok().and_then(|i| entries.get(i))
                    && !have.contains(id)
                {
                    coarse.push((0.0, entry));
                }
            }
        }
        let picked: Vec<crate::DocEntry<'_>> = coarse.iter().map(|(_, e)| **e).collect();
        let windows =
            crate::embed::rescore_windows(cache.model(), &picked, || door_texts(s, &picked, desc))?;
        picked
            .iter()
            .zip(windows.iter())
            .map(|(entry, w)| (best(w), entry.id.to_owned()))
            .collect()
    } else {
        entries
            .iter()
            .zip(door_vectors(s, &entries, desc, cache)?.iter())
            .filter(|(entry, _)| allowed(entry.id))
            .map(|(entry, w)| (best(w), entry.id.to_owned()))
            .collect()
    };
    semantic.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| a.1.as_bytes().cmp(b.1.as_bytes()))
    });
    semantic.truncate(RESCORE + extra.len());
    Ok(semantic)
}

/// D143: how many doors the sign bits of `#VEC` hand to the float rescoring.
pub const RESCORE: usize = 200;

/// The vector texts (FR-10/2) of `entries`: title, description and served text.
fn door_texts(
    s: &Sections<'_>,
    entries: &[crate::DocEntry<'_>],
    desc: &[crate::catalog::DescLine<'_>],
) -> Result<Vec<String>> {
    let titles: BTreeMap<&str, String> = crate::catalog::parse_map(s.map)?
        .doors
        .iter()
        .map(|d| (d.id, crate::catalog::unescape_field(d.title)))
        .collect();
    let descriptions: BTreeMap<&str, &str> = desc.iter().map(|d| (d.id, d.description)).collect();
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
}

/// The window vectors of every door in `#DOCS` order (D141): from the cache,
/// or computed from title, description and served text and cached.
fn door_vectors(
    s: &Sections<'_>,
    entries: &[crate::DocEntry<'_>],
    desc: &[crate::catalog::DescLine<'_>],
    cache: &crate::embed::VectorCache<'_>,
) -> Result<crate::embed::Vectors> {
    cache.door_vectors(&s.header.bodyhash, entries.len(), || {
        door_texts(s, entries, desc)
    })
}

/// Computes and caches the window vectors of every door (D141), so the first
/// search after a build does not wait for them. A file with `#VEC` needs
/// nothing (D143).
///
/// # Errors
/// A broken section or a tokenizer failure.
pub fn warm_in(s: &Sections<'_>, cache: &crate::embed::VectorCache<'_>) -> Result<()> {
    if s.vec.is_some() {
        return Ok(());
    }
    let desc = crate::catalog::parse_desc(s.desc)?;
    door_vectors(s, &s.doc_entries()?, &desc, cache).map(|_| ())
}

/// RRF (FR-16/3): `sum w / (k + rank)`, ranks from 1; FR-16/3 is `w = 1`,
/// `k = 60` (J4b weighs them). Per id: score, found by BM25, found by
/// vectors.
fn rrf(
    keyword: &[Hit],
    semantic: &[(f64, String)],
    w_bm25: f64,
    w_vec: f64,
    k: f64,
) -> BTreeMap<String, (f64, bool, bool)> {
    let mut fused: BTreeMap<String, (f64, bool, bool)> = BTreeMap::new();
    for (rank, h) in keyword.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let part = w_bm25 / (k + (rank + 1) as f64);
        let e = fused.entry(h.id.clone()).or_insert((0.0, false, false));
        e.0 += part;
        e.1 = true;
    }
    for (rank, (_, id)) in semantic.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let part = w_vec / (k + (rank + 1) as f64);
        let e = fused.entry(id.clone()).or_insert((0.0, false, false));
        e.0 += part;
        e.2 = true;
    }
    fused
}

/// J4d: each ranking's scores min-max normalised to 0..1 (all equal: 1),
/// then `w_bm25 * bm25 + w_vec * cosine`, a missing score counting 0.
fn convex(
    keyword: &[Hit],
    semantic: &[(f64, String)],
    w_bm25: f64,
    w_vec: f64,
) -> BTreeMap<String, (f64, bool, bool)> {
    fn normalised(scores: impl Iterator<Item = f64> + Clone) -> impl Fn(f64) -> f64 {
        let lo = scores.clone().fold(f64::INFINITY, f64::min);
        let hi = scores.fold(f64::NEG_INFINITY, f64::max);
        move |x| if hi > lo { (x - lo) / (hi - lo) } else { 1.0 }
    }
    let kw = normalised(keyword.iter().map(|h| h.score));
    let vec = normalised(semantic.iter().map(|(c, _)| *c));
    let mut fused: BTreeMap<String, (f64, bool, bool)> = BTreeMap::new();
    for h in keyword {
        let e = fused.entry(h.id.clone()).or_insert((0.0, false, false));
        e.0 += w_bm25 * kw(h.score);
        e.1 = true;
    }
    for (cosine, id) in semantic {
        let e = fused.entry(id.clone()).or_insert((0.0, false, false));
        e.0 += w_vec * vec(*cosine);
        e.2 = true;
    }
    fused
}

/// Longest excerpt shown under a search hit, in characters (D124; 160 before
/// v1.2, K-05, D145).
pub const EXCERPT_CHARS: usize = 100;

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
    excerpts_lines_in(s, ids, query, 1)
}

/// Longest two-line excerpt, in characters (J11).
pub const EXCERPT2_CHARS: usize = 160;

/// [`excerpts_in`] with `lines = 2` (J11): the best line and the line next to
/// it that adds the most query weight, or, when the best line is code or a
/// table row, the prose line before it; joined by ` / `, at most
/// [`EXCERPT2_CHARS`] characters. `lines = 1` is [`excerpts_in`].
///
/// # Errors
/// A broken section, or a door whose fingerprint does not match (E007).
pub fn excerpts_lines_in(
    s: &Sections<'_>,
    ids: &[&str],
    query: &str,
    lines: usize,
) -> Result<Vec<String>> {
    let entries = s.doc_entries()?;
    let weights = idf_of(s, entries.len(), &query_terms(query));
    ids.iter()
        .map(|id| match entries.iter().find(|e| e.id == *id) {
            Some(entry) => {
                let text = crate::docs::served_text(s.stored(entry)?)?;
                Ok(if lines >= 2 {
                    best_two_lines(&text, &weights)
                } else {
                    best_line(&text, &weights)
                })
            }
            None => Ok(String::new()),
        })
        .collect()
}

/// One line of a door's text for J11: trimmed, whether it is code (inside a
/// fence) or a table row, and its query entries.
struct ExcerptLine<'a> {
    text: &'a str,
    code: bool,
    terms: BTreeSet<String>,
}

/// J11: see [`excerpts_lines_in`].
fn best_two_lines(text: &str, weights: &BTreeMap<String, f64>) -> String {
    let mut lines: Vec<ExcerptLine<'_>> = Vec::new();
    let mut fence = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with("```") || line.starts_with("~~~") {
            fence = !fence;
            continue;
        }
        if line.is_empty() || line.starts_with("tags:") {
            continue;
        }
        lines.push(ExcerptLine {
            text: line,
            code: fence || line.starts_with('|'),
            terms: query_terms(line),
        });
    }
    let weight =
        |terms: &BTreeSet<String>| -> f64 { terms.iter().filter_map(|t| weights.get(t)).sum() };
    let mut best: Option<(f64, usize)> = None;
    for (i, line) in lines.iter().enumerate() {
        let w = weight(&line.terms);
        if w > 0.0 && best.is_none_or(|(b, _)| w > b + 1e-9) {
            best = Some((w, i));
        }
    }
    let Some((_, at)) = best else {
        let first = lines
            .iter()
            .find(|l| !l.code && !l.text.starts_with('#'))
            .map_or("", |l| l.text);
        return cut(&plain(first), EXCERPT2_CHARS);
    };
    let Some(anchor) = lines.get(at) else {
        return String::new();
    };
    let pair = if anchor.code {
        // code or a table: the prose line before it says what it is
        lines
            .get(..at)
            .and_then(|before| before.iter().rposition(|l| !l.code))
            .map(|p| (p, at))
    } else {
        let gain = |other: usize| {
            lines.get(other).map(|l| {
                let both: BTreeSet<String> = l.terms.union(&anchor.terms).cloned().collect();
                weight(&both)
            })
        };
        match (at.checked_sub(1).and_then(gain), gain(at + 1)) {
            (Some(before), Some(after)) if before > after + 1e-9 => Some((at - 1, at)),
            (_, Some(_)) => Some((at, at + 1)),
            (Some(_), None) => Some((at - 1, at)),
            (None, None) => None,
        }
    };
    let text = match pair.and_then(|(a, b)| Some((lines.get(a)?, lines.get(b)?))) {
        Some((a, b)) => format!("{} / {}", plain(a.text), plain(b.text)),
        None => plain(anchor.text),
    };
    cut(&text, EXCERPT2_CHARS)
}

/// Markdown markers off, whitespace collapsed, `kapu-room` broken up.
fn plain(line: &str) -> String {
    line.trim_start_matches(['#', '>', '-', '*', ' '])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("kapu-room", "kapu room")
}

/// At most `max` characters, cut at a word with `…`.
fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let head: String = text.chars().take(max - 1).collect();
    let head = head.rsplit_once(' ').map_or(head.as_str(), |(h, _)| h);
    format!("{head}…")
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
    cut(&plain(line), EXCERPT_CHARS)
}

/// The three rankings of one query, door ids best first (for `kapu bench`):
/// BM25 (top [`BENCH_DEPTH`]), vectors (every door with a float score, at
/// least the top 50; `None` without a matching model) and the fusion (up to
/// [`BENCH_DEPTH`], one door per room with `collapse = 1`).
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
    rankings_with(s, query, &RankOptions::default(), cache)
}

/// [`rankings_in`] with the ranking switches of `rank` (M0).
///
/// # Errors
/// A broken section or a tokenizer failure.
pub fn rankings_with(
    s: &Sections<'_>,
    query: &str,
    rank: &RankOptions,
    cache: Option<&crate::embed::VectorCache<'_>>,
) -> Result<Rankings> {
    let options = SearchOptions {
        k: BENCH_DEPTH,
        rank: rank.clone(),
        ..SearchOptions::default()
    };
    let r = ranked_in(s, query, &options, cache, BENCH_DEPTH)?;
    Ok(Rankings {
        bm25: r.bm25.into_iter().map(|h| h.id).collect(),
        vector: r.vector.map(|v| v.into_iter().map(|(_, id)| id).collect()),
        hybrid: r.hits.into_iter().map(|h| h.id).collect(),
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
