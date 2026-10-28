//! The builder (FR-04..FR-14, FR-18): body in, complete `.kapu` file out.
//!
//! The body is the only source of truth. [`build`] takes the rooms (id and
//! unescaped Markdown, in body order) and derives every other section and the
//! header, byte for byte like `tools/build_vectors.py` (D56) when no model is
//! used (D57: no `sim` edges, no `#VEC`, `emb=none`). With a model,
//! [`build_with`] adds the `sim` edges and the `#VEC` section (D143).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use crate::docs::{blake3_of, strip_comments, unescape};
use crate::embed::{Model, cosine, door_text, sign_line, vec_params};
use crate::error::{Error, ErrorCode, Result};
use crate::header::Hash;
use crate::leiden::leiden;
use crate::terms::{index_tokens, stems, tokens};
use crate::text::{
    KINDS, TITLE_MAX, code_fence_mask, escape, find_links, heading_title, indexed_text,
    manual_description, manual_kind, manual_tags, nfc, paragraphs, slug, take_chars,
    without_kind_line,
};
use crate::validate::is_valid_id;
use crate::{FORMAT_VERSION, HEADER_LEN, yake};

/// `sim` edges: nearest neighbours per room (FR-11/3, ASSUMPTION-06).
pub const SIM_K: usize = 5;
/// `sim` edges: minimum cosine similarity (FR-11/3, ASSUMPTION-06).
pub const SIM_THRESHOLD: f64 = 0.6;

/// Hard limit of a description in tokens (FR-06/2).
pub const DESCRIPTION_MAX_TOKENS: u64 = 300;

/// Model name written when the build runs without a vector model (D57).
pub const NO_MODEL: &str = "none";

/// FR-05: the content `kapu new` and a new empty room start with.
pub const TEMPLATE: &str = "# \n\
<!-- Ide jön a szoba címe. Ha üres marad, a szövegből lesz. -->\n\
\n\
> \n\
<!-- Rövid leírás: mi van ebben a szobában. Ha üres marad, a szövegből lesz. -->\n\
\n\
tags: \n\
<!-- Keresőszavak vesszővel. Ha üres marad, a szövegből lesznek. -->\n\
\n\
kind: \n\
<!-- Fajta: skill, project, pattern vagy note. Üresen is maradhat. -->\n\
\n\
<!-- Innen jöhet a tartalom, sima Markdownban. -->\n";

/// FR-05/2 (D144): the project card of `kapu new --kind project` and
/// `kapu template project`: the caption lines, `kind: project` and the seven
/// sections of a card (each a sub-door).
pub const PROJECT_TEMPLATE: &str = "# \n\
<!-- A projekt neve. -->\n\
\n\
> \n\
<!-- Egy mondat: mi ez a projekt, és mire jó. -->\n\
\n\
tags: \n\
<!-- Nyelvek, keretrendszerek, téma, vesszővel. -->\n\
\n\
kind: project\n\
\n\
## Goal\n\
<!-- Mit kellett megoldani, és kinek. -->\n\
\n\
## Period\n\
<!-- Mikor készült, például 2026-03 – 2026-05. -->\n\
\n\
## Stack\n\
<!-- Nyelvek, keretrendszerek, szolgáltatások, verziókkal. -->\n\
\n\
## Key decisions\n\
<!-- A fontos döntések és az okuk, pontokban. -->\n\
\n\
## What worked\n\
\n\
## What did not\n\
\n\
## Where the code is\n\
<!-- Repó, mappa vagy link. Egy kártya legfeljebb kb. 600 token. -->\n";

/// The template of a kind for `kapu new --kind` (FR-05, D144): the project
/// card for `project`, otherwise [`TEMPLATE`] with the kind filled in.
#[must_use]
pub fn template_for(kind: Option<&str>) -> String {
    match kind {
        None => TEMPLATE.to_owned(),
        Some("project") => PROJECT_TEMPLATE.to_owned(),
        Some(k) => TEMPLATE.replacen("kind: \n", &format!("kind: {k}\n"), 1),
    }
}

/// A project card is meant to stay within this many tokens (K-04, W007).
pub const PROJECT_CARD_TOKENS: u64 = 600;

/// GitHub warns about files above 50 MiB; the build warns too (K-17, W008).
pub const SIZE_WARNING_BYTES: usize = 50 * 1024 * 1024;

/// One room as the builder sees it: its id and its unescaped Markdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomSource {
    /// Room id (FR-04/3 grammar).
    pub id: String,
    /// Markdown content, without the `@@@` line, escape undone.
    pub content: String,
}

/// A non-fatal finding of the builder (FR-02/5, FR-11 edge case, D40).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// `W002` invisible character, `W003` link to a missing door,
    /// `W004` CRLF normalised (D83), `W006` unknown kind, `W007` project
    /// card above 600 tokens (D144), `W008` file above 50 MiB (D148), `W009`
    /// skill file left out, `W010` control characters removed (plan v3 F2).
    pub code: &'static str,
    /// Where and what.
    pub message: String,
}

/// The output of a build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Built {
    /// The complete file.
    pub bytes: Vec<u8>,
    /// Warnings, in room order.
    pub warnings: Vec<Warning>,
}

/// One door during the build.
#[derive(Debug, Clone)]
struct Door {
    id: String,
    title: String,
    description: String,
    tags: Vec<String>,
    /// `kind:` of a room (FR-06/5), empty if none.
    kind: String,
    stored: Vec<u8>,
    served: String,
    offset: usize,
    is_room: bool,
    tokens: u64,
    chars: usize,
    dl: u64,
    /// stemmed entry -> tf
    terms: BTreeMap<String, u64>,
    /// unstemmed term -> tf
    raw_terms: BTreeMap<String, u64>,
    gid: String,
    links: Vec<String>,
}

impl Door {
    fn room_id(&self) -> &str {
        self.id.split('/').next().unwrap_or(&self.id)
    }
}

// ---------------------------------------------------------------------------
// Body
// ---------------------------------------------------------------------------

/// Splits a stored body into rooms (D36): each `@@@ <id>` line starts a room
/// whose content runs to the LF before the next `@@@` line; the last room
/// runs to the end, minus one final LF. The escape is undone.
///
/// # Errors
/// E005 for text before the first `@@@` line, E004 for invalid UTF-8.
pub fn parse_body(body: &[u8]) -> Result<Vec<RoomSource>> {
    let text = std::str::from_utf8(body).map_err(|e| {
        Error::new(
            ErrorCode::E004,
            format!("body is not UTF-8 at offset {}", e.valid_up_to()),
        )
    })?;
    if text.is_empty() {
        return Ok(Vec::new());
    }
    if !text.starts_with("@@@ ") {
        return Err(Error::new(
            ErrorCode::E005,
            "the body must start with an `@@@ <id>` line",
        ));
    }
    let mut rooms: Vec<(String, Vec<&str>)> = Vec::new();
    for line in text.split('\n') {
        if let Some(id) = line.strip_prefix("@@@ ") {
            rooms.push((id.to_owned(), Vec::new()));
        } else if let Some((_, lines)) = rooms.last_mut() {
            lines.push(line);
        }
    }
    let count = rooms.len();
    Ok(rooms
        .into_iter()
        .enumerate()
        .map(|(i, (id, mut lines))| {
            // Splitting on LF already consumes the separator LF in front of the
            // next `@@@` line. Only the last room's separator, the final LF of
            // the body, is left over as one empty last line.
            if i + 1 == count && text.ends_with('\n') && lines.last() == Some(&"") {
                lines.pop();
            }
            RoomSource {
                id,
                content: unescape(&lines.join("\n")),
            }
        })
        .collect())
}

/// FR-02 rules 1-4 on one room's content (E003, E004 with line and column);
/// rule 5 (invisible characters) gives a warning.
fn check_content(room: &RoomSource, warnings: &mut Vec<Warning>) -> Result<()> {
    for (ln, line) in room.content.split('\n').enumerate() {
        for (col, c) in line.chars().enumerate() {
            let at = || format!("room `{}` line {} column {}", room.id, ln + 1, col + 1);
            match c {
                '\r' => return Err(Error::new(ErrorCode::E003, format!("CR in {}", at()))),
                '\t' => {}
                c if (c as u32) < 0x20 || c == '\u{7f}' => {
                    return Err(Error::new(
                        ErrorCode::E004,
                        format!("control character U+{:04X} in {}", c as u32, at()),
                    ));
                }
                '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' => {
                    return Err(Error::new(
                        ErrorCode::E004,
                        format!("bidirectional control U+{:04X} in {}", c as u32, at()),
                    ));
                }
                '\u{200B}'..='\u{200D}' | '\u{2060}' | '\u{FEFF}' => warnings.push(Warning {
                    code: "W002",
                    message: format!("invisible character U+{:04X} in {}", c as u32, at()),
                }),
                _ => {}
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Doors
// ---------------------------------------------------------------------------

fn auto_description(text: &str, room_tokens: u64, first_only: bool) -> Result<String> {
    // Python `round(0.05 * t)`: the same IEEE product, ties to even
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let target = ((0.05 * room_tokens as f64).round_ties_even() as u64).clamp(15, 300);
    cut_to_tokens(&paragraphs(text, first_only), target)
}

/// D60: word-boundary prefix within `target` tokens. Like the reference it
/// adds words until the first one that would exceed the budget.
fn cut_to_tokens(text: &str, target: u64) -> Result<String> {
    if tokens(text)? <= target {
        return Ok(text.to_owned());
    }
    let mut out: Vec<&str> = Vec::new();
    for word in text.split(' ') {
        out.push(word);
        if tokens(&out.join(" "))? > target {
            out.pop();
            break;
        }
    }
    Ok(out.join(" "))
}

impl Door {
    fn new(
        id: String,
        title: &str,
        stored: Vec<u8>,
        served: String,
        offset: usize,
        is_room: bool,
        tokens: u64,
    ) -> Self {
        let chars = served.chars().count();
        Self {
            id,
            title: take_chars(title, TITLE_MAX),
            description: String::new(),
            tags: Vec::new(),
            kind: String::new(),
            stored,
            served,
            offset,
            is_room,
            tokens,
            chars,
            dl: 0,
            terms: BTreeMap::new(),
            raw_terms: BTreeMap::new(),
            gid: String::new(),
            links: Vec::new(),
        }
    }
}

/// The room door: title, description (manual or automatic), tags (manual or
/// YAKE), price.
fn room_door(room: &RoomSource, stored: Vec<u8>, offset: usize) -> Result<Door> {
    let raw = &room.content;
    let served = strip_comments(raw);
    let price = tokens(&served)?;
    let mut door = Door::new(
        room.id.clone(),
        &heading_title(raw),
        stored,
        served,
        offset,
        true,
        price,
    );
    door.description = match manual_description(raw) {
        // D88: a manual description is cut only at the hard limit
        d if !d.is_empty() => cut_to_tokens(&d, DESCRIPTION_MAX_TOKENS)?,
        _ => auto_description(raw, price, false)?,
    };
    door.tags = match manual_tags(raw) {
        Some(t) if !t.is_empty() => t,
        _ => yake::keywords(&without_kind_line(&door.served)),
    };
    door.kind = manual_kind(raw).unwrap_or_default();
    Ok(door)
}

/// FR-07 sub-doors: every `## ` line outside fenced code opens one, up to the
/// next such line or the end of the stored room.
fn sub_doors(room_id: &str, stored: &[u8], room_offset: usize) -> Result<Vec<Door>> {
    let stored_text = std::str::from_utf8(stored).unwrap_or("");
    let lines: Vec<&str> = stored_text.split('\n').collect();
    let mask = code_fence_mask(&lines);
    let mut heads: Vec<(usize, &str)> = Vec::new();
    let mut pos = 0usize;
    for (line, in_code) in lines.iter().zip(mask) {
        if let Some(t) = line.strip_prefix("## ").filter(|_| !in_code) {
            heads.push((pos, t.trim()));
        }
        pos += line.len() + 1;
    }
    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut subs = Vec::with_capacity(heads.len());
    for (k, &(start, title)) in heads.iter().enumerate() {
        let end = heads.get(k + 1).map_or(stored.len(), |&(next, _)| next);
        let seg = stored.get(start..end).unwrap_or(&[]).to_vec();
        let base = format!("{room_id}/{}", slug(title));
        let mut sid = base.clone();
        let mut n = 2;
        while used.contains(&sid) {
            sid = format!("{base}-{n}");
            n += 1;
        }
        used.insert(sid.clone());
        let seg_raw = unescape(std::str::from_utf8(&seg).unwrap_or(""));
        let seg_served = strip_comments(&seg_raw);
        let price = tokens(&seg_served)?;
        let mut sub = Door::new(
            sid,
            title,
            seg,
            seg_served,
            room_offset + start,
            false,
            price,
        );
        sub.description = auto_description(&seg_raw, price, true)?;
        subs.push(sub);
    }
    Ok(subs)
}

/// D64: a link belongs to the most specific door that contains it.
fn assign_links(room: &mut Door, subs: &mut [Door]) {
    let mut room_links = find_links(&room.served);
    for sub in subs.iter_mut() {
        sub.links = find_links(&sub.served);
        for l in &sub.links {
            if let Some(i) = room_links.iter().position(|x| x == l) {
                room_links.remove(i);
            }
        }
    }
    room.links = room_links;
}

fn build_doors(rooms: &[RoomSource]) -> Result<(Vec<Door>, Vec<u8>)> {
    let mut doors: Vec<Door> = Vec::new();
    let mut body: Vec<u8> = Vec::new();
    for room in rooms {
        let stored = escape(&room.content).into_bytes();
        let marker = format!("@@@ {}\n", room.id);
        let offset = body.len() + marker.len();
        body.extend_from_slice(marker.as_bytes());
        body.extend_from_slice(&stored);
        body.push(b'\n');
        let mut subs = sub_doors(&room.id, &stored, offset)?;
        let mut door = room_door(room, stored, offset)?;
        assign_links(&mut door, &mut subs);
        doors.push(door);
        doors.extend(subs);
    }
    for d in &mut doors {
        count_terms(d);
    }
    Ok((doors, body))
}

/// The weight of the title and tag words in `#INDEX` (ASSUMPTION-05: 3).
static TITLE_WEIGHT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(3);

/// Plan v2 M0 title-weight axis, for measurement builds only: a file built
/// with another weight than 3 is not a valid KAPU file (`validate --full`
/// rebuilds the index with 3 and finds the difference).
pub fn set_title_weight_for_measurement(weight: u64) {
    TITLE_WEIGHT.store(weight.max(1), std::sync::atomic::Ordering::Relaxed);
}

fn count_terms(door: &mut Door) {
    let mut raw: BTreeMap<String, u64> = BTreeMap::new();
    let mut stemmed: BTreeMap<String, u64> = BTreeMap::new();
    let mut dl = 0u64;
    let tags = door.tags.join(" ");
    let content = indexed_text(&door.served);
    let title = TITLE_WEIGHT.load(std::sync::atomic::Ordering::Relaxed);
    for (text, weight) in [
        (door.title.as_str(), title),
        (tags.as_str(), title),
        (door.description.as_str(), 1),
        (content.as_str(), 1),
    ] {
        for t in index_tokens(text) {
            dl += weight;
            for s in stems(&t) {
                *stemmed.entry(s).or_insert(0) += weight;
            }
            *raw.entry(t).or_insert(0) += weight;
        }
    }
    door.terms = stemmed;
    door.raw_terms = raw;
    door.dl = dl;
}

// ---------------------------------------------------------------------------
// Graph and groups
// ---------------------------------------------------------------------------

/// D65: at most 4 decimals, trailing zeros removed, at least one decimal.
#[must_use]
pub fn fmt_weight(w: f64) -> String {
    let s = format!("{w:.4}");
    let s = s.trim_end_matches('0');
    if s.ends_with('.') {
        format!("{s}0")
    } else {
        s.to_owned()
    }
}

type EdgeRow = (String, String, &'static str, f64);

/// FR-11/1 `link` edges; a link to a missing door path (with `/`) is a W003
/// warning, a missing target without `/` an in-page anchor (D123).
fn link_edges(doors: &[Door], warnings: &mut Vec<Warning>) -> Vec<EdgeRow> {
    let ids: BTreeSet<&str> = doors.iter().map(|d| d.id.as_str()).collect();
    let mut edges = Vec::new();
    for door in doors {
        let room = door.id.split('/').next().unwrap_or("");
        for target in &door.links {
            // D123: `#x` also names a section of the same room, as an in-page
            // anchor would; an unknown target without `/` is such an anchor.
            let local = format!("{room}/{target}");
            let resolved = if ids.contains(target.as_str()) {
                Some(target.as_str())
            } else if !target.contains('/') && ids.contains(local.as_str()) {
                Some(local.as_str())
            } else {
                None
            };
            if let Some(t) = resolved {
                if t != door.id {
                    edges.push((door.id.clone(), t.to_owned(), "link", 1.0));
                }
            } else if target.contains('/') {
                warnings.push(Warning {
                    code: "W003",
                    message: format!("`{}` links to missing door `{target}`", door.id),
                });
            }
        }
    }
    edges
}

/// FR-11/2 `term` edges between rooms that share a rare term (top 10% IDF),
/// weighted by the IDF sum of the shared rare terms over the largest such sum.
fn term_edges(doors: &[Door]) -> Vec<EdgeRow> {
    let rooms: Vec<&Door> = doors.iter().filter(|d| d.is_room).collect();
    #[allow(clippy::cast_precision_loss)]
    let room_count = rooms.len() as f64;
    let mut df: BTreeMap<&str, u64> = BTreeMap::new();
    for room in &rooms {
        for term in room.terms.keys() {
            *df.entry(term.as_str()).or_insert(0) += 1;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let idf: BTreeMap<&str, f64> = df
        .iter()
        .map(|(term, &count)| {
            (
                *term,
                libm::log((room_count - count as f64 + 0.5) / (count as f64 + 0.5) + 1.0),
            )
        })
        .collect();
    // D93: the top-10% IDF threshold is taken among the terms that two or more
    // rooms share; a term of a single room can never be a shared term
    let mut values: Vec<f64> = idf
        .iter()
        .filter(|(t, _)| df.get(**t).copied().unwrap_or(0) >= 2)
        .map(|(_, v)| *v)
        .collect();
    if values.is_empty() {
        return Vec::new();
    }
    values.sort_by(|a, b| b.total_cmp(a));
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let top_k = ((0.1 * values.len() as f64).ceil() as usize).max(1);
    let threshold = values.get(top_k - 1).copied().unwrap_or(f64::INFINITY);
    let rare: BTreeSet<&str> = idf
        .iter()
        .filter(|&(t, v)| *v >= threshold && df.get(*t).copied().unwrap_or(0) >= 2)
        .map(|(t, _)| *t)
        .collect();

    // Inverted (NFR-05): for each rare term, in byte order, every pair of rooms
    // that holds it. Pushing the IDF in term byte order keeps the D81
    // summation order per pair, so the weights equal a pairwise scan.
    let mut holders: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, room) in rooms.iter().enumerate() {
        for term in room.terms.keys().filter(|t| rare.contains(t.as_str())) {
            holders.entry(term.as_str()).or_default().push(i);
        }
    }
    let mut parts: BTreeMap<(usize, usize), Vec<f64>> = BTreeMap::new();
    for (term, list) in &holders {
        let weight = idf.get(term).copied().unwrap_or(0.0);
        for (k, &a) in list.iter().enumerate() {
            for &b in list.iter().skip(k + 1) {
                parts.entry((a, b)).or_default().push(weight);
            }
        }
    }
    let mut sums: BTreeMap<(String, String), f64> = BTreeMap::new();
    for ((a, b), weights) in parts {
        let (Some(left), Some(right)) = (rooms.get(a), rooms.get(b)) else {
            continue;
        };
        let (first, second) = if left.id <= right.id {
            (&left.id, &right.id)
        } else {
            (&right.id, &left.id)
        };
        sums.insert((first.clone(), second.clone()), weights.iter().sum());
    }
    let max_sum = sums.values().copied().fold(0.0f64, f64::max);
    sums.into_iter()
        .map(|((first, second), sum)| (first, second, "term", sum / max_sum))
        .collect()
}

/// FR-11/3 `sim` edges: for every room its k = 5 nearest rooms by cosine
/// similarity of the room vectors, if the similarity is at least 0.6;
/// undirected, stored once with the smaller id first, weight = similarity.
/// Ties in similarity go to the smaller id.
fn sim_edges(rooms: &[(&str, Vec<f32>)]) -> Vec<EdgeRow> {
    let mut pairs: BTreeMap<(String, String), f64> = BTreeMap::new();
    for (i, (id, vector)) in rooms.iter().enumerate() {
        let mut near: Vec<(f64, &str)> = rooms
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, (other, v))| (cosine(vector, v), *other))
            .filter(|(sim, _)| *sim >= SIM_THRESHOLD)
            .collect();
        near.sort_by(|a, b| {
            b.0.total_cmp(&a.0)
                .then_with(|| a.1.as_bytes().cmp(b.1.as_bytes()))
        });
        for (sim, other) in near.into_iter().take(SIM_K) {
            let key = if *id <= other {
                ((*id).to_owned(), other.to_owned())
            } else {
                (other.to_owned(), (*id).to_owned())
            };
            pairs.insert(key, sim);
        }
    }
    pairs
        .into_iter()
        .map(|((a, b), w)| (a, b, "sim", w))
        .collect()
}

/// All edges, sorted by (source, target, type) (FR-11/5).
fn build_graph(
    doors: &[Door],
    vectors: Option<&[Vec<f32>]>,
    warnings: &mut Vec<Warning>,
) -> Vec<EdgeRow> {
    let mut edges = link_edges(doors, warnings);
    edges.extend(term_edges(doors));
    if let Some(vectors) = vectors {
        let rooms: Vec<(&str, Vec<f32>)> = doors
            .iter()
            .filter(|d| d.is_room)
            .zip(vectors)
            .map(|(d, v)| (d.id.as_str(), v.clone()))
            .collect();
        edges.extend(sim_edges(&rooms));
    }
    edges.sort_by(|a, b| (&a.0, &a.1, a.2).cmp(&(&b.0, &b.1, b.2)));
    edges
}

struct Group {
    gid: String,
    name: String,
    rooms: Vec<usize>,
}

/// FR-12: Leiden on the room graph. Sub-door endpoints are projected to their
/// room (D64), self-loops dropped, parallel edges summed; the weights are the
/// ones written to `#GRAPH` (4 decimals, D92), so the grouping follows from
/// the file alone. Returns groups as indices into `doors`, ordered by their
/// smallest room id (FR-12/3). An edgeless graph gives one group (FR-12/4).
fn communities(doors: &[Door], edges: &[EdgeRow]) -> Vec<Vec<usize>> {
    let room_idx: Vec<usize> = (0..doors.len())
        .filter(|&i| doors.get(i).is_some_and(|d| d.is_room))
        .collect();
    let position: BTreeMap<&str, usize> = room_idx
        .iter()
        .enumerate()
        .filter_map(|(k, &i)| doors.get(i).map(|d| (d.id.as_str(), k)))
        .collect();
    let room_of = |id: &str| id.split('/').next().and_then(|r| position.get(r)).copied();
    let mut graph: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    for (from, to, _, w) in edges {
        if let (Some(a), Some(b)) = (room_of(from), room_of(to))
            && a != b
        {
            let weight: f64 = fmt_weight(*w).parse().unwrap_or(*w);
            *graph.entry((a.min(b), a.max(b))).or_insert(0.0) += weight;
        }
    }
    if graph.is_empty() {
        return if room_idx.is_empty() {
            Vec::new()
        } else {
            vec![room_idx]
        };
    }
    let labels = leiden(room_idx.len(), &graph);
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (k, &i) in room_idx.iter().enumerate() {
        groups
            .entry(labels.get(k).copied().unwrap_or(k))
            .or_default()
            .push(i);
    }
    let mut ordered: Vec<Vec<usize>> = groups.into_values().collect();
    ordered.sort_by_key(|rooms| {
        rooms
            .iter()
            .filter_map(|&i| doors.get(i))
            .map(|d| d.id.clone())
            .min()
            .unwrap_or_default()
    });
    ordered
}

/// Words of API parameter tables that are never a group's topic (A41, D131).
/// Group names only; tags keep them (`str` can be a Rust topic).
pub const GROUP_NAME_SKIP: [&str; 12] = [
    "bool", "default", "false", "float", "int", "let", "none", "null", "optional", "required",
    "str", "true",
];

// D66 group names: class-level TF-IDF `tf * (1 + ln(G / df))` over the
/// unstemmed terms of at least 3 characters that are not all digits, not
/// stop words (D121) and not parameter-table words (D131); the best three
/// joined with `, `, ties in byte order.
fn group_names(doors: &[Door], groups: &[Vec<usize>]) -> Vec<String> {
    let candidate = |t: &str| {
        t.chars().count() >= 3
            && !t.chars().all(|c| c.is_ascii_digit())
            && !yake::is_stopword(t)
            && !GROUP_NAME_SKIP.contains(&t)
    };
    let mut gdf: BTreeMap<&str, u64> = BTreeMap::new();
    for rooms in groups {
        let seen: BTreeSet<&str> = rooms
            .iter()
            .filter_map(|&i| doors.get(i))
            .flat_map(|d| d.raw_terms.keys().map(String::as_str))
            .filter(|t| candidate(t))
            .collect();
        for term in seen {
            *gdf.entry(term).or_insert(0) += 1;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let group_count = groups.len() as f64;
    groups
        .iter()
        .map(|rooms| {
            let mut tf: BTreeMap<&str, u64> = BTreeMap::new();
            for door in rooms.iter().filter_map(|&i| doors.get(i)) {
                for (term, count) in door.raw_terms.iter().filter(|(t, _)| candidate(t)) {
                    *tf.entry(term.as_str()).or_insert(0) += count;
                }
            }
            #[allow(clippy::cast_precision_loss)]
            let mut scored: Vec<(&str, f64)> = tf
                .iter()
                .map(|(term, &count)| {
                    let df = gdf.get(term).copied().unwrap_or(1) as f64;
                    (*term, count as f64 * (1.0 + libm::log(group_count / df)))
                })
                .collect();
            // D105: equal to 1e-9 is a tie, broken by byte order
            scored.sort_by(|a, b| {
                yake::tie_key(b.1)
                    .total_cmp(&yake::tie_key(a.1))
                    .then_with(|| a.0.as_bytes().cmp(b.0.as_bytes()))
            });
            scored
                .iter()
                .take(3)
                .map(|(t, _)| *t)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .collect()
}

fn build_groups(doors: &mut [Door], edges: &[EdgeRow]) -> Vec<Group> {
    let ordered = communities(doors, edges);
    let names = group_names(doors, &ordered);
    let groups: Vec<Group> = ordered
        .into_iter()
        .zip(names)
        .enumerate()
        .map(|(k, (rooms, name))| Group {
            gid: format!("g{}", k + 1),
            name,
            rooms,
        })
        .collect();
    let mut room_gid: BTreeMap<String, String> = BTreeMap::new();
    for group in &groups {
        for door in group.rooms.iter().filter_map(|&i| doors.get(i)) {
            room_gid.insert(door.id.clone(), group.gid.clone());
        }
    }
    for door in doors.iter_mut() {
        if let Some(gid) = room_gid.get(door.room_id()) {
            door.gid.clone_from(gid);
        }
    }
    groups
}

// ---------------------------------------------------------------------------
// Sections and header
// ---------------------------------------------------------------------------

/// D135: NFC text without the invisible characters FR-02/5 forbids in a
/// derived section (the body may hold them, with W002).
fn derived(s: &str) -> String {
    nfc(s)
        .chars()
        .filter(|c| !matches!(c, '\u{200B}'..='\u{200D}' | '\u{2060}' | '\u{FEFF}'))
        .collect()
}

/// FR-13 field escape (D107): `\` becomes `\\` first, then `|` becomes `\|`;
/// TAB becomes a space. Invisible characters are left out (D135).
fn esc_field(s: &str) -> String {
    derived(s)
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\t', " ")
}

fn section(name: &str, lines: &[String]) -> Vec<u8> {
    let mut out = format!("#{name}\n");
    for l in lines {
        out.push_str(l);
        out.push('\n');
    }
    out.push_str("#END\n");
    out.into_bytes()
}

fn hex(h: &Hash) -> String {
    h.0.iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// FR-13 `#MAP`: per group its `G` line, then its rooms in body order, each
/// followed by its sub-doors.
fn map_lines(doors: &[Door], groups: &[Group]) -> Vec<String> {
    let mut lines = Vec::new();
    for group in groups {
        let total: u64 = group
            .rooms
            .iter()
            .filter_map(|&i| doors.get(i))
            .map(|d| d.tokens)
            .sum();
        lines.push(format!(
            "G {} | {} | {} | {total}t",
            group.gid,
            esc_field(&group.name),
            group.rooms.len()
        ));
        for room in doors.iter().filter(|d| d.is_room && d.gid == group.gid) {
            let short: Vec<&str> = room.tags.iter().take(5).map(String::as_str).collect();
            // D144: a room with a kind has it as a sixth field
            let kind = if room.kind.is_empty() {
                String::new()
            } else {
                format!(" | {}", room.kind)
            };
            lines.push(format!(
                "R {} | {} | {}t | {} | {}{kind}",
                room.id,
                esc_field(&room.title),
                room.tokens,
                group.gid,
                esc_field(&short.join(", "))
            ));
            for sub in doors
                .iter()
                .filter(|d| !d.is_room && d.room_id() == room.id)
            {
                lines.push(format!(
                    "S {} | {} | {}t",
                    sub.id,
                    esc_field(&sub.title),
                    sub.tokens
                ));
            }
        }
    }
    lines
}

/// FR-13 `#DESC`: one line per door in body order.
fn desc_lines(doors: &[Door]) -> Vec<String> {
    doors
        .iter()
        .map(|d| {
            format!(
                "{}\t{}\t{}",
                d.id,
                derived(&d.description).replace('\t', " "),
                derived(&d.tags.join(", ")).replace('\t', " ")
            )
        })
        .collect()
}

/// FR-09 `#INDEX`: terms in UTF-8 byte order (`BTreeMap` of `&str`); a
/// posting is `<gap>:<tf>`, the gap between the doors' line numbers in
/// `#DOCS`, which is in id byte order (D140).
fn index_lines(doors: &[Door]) -> Vec<String> {
    let mut ids: Vec<&str> = doors.iter().map(|d| d.id.as_str()).collect();
    ids.sort_unstable_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    let line_of: BTreeMap<&str, usize> = ids.iter().enumerate().map(|(n, id)| (*id, n)).collect();
    let mut postings: BTreeMap<&str, BTreeMap<usize, u64>> = BTreeMap::new();
    for door in doors {
        let at = line_of.get(door.id.as_str()).copied().unwrap_or(0);
        for (term, &tf) in &door.terms {
            postings.entry(term.as_str()).or_default().insert(at, tf);
        }
    }
    postings
        .iter()
        .map(|(term, list)| {
            let mut previous = 0;
            let gaps: Vec<String> = list
                .iter()
                .map(|(&at, tf)| {
                    let gap = at - previous;
                    previous = at;
                    format!("{gap}:{tf}")
                })
                .collect();
            format!("{term}\t{}\t{}", gaps.len(), gaps.join(","))
        })
        .collect()
}

/// FR-14 `#DOCS`: sorted by id bytes.
fn docs_lines(doors: &[Door]) -> Vec<String> {
    let mut sorted: Vec<&Door> = doors.iter().collect();
    sorted.sort_by(|a, b| a.id.as_bytes().cmp(b.id.as_bytes()));
    sorted
        .iter()
        .map(|d| {
            format!(
                "{}\t@{}+{}\tb3:{}\tdl={}\tc={}",
                d.id,
                d.offset,
                d.stored.len(),
                hex(&blake3_of(&d.stored)),
                d.dl,
                d.chars
            )
        })
        .collect()
}

/// FR-10/6 `#VEC` (D143): the parameter line, then per door in `#DOCS`
/// order (id bytes) its window sign bits.
fn vec_lines(doors: &[Door], windows: &[Vec<Vec<f32>>], dim: usize) -> Vec<String> {
    let mut sorted: Vec<(&Door, &Vec<Vec<f32>>)> = doors.iter().zip(windows).collect();
    sorted.sort_by(|a, b| a.0.id.as_bytes().cmp(b.0.id.as_bytes()));
    std::iter::once(vec_params(dim))
        .chain(sorted.iter().map(|(_, w)| sign_line(w)))
        .collect()
}

/// FR-03 header plus sections plus body.
fn assemble(
    sections: Vec<(&str, Vec<u8>)>,
    body: &[u8],
    counts: (usize, usize, usize),
    model: (&str, Hash),
) -> Result<Vec<u8>> {
    let (rooms, doors, groups) = counts;
    let (emb, embhash) = model;
    let mut header = format!(
        "%KAPU {FORMAT_VERSION}\neol=LF\nenc=UTF-8\ntok=o200k_base\nemb={emb}\nembhash=b3:{}\n",
        hex(&embhash)
    );
    let mut pos = HEADER_LEN;
    for (name, bytes) in &sections {
        let _ = writeln!(header, "{name}={pos}+{}", bytes.len());
        pos += bytes.len();
    }
    let _ = writeln!(header, "body={}+{}", pos + "#BODY\n".len(), body.len());
    let _ = writeln!(header, "rooms={rooms}\ndoors={doors}\ngroups={groups}");
    let _ = writeln!(header, "bodyhash=b3:{}\nguide=std-1", hex(&blake3_of(body)));
    if header.len() > HEADER_LEN - 1 {
        return Err(Error::new(
            ErrorCode::E002,
            format!("header overflow: {} bytes (D39)", header.len()),
        ));
    }
    let mut bytes = header.into_bytes();
    bytes.resize(HEADER_LEN - 1, b' ');
    bytes.push(b'\n');
    for (_, s) in sections {
        bytes.extend(s);
    }
    bytes.extend_from_slice(b"#BODY\n");
    bytes.extend_from_slice(body);
    Ok(bytes)
}

/// Builds a complete file from rooms in body order.
///
/// # Errors
/// E008 for an invalid or duplicate room id, E003/E004 for forbidden
/// characters in a room (with room, line and column), E002 if the header
/// overflows 512 bytes (D39).
pub fn build(rooms: &[RoomSource]) -> Result<Built> {
    build_with(rooms, None)
}

/// Builds with a vector model (FR-10/2): `sim` edges from the room vectors
/// and the model's name and fingerprint in the header (D29, D38). Without a
/// model this is [`build`] (D57).
///
/// # Errors
/// As [`build`], plus a tokenizer failure (E012).
pub fn build_with(rooms: &[RoomSource], model: Option<&Model>) -> Result<Built> {
    let mut warnings = Vec::new();
    let mut seen = BTreeSet::new();
    for room in rooms {
        if !is_valid_id(&room.id) || room.id.contains('/') {
            return Err(Error::new(
                ErrorCode::E008,
                format!("`{}` is not a valid room id", room.id),
            ));
        }
        if !seen.insert(room.id.as_str()) {
            return Err(Error::new(
                ErrorCode::E008,
                format!("room id `{}` appears twice", room.id),
            ));
        }
        check_content(room, &mut warnings)?;
    }
    let (mut doors, body) = build_doors(rooms)?;
    kind_warnings(&doors, &mut warnings);
    // D141, D143: every door's window vectors; a room's first window is the
    // vector of its whole text as `embed` sees it, for the `sim` edges
    let windows: Option<Vec<Vec<Vec<f32>>>> = match model {
        Some(m) => Some(
            doors
                .iter()
                .map(|d| m.embed_windows(&door_text(&d.title, &d.description, &d.served)))
                .collect::<Result<_>>()?,
        ),
        None => None,
    };
    let vectors: Option<Vec<Vec<f32>>> = windows.as_ref().map(|all| {
        doors
            .iter()
            .zip(all)
            .filter(|(d, _)| d.is_room)
            .map(|(_, w)| w.first().cloned().unwrap_or_default())
            .collect()
    });
    let edges = build_graph(&doors, vectors.as_deref(), &mut warnings);
    let groups = build_groups(&mut doors, &edges);
    let graph: Vec<String> = edges
        .iter()
        .map(|(a, b, t, w)| format!("{a}\t{b}\t{t}\t{}", fmt_weight(*w)))
        .collect();
    let mut sections = vec![
        ("map", section("MAP", &map_lines(&doors, &groups))),
        ("desc", section("DESC", &desc_lines(&doors))),
        ("graph", section("GRAPH", &graph)),
        ("index", section("INDEX", &index_lines(&doors))),
        ("docs", section("DOCS", &docs_lines(&doors))),
    ];
    if let (Some(m), Some(windows)) = (model, &windows) {
        sections.push(("vec", section("VEC", &vec_lines(&doors, windows, m.dim()))));
    }
    let counts = (rooms.len(), doors.len(), groups.len());
    let model_id = model.map_or((NO_MODEL, blake3_of(b"")), |m| (m.name(), m.hash()));
    let bytes = assemble(sections, &body, counts, model_id)?;
    if bytes.len() > SIZE_WARNING_BYTES {
        warnings.push(Warning {
            code: "W008",
            message: format!(
                "the file is {:.1} MiB; GitHub warns above 50 MiB (see the README on git)",
                mib(bytes.len())
            ),
        });
    }
    Ok(Built { bytes, warnings })
}

/// Size in MiB, for messages.
#[allow(clippy::cast_precision_loss)]
fn mib(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

/// W006 for a kind outside [`KINDS`], W007 for a project card above
/// [`PROJECT_CARD_TOKENS`] (FR-06/5, D144).
fn kind_warnings(doors: &[Door], warnings: &mut Vec<Warning>) {
    for room in doors.iter().filter(|d| d.is_room && !d.kind.is_empty()) {
        if !KINDS.contains(&room.kind.as_str()) {
            warnings.push(Warning {
                code: "W006",
                message: format!(
                    "room `{}` has kind `{}`, not one of {}",
                    room.id,
                    room.kind,
                    KINDS.join(", ")
                ),
            });
        }
        if room.kind == "project" && room.tokens > PROJECT_CARD_TOKENS {
            warnings.push(Warning {
                code: "W007",
                message: format!(
                    "project card `{}` is {} tokens; a card is meant to stay within {PROJECT_CARD_TOKENS}",
                    room.id, room.tokens
                ),
            });
        }
    }
}

/// Rebuilds an existing file from its body (FR-18/1, `kapu build`). The body
/// is read tolerantly, so a hand-edited file whose derived sections are stale
/// rebuilds fine.
///
/// # Errors
/// A broken header or layout, or any [`build`] error.
pub fn rebuild(file: &[u8]) -> Result<Built> {
    rebuild_with(file, None)
}

/// [`rebuild`] with a vector model (or without, `None`).
///
/// # Errors
/// As [`rebuild`] and [`build_with`].
pub fn rebuild_with(file: &[u8], model: Option<&Model>) -> Result<Built> {
    let s =
        crate::Sections::parse_with(file, &crate::Limits::default(), crate::BodyLength::Tolerate)?;
    build_with(&parse_body(s.body)?, model)
}

/// A free room id for a new room (FR-04/4): the slug of its title, with
/// `-2`, `-3`... on collision.
#[must_use]
pub fn new_room_id(content: &str, used: &BTreeSet<String>) -> String {
    unique_id(&slug(&heading_title(content)), used)
}

/// FR-04/4: `base` (a slug), or with `-2`, `-3`... on a collision, the base
/// shortened so the id stays within 64 characters (FR-04/3). Before plan v3
/// F1 a 64-character base with a suffix was an invalid id (E008).
#[must_use]
pub fn unique_id(base: &str, used: &BTreeSet<String>) -> String {
    let mut id = base.to_owned();
    let mut n = 2;
    while used.contains(&id) {
        let suffix = format!("-{n}");
        let head: String = base.chars().take(64 - suffix.len()).collect();
        id = format!("{}{suffix}", head.trim_end_matches('-'));
        n += 1;
    }
    id
}

/// Plan v3 F2: the characters FR-02 forbids in a body (C0 controls other
/// than TAB and LF, a lone CR, the bidirectional controls) are removed from
/// imported text, with a W010 warning, instead of stopping the import (E004).
#[must_use]
pub fn strip_controls(name: &str, text: &str, warnings: &mut Vec<Warning>) -> String {
    let forbidden = |c: char| {
        (c.is_ascii_control() && c != '\t' && c != '\n' && c != '\u{7f}')
            || ('\u{202a}'..='\u{202e}').contains(&c)
            || ('\u{2066}'..='\u{2069}').contains(&c)
    };
    let removed = text.chars().filter(|c| forbidden(*c)).count();
    if removed == 0 {
        return text.to_owned();
    }
    warnings.push(Warning {
        code: "W010",
        message: format!("{name}: {removed} control characters removed"),
    });
    text.chars().filter(|c| !forbidden(*c)).collect()
}

/// FR-18/2: writes `<path>.tmp` in the same directory, then renames it over
/// `path` (replacing an existing file on every platform).
///
/// # Errors
/// I/O errors, reported as E012 with the OS message.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let io = |e: std::io::Error| {
        Error::new(
            ErrorCode::E012,
            format!("cannot write {}: {e}", path.display()),
        )
    };
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, bytes).map_err(io)?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        io(e)
    })
}

/// D40: CRLF to LF for imported Markdown, with a W004 warning.
#[must_use]
pub fn normalize_newlines(name: &str, text: &str, warnings: &mut Vec<Warning>) -> String {
    if text.contains('\r') {
        warnings.push(Warning {
            code: "W004",
            message: format!("{name}: CRLF line endings converted to LF"),
        });
        text.replace("\r\n", "\n")
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(id: &str, content: &str) -> RoomSource {
        RoomSource {
            id: id.to_owned(),
            content: content.to_owned(),
        }
    }

    #[test]
    fn group_names_leave_out_api_table_words() {
        // A41: the words of parameter tables are not a topic
        let table = "| name | type | optional | default |
|---|---|---|---|
| size | str | optional | none |
| seed | int | optional | null |
";
        let rooms: Vec<RoomSource> = (0..3)
            .map(|i| {
                room(
                    &format!("api-{i}"),
                    &format!(
                        "# Image API {i}
{}
pixel sprite image
",
                        table.repeat(6)
                    ),
                )
            })
            .collect();
        let built = build(&rooms).unwrap();
        let view = crate::view::map_view(&built.bytes, None).unwrap();
        for g in &view.groups {
            for word in ["optional", "str", "int", "default", "null"] {
                assert!(!g.name.split(", ").any(|t| t == word), "{}", g.name);
            }
        }
    }

    #[test]
    fn invisible_characters_stay_out_of_derived_sections() {
        // D135: the body may hold U+200B (W002), a derived section may not (FR-02/5)
        let built = build(&[room(
            "a",
            "# Zero\u{200B}width title\nfirst\u{200B}line of the text\n",
        )])
        .unwrap();
        crate::validate(&built.bytes, crate::Level::Full).unwrap();
        let view = crate::view::map_view(&built.bytes, None).unwrap();
        assert_eq!(view.doors[0].title, "Zerowidth title");
    }

    #[test]
    fn kind_goes_to_the_map_and_an_unknown_one_warns() {
        // D144: the sixth field of an `R` line; W006 for an unknown kind
        let card = template_for(Some("project")).replacen("# \n", "# Card\n", 1);
        let built = build(&[
            room("a", "# A\nkind: skill\n\ntext\n"),
            room("b", "# B\nkind: recipe\n\ntext\n"),
            room("c", &card),
        ])
        .unwrap();
        let s = crate::Sections::parse(&built.bytes).unwrap();
        let map = crate::catalog::parse_map(s.map).unwrap();
        let kind = |id: &str| map.door(id).map(|d| d.kind);
        assert_eq!(
            (kind("a"), kind("b"), kind("c")),
            (Some("skill"), Some("recipe"), Some("project"))
        );
        assert!(
            map.door("c/goal").is_some(),
            "a card's sections are sub-doors"
        );
        let codes: Vec<&str> = built.warnings.iter().map(|w| w.code).collect();
        assert_eq!(codes, ["W006"], "{:?}", built.warnings);
        let report = crate::validate(&built.bytes, crate::Level::Full).unwrap();
        assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
        assert!(template_for(Some("note")).contains("kind: note\n"));
    }

    #[test]
    fn a_long_project_card_warns() {
        // K-04 (D144): a card is meant to stay within 600 tokens
        let long = format!("# P\nkind: project\n\n{}\n", "word ".repeat(700));
        let built = build(&[room("p", &long)]).unwrap();
        assert!(
            built.warnings.iter().any(|w| w.code == "W007"),
            "{:?}",
            built.warnings
        );
    }

    #[test]
    fn weights() {
        assert_eq!(fmt_weight(1.0), "1.0");
        assert_eq!(fmt_weight(0.632_12), "0.6321");
        assert_eq!(fmt_weight(0.5), "0.5");
    }

    #[test]
    fn body_round_trip() {
        // middle rooms with and without trailing LFs are the tricky cases
        let rooms = vec![
            room("a", "x\n@@@ fake\ny"),
            room("b", ""),
            room("m", "mid\n"),
            room("n", "\n\n"),
            room("c", "z\n"),
        ];
        let built = build(&rooms).unwrap();
        let s = crate::Sections::parse(&built.bytes).unwrap();
        assert_eq!(parse_body(s.body).unwrap(), rooms);
        crate::validate(&built.bytes, crate::Level::Full).unwrap();
    }

    #[test]
    fn content_rules() {
        assert_eq!(
            build(&[room("a", "x\u{1b}")]).unwrap_err().code,
            ErrorCode::E004
        );
        assert_eq!(
            build(&[room("a", "x\ry")]).unwrap_err().code,
            ErrorCode::E003
        );
        assert_eq!(build(&[room("A", "x")]).unwrap_err().code, ErrorCode::E008);
        assert_eq!(
            build(&[room("a", ""), room("a", "")]).unwrap_err().code,
            ErrorCode::E008
        );
        let w = build(&[room("a", "x\u{200b}y")]).unwrap().warnings;
        assert_eq!(w.first().map(|w| w.code), Some("W002"));
    }

    #[test]
    fn in_page_anchors_link_to_sections_or_stay_quiet() {
        let built = build(&[
            room(
                "a",
                "# A\n\nSee [setup](#setup) and [top](#top).\n\n## Setup\nsteps\n",
            ),
            room("b", "# B\n\nGo to [a](#a) and [gone](#a/gone).\n"),
        ])
        .unwrap();
        let s = crate::Sections::parse(&built.bytes).unwrap();
        let edges: Vec<_> = crate::catalog::parse_graph(s.graph)
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == "link")
            .map(|e| (e.from.to_owned(), e.to.to_owned()))
            .collect();
        assert!(
            edges.contains(&("a".to_owned(), "a/setup".to_owned())),
            "{edges:?}"
        );
        assert!(
            edges.contains(&("b".to_owned(), "a".to_owned())),
            "{edges:?}"
        );
        let w003: Vec<_> = built.warnings.iter().filter(|w| w.code == "W003").collect();
        assert_eq!(
            w003.len(),
            1,
            "only the door path is a missing link: {w003:?}"
        );
        assert!(w003.first().is_some_and(|w| w.message.contains("a/gone")));
    }

    #[test]
    fn empty_file_builds() {
        let built = build(&[]).unwrap();
        crate::validate(&built.bytes, crate::Level::Full).unwrap();
    }

    #[test]
    fn new_ids() {
        let used: BTreeSet<String> = ["elso".to_owned()].into();
        assert_eq!(new_room_id("# Első\n", &used), "elso-2");
        assert_eq!(new_room_id("", &BTreeSet::new()), "szoba");
    }
}
