//! Markdown to blocks. The Office, HTML, EPUB and Markdown readers all end
//! here: the top-level blocks of the `CommonMark` parse, each kept as its own
//! source text, and the headings as level and title.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::doc::{Block, Kind};
use crate::text::clean;

/// What a Markdown text gives: the front matter's title and description,
/// if any, and the blocks.
#[derive(Debug, Default)]
pub struct Parsed {
    /// `title:` of a YAML front matter.
    pub title: Option<String>,
    /// `description:` of a YAML front matter.
    pub description: Option<String>,
    /// The top-level blocks.
    pub blocks: Vec<Block>,
}

/// The top-level blocks of `md`.
#[must_use]
pub fn parse(md: &str) -> Parsed {
    let md = clean(md);
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    let mut out = Parsed::default();
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut kind = Kind::Other;
    let mut level = 0u8;
    let mut in_meta = false;
    for (event, range) in Parser::new_ext(&md, options).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if depth == 0 {
                    start = range.start;
                    (kind, level) = match &tag {
                        Tag::Heading { level, .. } => (Kind::Heading, heading_level(*level)),
                        Tag::Paragraph => (Kind::Paragraph, 0),
                        Tag::List(_) => (Kind::List, 0),
                        Tag::Table(_) => (Kind::Table, 0),
                        Tag::CodeBlock(_) => (Kind::Code, 0),
                        Tag::BlockQuote(_) => (Kind::Quote, 0),
                        _ => (Kind::Other, 0),
                    };
                    in_meta = matches!(tag, Tag::MetadataBlock(_));
                }
                depth += 1;
            }
            Event::End(end) => {
                depth = depth.saturating_sub(1);
                if depth > 0 {
                    continue;
                }
                let source = md.get(start..range.end).unwrap_or_default();
                if in_meta || matches!(end, TagEnd::MetadataBlock(_)) {
                    front_matter(source, &mut out);
                    in_meta = false;
                    continue;
                }
                push(&mut out.blocks, kind, level, source);
            }
            Event::Text(_) | Event::Html(_) if depth == 0 => {
                push(
                    &mut out.blocks,
                    Kind::Other,
                    0,
                    md.get(range).unwrap_or_default(),
                );
            }
            _ => {}
        }
    }
    out
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn push(blocks: &mut Vec<Block>, kind: Kind, level: u8, source: &str) {
    let source = source.trim_end();
    if source.trim().is_empty() {
        return;
    }
    if kind == Kind::Heading {
        let title = heading_text(source);
        if !title.is_empty() {
            blocks.push(Block::heading(level, title));
        }
        return;
    }
    blocks.push(Block::new(kind, source.trim_start_matches('\n')));
}

/// The title of an ATX (`## Title ##`) or setext (`Title` + `---`) heading,
/// on one line.
fn heading_text(source: &str) -> String {
    let source = source.trim();
    let text = if source.starts_with('#') {
        let inner = source.trim_start_matches('#');
        let inner = inner.trim_end();
        let without_close = inner.trim_end_matches('#');
        if without_close.ends_with(' ') || without_close.is_empty() {
            without_close.to_owned()
        } else {
            inner.to_owned()
        }
    } else {
        let mut lines: Vec<&str> = source.lines().collect();
        lines.pop();
        lines.join(" ")
    };
    crate::text::squash(&text)
}

fn front_matter(source: &str, out: &mut Parsed) {
    for line in source.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().trim_matches(['"', '\'']).trim();
        if value.is_empty() {
            continue;
        }
        match key.trim() {
            "title" if out.title.is_none() => out.title = Some(value.to_owned()),
            "description" if out.description.is_none() => out.description = Some(value.to_owned()),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_keep_their_source() {
        let md = "---\ntitle: Guide\ndescription: \"How to\"\n---\n# Start\n\nSome *text*\non two lines.\n\n- a\n- b\n\n| x | y |\n|---|---|\n| 1 | 2 |\n\n```rust\nfn main() {}\n```\n\nSub\n---\n\n> quoted\n";
        let p = parse(md);
        assert_eq!(p.title.as_deref(), Some("Guide"));
        assert_eq!(p.description.as_deref(), Some("How to"));
        let kinds: Vec<Kind> = p.blocks.iter().map(|b| b.kind).collect();
        assert_eq!(
            kinds,
            [
                Kind::Heading,
                Kind::Paragraph,
                Kind::List,
                Kind::Table,
                Kind::Code,
                Kind::Heading,
                Kind::Quote
            ]
        );
        assert_eq!(
            p.blocks.first().map(|b| (b.level, b.text.as_str())),
            Some((1, "Start"))
        );
        assert_eq!(
            p.blocks.get(1).map(|b| b.text.as_str()),
            Some("Some *text*\non two lines.")
        );
        assert_eq!(
            p.blocks.get(5).map(|b| (b.level, b.text.as_str())),
            Some((2, "Sub"))
        );
        assert_eq!(
            p.blocks.get(4).map(|b| b.text.as_str()),
            Some("```rust\nfn main() {}\n```")
        );
    }

    #[test]
    fn closing_hashes_are_dropped() {
        let p = parse("## Title ##\n\n### C# notes\n");
        let titles: Vec<&str> = p.blocks.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(titles, ["Title", "C# notes"]);
    }
}
