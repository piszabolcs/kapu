//! The common document model (layer L2). Every reader produces it, the room
//! planner (L3) reads nothing else, and the cache stores it as JSON (KD16).

use serde::{Deserialize, Serialize};

/// What a block is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A heading; `level` is 1-6.
    Heading,
    /// A paragraph of Markdown.
    Paragraph,
    /// A list, whole.
    List,
    /// A Markdown table.
    Table,
    /// A fenced code block.
    Code,
    /// A block quote.
    Quote,
    /// Text read from an image by OCR.
    Image,
    /// Anything else, kept verbatim (raw HTML, footnotes).
    Other,
}

/// One block of a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    /// What it is.
    pub kind: Kind,
    /// The heading level, 1-6; 0 for other blocks.
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub level: u8,
    /// A heading the reader inferred from the layout, not one the source
    /// marks (third tier of the structure: plan L3/3).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inferred: bool,
    /// The Markdown text; for a heading the title without `#` marks.
    pub text: String,
    /// The 1-based physical page; 0 when the source has no pages.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub page: u32,
}

// serde's skip_serializing_if passes a reference
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero_u8(n: &u8) -> bool {
    *n == 0
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero_u32(n: &u32) -> bool {
    *n == 0
}

impl Block {
    /// A block of `kind` with `text`, no page.
    #[must_use]
    pub fn new(kind: Kind, text: impl Into<String>) -> Self {
        Self {
            kind,
            level: 0,
            inferred: false,
            text: text.into(),
            page: 0,
        }
    }

    /// A heading of `level` (clamped to 1-6).
    #[must_use]
    pub fn heading(level: u8, text: impl Into<String>) -> Self {
        Self {
            kind: Kind::Heading,
            level: level.clamp(1, 6),
            inferred: false,
            text: text.into(),
            page: 0,
        }
    }

    /// The same block on `page`.
    #[must_use]
    pub fn on_page(mut self, page: u32) -> Self {
        self.page = page;
        self
    }

    /// Whether this is a heading.
    #[must_use]
    pub fn is_heading(&self) -> bool {
        self.kind == Kind::Heading
    }
}

/// One document: a file, a page of a web site, an archive member or a mail
/// attachment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    /// Where it came from, unique in a run: a path with `/`, a URL without
    /// its fragment, or `outer!inner` for an archive member or attachment.
    pub key: String,
    /// The name for the `Source:` line (file name, URL, `pack.zip > a.docx`).
    pub name: String,
    /// The title: from the source's metadata, else its first heading, else
    /// the file name.
    pub title: String,
    /// A description the source gives (HTML meta, `llms.txt`), or empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Which reader made it (`pdfium`, `markdown`, `office`, ...).
    pub reader: String,
    /// The family for the boilerplate rule: the host of a web page, empty
    /// for files.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub site: String,
    /// The blocks in reading order.
    pub blocks: Vec<Block>,
    /// The number of pages, 0 when the source has none.
    #[serde(default)]
    pub pages: u32,
    /// The printed page labels by physical page (`3-12`), empty when the
    /// source has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub page_labels: Vec<String>,
    /// Metadata lines for the overview room (`Author`, `Date`, ...).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub meta: Vec<(String, String)>,
    /// What the reader could not do (image pages without OCR, cut tables).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Lines the reader dropped as running heads, contents or index pages,
    /// for the report (plan L3/2).
    #[serde(default)]
    pub removed: usize,
}

impl Document {
    /// An empty document for `key`, shown as `name`, read by `reader`.
    #[must_use]
    pub fn new(key: impl Into<String>, name: impl Into<String>, reader: &str) -> Self {
        let name = name.into();
        Self {
            key: key.into(),
            title: title_from_name(&name),
            name,
            reader: reader.to_owned(),
            ..Self::default()
        }
    }

    /// The printed label of `page` if the source has one, else its number.
    #[must_use]
    pub fn page_label(&self, page: u32) -> String {
        usize::try_from(page)
            .ok()
            .and_then(|p| p.checked_sub(1))
            .and_then(|i| self.page_labels.get(i))
            .filter(|l| !l.trim().is_empty())
            .map_or_else(|| page.to_string(), |l| l.trim().to_owned())
    }

    /// The number of characters of text in the blocks.
    #[must_use]
    pub fn text_len(&self) -> usize {
        self.blocks.iter().map(|b| b.text.len()).sum()
    }
}

/// A readable title from a file name: the stem, `_` as spaces.
#[must_use]
pub fn title_from_name(name: &str) -> String {
    let last = name.rsplit(['/', '\\', '>']).next().unwrap_or(name).trim();
    let stem = match last.rfind('.') {
        Some(i) if i > 0 && last.len() - i <= 6 => last.get(..i).unwrap_or(last),
        _ => last,
    };
    let title = stem.replace('_', " ");
    let title = title.trim();
    if title.is_empty() {
        name.to_owned()
    } else {
        title.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_come_from_file_names() {
        assert_eq!(
            title_from_name("F:/docs/Project_Planning.pdf"),
            "Project Planning"
        );
        assert_eq!(title_from_name("pack.zip > notes/read_me.txt"), "read me");
        assert_eq!(title_from_name("v1.2 release"), "v1.2 release");
    }

    #[test]
    fn page_labels_fall_back_to_numbers() {
        let mut d = Document::new("k", "n.pdf", "pdfium");
        d.page_labels = vec!["i".into(), String::new()];
        assert_eq!(d.page_label(1), "i");
        assert_eq!(d.page_label(2), "2");
        assert_eq!(d.page_label(9), "9");
    }
}
