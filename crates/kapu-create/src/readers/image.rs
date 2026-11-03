//! Images (PNG, JPEG, TIFF, BMP, GIF, WebP): their text by OCR (KD5).

use crate::doc::{Block, Document, Kind};
use crate::readers::{Ctx, ocr};
use crate::{Error, Result};

/// An image file.
///
/// # Errors
/// When Tesseract is not found or fails, or the image has no text.
pub fn read(bytes: &[u8], key: &str, name: &str, ctx: &Ctx<'_>) -> Result<Document> {
    let options = &ctx.options.ocr;
    let exe = ocr::find(options).ok_or_else(|| {
        Error::new("an image: its text needs OCR, and Tesseract was not found (--tesseract or KAPU_TESSERACT)")
    })?;
    let ext = crate::detect::extension(name);
    let ext = if ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        ext
    } else {
        "png".into()
    };
    let text = ocr::image_bytes(&exe, bytes, &ext, options)?;
    let mut doc = Document::new(key, name, "ocr");
    for paragraph in text.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
        doc.blocks.push(Block::new(Kind::Image, paragraph));
    }
    if doc.blocks.is_empty() {
        return Err(Error::new("an image without readable text"));
    }
    Ok(doc)
}
