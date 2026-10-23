//! The reader side of FR-15: open and describe doors, frame their content,
//! and keep serving when the derived sections no longer match the body.
//!
//! Fast path: the body still matches the header (`bodyhash`, length) and every
//! requested door's fingerprint matches, so positions from `#DOCS` are used.
//! Otherwise (D77): the file is read again once (another program may have just
//! saved it, FR-15 edge 1), and if it is still inconsistent the door positions
//! are rebuilt in memory from the `@@@` lines of the body (edge 2). Such a
//! response carries `index_stale = true`. The reader never writes the file
//! (D16).

use std::fmt::Write as _;
use std::path::Path;

use crate::catalog::{Map, parse_desc, parse_graph, parse_map, unescape_field};
use crate::docs::{DocEntry, served_text};
use crate::error::{Error, ErrorCode, Result};
use crate::sections::{BodyLength, Limits, Sections};

/// A graph neighbour of an opened door (FR-15: id, title, price, edge type).
/// Data only, not a suggestion (FR-15/3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Neighbour {
    /// Door id of the neighbour.
    pub id: String,
    /// Its title.
    pub title: String,
    /// Its price in tokens.
    pub tokens: u64,
    /// `link`, `term` or `sim`.
    pub kind: String,
}

/// One opened door.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedDoor {
    /// Door id.
    pub id: String,
    /// Title from `#MAP`.
    pub title: String,
    /// Price from `#MAP`. After a stale rebuild it is the price at the last build.
    pub tokens: u64,
    /// Served content: escape undone, comments removed.
    pub text: String,
    /// Graph neighbours, sorted by id then edge type.
    pub neighbours: Vec<Neighbour>,
}

/// The result of one open call: one entry per requested id, in request order.
/// An unknown id is an error for that id only (FR-15/4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    /// Per requested id, the door or why it could not be served.
    pub doors: Vec<std::result::Result<OpenedDoor, Error>>,
    /// True when positions were rebuilt from the `@@@` lines (`index: stale`).
    pub index_stale: bool,
}

/// Long description and all tags of one door (`kapu_describe`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Described {
    /// Door id.
    pub id: String,
    /// Title from `#MAP`.
    pub title: String,
    /// Long description from `#DESC`.
    pub description: String,
    /// All tags.
    pub tags: Vec<String>,
}

fn e009(id: &str) -> Error {
    Error::new(ErrorCode::E009, format!("unknown door `{id}`"))
}

/// Opens doors of an in-memory file. Without a path there is nothing to
/// re-read, so an inconsistent file goes straight to the in-memory rebuild.
///
/// # Errors
/// Only for a file whose header or section layout is broken; per-id problems
/// are reported inside [`Opened::doors`].
pub fn open_bytes(file: &[u8], ids: &[&str]) -> Result<Opened> {
    match open_fast(file, ids)? {
        Some(opened) => Ok(opened),
        None => open_rebuilt(file, ids),
    }
}

/// Opens doors of a file on disk with the full FR-15 procedure: fast path,
/// one re-read on inconsistency, then the in-memory rebuild.
///
/// # Errors
/// I/O errors are reported as E012 with the OS message; a broken header or
/// layout as its own code.
pub fn open_path(path: &Path, ids: &[&str]) -> Result<Opened> {
    let first = read(path)?;
    if let Some(opened) = open_fast(&first, ids)? {
        return Ok(opened);
    }
    let second = read(path)?;
    match open_fast(&second, ids)? {
        Some(opened) => Ok(opened),
        None => open_rebuilt(&second, ids),
    }
}

fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| {
        Error::new(
            ErrorCode::E012,
            format!("cannot read {}: {e}", path.display()),
        )
    })
}

/// `kapu_describe`: long description and tags per id. Unknown ids are errors
/// for that id only.
///
/// # Errors
/// Only for a broken header or layout.
pub fn describe_bytes(
    file: &[u8],
    ids: &[&str],
) -> Result<Vec<std::result::Result<Described, Error>>> {
    let s = Sections::parse_with(file, &Limits::default(), BodyLength::Tolerate)?;
    let map = parse_map(s.map)?;
    let desc = parse_desc(s.desc)?;
    Ok(ids
        .iter()
        .map(|id| {
            let line = desc.iter().find(|d| d.id == *id).ok_or_else(|| e009(id))?;
            Ok(Described {
                id: (*id).to_owned(),
                title: map
                    .door(id)
                    .map(|d| unescape_field(d.title))
                    .unwrap_or_default(),
                description: line.description.to_owned(),
                tags: line.tags.iter().map(|t| (*t).to_owned()).collect(),
            })
        })
        .collect())
}

/// A boundary nobody can know when writing the content (D106): BLAKE3 of
/// the time, the process id, a counter and the door id, 12 hex digits.
#[must_use]
pub fn new_boundary(id: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let mut hasher = blake3::Hasher::new();
    hasher.update(&nanos.to_le_bytes());
    hasher.update(&std::process::id().to_le_bytes());
    hasher.update(&COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    hasher.update(id.as_bytes());
    hasher
        .finalize()
        .to_hex()
        .get(..12)
        .unwrap_or("")
        .to_owned()
}

/// The FR-15/2 frame around one door's content, with a fresh boundary
/// (D106). `file_name` is the name the caller shows (normally the file name
/// without directories).
#[must_use]
pub fn frame(file_name: &str, door: &OpenedDoor) -> String {
    frame_with_boundary(file_name, door, &new_boundary(&door.id))
}

/// The frame with a given boundary: `<kapu-room file="…" id="…" tokens="…"
/// boundary="…">`, the content, `</kapu-room boundary="…">`. A
/// `</kapu-room` inside the content cannot close it, because it cannot carry
/// the boundary (AGGALYOK A2).
#[must_use]
pub fn frame_with_boundary(file_name: &str, door: &OpenedDoor, boundary: &str) -> String {
    let file = file_name.replace('&', "&amp;").replace('"', "&quot;");
    let mut out = format!(
        "<kapu-room file=\"{file}\" id=\"{}\" tokens=\"{}\" boundary=\"{boundary}\">\n",
        door.id, door.tokens
    );
    out.push_str(&door.text);
    if !door.text.ends_with('\n') {
        out.push('\n');
    }
    let _ = writeln!(out, "</kapu-room boundary=\"{boundary}\">");
    out
}

// Context shared by both paths: titles, prices and edges of the file.
struct Catalog<'a> {
    map: Map<'a>,
    edges: Vec<crate::catalog::Edge<'a>>,
}

impl<'a> Catalog<'a> {
    fn load(s: &Sections<'a>) -> Result<Self> {
        Ok(Self {
            map: parse_map(s.map)?,
            edges: parse_graph(s.graph)?,
        })
    }

    fn door(&self, id: &str, text: String) -> OpenedDoor {
        let line = self.map.door(id);
        let mut neighbours: Vec<Neighbour> = self
            .edges
            .iter()
            .filter_map(|e| {
                let other = if e.from == id {
                    e.to
                } else if e.to == id {
                    e.from
                } else {
                    return None;
                };
                let n = self.map.door(other);
                Some(Neighbour {
                    id: other.to_owned(),
                    title: n.map(|d| unescape_field(d.title)).unwrap_or_default(),
                    tokens: n.map_or(0, |d| d.tokens),
                    kind: e.kind.to_owned(),
                })
            })
            .collect();
        neighbours.sort_by(|a, b| {
            (a.id.as_str(), a.kind.as_str()).cmp(&(b.id.as_str(), b.kind.as_str()))
        });
        neighbours.dedup();
        OpenedDoor {
            id: id.to_owned(),
            title: line.map(|d| unescape_field(d.title)).unwrap_or_default(),
            tokens: line.map_or(0, |d| d.tokens),
            text,
            neighbours,
        }
    }
}

/// The fast path. `Ok(None)` means "inconsistent, fall back".
fn open_fast(file: &[u8], ids: &[&str]) -> Result<Option<Opened>> {
    let s = Sections::parse_with(file, &Limits::default(), BodyLength::Tolerate)?;
    if !s.body_length_matches || s.check_bodyhash().is_err() {
        return Ok(None);
    }
    let entries = s.doc_entries()?;
    let catalog = Catalog::load(&s)?;
    let mut doors = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(entry) = entries.iter().find(|e| e.id == *id) else {
            doors.push(Err(e009(id)));
            continue;
        };
        match s.stored(entry) {
            Ok(bytes) => doors.push(served_text(bytes).map(|t| catalog.door(id, t))),
            Err(e) if matches!(e.code, ErrorCode::E005 | ErrorCode::E007) => return Ok(None),
            Err(e) => doors.push(Err(e)),
        }
    }
    Ok(Some(Opened {
        doors,
        index_stale: false,
    }))
}

/// The edge-2 path: room spans from the `@@@` lines, sub-door spans from the
/// `## ` headings of the rebuilt room (D77).
fn open_rebuilt(file: &[u8], ids: &[&str]) -> Result<Opened> {
    let s = Sections::parse_with(file, &Limits::default(), BodyLength::Tolerate)?;
    let entries = s.doc_entries()?;
    let catalog = Catalog::load(&s)?;
    let rooms = room_spans(s.body);
    let doors = ids
        .iter()
        .map(|id| {
            let bytes = rebuilt_bytes(s.body, &rooms, &entries, id)?;
            served_text(bytes).map(|t| catalog.door(id, t))
        })
        .collect();
    Ok(Opened {
        doors,
        index_stale: true,
    })
}

/// `(id, start, end)` of every room's stored content, found from the `@@@`
/// lines (D36: content runs to the LF before the next `@@@` line, and the
/// last room to the final LF of the body).
fn room_spans(body: &[u8]) -> Vec<(&str, usize, usize)> {
    let mut markers: Vec<(&str, usize, usize)> = Vec::new(); // (id, marker start, content start)
    let mut pos = 0usize;
    for line in body.split(|&b| b == b'\n') {
        if let Some(rest) = line.strip_prefix(b"@@@ ") {
            let id = std::str::from_utf8(rest).unwrap_or("");
            markers.push((id, pos, pos + line.len() + 1));
        }
        pos += line.len() + 1;
    }
    let mut out = Vec::with_capacity(markers.len());
    for (i, &(id, _, content_start)) in markers.iter().enumerate() {
        let next = markers.get(i + 1).map_or(body.len(), |&(_, m, _)| m);
        let end = next.saturating_sub(1).max(content_start).min(body.len());
        out.push((id, content_start.min(end), end));
    }
    out
}

fn rebuilt_bytes<'b>(
    body: &'b [u8],
    rooms: &[(&str, usize, usize)],
    entries: &[DocEntry<'_>],
    id: &str,
) -> Result<&'b [u8]> {
    let (room_id, sub) = match id.split_once('/') {
        Some((r, _)) => (r, true),
        None => (id, false),
    };
    let &(_, start, end) = rooms
        .iter()
        .find(|(r, _, _)| *r == room_id)
        .ok_or_else(|| e009(id))?;
    let room = body.get(start..end).ok_or_else(|| e009(id))?;
    if !sub {
        return Ok(room);
    }
    // The k-th sub-door of the room in #DOCS order of offsets is the k-th
    // heading of the rebuilt room.
    let mut subs: Vec<&DocEntry<'_>> = entries
        .iter()
        .filter(|e| e.id.split_once('/').is_some_and(|(r, _)| r == room_id))
        .collect();
    subs.sort_by_key(|e| e.span.start);
    let k = subs
        .iter()
        .position(|e| e.id == id)
        .ok_or_else(|| e009(id))?;
    let heads = subdoor_starts(room);
    if heads.len() != subs.len() {
        return Err(Error::new(
            ErrorCode::E007,
            format!(
                "room `{room_id}` was edited and its sub-doors changed; `{id}` cannot be located until the next build"
            ),
        ));
    }
    let from = heads.get(k).copied().unwrap_or(room.len());
    let to = heads.get(k + 1).copied().unwrap_or(room.len());
    room.get(from..to).ok_or_else(|| e009(id))
}

/// Byte offsets of the `## ` lines outside fenced code blocks (FR-07).
fn subdoor_starts(room: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut fence: Option<&[u8]> = None;
    let mut pos = 0usize;
    for line in room.split(|&b| b == b'\n') {
        let trimmed = line.trim_ascii_start();
        match fence {
            None => {
                if trimmed.starts_with(b"```") {
                    fence = Some(b"```");
                } else if trimmed.starts_with(b"~~~") {
                    fence = Some(b"~~~");
                } else if line.starts_with(b"## ") {
                    out.push(pos);
                }
            }
            Some(f) => {
                if trimmed.starts_with(f) {
                    fence = None;
                }
            }
        }
        pos += line.len() + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_spans_follow_the_separator_rule() {
        let body = b"@@@ a\nhello\n@@@ b\n\n@@@ c\nx\ny\n";
        let spans = room_spans(body);
        let text: Vec<(&str, &[u8])> = spans.iter().map(|&(id, s, e)| (id, &body[s..e])).collect();
        assert_eq!(
            text,
            [("a", &b"hello"[..]), ("b", &b""[..]), ("c", &b"x\ny"[..])]
        );
    }

    #[test]
    fn subdoor_starts_skip_fenced_headings() {
        let room = b"# T\n## A\n```\n## not\n```\n## B\n~~~\n## no\n~~~\n";
        let starts = subdoor_starts(room);
        assert_eq!(starts.len(), 2);
        assert!(room[starts[0]..].starts_with(b"## A"));
        assert!(room[starts[1]..].starts_with(b"## B"));
    }

    #[test]
    fn frame_shape() {
        let door = OpenedDoor {
            id: "a".into(),
            title: "A".into(),
            tokens: 3,
            text: "x".into(),
            neighbours: vec![],
        };
        assert_eq!(
            frame_with_boundary("my \"f\".kapu", &door, "b0"),
            "<kapu-room file=\"my &quot;f&quot;.kapu\" id=\"a\" tokens=\"3\" boundary=\"b0\">\nx\n</kapu-room boundary=\"b0\">\n"
        );
        // every frame gets a different, 12-hex-digit boundary
        let (a, b) = (new_boundary("a"), new_boundary("a"));
        assert_ne!(a, b);
        assert_eq!(a.len(), 12);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
    }
}
