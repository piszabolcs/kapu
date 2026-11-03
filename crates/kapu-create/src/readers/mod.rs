//! Layer L1: one reader per file type, each turning bytes into documents.
//! A reader never runs anything from the content.

pub mod archive;
pub mod chm;
pub mod html;
pub mod image;
pub mod mail;
pub mod ocr;
pub mod office;
pub mod pdf;
pub mod text;

use std::path::PathBuf;

use crate::detect::{FileType, detect};
use crate::doc::Document;
use crate::{Error, Result};

/// What the readers may use.
#[derive(Debug, Clone)]
pub struct Options {
    /// OCR settings (KD5).
    pub ocr: ocr::Options,
    /// The `PDFium` library, if not next to the program (KD4).
    pub pdfium: Option<PathBuf>,
    /// Archive and attachment nesting limit (KD8).
    pub max_depth: usize,
    /// Largest unpacked archive member, bytes (zip-bomb guard).
    pub max_member: u64,
    /// Largest unpacked archive in total, bytes.
    pub max_unpacked: u64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            ocr: ocr::Options::default(),
            pdfium: None,
            max_depth: 5,
            max_member: 1 << 30,
            max_unpacked: 4 << 30,
        }
    }
}

/// The reading context: the options and how deep in archives and mails.
#[derive(Debug, Clone, Copy)]
pub struct Ctx<'a> {
    /// The options.
    pub options: &'a Options,
    /// Nesting depth: 0 for a file given by the user.
    pub depth: usize,
}

impl Ctx<'_> {
    /// The context one level deeper, or an error past the limit.
    ///
    /// # Errors
    /// When the nesting limit is reached.
    pub fn deeper(&self) -> Result<Self> {
        if self.depth >= self.options.max_depth {
            return Err(Error::new(format!(
                "nested more than {} levels deep (archive or attachment in another)",
                self.options.max_depth
            )));
        }
        Ok(Ctx {
            options: self.options,
            depth: self.depth + 1,
        })
    }
}

/// Reads one file: `bytes` named `name`, identified by `key`.
///
/// # Errors
/// When the type is unknown or not readable, or the reader fails.
pub fn read(bytes: &[u8], key: &str, name: &str, ctx: &Ctx<'_>) -> Result<Vec<Document>> {
    let one = |d: Document| Ok(vec![d]);
    match detect(bytes, name) {
        FileType::Markdown => one(text::markdown(bytes, key, name)),
        FileType::Text => one(text::plain(bytes, key, name)),
        FileType::Code(lang) => one(text::code(bytes, key, name, lang)),
        FileType::Csv => text::csv(bytes, key, name).map(|d| vec![d]),
        FileType::Html => one(html::read(bytes, key, name)),
        FileType::Office(format) => office::microsoft(bytes, format, key, name).map(|d| vec![d]),
        FileType::Anydoc(format) => office::other(bytes, format, key, name).map(|d| vec![d]),
        FileType::Pdf => pdf::read(bytes, key, name, ctx).map(|d| vec![d]),
        FileType::Image => image::read(bytes, key, name, ctx).map(|d| vec![d]),
        FileType::Zip => archive::zip(bytes, key, name, ctx),
        FileType::SevenZ => archive::sevenz(bytes, key, name, ctx),
        FileType::Rar => Err(Error::new(
            "RAR archives are not unpacked (their licence does not allow it): unpack it and add the folder",
        )),
        FileType::Eml => mail::eml(bytes, key, name, ctx),
        FileType::Msg => mail::msg(bytes, key, name, ctx),
        FileType::Chm => chm::read(bytes, key, name, ctx),
        FileType::Unknown => Err(Error::new("unknown file type, not read")),
    }
}
