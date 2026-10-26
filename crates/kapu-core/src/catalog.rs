//! Line parsers for the caption sections: `#MAP` (FR-13), `#DESC` (FR-13) and
//! `#GRAPH` (FR-11). Shared by the validator and the reader.
//!
//! The parsers check shape only (field count, number syntax) and report E005
//! for a malformed line. Whether ids exist, edge types are known and limits
//! hold is the validator's job.

use crate::error::{Error, ErrorCode, Result};
use crate::header::parse_decimal;

/// One door line of `#MAP`: `R <id> | <title> | <n>t | <gid> | <tags>`,
/// with ` | <kind>` for a room that names one (D144), or
/// `S <id> | <title> | <n>t`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapDoor<'a> {
    /// Door id.
    pub id: &'a str,
    /// Title as stored, with `\|` escapes (see [`unescape_field`]).
    pub title: &'a str,
    /// Price in `o200k_base` tokens (FR-08).
    pub tokens: u64,
    /// True for a room (`R`), false for a sub-door (`S`).
    pub is_room: bool,
    /// The room's `kind:` (FR-06/5), empty for none and for a sub-door.
    pub kind: &'a str,
}

/// The parsed `#MAP` section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Map<'a> {
    /// Number of `G` lines.
    pub groups: usize,
    /// `R` and `S` lines in file order.
    pub doors: Vec<MapDoor<'a>>,
}

impl<'a> Map<'a> {
    /// The `#MAP` line of a door, if there is one.
    #[must_use]
    pub fn door(&self, id: &str) -> Option<&MapDoor<'a>> {
        self.doors.iter().find(|d| d.id == id)
    }
}

/// One line of `#DESC`: `<id>\t<description>\t<tags, comma separated>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescLine<'a> {
    /// Door id.
    pub id: &'a str,
    /// Long description.
    pub description: &'a str,
    /// All tags (empty if none).
    pub tags: Vec<&'a str>,
}

/// One line of `#GRAPH`: `<from>\t<to>\t<type>\t<weight>`.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge<'a> {
    /// Source door.
    pub from: &'a str,
    /// Target door.
    pub to: &'a str,
    /// `link`, `term` or `sim`.
    pub kind: &'a str,
    /// Weight, 0..1.
    pub weight: f64,
}

fn e005(section: &str, n: usize, what: &str) -> Error {
    Error::new(ErrorCode::E005, format!("{section} line {n}: {what}"))
}

/// FR-13 (D107): in a `#MAP` field `\\` stands for `\` and `\|` for `|`,
/// read left to right. Any other `\` is kept as it is.
#[must_use]
pub fn unescape_field(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let mut chars = field.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(next @ ('\\' | '|')) => out.push(next),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Parses the inner text of `#MAP`.
///
/// # Errors
/// E005 for a line that is not a well-formed `G`, `R` or `S` line.
pub fn parse_map(text: &str) -> Result<Map<'_>> {
    let mut map = Map {
        groups: 0,
        doors: Vec::new(),
    };
    for (n, line) in text.lines().enumerate() {
        let n = n + 1;
        let fields: Vec<&str> = line.split(" | ").collect();
        let is_room = match (line.get(..2), fields.len()) {
            (Some("G "), 4) => {
                map.groups += 1;
                continue;
            }
            (Some("R "), 5 | 6) => true,
            (Some("S "), 3) => false,
            _ => {
                return Err(e005(
                    "#MAP",
                    n,
                    "not a `G`, `R` or `S` line with the right fields",
                ));
            }
        };
        let [head, title, price, ..] = fields.as_slice() else {
            return Err(e005("#MAP", n, "too few fields"));
        };
        let tokens = price
            .strip_suffix('t')
            .and_then(parse_decimal)
            .ok_or_else(|| e005("#MAP", n, "price is not `<n>t`"))?;
        map.doors.push(MapDoor {
            id: head.get(2..).unwrap_or(""),
            title,
            tokens,
            is_room,
            kind: fields.get(5).copied().unwrap_or(""),
        });
    }
    Ok(map)
}

/// Parses the inner text of `#DESC`.
///
/// # Errors
/// E005 for a line without exactly three tab-separated fields.
pub fn parse_desc(text: &str) -> Result<Vec<DescLine<'_>>> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let f: Vec<&str> = line.split('\t').collect();
        let [id, description, tags] = f.as_slice() else {
            return Err(e005("#DESC", n + 1, "expected 3 fields"));
        };
        let tags = if tags.is_empty() {
            Vec::new()
        } else {
            tags.split(", ").collect()
        };
        out.push(DescLine {
            id,
            description,
            tags,
        });
    }
    Ok(out)
}

/// Parses the inner text of `#GRAPH`.
///
/// # Errors
/// E005 for a line without four fields or with a weight that is not a number.
pub fn parse_graph(text: &str) -> Result<Vec<Edge<'_>>> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let f: Vec<&str> = line.split('\t').collect();
        let [from, to, kind, weight] = f.as_slice() else {
            return Err(e005("#GRAPH", n + 1, "expected 4 fields"));
        };
        let weight = weight
            .parse()
            .map_err(|_| e005("#GRAPH", n + 1, &format!("bad weight `{weight}`")))?;
        out.push(Edge {
            from,
            to,
            kind,
            weight,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_lines() {
        let m = parse_map("G g1 | a, b | 1 | 5t\nR r | T \\| x | 5t | g1 | a\nS r/s | U | 2t\n")
            .unwrap();
        assert_eq!(m.groups, 1);
        assert_eq!(m.doors.len(), 2);
        assert_eq!(unescape_field(m.doors[0].title), "T | x");
        assert_eq!(m.doors[1].id, "r/s");
        assert_eq!(
            m.door("r/s").map(|d| (d.tokens, d.is_room)),
            Some((2, false))
        );
        assert_eq!(parse_map("X what\n").unwrap_err().code, ErrorCode::E005);
        assert_eq!(
            parse_map("S r/s | U | 2\n").unwrap_err().code,
            ErrorCode::E005
        );
    }

    #[test]
    fn desc_lines() {
        let d = parse_desc("r\tleírás\ta, b\nr/s\t\t\n").unwrap();
        assert_eq!(d[0].tags, ["a", "b"]);
        assert_eq!(d[0].description, "leírás");
        assert_eq!(d[1].tags, Vec::<&str>::new());
        assert_eq!(
            parse_desc("r\tonly two\n").unwrap_err().code,
            ErrorCode::E005
        );
    }

    #[test]
    fn graph_lines() {
        let g = parse_graph("a\tb\tlink\t1.0\n").unwrap();
        assert_eq!(g[0].kind, "link");
        assert!((g[0].weight - 1.0).abs() < f64::EPSILON);
        assert_eq!(
            parse_graph("a\tb\tlink\n").unwrap_err().code,
            ErrorCode::E005
        );
    }
}
