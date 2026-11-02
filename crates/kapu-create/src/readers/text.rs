//! Markdown, plain text, source code and CSV.

use crate::doc::{Block, Document, Kind};
use crate::markdown;
use crate::text::{clean, decode};

/// A Markdown file: its blocks; the title from the front matter.
#[must_use]
pub fn markdown(bytes: &[u8], key: &str, name: &str) -> Document {
    from_markdown(&decode(bytes), key, name, "markdown")
}

/// A document from Markdown made by another reader (Office, HTML).
#[must_use]
pub fn from_markdown(md: &str, key: &str, name: &str, reader: &str) -> Document {
    let parsed = markdown::parse(md);
    let mut doc = Document::new(key, name, reader);
    if let Some(title) = parsed.title {
        doc.title = title;
    } else if let Some(h) = first_title(&parsed.blocks) {
        doc.title = h;
    }
    doc.description = parsed.description.unwrap_or_default();
    doc.blocks = parsed.blocks;
    doc
}

/// The text of the first heading if it is the only one at the top level.
fn first_title(blocks: &[Block]) -> Option<String> {
    let top = blocks.iter().filter(|b| b.is_heading()).map(|b| b.level).min()?;
    let mut at_top = blocks.iter().filter(|b| b.is_heading() && b.level == top);
    let first = at_top.next()?;
    at_top.next().is_none().then(|| first.text.clone())
}

/// Plain text: paragraphs at blank lines.
#[must_use]
pub fn plain(bytes: &[u8], key: &str, name: &str) -> Document {
    let text = clean(&decode(bytes));
    let mut doc = Document::new(key, name, "text");
    let mut paragraph: Vec<&str> = Vec::new();
    for line in text.lines().chain(std::iter::once("")) {
        if line.trim().is_empty() {
            if !paragraph.is_empty() {
                doc.blocks.push(Block::new(Kind::Paragraph, paragraph.join("\n")));
                paragraph.clear();
            }
        } else {
            paragraph.push(line.trim_end());
        }
    }
    doc
}

/// A source file: one code block in its language.
#[must_use]
pub fn code(bytes: &[u8], key: &str, name: &str, lang: &str) -> Document {
    let text = clean(&decode(bytes));
    let mut doc = Document::new(key, name, "code");
    doc.blocks.push(Block::new(Kind::Code, fence(&text, lang)));
    doc
}

/// `text` as a fenced code block whose fence is longer than any backtick
/// run inside it.
#[must_use]
pub fn fence(text: &str, lang: &str) -> String {
    let mut longest = 0usize;
    let mut run = 0usize;
    for c in text.chars() {
        if c == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    let marks = "`".repeat(longest.max(2) + 1);
    format!("{marks}{lang}\n{}\n{marks}", text.trim_end_matches('\n'))
}

/// CSV or TSV: a Markdown table through `anydoc`.
///
/// # Errors
/// When `anydoc` cannot parse it.
pub fn csv(bytes: &[u8], key: &str, name: &str) -> crate::Result<Document> {
    let md = anydoc::to_markdown_bytes(bytes, anydoc::Format::Csv)
        .map_err(|e| crate::Error::new(format!("CSV: {e}")))?;
    Ok(from_markdown(&md, key, name, "csv"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_splits_at_blank_lines() {
        let d = plain(b"one\ntwo\n\n\nthree\r\n", "k", "a.txt");
        let texts: Vec<&str> = d.blocks.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(texts, ["one\ntwo", "three"]);
        assert_eq!(d.title, "a");
    }

    #[test]
    fn a_fence_outgrows_backticks_inside() {
        assert_eq!(fence("a ``` b", "md"), "````md\na ``` b\n````");
        assert_eq!(fence("x", ""), "```\nx\n```");
    }

    #[test]
    fn a_single_top_heading_is_the_title() {
        let d = markdown(b"# Guide\n\ntext\n\n## A\n\n## B\n", "k", "g.md");
        assert_eq!(d.title, "Guide");
        let d = markdown(b"# One\n\n# Two\n", "k", "g.md");
        assert_eq!(d.title, "g");
    }
}
