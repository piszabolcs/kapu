//! PDF (KD4). WIP: the PDFium reader (bookmarks, font sizes, running heads,
//! contents and index pages, image pages to OCR) is the next step.

use crate::doc::Document;
use crate::readers::Ctx;
use crate::{Error, Result};

/// A PDF file.
///
/// # Errors
/// Always, until the PDFium reader is written.
pub fn read(bytes: &[u8], key: &str, name: &str, ctx: &Ctx<'_>) -> Result<Document> {
    let _ = (bytes, key, name, ctx);
    Err(Error::new("PDF reading is not implemented yet (WIP)"))
}
