//! Office files (KD6): Microsoft formats with `office_oxide`, ODF, RTF and
//! EPUB with `anydoc`. Both give Markdown; the Markdown reader makes the
//! blocks, so the headings, slide titles and sheet names are real headings.

use std::io::Cursor;

use crate::doc::{Document, Kind};
use crate::readers::text::from_markdown;
use crate::{Error, Result};

/// The most table rows kept from one spreadsheet table (plan L1/3).
pub const MAX_ROWS: usize = 20_000;

/// A Word, `PowerPoint` or Excel file, old or new.
///
/// # Errors
/// When the file cannot be parsed (damaged, encrypted).
pub fn microsoft(
    bytes: &[u8],
    format: office_oxide::DocumentFormat,
    key: &str,
    name: &str,
) -> Result<Document> {
    let doc = office_oxide::Document::from_reader(Cursor::new(bytes.to_vec()), format)
        .map_err(|e| Error::new(format!("{format:?}: {e}")))?;
    let mut out = from_markdown(&doc.to_markdown(), key, name, "office");
    cap_tables(&mut out);
    Ok(out)
}

/// ODT, ODS, ODP, RTF, EPUB.
///
/// # Errors
/// When the file cannot be parsed.
pub fn other(bytes: &[u8], format: anydoc::Format, key: &str, name: &str) -> Result<Document> {
    let md = anydoc::to_markdown_bytes(bytes, format)
        .map_err(|e| Error::new(format!("{format:?}: {e}")))?;
    let mut out = from_markdown(&md, key, name, "anydoc");
    cap_tables(&mut out);
    Ok(out)
}

/// Cuts tables longer than [`MAX_ROWS`] rows, with a warning.
fn cap_tables(doc: &mut Document) {
    let mut cut = 0usize;
    for block in doc.blocks.iter_mut().filter(|b| b.kind == Kind::Table) {
        let rows = block.text.lines().count();
        if rows > MAX_ROWS + 2 {
            let kept: Vec<String> = block
                .text
                .lines()
                .take(MAX_ROWS + 2)
                .map(str::to_owned)
                .collect();
            cut += rows - kept.len();
            block.text = kept.join("\n");
        }
    }
    if cut > 0 {
        doc.warnings.push(format!(
            "{cut} table rows over the limit of {MAX_ROWS} per table were left out"
        ));
    }
}
