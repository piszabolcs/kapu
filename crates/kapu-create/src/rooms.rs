//! Layer L3: documents to rooms. A pure, deterministic function: the same
//! documents give byte for byte the same rooms (plan L3/1).
//!
//! The steps: drop the web boilerplate repeated across a site, build each
//! document's section tree, mark sections copied word for word from an
//! earlier one (KD13), cut the trees into rooms that fit the frame (KD11),
//! add an overview room per larger document (KD12), give stable ids and
//! unique titles (KD14), and render every room with its `Source:` and
//! `Path:` lines and the links between documents rewritten to rooms.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

use kapu_core::build::RoomSource;
use serde::Serialize;

use crate::doc::{Document, Kind};
use crate::text::{numbering, slug, squash, tokens};

/// The sizes and switches of the planner (KD11-KD13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Settings {
    /// The most tokens of a room.
    pub room_max: usize,
    /// The target size of a sub-door (`##` section); a larger one is opened
    /// in parts by `kapu_open`.
    pub sub_max: usize,
    /// Sections smaller than this are packed with their neighbours.
    pub min_section: usize,
    /// An overview room for every document of three or more rooms.
    pub overview: bool,
    /// Sections copied word for word are kept once.
    pub dedupe: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            room_max: 4000,
            sub_max: 1500,
            min_section: 200,
            overview: true,
            dedupe: true,
        }
    }
}

/// How a document got its structure (plan L3/3), for the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// Headings, bookmarks or styles of the source.
    Explicit,
    /// Headings inferred from the layout.
    Inferred,
    /// No headings: cut into pieces of the frame size.
    Fallback,
    /// No headings and small enough for one room.
    Flat,
}

/// One planned room, for the preview and the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RoomInfo {
    /// The room id.
    pub id: String,
    /// The title.
    pub title: String,
    /// The `o200k_base` tokens of the content (its price on the map).
    pub tokens: usize,
    /// The titles of its `##` sections (sub-doors).
    pub sections: Vec<String>,
    /// The first and last physical page, when the source has pages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<(u32, u32)>,
}

/// The rooms of one document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocPlan {
    /// The document's key.
    pub key: String,
    /// Its title.
    pub title: String,
    /// How its structure was found.
    pub tier: Tier,
    /// Its rooms in order.
    pub rooms: Vec<RoomInfo>,
}

/// Counts for the report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Stats {
    /// Blocks dropped as boilerplate repeated across a web site.
    pub boilerplate: usize,
    /// Sections kept once and linked from their copies.
    pub duplicates: usize,
    /// Links rewritten from a document or page to a room.
    pub links: usize,
}

/// The planner's result.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    /// The rooms, ready for `kapu_core::build`.
    pub rooms: Vec<RoomSource>,
    /// The rooms per document.
    pub docs: Vec<DocPlan>,
    /// Section key to room id, kept in the source list so a refresh gives
    /// the same section the same id (KD14, KD15).
    pub ids: BTreeMap<String, String>,
    /// Counts for the report.
    pub stats: Stats,
}

/// Plans the rooms of `docs`. `previous` maps section keys to the ids of
/// an earlier run (empty for a new file); `reserved` are ids already taken
/// by rooms the planner must not touch (hand-written rooms of a refreshed
/// file).
#[must_use]
pub fn plan(
    docs: &[Document],
    settings: &Settings,
    previous: &BTreeMap<String, String>,
    reserved: &BTreeSet<String>,
) -> Plan {
    let mut docs: Vec<Document> = docs
        .iter()
        .filter(|d| !d.blocks.is_empty())
        .cloned()
        .collect();
    let mut stats = Stats {
        boilerplate: drop_boilerplate(&mut docs),
        ..Stats::default()
    };
    let mut trees: Vec<Tree> = docs.iter().map(Tree::new).collect();
    if settings.dedupe {
        stats.duplicates = mark_duplicates(&docs, &mut trees, settings);
    }
    for tree in &mut trees {
        tree.sum();
    }
    let mut planner = Planner {
        settings: *settings,
        docs: &docs,
        trees: &trees,
        drafts: Vec::new(),
    };
    for d in 0..docs.len() {
        planner.document(d);
    }
    let mut drafts = planner.drafts;
    unique_titles(&mut drafts, &docs, &trees);
    let ids = assign_ids(&mut drafts, &docs, previous, reserved);
    let mut render = Render::new(&docs, &trees, &drafts, contents_budget(settings));
    let rooms: Vec<RoomSource> = (0..drafts.len()).map(|i| render.room(i)).collect();
    stats.links = render.links;
    let docs_plan = doc_plans(&docs, &trees, &drafts, &rooms);
    Plan {
        rooms,
        docs: docs_plan,
        ids,
        stats,
    }
}

// ---------------------------------------------------------------- the tree

#[derive(Debug, Clone, Default)]
struct Node {
    title: String,
    level: u8,
    blocks: Vec<usize>,
    children: Vec<usize>,
    parent: Option<usize>,
    head: usize,
    own: usize,
    total: usize,
    /// The section is a copy of this earlier one (document, node).
    dup: Option<(usize, usize)>,
}

#[derive(Debug, Clone)]
struct Tree {
    nodes: Vec<Node>,
    /// Tokens per block of the document.
    tok: Vec<usize>,
    headings: usize,
    inferred: usize,
}

/// The tokens of a `## title` line and its blank line.
fn head_tokens(title: &str) -> usize {
    tokens(title) + 3
}

/// The tokens of the line that replaces a copied section.
const DUP_LINE: usize = 24;

impl Tree {
    fn new(doc: &Document) -> Self {
        let tok: Vec<usize> = doc.blocks.iter().map(|b| tokens(&b.text) + 1).collect();
        let mut nodes = vec![Node {
            title: doc.title.clone(),
            ..Node::default()
        }];
        let mut stack = vec![0usize];
        let (mut headings, mut inferred) = (0, 0);
        for (i, block) in doc.blocks.iter().enumerate() {
            if !block.is_heading() {
                let top = stack.last().copied().unwrap_or(0);
                if let Some(n) = nodes.get_mut(top) {
                    n.blocks.push(i);
                }
                continue;
            }
            headings += 1;
            inferred += usize::from(block.inferred);
            while stack.len() > 1 {
                let top = stack.last().copied().unwrap_or(0);
                if nodes.get(top).is_some_and(|n| n.level >= block.level) {
                    stack.pop();
                } else {
                    break;
                }
            }
            let parent = stack.last().copied().unwrap_or(0);
            let id = nodes.len();
            nodes.push(Node {
                title: block.text.clone(),
                level: block.level,
                parent: Some(parent),
                head: head_tokens(&block.text),
                ..Node::default()
            });
            if let Some(p) = nodes.get_mut(parent) {
                p.children.push(id);
            }
            stack.push(id);
        }
        let mut tree = Self {
            nodes,
            tok,
            headings,
            inferred,
        };
        tree.collapse_root(doc);
        tree
    }

    /// A document whose text is all under one top heading (an `h1` page, a
    /// manual with one bookmark at the top) has that heading as its title.
    fn collapse_root(&mut self, doc: &Document) {
        loop {
            let Some(root) = self.nodes.first() else {
                return;
            };
            if !root.blocks.is_empty() || root.children.len() != 1 {
                return;
            }
            let Some(&child) = root.children.first() else {
                return;
            };
            let Some(c) = self.nodes.get_mut(child).map(std::mem::take) else {
                return;
            };
            for &g in &c.children {
                if let Some(n) = self.nodes.get_mut(g) {
                    n.parent = Some(0);
                }
            }
            if let Some(root) = self.nodes.get_mut(0) {
                if root.title.is_empty() || root.title == crate::doc::title_from_name(&doc.name) {
                    root.title = c.title;
                }
                root.blocks = c.blocks;
                root.children = c.children;
            }
        }
    }

    fn node(&self, n: usize) -> Option<&Node> {
        self.nodes.get(n)
    }

    fn total(&self, n: usize) -> usize {
        self.node(n).map_or(0, |x| x.total)
    }

    /// Own and total tokens, children before parents (a child's index is
    /// always larger than its parent's).
    fn sum(&mut self) {
        for n in (0..self.nodes.len()).rev() {
            let Some(node) = self.nodes.get(n) else {
                continue;
            };
            let (own, total) = if node.dup.is_some() {
                (DUP_LINE, node.head + DUP_LINE)
            } else {
                let own: usize = node.blocks.iter().filter_map(|&b| self.tok.get(b)).sum();
                let kids: usize = node.children.iter().map(|&c| self.total(c)).sum();
                (own, node.head + own + kids)
            };
            if let Some(node) = self.nodes.get_mut(n) {
                node.own = own;
                node.total = total;
            }
        }
    }

    /// The node's blocks and those of its descendants, in order.
    fn subtree_blocks(&self, n: usize, out: &mut Vec<usize>) {
        let Some(node) = self.node(n) else { return };
        out.extend(&node.blocks);
        for &c in &node.children {
            self.subtree_blocks(c, out);
        }
    }

    /// The titles from the root (excluded) down to `n`.
    fn path(&self, n: usize) -> Vec<String> {
        let mut out = Vec::new();
        let mut at = Some(n);
        while let Some(i) = at {
            let Some(node) = self.node(i) else { break };
            if node.parent.is_none() {
                break;
            }
            out.push(node.title.clone());
            at = node.parent;
        }
        out.reverse();
        out
    }
}

// ------------------------------------------------- boilerplate, duplicates

/// Drops the short blocks that repeat on at least half of the pages of a
/// web site of four or more pages (cookie bars, "Was this page helpful?").
fn drop_boilerplate(docs: &mut [Document]) -> usize {
    let mut sites: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, d) in docs.iter().enumerate() {
        if !d.site.is_empty() {
            sites.entry(d.site.clone()).or_default().push(i);
        }
    }
    let mut dropped = 0;
    for pages in sites.values().filter(|p| p.len() >= 4) {
        let mut seen: HashMap<String, usize> = HashMap::new();
        for &p in pages {
            let Some(d) = docs.get(p) else { continue };
            let distinct: BTreeSet<String> = d
                .blocks
                .iter()
                .filter(|b| !b.is_heading() && b.text.len() <= 600)
                .map(|b| squash(&b.text))
                .collect();
            for text in distinct {
                *seen.entry(text).or_default() += 1;
            }
        }
        let limit = pages.len().div_ceil(2).max(3);
        let common: BTreeSet<&String> = seen
            .iter()
            .filter(|(_, n)| **n >= limit)
            .map(|(t, _)| t)
            .collect();
        if common.is_empty() {
            continue;
        }
        for &p in pages {
            let Some(d) = docs.get_mut(p) else { continue };
            let before = d.blocks.len();
            d.blocks
                .retain(|b| b.is_heading() || !common.contains(&squash(&b.text)));
            dropped += before - d.blocks.len();
        }
    }
    dropped
}

/// Marks every section of at least `min_section` tokens whose text repeats
/// an earlier section word for word (KD13). Returns how many.
fn mark_duplicates(docs: &[Document], trees: &mut [Tree], settings: &Settings) -> usize {
    for tree in trees.iter_mut() {
        tree.sum();
    }
    let mut first: HashMap<blake3::Hash, (usize, usize)> = HashMap::new();
    let mut marked = 0;
    for (d, doc) in docs.iter().enumerate() {
        let Some(tree) = trees.get(d) else { continue };
        let mut found: Vec<(usize, (usize, usize))> = Vec::new();
        let mut stack = vec![0usize];
        while let Some(n) = stack.pop() {
            let Some(node) = tree.node(n) else { continue };
            if node.total < settings.min_section.max(1) {
                continue;
            }
            // The text with the titles of the sub-sections (not the node's
            // own): a section with one sub-section is not a copy of it.
            let mut hasher = blake3::Hasher::new();
            let mut todo = vec![(n, false)];
            while let Some((m, titled)) = todo.pop() {
                let Some(x) = tree.node(m) else { continue };
                if titled {
                    hasher.update(b"#");
                    hasher.update(squash(&x.title).to_lowercase().as_bytes());
                    hasher.update(b"\n");
                }
                for b in x.blocks.iter().filter_map(|&b| doc.blocks.get(b)) {
                    hasher.update(squash(&b.text).to_lowercase().as_bytes());
                    hasher.update(b"\n");
                }
                todo.extend(x.children.iter().rev().map(|&c| (c, true)));
            }
            let hash = hasher.finalize();
            if let Some(&orig) = first.get(&hash) {
                found.push((n, orig));
                continue;
            }
            first.insert(hash, (d, n));
            stack.extend(node.children.iter().rev());
        }
        if let Some(tree) = trees.get_mut(d) {
            for (n, orig) in found {
                if let Some(node) = tree.nodes.get_mut(n) {
                    node.dup = Some(orig);
                    marked += 1;
                }
            }
        }
    }
    marked
}

// --------------------------------------------------------------- cutting

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    /// The node and everything below it.
    Whole(usize),
    /// The node's own text before its first sub-section.
    Intro(usize),
}

#[derive(Debug, Clone)]
enum Body {
    /// One part, or several packed as `##` sections.
    Parts(Vec<Part>),
    /// A piece of a section too long for one room.
    Chunk { node: usize, pieces: Vec<Piece> },
    /// The overview of a document: its intro text and contents.
    Overview { intro: Vec<usize> },
}

#[derive(Debug, Clone)]
struct Piece {
    block: usize,
    text: String,
}

#[derive(Debug, Clone)]
struct Draft {
    doc: usize,
    anchor: usize,
    title: String,
    key: String,
    body: Body,
    id: String,
}

struct Planner<'a> {
    settings: Settings,
    docs: &'a [Document],
    trees: &'a [Tree],
    drafts: Vec<Draft>,
}

impl Planner<'_> {
    fn document(&mut self, d: usize) {
        let (Some(doc), Some(tree)) = (self.docs.get(d), self.trees.get(d)) else {
            return;
        };
        if tree.total(0) <= self.settings.room_max {
            let title = tree.node(0).map(|n| n.title.clone()).unwrap_or_default();
            self.push(
                d,
                0,
                title,
                doc.key.clone(),
                Body::Parts(vec![Part::Whole(0)]),
            );
            return;
        }
        let start = self.drafts.len();
        let has_intro = tree.node(0).is_some_and(|n| !n.blocks.is_empty());
        let budget = contents_budget(&self.settings);
        let intro_fits = tree
            .node(0)
            .is_some_and(|n| n.own + budget + 100 <= self.settings.room_max);
        let has_children = tree.node(0).is_some_and(|n| !n.children.is_empty());
        if self.settings.overview && has_children && (!has_intro || intro_fits) {
            self.cut(d, 0, false);
            if self.drafts.len() - start >= 2 {
                let intro = tree.node(0).map(|n| n.blocks.clone()).unwrap_or_default();
                let title = tree.node(0).map(|n| n.title.clone()).unwrap_or_default();
                let overview = Draft {
                    doc: d,
                    anchor: 0,
                    title,
                    key: format!("{}#overview", doc.key),
                    body: Body::Overview { intro },
                    id: String::new(),
                };
                self.drafts.insert(start, overview);
                return;
            }
            self.drafts.truncate(start);
        }
        self.cut(d, 0, true);
    }

    fn push(&mut self, doc: usize, anchor: usize, title: String, key: String, body: Body) {
        self.drafts.push(Draft {
            doc,
            anchor,
            title,
            key,
            body,
            id: String::new(),
        });
    }

    fn part_tokens(&self, d: usize, part: Part) -> usize {
        let Some(tree) = self.trees.get(d) else {
            return 0;
        };
        match part {
            Part::Whole(n) => tree.total(n),
            Part::Intro(n) => tree.node(n).map_or(0, |x| x.head + x.own),
        }
    }

    /// Cuts node `n`, too large for one room, into rooms.
    fn cut(&mut self, d: usize, n: usize, with_intro: bool) {
        let Some(tree) = self.trees.get(d) else {
            return;
        };
        let Some(node) = tree.node(n) else { return };
        if node.children.is_empty() || node.dup.is_some() {
            self.split(d, n);
            return;
        }
        let mut parts = Vec::new();
        if with_intro && !node.blocks.is_empty() {
            parts.push(Part::Intro(n));
        }
        parts.extend(node.children.iter().map(|&c| Part::Whole(c)));
        let (max, min) = (self.settings.room_max, self.settings.min_section);
        let mut acc: Vec<Part> = Vec::new();
        let (mut acc_tokens, mut acc_big) = (0usize, false);
        for part in parts {
            let t = self.part_tokens(d, part);
            if t > max {
                self.flush(d, &mut acc);
                match part {
                    Part::Whole(c) => self.cut(d, c, true),
                    Part::Intro(i) => self.split(d, i),
                }
                continue;
            }
            let fits = !acc.is_empty() && acc_tokens + t <= max;
            if fits && (t < min || !acc_big) {
                acc.push(part);
                acc_tokens += t;
                acc_big |= t >= min;
            } else {
                self.flush(d, &mut acc);
                acc.push(part);
                acc_tokens = t;
                acc_big = t >= min;
            }
        }
        self.flush(d, &mut acc);
    }

    fn flush(&mut self, d: usize, acc: &mut Vec<Part>) {
        if acc.is_empty() {
            return;
        }
        let parts = std::mem::take(acc);
        let (Some(doc), Some(tree)) = (self.docs.get(d), self.trees.get(d)) else {
            return;
        };
        let title_of = |p: &Part| match p {
            Part::Whole(n) | Part::Intro(n) => {
                tree.node(*n).map(|x| x.title.clone()).unwrap_or_default()
            }
        };
        let (first, last) = (parts.first().copied(), parts.last().copied());
        let anchor = match first {
            Some(Part::Whole(n) | Part::Intro(n)) => n,
            None => 0,
        };
        let title = match (first, last) {
            (Some(f @ Part::Intro(_)), _) => title_of(&f),
            (Some(f), Some(l)) if parts.len() > 1 => format!("{} - {}", title_of(&f), title_of(&l)),
            (Some(f), _) => title_of(&f),
            (None, _) => String::new(),
        };
        let mut key = format!("{}#{}", doc.key, tree.path(anchor).join("/"));
        if matches!(first, Some(Part::Intro(_))) {
            key.push_str("#intro");
        }
        self.push(d, anchor, title, key, Body::Parts(parts));
    }

    /// Splits the own text of node `n` (a section without sub-sections, or
    /// an intro too long for a room) into rooms of the frame size.
    fn split(&mut self, d: usize, n: usize) {
        let (Some(doc), Some(tree)) = (self.docs.get(d), self.trees.get(d)) else {
            return;
        };
        let Some(node) = tree.node(n) else { return };
        let max = self
            .settings
            .room_max
            .saturating_sub(node.head + 40)
            .max(200);
        let mut pieces: Vec<Piece> = Vec::new();
        for &b in &node.blocks {
            let Some(block) = doc.blocks.get(b) else {
                continue;
            };
            let t = tree.tok.get(b).copied().unwrap_or(0);
            if t <= max {
                pieces.push(Piece {
                    block: b,
                    text: block.text.clone(),
                });
            } else {
                pieces.extend(
                    split_block(block.kind, &block.text, max)
                        .into_iter()
                        .map(|text| Piece { block: b, text }),
                );
            }
        }
        let mut chunks: Vec<Vec<Piece>> = Vec::new();
        let mut size = 0usize;
        for piece in pieces {
            let t = tokens(&piece.text) + 1;
            match chunks.last_mut() {
                Some(chunk) if size + t <= max => {
                    chunk.push(piece);
                    size += t;
                }
                _ => {
                    chunks.push(vec![piece]);
                    size = t;
                }
            }
        }
        let count = chunks.len();
        let base = format!("{}#{}", doc.key, tree.path(n).join("/"));
        for (k, pieces) in chunks.into_iter().enumerate() {
            let title = chunk_title(doc, &node.title, &pieces, k + 1, count);
            let key = if count == 1 {
                base.clone()
            } else {
                format!("{base}#{}", k + 1)
            };
            self.push(d, n, title, key, Body::Chunk { node: n, pieces });
        }
    }
}

/// The tokens an overview's contents list may take: a fifth of the frame,
/// at most 700 (KD12: an overview of at most about 800 tokens).
fn contents_budget(settings: &Settings) -> usize {
    (settings.room_max / 5).min(700)
}

/// `Title, p. 12-15` for a paged source, else `Title (2/5)`.
fn chunk_title(doc: &Document, title: &str, pieces: &[Piece], k: usize, count: usize) -> String {
    if count <= 1 {
        return title.to_owned();
    }
    let pages: Vec<u32> = pieces
        .iter()
        .filter_map(|p| doc.blocks.get(p.block))
        .map(|b| b.page)
        .filter(|&p| p > 0)
        .collect();
    match (pages.iter().min(), pages.iter().max()) {
        (Some(&a), Some(&b)) if a == b => format!("{title}, p. {}", doc.page_label(a)),
        (Some(&a), Some(&b)) => format!("{title}, p. {}-{}", doc.page_label(a), doc.page_label(b)),
        _ => format!("{title} ({k}/{count})"),
    }
}

/// Splits one block longer than `max` tokens: a table by rows with its
/// header repeated, a code block by lines with its fence repeated, other
/// text by lines, then by sentences.
fn split_block(kind: Kind, text: &str, max: usize) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    match kind {
        Kind::Table if lines.len() > 2 => {
            let head = lines.get(..2).map(|h| h.join("\n")).unwrap_or_default();
            group(
                lines.get(2..).unwrap_or_default(),
                max.saturating_sub(tokens(&head)).max(50),
            )
            .into_iter()
            .map(|rows| format!("{head}\n{rows}"))
            .collect()
        }
        Kind::Code if lines.len() > 2 => {
            let open = lines.first().copied().unwrap_or("```");
            let close = lines.last().copied().unwrap_or("```");
            let inner = lines.get(1..lines.len() - 1).unwrap_or_default();
            group(inner, max.saturating_sub(10).max(50))
                .into_iter()
                .map(|body| format!("{open}\n{body}\n{close}"))
                .collect()
        }
        _ => {
            let mut units: Vec<String> = Vec::new();
            for line in lines {
                if tokens(line) <= max {
                    units.push(line.to_owned());
                } else {
                    units.extend(sentences(line, max));
                }
            }
            let refs: Vec<&str> = units.iter().map(String::as_str).collect();
            group(&refs, max)
        }
    }
}

/// Lines joined into groups of at most `max` tokens.
fn group(lines: &[&str], max: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut size = 0usize;
    for line in lines {
        let t = tokens(line) + 1;
        match out.last_mut() {
            Some(last) if size + t <= max => {
                last.push('\n');
                last.push_str(line);
                size += t;
            }
            _ => {
                out.push((*line).to_owned());
                size = t;
            }
        }
    }
    out
}

/// A very long line cut at sentence ends, or at spaces.
fn sentences(line: &str, max: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut size = 0usize;
    for word in line.split_inclusive(' ') {
        let t = tokens(word);
        match out.last_mut() {
            Some(last) if size + t <= max => {
                last.push_str(word);
                size += t;
            }
            _ => {
                out.push(word.to_owned());
                size = t;
            }
        }
    }
    out.into_iter().map(|s| s.trim().to_owned()).collect()
}

// --------------------------------------------------------- titles and ids

/// Titles that repeat in the file get their parent section, then their
/// document, in brackets.
fn unique_titles(drafts: &mut [Draft], docs: &[Document], trees: &[Tree]) {
    for round in 0..2 {
        let mut count: HashMap<String, usize> = HashMap::new();
        for d in drafts.iter() {
            *count.entry(d.title.to_lowercase()).or_default() += 1;
        }
        for draft in drafts.iter_mut() {
            if count.get(&draft.title.to_lowercase()).copied().unwrap_or(0) < 2 {
                continue;
            }
            let (Some(doc), Some(tree)) = (docs.get(draft.doc), trees.get(draft.doc)) else {
                continue;
            };
            let extra = if round == 0 && !matches!(draft.body, Body::Overview { .. }) {
                let parent = tree.node(draft.anchor).and_then(|n| n.parent);
                match parent {
                    Some(p) if p != 0 => tree.node(p).map(|n| n.title.clone()).unwrap_or_default(),
                    Some(_) => doc.title.clone(),
                    None => doc.name.clone(),
                }
            } else {
                doc.name.clone()
            };
            if !extra.is_empty() && !draft.title.contains(&extra) {
                draft.title = format!("{} ({})", draft.title, crate::text::shorten(&extra, 80));
            }
        }
    }
}

/// The id prefix of a document: the path of a web page, else its title.
fn prefix(doc: &Document) -> String {
    if let Ok(url) = url::Url::parse(&doc.key)
        && matches!(url.scheme(), "http" | "https")
    {
        let path = url.path().trim_matches('/');
        // `guide/intro.html` and `guide/intro` give the same id.
        let path = match path.rsplit_once('.') {
            Some((stem, ext)) if !ext.contains('/') && ext.len() <= 5 => stem,
            _ => path,
        };
        let base = if path.is_empty() {
            url.host_str().unwrap_or("index").to_owned()
        } else {
            path.to_owned()
        };
        return slug(&base, 48);
    }
    let title = if doc.title.trim().is_empty() {
        &doc.name
    } else {
        &doc.title
    };
    slug(title, 24)
}

fn take_id(base: &str, used: &mut BTreeSet<String>) -> String {
    let base = if kapu_core::is_valid_id(base) {
        base.to_owned()
    } else {
        "room".to_owned()
    };
    let mut id = base.clone();
    let mut n = 2usize;
    while used.contains(&id) {
        let suffix = format!("-{n}");
        let cut = base
            .get(..64usize.saturating_sub(suffix.len()).min(base.len()))
            .unwrap_or(&base);
        id = format!("{}{suffix}", cut.trim_end_matches('-'));
        n += 1;
    }
    used.insert(id.clone());
    id
}

/// Gives every draft its id: the one of the earlier run for the same
/// section if free, else `<prefix>-<number or slug>`.
fn assign_ids(
    drafts: &mut [Draft],
    docs: &[Document],
    previous: &BTreeMap<String, String>,
    reserved: &BTreeSet<String>,
) -> BTreeMap<String, String> {
    let mut used: BTreeSet<String> = reserved.clone();
    for draft in drafts.iter_mut() {
        if let Some(id) = previous.get(&draft.key)
            && kapu_core::is_valid_id(id)
            && !used.contains(id)
        {
            used.insert(id.clone());
            draft.id.clone_from(id);
        }
    }
    let mut per_doc: HashMap<usize, usize> = HashMap::new();
    for d in drafts.iter() {
        *per_doc.entry(d.doc).or_default() += 1;
    }
    let mut prefixes: HashMap<usize, String> = HashMap::new();
    let mut taken_prefixes: BTreeSet<String> = BTreeSet::new();
    for draft in drafts.iter_mut().filter(|d| d.id.is_empty()) {
        let Some(doc) = docs.get(draft.doc) else {
            continue;
        };
        let p = prefixes
            .entry(draft.doc)
            .or_insert_with(|| {
                let base = prefix(doc);
                let mut p = base.clone();
                let mut n = 2;
                while taken_prefixes.contains(&p) {
                    p = format!("{base}-{n}");
                    n += 1;
                }
                taken_prefixes.insert(p.clone());
                p
            })
            .clone();
        let single = per_doc.get(&draft.doc).copied().unwrap_or(1) == 1;
        let base = if single || matches!(draft.body, Body::Overview { .. }) {
            p
        } else {
            let part = numbering(&draft.title).unwrap_or_else(|| slug(&draft.title, 40));
            let room = 63usize.saturating_sub(part.len()).max(8);
            let p = p
                .get(..room.min(p.len()))
                .unwrap_or(&p)
                .trim_end_matches('-');
            format!("{p}-{part}")
        };
        let base = base
            .get(..64.min(base.len()))
            .unwrap_or(&base)
            .trim_matches('-')
            .to_owned();
        draft.id = take_id(&base, &mut used);
    }
    drafts
        .iter()
        .map(|d| (d.key.clone(), d.id.clone()))
        .collect()
}

// -------------------------------------------------------------- rendering

struct Render<'a> {
    docs: &'a [Document],
    trees: &'a [Tree],
    drafts: &'a [Draft],
    /// (document, node) to the draft where the node's heading lives.
    home: HashMap<(usize, usize), usize>,
    /// Normalised document key to the id of its first room.
    targets: HashMap<String, String>,
    /// Drafts per document, in order.
    by_doc: BTreeMap<usize, Vec<usize>>,
    /// Token budget of an overview's contents list.
    budget: usize,
    links: usize,
}

impl<'a> Render<'a> {
    fn new(docs: &'a [Document], trees: &'a [Tree], drafts: &'a [Draft], budget: usize) -> Self {
        let mut home = HashMap::new();
        let mut by_doc: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (i, draft) in drafts.iter().enumerate() {
            by_doc.entry(draft.doc).or_default().push(i);
            match &draft.body {
                Body::Parts(parts) => {
                    for part in parts {
                        let (Part::Whole(n) | Part::Intro(n)) = part;
                        home.entry((draft.doc, *n)).or_insert(i);
                    }
                }
                Body::Chunk { node, .. } => {
                    home.entry((draft.doc, *node)).or_insert(i);
                }
                Body::Overview { .. } => {
                    home.insert((draft.doc, 0), i);
                }
            }
        }
        let mut targets = HashMap::new();
        for (d, list) in &by_doc {
            if let (Some(doc), Some(first)) =
                (docs.get(*d), list.first().and_then(|&i| drafts.get(i)))
            {
                targets
                    .entry(norm_key(&doc.key))
                    .or_insert_with(|| first.id.clone());
            }
        }
        Self {
            docs,
            trees,
            drafts,
            home,
            targets,
            by_doc,
            budget,
            links: 0,
        }
    }

    /// The draft that holds node `n` of document `d`: its own, or the
    /// nearest ancestor's.
    fn holder(&self, d: usize, n: usize) -> Option<usize> {
        let tree = self.trees.get(d)?;
        let mut at = Some(n);
        while let Some(i) = at {
            if let Some(&draft) = self.home.get(&(d, i)) {
                return Some(draft);
            }
            at = tree.node(i).and_then(|x| x.parent);
        }
        None
    }

    fn room(&mut self, i: usize) -> RoomSource {
        let Some(draft) = self.drafts.get(i) else {
            return RoomSource {
                id: String::new(),
                content: String::new(),
            };
        };
        let mut out = format!("# {}\n", one_line(&draft.title));
        let (Some(doc), Some(tree)) = (self.docs.get(draft.doc), self.trees.get(draft.doc)) else {
            return RoomSource {
                id: draft.id.clone(),
                content: out,
            };
        };
        let first_of_doc = self.by_doc.get(&draft.doc).and_then(|l| l.first()) == Some(&i);
        if first_of_doc && !doc.description.trim().is_empty() {
            let _ = write!(out, "\n> {}\n", one_line(&doc.description));
        }
        let blocks = self.blocks_of(draft);
        let _ = write!(
            out,
            "\nSource: {}{}\n",
            doc.name,
            pages(doc, &blocks, matches!(draft.body, Body::Overview { .. }))
        );
        let path = self.path_line(i, draft, tree);
        if !path.is_empty() {
            let _ = writeln!(out, "Path: {path}");
        }
        out.push('\n');
        match &draft.body {
            Body::Parts(parts) => self.parts(&mut out, draft.doc, parts),
            Body::Chunk { pieces, .. } => {
                for piece in pieces {
                    let kind = doc
                        .blocks
                        .get(piece.block)
                        .map_or(Kind::Paragraph, |b| b.kind);
                    self.text(&mut out, doc, kind, &piece.text);
                }
            }
            Body::Overview { intro } => {
                for &b in intro {
                    self.block(&mut out, draft.doc, b);
                }
                self.contents(&mut out, i, draft);
            }
        }
        RoomSource {
            id: draft.id.clone(),
            content: format!("{}\n", out.trim_end()),
        }
    }

    fn blocks_of(&self, draft: &Draft) -> Vec<usize> {
        let Some(tree) = self.trees.get(draft.doc) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        match &draft.body {
            Body::Parts(parts) => {
                for part in parts {
                    match part {
                        Part::Whole(n) => tree.subtree_blocks(*n, &mut out),
                        Part::Intro(n) => {
                            out.extend(tree.node(*n).map(|x| x.blocks.clone()).unwrap_or_default());
                        }
                    }
                }
            }
            Body::Chunk { pieces, .. } => out.extend(pieces.iter().map(|p| p.block)),
            Body::Overview { .. } => {}
        }
        out
    }

    /// `[Manual](#manual) > [3 Parameters](#manual-3)`: the rooms of the
    /// ancestors, outermost first.
    fn path_line(&self, i: usize, draft: &Draft, tree: &Tree) -> String {
        let mut chain = Vec::new();
        let mut at = tree.node(draft.anchor).and_then(|n| n.parent);
        if matches!(draft.body, Body::Overview { .. }) {
            at = None;
        }
        while let Some(n) = at {
            if let Some(&h) = self.home.get(&(draft.doc, n))
                && h != i
                && let (Some(target), Some(node)) = (self.drafts.get(h), tree.node(n))
            {
                let title = if n == 0 {
                    target.title.clone()
                } else {
                    node.title.clone()
                };
                chain.push(format!("[{}](#{})", link_text(&title), target.id));
            }
            at = tree.node(n).and_then(|x| x.parent);
        }
        chain.reverse();
        chain.dedup();
        chain.join(" > ")
    }

    fn parts(&mut self, out: &mut String, d: usize, parts: &[Part]) {
        let Some(tree) = self.trees.get(d) else {
            return;
        };
        let single = parts.len() == 1;
        for part in parts {
            match *part {
                Part::Intro(n) => {
                    for b in tree.node(n).map(|x| x.blocks.clone()).unwrap_or_default() {
                        self.block(out, d, b);
                    }
                }
                Part::Whole(n) if single => self.node(out, d, n, 2),
                Part::Whole(n) => {
                    let title = tree.node(n).map(|x| x.title.clone()).unwrap_or_default();
                    heading(out, 2, &title);
                    self.node(out, d, n, 3);
                }
            }
        }
    }

    /// The node's own text, then its sub-sections with headings of `depth`.
    fn node(&mut self, out: &mut String, d: usize, n: usize, depth: usize) {
        let Some(tree) = self.trees.get(d) else {
            return;
        };
        let Some(node) = tree.node(n) else { return };
        if let Some((od, on)) = node.dup {
            let target = self.holder(od, on).and_then(|h| self.drafts.get(h));
            if let Some(target) = target {
                let _ = write!(
                    out,
                    "Same text as in [{}](#{}).\n\n",
                    link_text(&target.title),
                    target.id
                );
                return;
            }
        }
        for b in node.blocks.clone() {
            self.block(out, d, b);
        }
        for c in node.children.clone() {
            let title = tree.node(c).map(|x| x.title.clone()).unwrap_or_default();
            heading(out, depth, &title);
            self.node(out, d, c, depth + 1);
        }
    }

    fn block(&mut self, out: &mut String, d: usize, b: usize) {
        let Some(doc) = self.docs.get(d) else { return };
        let Some(block) = doc.blocks.get(b) else {
            return;
        };
        self.text(out, doc, block.kind, &block.text);
    }

    fn text(&mut self, out: &mut String, doc: &Document, kind: Kind, text: &str) {
        if kind == Kind::Code {
            out.push_str(text);
        } else {
            let (text, n) = rewrite_links(text, &doc.key, &self.targets);
            self.links += n;
            out.push_str(&text);
        }
        out.push_str("\n\n");
    }

    /// The contents list of an overview: the document's other rooms of the
    /// top level, within the contents budget.
    fn contents(&self, out: &mut String, i: usize, draft: &Draft) {
        let Some(tree) = self.trees.get(draft.doc) else {
            return;
        };
        let Some(list) = self.by_doc.get(&draft.doc) else {
            return;
        };
        let top: Vec<&Draft> = list
            .iter()
            .filter(|&&j| j != i)
            .filter_map(|&j| self.drafts.get(j))
            .filter(|x| x.anchor == 0 || tree.node(x.anchor).and_then(|n| n.parent) == Some(0))
            .collect();
        let items: Vec<&Draft> = if top.is_empty() {
            list.iter()
                .filter(|&&j| j != i)
                .filter_map(|&j| self.drafts.get(j))
                .collect()
        } else {
            top
        };
        out.push_str("## Contents\n\n");
        let mut used = 0usize;
        for (k, x) in items.iter().enumerate() {
            let line = format!("- [{}](#{})\n", link_text(&x.title), x.id);
            used += tokens(&line);
            if used > self.budget {
                let _ = writeln!(out, "- ... and {} more rooms", items.len() - k);
                break;
            }
            out.push_str(&line);
        }
        out.push('\n');
    }
}

fn heading(out: &mut String, depth: usize, title: &str) {
    let title = one_line(title);
    if depth <= 6 {
        let _ = write!(out, "{} {title}\n\n", "#".repeat(depth));
    } else {
        let _ = write!(out, "**{title}**\n\n");
    }
}

fn one_line(text: &str) -> String {
    squash(text)
}

/// Link text without the brackets that would end it.
fn link_text(text: &str) -> String {
    one_line(text).replace(['[', ']'], "")
}

/// `, p. 12-15` for the pages of `blocks`, or the page count for an
/// overview.
fn pages(doc: &Document, blocks: &[usize], overview: bool) -> String {
    if doc.pages == 0 {
        return String::new();
    }
    if overview {
        return format!(", {} pages", doc.pages);
    }
    let pages: Vec<u32> = blocks
        .iter()
        .filter_map(|&b| doc.blocks.get(b))
        .map(|b| b.page)
        .filter(|&p| p > 0)
        .collect();
    match (pages.iter().min(), pages.iter().max()) {
        (Some(&a), Some(&b)) if a == b => format!(", p. {}", doc.page_label(a)),
        (Some(&a), Some(&b)) => format!(", p. {}-{}", doc.page_label(a), doc.page_label(b)),
        _ => String::new(),
    }
}

// ------------------------------------------------------------------ links

/// A document key in the form links are compared: a URL without fragment
/// and trailing slash; a path with `/`, lower case.
#[must_use]
pub fn norm_key(key: &str) -> String {
    if let Ok(mut url) = url::Url::parse(key)
        && matches!(url.scheme(), "http" | "https")
    {
        url.set_fragment(None);
        let s = url.to_string();
        return s.strip_suffix('/').map_or(s.clone(), str::to_owned);
    }
    norm_path(&key.replace('\\', "/")).to_lowercase()
}

/// `a/b/../c/./d` to `a/c/d`.
fn norm_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "." => {}
            ".." if parts.last().is_some_and(|p| *p != ".." && !p.is_empty()) => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    parts.join("/")
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'%' {
            let hex = bytes
                .get(i + 1..i + 3)
                .and_then(|h| std::str::from_utf8(h).ok());
            if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The document key a link `target` in a document with key `base` points
/// to, normalised as [`norm_key`].
fn resolve(base: &str, target: &str) -> Option<String> {
    let target = target.trim().trim_matches(['<', '>']);
    if target.is_empty() || target.starts_with('#') || target.starts_with("mailto:") {
        return None;
    }
    // A reader's link to another document of the run by its key (a mail's
    // attachments).
    if let Some(key) = target.strip_prefix("kapu-key:") {
        return Some(norm_key(key));
    }
    if let Ok(url) = url::Url::parse(base)
        && matches!(url.scheme(), "http" | "https")
    {
        return url.join(target).ok().map(|u| norm_key(u.as_str()));
    }
    if target.contains("://") || target.starts_with("data:") {
        return None;
    }
    let target = target.split(['#', '?']).next().unwrap_or(target);
    let base = base.replace('\\', "/");
    let dir = base.rsplit_once('/').map_or("", |(d, _)| d);
    let joined = if dir.is_empty() {
        percent_decode(target)
    } else {
        format!("{dir}/{}", percent_decode(target))
    };
    Some(norm_key(&joined))
}

/// Rewrites the Markdown links of `text` that point to a document of the
/// run into room links; on a web page, the other relative links become
/// absolute. Returns the text and the number of room links made.
fn rewrite_links(text: &str, base: &str, targets: &HashMap<String, String>) -> (String, usize) {
    let web = url::Url::parse(base)
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https"));
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut made = 0;
    while let Some(i) = rest.find("](") {
        let (head, tail) = rest.split_at(i + 2);
        out.push_str(head);
        let angle = if tail.starts_with('<') {
            tail.find('>')
        } else {
            None
        };
        let Some(end) = tail
            .get(angle.unwrap_or(0)..)
            .and_then(|t| t.find(')'))
            .map(|e| e + angle.unwrap_or(0))
        else {
            rest = tail;
            break;
        };
        let inner = tail.get(..end).unwrap_or_default();
        let split = match angle {
            Some(a) => Some(a + 1).filter(|&a| a < inner.len()),
            None => inner.find(char::is_whitespace),
        };
        let (target, title) = match split {
            Some(s) => (
                inner.get(..s).unwrap_or(inner),
                inner.get(s..).unwrap_or_default(),
            ),
            None => (inner, ""),
        };
        if let Some(id) = resolve(base, target).and_then(|k| targets.get(&k)) {
            out.push('#');
            out.push_str(id);
            made += 1;
        } else if let Some(abs) = web
            .as_ref()
            .filter(|_| !target.starts_with(['#', '<']) && !target.contains("://"))
            .and_then(|u| u.join(target).ok())
        {
            out.push_str(abs.as_str());
            out.push_str(title);
        } else {
            out.push_str(inner);
        }
        out.push(')');
        rest = tail.get(end + 1..).unwrap_or_default();
    }
    out.push_str(rest);
    (out, made)
}

// ----------------------------------------------------------------- report

fn doc_plans(
    docs: &[Document],
    trees: &[Tree],
    drafts: &[Draft],
    rooms: &[RoomSource],
) -> Vec<DocPlan> {
    let mut out: Vec<DocPlan> = Vec::new();
    for (d, doc) in docs.iter().enumerate() {
        let Some(tree) = trees.get(d) else { continue };
        let list: Vec<RoomInfo> = drafts
            .iter()
            .zip(rooms)
            .filter(|(x, _)| x.doc == d)
            .map(|(x, room)| RoomInfo {
                id: room.id.clone(),
                title: x.title.clone(),
                tokens: tokens(&room.content),
                sections: room
                    .content
                    .lines()
                    .filter_map(|l| l.strip_prefix("## "))
                    .map(str::to_owned)
                    .collect(),
                pages: room_pages(doc, tree, x),
            })
            .collect();
        let tier = if tree.headings == 0 {
            if list.len() > 1 {
                Tier::Fallback
            } else {
                Tier::Flat
            }
        } else if tree.inferred * 2 > tree.headings {
            Tier::Inferred
        } else {
            Tier::Explicit
        };
        out.push(DocPlan {
            key: doc.key.clone(),
            title: doc.title.clone(),
            tier,
            rooms: list,
        });
    }
    out
}

fn room_pages(doc: &Document, tree: &Tree, draft: &Draft) -> Option<(u32, u32)> {
    if doc.pages == 0 {
        return None;
    }
    let mut blocks = Vec::new();
    match &draft.body {
        Body::Parts(parts) => {
            for part in parts {
                match part {
                    Part::Whole(n) => tree.subtree_blocks(*n, &mut blocks),
                    Part::Intro(n) => {
                        blocks.extend(tree.node(*n).map(|x| x.blocks.clone()).unwrap_or_default());
                    }
                }
            }
        }
        Body::Chunk { pieces, .. } => blocks.extend(pieces.iter().map(|p| p.block)),
        Body::Overview { intro } => blocks.extend(intro),
    }
    let pages: Vec<u32> = blocks
        .iter()
        .filter_map(|&b| doc.blocks.get(b))
        .map(|b| b.page)
        .filter(|&p| p > 0)
        .collect();
    Some((*pages.iter().min()?, *pages.iter().max()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Block;

    fn doc(key: &str, title: &str, blocks: Vec<Block>) -> Document {
        let mut d = Document::new(key, key, "test");
        d.title = title.to_owned();
        d.blocks = blocks;
        d
    }

    fn para(words: usize, seed: &str) -> Block {
        let text: Vec<String> = (0..words).map(|i| format!("{seed}{i}")).collect();
        Block::new(Kind::Paragraph, text.join(" "))
    }

    fn small() -> Settings {
        Settings {
            room_max: 400,
            sub_max: 150,
            min_section: 60,
            overview: true,
            dedupe: true,
        }
    }

    #[test]
    fn a_small_document_is_one_room() {
        let d = doc(
            "a.md",
            "Guide",
            vec![
                Block::heading(1, "Guide"),
                para(20, "w"),
                Block::heading(2, "Part"),
                para(20, "p"),
            ],
        );
        let p = plan(
            &[d],
            &Settings::default(),
            &BTreeMap::new(),
            &BTreeSet::new(),
        );
        assert_eq!(p.rooms.len(), 1);
        let room = p
            .rooms
            .first()
            .map(|r| r.content.clone())
            .unwrap_or_default();
        assert!(room.starts_with("# Guide\n\nSource: a.md\n\n"), "{room}");
        assert!(room.contains("\n## Part\n"));
        assert_eq!(p.rooms.first().map(|r| r.id.as_str()), Some("guide"));
    }

    #[test]
    fn a_large_document_is_cut_at_its_chapters_with_an_overview() {
        let mut blocks = vec![para(30, "intro")];
        for c in 1..=4 {
            blocks.push(Block::heading(1, format!("{c} Chapter {c}")));
            blocks.push(para(60, &format!("c{c}x")));
            for s in 1..=3 {
                blocks.push(Block::heading(2, format!("{c}.{s} Section")));
                blocks.push(para(50, &format!("c{c}s{s}y")));
            }
        }
        let d = doc("m.pdf", "Manual", blocks);
        let p = plan(&[d], &small(), &BTreeMap::new(), &BTreeSet::new());
        let ids: Vec<&str> = p.rooms.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids.first(), Some(&"manual"), "{ids:?}");
        assert!(ids.contains(&"manual-1"), "{ids:?}");
        let overview = p
            .rooms
            .first()
            .map(|r| r.content.clone())
            .unwrap_or_default();
        assert!(overview.contains("## Contents"), "{overview}");
        assert!(
            overview.contains("intro0"),
            "the intro text is in the overview"
        );
        let chapter = p
            .rooms
            .iter()
            .find(|r| r.id == "manual-1")
            .map(|r| r.content.clone())
            .unwrap_or_default();
        assert!(chapter.contains("Path: [Manual](#manual)"), "{chapter}");
        // every word lands exactly once
        for c in 1..=4 {
            for s in 1..=3 {
                let word = format!("c{c}s{s}y7 ");
                let n: usize = p
                    .rooms
                    .iter()
                    .map(|r| r.content.matches(&word).count())
                    .sum();
                assert_eq!(n, 1, "{word}");
            }
        }
    }

    #[test]
    fn small_sections_are_packed() {
        let mut blocks = Vec::new();
        for i in 1..=12 {
            blocks.push(Block::heading(1, format!("P-{i:03} Param")));
            blocks.push(para(12, &format!("p{i}x")));
        }
        blocks.push(Block::heading(1, "Big"));
        blocks.push(para(250, "big"));
        let d = doc("f.pdf", "Firmware", blocks);
        let p = plan(&[d], &small(), &BTreeMap::new(), &BTreeSet::new());
        let packed = p
            .rooms
            .iter()
            .filter(|r| r.content.contains("## P-001 Param"))
            .count();
        assert_eq!(packed, 1);
        assert!(p.rooms.len() < 8, "{}", p.rooms.len());
    }

    #[test]
    fn a_long_section_without_headings_is_split_at_blocks() {
        let blocks: Vec<Block> = (0..30)
            .map(|i| para(40, &format!("b{i}x")).on_page(i / 3 + 1))
            .collect();
        let mut d = doc("scan.pdf", "Scan", blocks);
        d.pages = 10;
        let p = plan(&[d], &small(), &BTreeMap::new(), &BTreeSet::new());
        assert!(p.rooms.len() >= 3);
        assert!(
            p.rooms.iter().all(|r| tokens(&r.content) <= 450),
            "frame kept"
        );
        let first = p
            .rooms
            .first()
            .map(|r| r.content.clone())
            .unwrap_or_default();
        assert!(first.starts_with("# Scan, p. 1"), "{first}");
        assert_eq!(p.docs.first().map(|d| d.tier), Some(Tier::Fallback));
    }

    #[test]
    fn copied_sections_are_kept_once() {
        let safety = para(80, "safety");
        let a = doc(
            "a.pdf",
            "Drive A",
            vec![
                Block::heading(1, "Safety"),
                safety.clone(),
                Block::heading(1, "A only"),
                para(80, "aa"),
            ],
        );
        let b = doc(
            "b.pdf",
            "Drive B",
            vec![
                Block::heading(1, "Safety"),
                safety,
                Block::heading(1, "B only"),
                para(80, "bb"),
            ],
        );
        let s = Settings {
            room_max: 150,
            ..small()
        };
        let p = plan(&[a, b], &s, &BTreeMap::new(), &BTreeSet::new());
        assert_eq!(p.stats.duplicates, 1);
        let n: usize = p
            .rooms
            .iter()
            .map(|r| r.content.matches("safety3 ").count())
            .sum();
        assert_eq!(n, 1);
        assert!(
            p.rooms
                .iter()
                .any(|r| r.content.contains("Same text as in [Safety"))
        );
    }

    #[test]
    fn links_between_documents_become_room_links() {
        let a = doc(
            "docs/a.md",
            "A",
            vec![Block::new(
                Kind::Paragraph,
                "See [the other](b.md#part) and [web](https://x.org).",
            )],
        );
        let b = doc("docs/b.md", "B", vec![para(10, "b")]);
        let p = plan(
            &[a, b],
            &Settings::default(),
            &BTreeMap::new(),
            &BTreeSet::new(),
        );
        let room = p
            .rooms
            .first()
            .map(|r| r.content.clone())
            .unwrap_or_default();
        assert!(room.contains("[the other](#b)"), "{room}");
        assert!(room.contains("[web](https://x.org)"));
        assert_eq!(p.stats.links, 1);
    }

    #[test]
    fn web_links_resolve_and_boilerplate_goes() {
        let mut docs = Vec::new();
        for i in 0..5 {
            let mut d = doc(
                &format!("https://ex.org/docs/p{i}"),
                &format!("Page {i}"),
                vec![
                    para(10, &format!("own{i}x")),
                    Block::new(Kind::Paragraph, "Was this page helpful?"),
                    Block::new(
                        Kind::Paragraph,
                        format!("Next: [page](/docs/p{})", (i + 1) % 5),
                    ),
                ],
            );
            d.site = "ex.org".into();
            docs.push(d);
        }
        let p = plan(
            &docs,
            &Settings::default(),
            &BTreeMap::new(),
            &BTreeSet::new(),
        );
        assert_eq!(p.stats.boilerplate, 5);
        let first = p
            .rooms
            .first()
            .map(|r| r.content.clone())
            .unwrap_or_default();
        assert!(first.contains("[page](#docs-p1)"), "{first}");
        assert!(!first.contains("helpful"));
    }

    #[test]
    fn ids_are_stable_across_runs() {
        let d1 = doc(
            "a.md",
            "Guide",
            vec![
                Block::heading(1, "Install"),
                para(100, "i"),
                Block::heading(1, "Use"),
                para(100, "u"),
            ],
        );
        let first = plan(
            std::slice::from_ref(&d1),
            &small(),
            &BTreeMap::new(),
            &BTreeSet::new(),
        );
        let d2 = doc(
            "a.md",
            "Guide",
            vec![
                Block::heading(1, "News"),
                para(100, "n"),
                Block::heading(1, "Install"),
                para(100, "i"),
                Block::heading(1, "Use"),
                para(100, "u"),
            ],
        );
        let second = plan(&[d2], &small(), &first.ids, &BTreeSet::new());
        let id_of = |p: &Plan, key: &str| p.ids.get(key).cloned();
        assert_eq!(
            id_of(&first, "a.md#Install"),
            id_of(&second, "a.md#Install")
        );
        assert_eq!(id_of(&first, "a.md#Use"), id_of(&second, "a.md#Use"));
    }

    #[test]
    fn key_links_reach_attachments() {
        let mail = doc(
            "in/m.eml",
            "Mail",
            vec![Block::new(
                Kind::List,
                "- [Manual (v2).pdf](<kapu-key:in/m.eml!Manual (v2).pdf>)",
            )],
        );
        let att = doc("in/m.eml!Manual (v2).pdf", "Manual v2", vec![para(10, "m")]);
        let p = plan(
            &[mail, att],
            &Settings::default(),
            &BTreeMap::new(),
            &BTreeSet::new(),
        );
        let room = p
            .rooms
            .first()
            .map(|r| r.content.clone())
            .unwrap_or_default();
        assert!(room.contains("[Manual (v2).pdf](#manual-v2)"), "{room}");
    }

    #[test]
    fn rooms_build() {
        let d = doc(
            "a.md",
            "Guide",
            vec![
                Block::heading(1, "Guide"),
                para(20, "w"),
                Block::heading(2, "Part"),
                para(20, "p"),
            ],
        );
        let p = plan(
            &[d],
            &Settings::default(),
            &BTreeMap::new(),
            &BTreeSet::new(),
        );
        let built = kapu_core::build::build(&p.rooms);
        assert!(built.is_ok(), "{:?}", built.err());
    }
}
