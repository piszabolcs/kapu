//! PDF (KD4). `PDFium` gives the characters in reading order with their size,
//! weight and position; from them come the lines, and from the lines of the
//! whole document:
//!
//! 1. running heads and page numbers (lines that repeat at the top or the
//!    bottom of many pages) are dropped;
//! 2. contents pages and index pages are dropped (the map and the search
//!    replace them);
//! 3. the bookmarks are the headings; without them, or when they are only
//!    a few, headings are inferred from size, weight and numbering;
//! 4. the lines become paragraphs (hyphenated words joined), and runs of
//!    column-aligned lines become tables;
//! 5. pages with no text but an image are read by OCR.
//!
//! Without `PDFium` the text comes from `pdf-extract` and the bookmarks from
//! `lopdf`; there is no font information then, so only numbered headings
//! are inferred.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::OnceLock;

use pdfium_render::prelude::*;

use crate::doc::{Block, Document, Kind};
use crate::readers::{Ctx, Options, ocr};
use crate::text::{clean, numbering, squash};
use crate::{Error, Result};

/// One line of a page.
#[derive(Debug, Clone, Default)]
struct Line {
    text: String,
    /// The largest font size of its letters, points (0 without `PDFium`).
    size: f32,
    /// The share of bold letters.
    bold: f32,
    top: f32,
    bottom: f32,
    /// In the running-head zone: the first or last three lines of the page,
    /// or the top or bottom 7%.
    zone: bool,
    /// Heading level and whether it was inferred.
    heading: Option<(u8, bool)>,
    removed: bool,
}

/// One bookmark: level from 1, title, 0-based page.
struct Mark {
    level: u8,
    title: String,
    page: usize,
}

static PDFIUM: OnceLock<Option<Pdfium>> = OnceLock::new();

/// The `PDFium` library: `--pdfium`, `KAPU_PDFIUM`, next to the program, in
/// a `pdfium` folder there, or the system's.
fn pdfium(options: &Options) -> Option<&'static Pdfium> {
    PDFIUM
        .get_or_init(|| {
            let mut candidates: Vec<PathBuf> = Vec::new();
            candidates.extend(options.pdfium.clone());
            candidates.extend(std::env::var_os("KAPU_PDFIUM").map(PathBuf::from));
            if let Some(dir) = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
            {
                candidates.push(dir.join("pdfium"));
                candidates.push(dir.clone());
            }
            for c in candidates {
                let path = if c.is_dir() {
                    Pdfium::pdfium_platform_library_name_at_path(&c)
                } else {
                    c
                };
                if path.is_file()
                    && let Ok(bindings) = Pdfium::bind_to_library(&path)
                {
                    return Some(Pdfium::new(bindings));
                }
            }
            Pdfium::bind_to_system_library().ok().map(Pdfium::new)
        })
        .as_ref()
}

/// Whether `PDFium` can be loaded (for the report and the window).
#[must_use]
pub fn has_pdfium(options: &Options) -> bool {
    pdfium(options).is_some()
}

/// A PDF file.
///
/// # Errors
/// When the file is damaged or encrypted.
pub fn read(bytes: &[u8], key: &str, name: &str, ctx: &Ctx<'_>) -> Result<Document> {
    let mut doc = Document::new(key, name, "pdfium");
    let (pages, marks) = if let Some(lib) = pdfium(ctx.options) {
        with_pdfium(lib, bytes, &mut doc, &ctx.options.ocr)?
    } else {
        doc.reader = "pdf-extract".into();
        doc.warnings.push("PDFium was not found: plain text without font sizes, headings only from bookmarks and numbers".into());
        without_pdfium(bytes)?
    };
    doc.pages = u32::try_from(pages.len()).unwrap_or(u32::MAX);
    assemble(&mut doc, pages, &marks);
    Ok(doc)
}

fn pdf_error(e: &PdfiumError) -> Error {
    let text = format!("{e:?}");
    if text.contains("Password") {
        Error::new("the PDF is encrypted (it needs a password), not read")
    } else {
        Error::new(format!("PDF: {text}"))
    }
}

fn with_pdfium(
    lib: &Pdfium,
    bytes: &[u8],
    doc: &mut Document,
    ocr_options: &ocr::Options,
) -> Result<(Vec<Vec<Line>>, Vec<Mark>)> {
    let pdf = lib
        .load_pdf_from_byte_slice(bytes, None)
        .map_err(|e| pdf_error(&e))?;
    if let Some(title) = pdf.metadata().get(PdfDocumentMetadataTagType::Title) {
        let t = squash(&clean(title.value()));
        if readable_title(&t) {
            doc.title = t;
        }
    }
    if let Some(author) = pdf.metadata().get(PdfDocumentMetadataTagType::Author)
        && !author.value().trim().is_empty()
    {
        doc.meta.push(("Author".into(), squash(author.value())));
    }
    let mut pages: Vec<Vec<Line>> = Vec::new();
    let mut image_pages: Vec<usize> = Vec::new();
    for (i, page) in pdf.pages().iter().enumerate() {
        doc.page_labels
            .push(page.label().unwrap_or_default().to_owned());
        let (lines, letters) = page_lines(&page);
        if letters < 20
            && page
                .objects()
                .iter()
                .any(|o| o.object_type() == PdfPageObjectType::Image)
        {
            image_pages.push(i);
        }
        pages.push(lines);
    }
    if doc.page_labels.iter().all(|l| l.trim().is_empty()) {
        doc.page_labels.clear();
    }
    if !image_pages.is_empty() {
        ocr_pages(&pdf, &image_pages, &mut pages, doc, ocr_options);
    }
    let mut marks = Vec::new();
    if let Some(root) = pdf.bookmarks().root() {
        // `iter_siblings` leaves the bookmark itself out, so the top level is
        // walked by hand.
        let mut top: Vec<(PdfBookmark<'_>, u8)> = Vec::new();
        let mut at = Some(root);
        while let Some(b) = at {
            at = b.next_sibling();
            top.push((b, 1));
            if top.len() > 20_000 {
                break;
            }
        }
        let mut stack = top;
        stack.reverse();
        while let Some((b, level)) = stack.pop() {
            let page = b
                .destination()
                .and_then(|d| d.page_index().ok())
                .and_then(|p| usize::try_from(p).ok());
            if let (Some(title), Some(page)) = (b.title(), page) {
                let title = squash(&clean(&title));
                if !title.is_empty() {
                    marks.push(Mark { level, title, page });
                }
            }
            let mut children: Vec<(PdfBookmark<'_>, u8)> = b
                .iter_direct_children()
                .map(|c| (c, level.saturating_add(1)))
                .collect();
            children.reverse();
            stack.extend(children);
            if marks.len() > 20_000 {
                break;
            }
        }
    }
    Ok((pages, marks))
}

/// Whether a metadata title reads as a title, not a document number or a
/// file name (`29469002`, `DOK-INDRV*-MP*`, `Microsoft Word - a.doc`).
fn readable_title(t: &str) -> bool {
    let lower = t.to_lowercase();
    let words = t
        .split_whitespace()
        .filter(|w| w.chars().filter(|c| c.is_alphabetic()).count() >= 2)
        .count();
    let visible = t.chars().filter(|c| !c.is_whitespace()).count();
    let letters = t.chars().filter(|c| c.is_alphabetic()).count();
    words >= 2
        && letters * 10 >= visible * 6
        && !t.contains(['*', '_'])
        && !lower.contains(".doc")
        && !lower.contains(".pdf")
        && !lower.starts_with("untitled")
        && !lower.starts_with("microsoft")
}

/// The lines of a page and its number of letters.
#[allow(clippy::cast_precision_loss)]
fn page_lines(page: &PdfPage<'_>) -> (Vec<Line>, usize) {
    let height = page.height().value;
    let Ok(text) = page.text() else {
        return (Vec::new(), 0);
    };
    let mut lines: Vec<Line> = Vec::new();
    let mut cur = Acc::default();
    let mut letters = 0usize;
    let mut run_bold = false;
    for ch in text.chars().iter() {
        let Some(c) = ch.unicode_char() else { continue };
        if c == '\n' {
            cur.finish(&mut lines);
            continue;
        }
        if c == '\r' {
            continue;
        }
        if ch.is_generated().unwrap_or(false) || c.is_whitespace() {
            cur.space();
            continue;
        }
        letters += 1;
        let size = ch.scaled_font_size().value;
        // The font name is asked once per run of one size: it is a string
        // through the library, the weight is often 0 in older files.
        if cur.count == 0 || (size - cur.last_size).abs() > 0.05 {
            let weight = ch.font_weight().is_some_and(|w| match w {
                PdfFontWeight::Weight600
                | PdfFontWeight::Weight700Bold
                | PdfFontWeight::Weight800
                | PdfFontWeight::Weight900 => true,
                PdfFontWeight::Custom(n) => n >= 600,
                _ => false,
            });
            let name = ch.font_name().to_lowercase();
            run_bold = weight
                || ch.font_is_bold_reenforced()
                || ["bold", "black", "heavy", "semibold", "demi", "-bd", ",bd"]
                    .iter()
                    .any(|b| name.contains(b));
        }
        let bounds = ch.loose_bounds().ok();
        cur.push(c, size, run_bold, bounds.as_ref());
    }
    cur.finish(&mut lines);
    let n = lines.len();
    for (i, line) in lines.iter_mut().enumerate() {
        line.zone = i < 3 || i + 3 >= n || line.top > height * 0.93 || line.bottom < height * 0.07;
    }
    (lines, letters)
}

/// A line being built from characters.
#[derive(Default)]
struct Acc {
    text: String,
    size: f32,
    /// Sum and count of the sizes of letters and digits: a symbol glyph of
    /// another size does not make the line a heading.
    letter_size: f32,
    letter_count: usize,
    bold: usize,
    count: usize,
    top: f32,
    bottom: f32,
    right: Option<f32>,
    last_size: f32,
}

impl Acc {
    fn space(&mut self) {
        if !self.text.is_empty() && !self.text.ends_with([' ', '\t']) {
            self.text.push(' ');
        }
    }

    fn push(&mut self, c: char, size: f32, bold: bool, bounds: Option<&PdfRect>) {
        if let Some(b) = bounds {
            let left = b.left().value;
            if let Some(right) = self.right {
                // A wide gap between two letters of a line is a column break.
                let em = size.max(self.last_size).max(4.0);
                if left - right > em * 1.6 && !self.text.is_empty() {
                    while self.text.ends_with(' ') {
                        self.text.pop();
                    }
                    if !self.text.ends_with('\t') {
                        self.text.push('\t');
                    }
                }
            }
            self.right = Some(b.right().value);
            if self.count == 0 {
                self.top = b.top().value;
                self.bottom = b.bottom().value;
            } else {
                self.top = self.top.max(b.top().value);
                self.bottom = self.bottom.min(b.bottom().value);
            }
        }
        // A C1 control is a Windows-1252 character with a broken mapping in
        // older files (U+0095 is the bullet, U+0096 the dash).
        if ('\u{80}'..='\u{9F}').contains(&c) {
            let byte = u8::try_from(u32::from(c)).unwrap_or(b'?');
            self.text
                .push_str(&encoding_rs::WINDOWS_1252.decode(&[byte]).0);
        } else {
            self.text.push(c);
        }
        self.size = self.size.max(size);
        if c.is_alphanumeric() {
            self.letter_size += size;
            self.letter_count += 1;
        }
        self.last_size = size;
        self.bold += usize::from(bold);
        self.count += 1;
    }

    #[allow(clippy::cast_precision_loss)]
    fn finish(&mut self, lines: &mut Vec<Line>) {
        let text = clean(&self.text).trim().to_owned();
        if !text.is_empty() {
            let size = if self.letter_count > 0 {
                self.letter_size / self.letter_count as f32
            } else {
                self.size
            };
            lines.push(Line {
                text,
                size,
                bold: self.bold as f32 / self.count.max(1) as f32,
                top: self.top,
                bottom: self.bottom,
                ..Line::default()
            });
        }
        *self = Self::default();
    }
}

/// Renders the image pages and reads them by OCR, in batches, so a long
/// scanned file never holds all its page images at once.
fn ocr_pages(
    pdf: &PdfDocument<'_>,
    image_pages: &[usize],
    pages: &mut [Vec<Line>],
    doc: &mut Document,
    options: &ocr::Options,
) {
    let Some(exe) = ocr::find(options) else {
        doc.warnings.push(format!(
            "{} pages are images without text; OCR needs Tesseract, which was not found",
            image_pages.len()
        ));
        return;
    };
    let workers = options.workers();
    let mut failed = 0usize;
    for batch in image_pages.chunks(workers * 2) {
        let images: Vec<(usize, Vec<u8>)> = batch
            .iter()
            .filter_map(|&i| render(pdf, i, options.dpi).map(|png| (i, png)))
            .collect();
        failed += batch.len() - images.len();
        let texts = ocr::parallel(&images, workers, |(_, png)| {
            ocr::image_bytes(&exe, png, "png", options)
        });
        for ((i, _), text) in images.iter().zip(texts) {
            match text {
                Ok(text) => {
                    if let Some(lines) = pages.get_mut(*i) {
                        lines.extend(
                            text.split("\n\n")
                                .map(str::trim)
                                .filter(|p| !p.is_empty())
                                .map(|p| Line {
                                    text: p.to_owned(),
                                    ..Line::default()
                                }),
                        );
                    }
                }
                Err(_) => failed += 1,
            }
        }
    }
    doc.reader = "pdfium+ocr".into();
    let read = image_pages.len() - failed;
    doc.warnings.push(format!(
        "{read} image pages read by OCR ({})",
        options.langs
    ));
    if failed > 0 {
        doc.warnings
            .push(format!("{failed} image pages could not be read by OCR"));
    }
}

/// One page as a grey PNG at `dpi`.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn render(pdf: &PdfDocument<'_>, index: usize, dpi: u32) -> Option<Vec<u8>> {
    let page = pdf.pages().get(PdfPageIndex::try_from(index).ok()?).ok()?;
    let width = (page.width().value / 72.0 * dpi as f32)
        .round()
        .clamp(100.0, 10_000.0) as i32;
    let config = PdfRenderConfig::new()
        .set_target_width(width)
        .render_form_data(false);
    let bitmap = page.render_with_config(&config).ok()?;
    let grey = image::DynamicImage::ImageLuma8(bitmap.as_image().ok()?.to_luma8());
    let mut png = Vec::new();
    grey.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    Some(png)
}

fn without_pdfium(bytes: &[u8]) -> Result<(Vec<Vec<Line>>, Vec<Mark>)> {
    let texts = pdf_extract::extract_text_from_mem_by_pages(bytes).map_err(|e| {
        let text = e.to_string();
        if text.to_lowercase().contains("encrypt") {
            Error::new("the PDF is encrypted (it needs a password), not read")
        } else {
            Error::new(format!("PDF: {text}"))
        }
    })?;
    let pages: Vec<Vec<Line>> = texts
        .iter()
        .map(|t| {
            let lines: Vec<&str> = t.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
            let n = lines.len();
            lines
                .iter()
                .enumerate()
                .map(|(i, l)| Line {
                    text: clean(l),
                    zone: i < 3 || i + 3 >= n,
                    ..Line::default()
                })
                .collect()
        })
        .collect();
    let marks = lopdf::Document::load_mem(bytes)
        .ok()
        .and_then(|d| d.get_toc().ok())
        .map(|toc| {
            toc.toc
                .into_iter()
                .map(|t| Mark {
                    level: u8::try_from(t.level.max(1)).unwrap_or(6),
                    title: squash(&clean(&t.title)),
                    page: t.page.saturating_sub(1),
                })
                .filter(|m| !m.title.is_empty())
                .collect()
        })
        .unwrap_or_default();
    Ok((pages, marks))
}

// ------------------------------------------------------ the document pass

fn assemble(doc: &mut Document, mut pages: Vec<Vec<Line>>, marks: &[Mark]) {
    let total = pages.len();
    let body = body_size(&pages);
    doc.removed += running_heads(&mut pages, body);
    doc.removed += contents_and_index(&mut pages);
    let mapped = apply_marks(&mut pages, marks);
    let max_level = marks.iter().map(|m| m.level).max().unwrap_or(0);
    let sparse = marks.len() < 3 || total / marks.len().max(1) > 10;
    if sparse {
        infer_headings(&mut pages, body, max_level, marks.is_empty());
    }
    if !marks.is_empty() && mapped * 2 < marks.len() {
        doc.warnings.push(format!(
            "{} of {} bookmarks were not found on their pages",
            marks.len() - mapped,
            marks.len()
        ));
    }
    doc.blocks = blocks(&pages);
    // A file name that is only a number: the title page's first heading.
    if !readable_title(&doc.title)
        && let Some(h) = doc
            .blocks
            .iter()
            .take_while(|b| b.page <= 2)
            .find(|b| b.is_heading() && readable_title(&b.text))
    {
        doc.title = h.text.clone();
    }
}

/// The key a line repeats under: digits as `#`, lower case, spaces squashed.
fn head_key(text: &str) -> String {
    squash(text)
        .chars()
        .map(|c| if c.is_ascii_digit() { '#' } else { c })
        .collect::<String>()
        .to_lowercase()
}

fn is_page_number(text: &str) -> bool {
    let t = text
        .trim()
        .trim_matches(['-', '–', '|', ' '])
        .to_lowercase();
    if t.is_empty() {
        return false;
    }
    let roman = t.chars().all(|c| matches!(c, 'i' | 'v' | 'x' | 'l' | 'c')) && t.len() <= 6;
    let digits = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_digit() || c == '-' || c == '\u{2013}' || c == '/')
            && s.len() <= 9
    };
    let words = ["page", "seite", "oldal", "p."];
    roman
        || digits(&t.replace(' ', ""))
        || words.iter().any(|w| {
            t.strip_prefix(w).is_some_and(|rest| {
                digits(
                    &rest
                        .replace(" of ", "/")
                        .replace(" von ", "/")
                        .replace(' ', ""),
                )
            })
        })
        || t.strip_suffix(". oldal").is_some_and(|r| digits(r.trim()))
}

/// Drops running heads and page numbers. Returns the lines dropped.
fn running_heads(pages: &mut [Vec<Line>], body: f32) -> usize {
    let mut count: HashMap<String, usize> = HashMap::new();
    let mut with_ref: HashMap<String, usize> = HashMap::new();
    for page in pages.iter() {
        let keys: HashSet<String> = page
            .iter()
            .filter(|l| l.zone)
            .map(|l| head_key(&l.text))
            .collect();
        for k in keys {
            *count.entry(k).or_default() += 1;
        }
        let keys: HashSet<String> = page
            .iter()
            .filter(|l| l.zone)
            .filter_map(|l| ref_key(&l.text))
            .collect();
        for k in keys {
            *with_ref.entry(k).or_default() += 1;
        }
    }
    let limit = (pages.len() * 3).div_ceil(10).max(3);
    let mut removed = 0;
    for page in pages.iter_mut() {
        for line in page.iter_mut().filter(|l| l.zone) {
            // A head that carries the page number repeats only through its
            // chapter (`Overview 2-3`), so three pages are enough.
            let chapter_head = !(body > 0.0 && line.size > body * 1.15)
                && ref_key(&line.text).is_some_and(|k| with_ref.get(&k).copied().unwrap_or(0) >= 3);
            let repeated = count.get(&head_key(&line.text)).copied().unwrap_or(0) >= limit;
            if repeated || chapter_head || is_page_number(&line.text) {
                line.removed = true;
                removed += 1;
            }
        }
    }
    removed
}

/// The text of a line without its page references, if it had one and a
/// few words: the key of a running head with a page number.
fn ref_key(text: &str) -> Option<String> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let refs = tokens.iter().filter(|t| is_page_ref(t)).count();
    let words: Vec<&str> = tokens.into_iter().filter(|t| !is_page_ref(t)).collect();
    (refs > 0 && !words.is_empty() && words.len() <= 12).then(|| words.join(" ").to_lowercase())
}

/// Whether a line ends with a page reference as in a contents list: dot
/// leaders or a column gap, then a number.
fn contents_line(text: &str) -> bool {
    let t = text.trim_end();
    let leader = t.contains("....") || t.contains(". . .") || t.contains('\u{2026}');
    let page_ref = t.split(char::is_whitespace).any(is_page_ref);
    let tail_ref = t
        .rfind('\t')
        .or_else(|| t.rfind("  "))
        .and_then(|i| t.get(i..))
        .is_some_and(|tail| is_page_ref(tail.trim()));
    (leader && page_ref) || tail_ref
}

/// `12`, `3-12`, `3\u{2013}12`, `A-4`: a page number or a chapter-page number.
fn is_page_ref(token: &str) -> bool {
    let t = token.trim_matches(['.', ',', ')', '(']);
    if t.is_empty() || t.len() > 9 {
        return false;
    }
    let parts: Vec<&str> = t.split(['-', '\u{2013}']).collect();
    parts.len() <= 2
        && parts
            .last()
            .is_some_and(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        && parts.first().is_some_and(|p| {
            !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric()) && p.len() <= 3
        })
}

/// Whether a line looks like an index entry: `term, 12, 45` or `term 12`.
fn index_line(text: &str) -> bool {
    let t = text.trim();
    let tail: String = t
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit() || matches!(c, ',' | ' ' | '-' | '–' | 'f' | '.'))
        .collect();
    tail.chars().any(|c| c.is_ascii_digit())
        && t.len() <= 80
        && t.chars().next().is_some_and(char::is_alphabetic)
}

/// Drops the contents pages near the start and the index pages near the
/// end. Returns the lines dropped.
fn contents_and_index(pages: &mut [Vec<Line>]) -> usize {
    let n = pages.len();
    let front = (n * 15 / 100).max(10);
    let back = n.saturating_sub((n * 15 / 100).max(10));
    let titles = [
        "index",
        "stichwortverzeichnis",
        "sachregister",
        "register",
        "tárgymutató",
        "targymutato",
        "indice",
        "index des mots",
    ];
    let mut removed = 0;
    let mut in_index = false;
    for (i, page) in pages.iter_mut().enumerate() {
        let live: Vec<&Line> = page.iter().filter(|l| !l.removed).collect();
        if i >= back
            && i > front
            && live
                .iter()
                .take(4)
                .any(|l| titles.contains(&squash(&l.text).to_lowercase().as_str()))
        {
            in_index = true;
        }
        if live.len() < 5 {
            continue;
        }
        let toc = i < front
            && live.iter().filter(|l| contents_line(&l.text)).count() * 10 >= live.len() * 4;
        let index =
            in_index && live.iter().filter(|l| index_line(&l.text)).count() * 10 >= live.len() * 6;
        if toc || index {
            for line in page.iter_mut().filter(|l| !l.removed) {
                line.removed = true;
                removed += 1;
            }
        }
    }
    removed
}

/// The body text size: the most common size, by letters.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn body_size(pages: &[Vec<Line>]) -> f32 {
    let mut count: BTreeMap<i32, usize> = BTreeMap::new();
    for line in pages
        .iter()
        .flatten()
        .filter(|l| !l.removed && l.size > 0.0)
    {
        *count.entry((line.size * 2.0).round() as i32).or_default() += line.text.len();
    }
    count
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map_or(0.0, |(k, _)| k as f32 / 2.0)
}

/// Letters and digits only, lower case, for matching bookmarks to lines.
fn norm(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Marks the line of every bookmark as a heading, or inserts the heading at
/// the top of its page. Returns how many were found on their page.
fn apply_marks(pages: &mut [Vec<Line>], marks: &[Mark]) -> usize {
    let mut found = 0;
    let mut inserts: BTreeMap<usize, Vec<Line>> = BTreeMap::new();
    for mark in marks {
        let want = norm(&mark.title);
        if want.is_empty() {
            continue;
        }
        let mut hit = false;
        for p in [mark.page, mark.page + 1] {
            let Some(page) = pages.get_mut(p) else {
                continue;
            };
            let pos = page.iter().position(|l| {
                if l.removed || l.heading.is_some() {
                    return false;
                }
                let have = norm(&l.text);
                // A paragraph that only starts with the title's words is not
                // the heading line.
                !have.is_empty()
                    && ((have.starts_with(&want) && have.len() <= want.len() + 12)
                        || (have.len() >= 8 && want.starts_with(&have)))
            });
            if let Some(i) = pos {
                let mut covered = page.get(i).map_or(0, |l| norm(&l.text).len());
                if let Some(line) = page.get_mut(i) {
                    line.heading = Some((mark.level.clamp(1, 6), false));
                    line.text.clone_from(&mark.title);
                }
                // A title wrapped onto the next line.
                let mut j = i + 1;
                while covered < want.len() {
                    let Some(next) = page.get_mut(j) else { break };
                    let n = norm(&next.text);
                    if n.is_empty() || !want.get(covered..).is_some_and(|rest| rest.starts_with(&n))
                    {
                        break;
                    }
                    covered += n.len();
                    next.removed = true;
                    j += 1;
                }
                hit = true;
                break;
            }
        }
        if hit {
            found += 1;
        } else {
            inserts.entry(mark.page).or_default().push(Line {
                text: mark.title.clone(),
                heading: Some((mark.level.clamp(1, 6), false)),
                ..Line::default()
            });
        }
    }
    for (p, lines) in inserts {
        if let Some(page) = pages.get_mut(p) {
            let at = page.iter().position(|l| !l.removed).unwrap_or(page.len());
            for (k, line) in lines.into_iter().enumerate() {
                page.insert(at + k, line);
            }
        }
    }
    found
}

/// Infers headings from size, weight and numbering (plan L3/3, second
/// tier). Levels come after the bookmarks' deepest level.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn infer_headings(pages: &mut [Vec<Line>], body: f32, after: u8, alone: bool) {
    let candidate = |l: &Line| -> Option<bool> {
        if l.removed || l.heading.is_some() {
            return None;
        }
        let text = heading_text(&l.text)?;
        let t = text.trim();
        let chars = t.chars().count();
        let words = t.split_whitespace().count();
        if !(2..=120).contains(&chars) || words > 15 || t.ends_with(['.', ',', ';']) {
            return None;
        }
        let first = t.chars().next()?;
        if !(first.is_uppercase() || first.is_ascii_digit()) {
            return None;
        }
        let letters = t.chars().filter(|c| c.is_alphabetic()).count();
        if letters * 2 < t.chars().filter(|c| !c.is_whitespace()).count() || !balanced(t) {
            return None;
        }
        // `1:` and `1)` number list items and values, not sections; a section
        // with one number has a short title.
        let first_token = t.split_whitespace().next().unwrap_or_default();
        if first_token.ends_with([':', ')'])
            && first_token.starts_with(|c: char| c.is_ascii_digit())
        {
            return None;
        }
        let depth = section_depth(t);
        if depth == 1 && words > 8 {
            return None;
        }
        let numbered = depth > 0;
        if body <= 0.0 {
            // No font sizes: only multi-level section numbers.
            return (depth >= 2 && words >= 2).then_some(false);
        }
        let big = l.size >= body * 1.15;
        let boldish = l.bold >= 0.8 && l.size >= body * 0.95;
        (big || (boldish && (numbered || words <= 10))).then_some(big)
    };
    // The title pages' large type is left out of the size ranks, so the
    // chapters are not nested under the title lines.
    let title_pages = if pages.len() >= 6 { 2 } else { 0 };
    let mut found: Vec<(usize, usize, bool, i32, usize)> = Vec::new();
    let mut lines = 0usize;
    for (p, page) in pages.iter().enumerate() {
        for (i, line) in page.iter().enumerate().filter(|(_, l)| !l.removed) {
            lines += 1;
            if let Some(big) = candidate(line) {
                let depth = heading_text(&line.text)
                    .and_then(|t| numbering(&t))
                    .map_or(0, |n| n.split('-').count());
                found.push((p, i, big, (line.size * 2.0).round() as i32, depth));
            }
        }
    }
    // Too many candidates: the layout is not text with headings (slides,
    // forms); keep only clearly larger lines.
    let strict = lines >= 50 && found.len() * 4 > lines;
    let mut sizes: Vec<i32> = found
        .iter()
        .filter(|f| f.2 && f.0 >= title_pages)
        .map(|f| f.3)
        .collect();
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    sizes.dedup();
    sizes.truncate(3);
    // Mostly numbered headings (`3.2.1`): the number's depth is the level.
    let numbered = found.iter().filter(|f| f.4 > 0).count();
    let by_number = body <= 0.0 || numbered * 2 >= found.len().max(1);
    let deepest = found.iter().map(|f| f.4).max().unwrap_or(1).max(1);
    let base = if alone { 0 } else { after };
    for (p, i, big, key, depth) in found {
        let Some(line) = pages.get_mut(p).and_then(|page| page.get_mut(i)) else {
            continue;
        };
        if strict && !(big && line.size >= body * 1.3) {
            continue;
        }
        // A section number always gives the level (3.1.1 under 3.1).
        let level = if depth > 0 {
            depth
        } else if by_number {
            if big && sizes.first().is_none_or(|top| key >= *top) {
                1
            } else {
                deepest + 1
            }
        } else if big {
            sizes.iter().position(|s| *s == key).map_or_else(
                || {
                    if key > sizes.first().copied().unwrap_or(0) {
                        1
                    } else {
                        sizes.len() + 1
                    }
                },
                |r| r + 1,
            )
        } else {
            sizes.len() + 1
        };
        let level = u8::try_from(level)
            .unwrap_or(6)
            .saturating_add(base)
            .clamp(1, 6);
        line.heading = Some((level, true));
    }
}

/// The depth of a heading's section number (`3.2.1 x` is 3), 0 without one.
fn section_depth(text: &str) -> usize {
    numbering(text).map_or(0, |n| n.split('-').count())
}

/// Whether the brackets of a text are paired (a cut sentence is not).
fn balanced(text: &str) -> bool {
    let mut depth = 0i32;
    for c in text.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

/// The text of a possible heading: a gap after a leading section number
/// (`2.3 <gap> Title`) is a space; any other column gap means it is not one.
fn heading_text(text: &str) -> Option<String> {
    let mut parts = text.split('\t');
    let first = parts.next()?;
    let rest: Vec<&str> = parts.collect();
    match rest.as_slice() {
        [] => Some(text.to_owned()),
        [title]
            if first.trim().len() <= 8 && numbering(&format!("{} x", first.trim())).is_some() =>
        {
            Some(format!("{} {}", first.trim(), title.trim()))
        }
        _ => None,
    }
}

/// The lines as blocks: headings, paragraphs with hyphenated words joined,
/// and tables from runs of column-aligned lines.
fn blocks(pages: &[Vec<Line>]) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let mut para: Option<(String, u32, f32, f32)> = None;
    let flush = |para: &mut Option<(String, u32, f32, f32)>, out: &mut Vec<Block>| {
        if let Some((text, page, _, _)) = para.take() {
            out.push(Block::new(Kind::Paragraph, escape(&text)).on_page(page));
        }
    };
    for (p, page) in pages.iter().enumerate() {
        let number = u32::try_from(p + 1).unwrap_or(u32::MAX);
        let live: Vec<&Line> = page.iter().filter(|l| !l.removed).collect();
        let mut i = 0;
        while let Some(line) = live.get(i) {
            if let Some((level, inferred)) = line.heading {
                flush(&mut para, &mut out);
                let mut text = squash(
                    &heading_text(&line.text).unwrap_or_else(|| line.text.replace('\t', " ")),
                );
                // A heading wrapped onto the next line of the same style.
                while let Some(next) = live.get(i + 1) {
                    let same = next.heading == line.heading
                        && inferred
                        && (next.size - line.size).abs() < 0.3;
                    if !same || numbering(&next.text).is_some() || text.len() > 120 {
                        break;
                    }
                    text.push(' ');
                    text.push_str(&squash(&next.text));
                    i += 1;
                }
                let mut b = Block::heading(level, text).on_page(number);
                b.inferred = inferred;
                out.push(b);
                i += 1;
                continue;
            }
            let run = live
                .get(i..)
                .unwrap_or_default()
                .iter()
                .take_while(|l| l.heading.is_none() && l.text.contains('\t'))
                .count();
            if run >= 3 {
                flush(&mut para, &mut out);
                let rows: Vec<&str> = live
                    .get(i..i + run)
                    .unwrap_or_default()
                    .iter()
                    .map(|l| l.text.as_str())
                    .collect();
                out.push(Block::new(Kind::Table, table(&rows)).on_page(number));
                i += run;
                continue;
            }
            let text = line.text.replace('\t', "  ");
            let bullet = is_bullet(&text);
            let starts_lower = text.chars().next().is_some_and(char::is_lowercase);
            let new_para = match &para {
                None => true,
                Some((prev, prev_page, prev_bottom, prev_size)) => {
                    let gap = if line.size > 0.0 && *prev_page == number {
                        prev_bottom - line.top
                    } else {
                        0.0
                    };
                    let size_jump = line.size > 0.0
                        && *prev_size > 0.0
                        && ((line.size - prev_size).abs() / prev_size) > 0.15;
                    let page_break = *prev_page != number
                        && (!starts_lower || prev.ends_with(['.', ':', '!', '?']));
                    bullet || size_jump || page_break || gap > line.size.max(*prev_size) * 0.9
                }
            };
            if new_para {
                flush(&mut para, &mut out);
                let text = if bullet {
                    format!(
                        "- {}",
                        text.chars().skip(1).collect::<String>().trim_start()
                    )
                } else {
                    text
                };
                para = Some((text, number, line.bottom, line.size));
            } else if let Some((acc, _, bottom, size)) = para.as_mut() {
                join(acc, &text);
                *bottom = line.bottom;
                *size = line.size;
            }
            i += 1;
        }
        // keep a paragraph open across the page break; `new_para` decides
    }
    flush(&mut para, &mut out);
    out
}

/// Appends a line to a paragraph; a word hyphenated at the line end is
/// joined back when the next line goes on in lower case.
fn join(acc: &mut String, next: &str) {
    let hyphen = acc.ends_with('-') && acc.chars().rev().nth(1).is_some_and(char::is_alphabetic);
    if hyphen && next.chars().next().is_some_and(char::is_lowercase) {
        acc.pop();
        acc.push_str(next);
    } else {
        acc.push(' ');
        acc.push_str(next);
    }
}

/// Whether a line starts with a list bullet: a bullet sign, or a symbol-font
/// glyph of the private use area (Wingdings and the like), with or without
/// a space after it, or a dash and a space.
fn is_bullet(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let next_space = chars.next().is_some_and(char::is_whitespace);
    match first {
        '•' | '◦' | '▪' | '■' | '●' | '○' | '►' | '▸' | '✓' | '\u{E000}'..='\u{F8FF}' => {
            true
        }
        '–' | '-' | '*' => next_space,
        _ => false,
    }
}

/// A paragraph that would read as Markdown structure is escaped.
fn escape(text: &str) -> String {
    let t = text.trim();
    if t.starts_with('#') || t.starts_with("@@@") || t.starts_with("```") {
        format!("\\{t}")
    } else {
        t.to_owned()
    }
}

/// Rows of tab-separated cells as a Markdown table.
fn table(rows: &[&str]) -> String {
    let cells: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            r.split('\t')
                .map(|c| squash(c).replace('|', "\\|"))
                .collect()
        })
        .collect();
    let width = cells.iter().map(Vec::len).max().unwrap_or(1);
    let line = |row: &Vec<String>| {
        let mut cols: Vec<String> = row.clone();
        cols.resize(width, String::new());
        format!("| {} |", cols.join(" | "))
    };
    let mut out = Vec::new();
    if let Some(head) = cells.first() {
        out.push(line(head));
        out.push(format!("|{}", "---|".repeat(width)));
    }
    out.extend(cells.iter().skip(1).map(line));
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, size: f32, zone: bool) -> Line {
        Line {
            text: text.into(),
            size,
            zone,
            ..Line::default()
        }
    }

    #[test]
    fn running_heads_and_page_numbers_go() {
        let mut pages: Vec<Vec<Line>> = (1..=10)
            .map(|p| {
                vec![
                    line("Drive Manual  Chapter 3", 8.0, true),
                    line(
                        &format!("Body text of page {p} that is long enough"),
                        10.0,
                        false,
                    ),
                    line(&format!("Page {p} of 10"), 8.0, true),
                ]
            })
            .collect();
        let removed = running_heads(&mut pages, 10.0);
        assert_eq!(removed, 20);
        assert!(
            pages
                .iter()
                .flatten()
                .filter(|l| !l.removed)
                .all(|l| l.text.starts_with("Body"))
        );
    }

    #[test]
    fn contents_pages_go() {
        let mut pages: Vec<Vec<Line>> = vec![
            (1..=8)
                .map(|i| {
                    line(
                        &format!("{i} Chapter title ........ {}", i * 10),
                        10.0,
                        false,
                    )
                })
                .collect(),
            (1..=8)
                .map(|i| line(&format!("Real text line {i}"), 10.0, false))
                .collect(),
        ];
        assert_eq!(contents_and_index(&mut pages), 8);
    }

    #[test]
    fn headings_by_size_and_bookmarks() {
        let mut pages = vec![vec![
            line("1 Introduction", 16.0, false),
            line(
                "The body text of the introduction goes on here.",
                10.0,
                false,
            ),
            line("1.1 Scope", 12.0, false),
            line("More body text, and then some more of it.", 10.0, false),
            line("And more body text for the statistics.", 10.0, false),
        ]];
        let body = body_size(&pages);
        assert!((body - 10.0).abs() < 0.1);
        infer_headings(&mut pages, body, 0, true);
        let levels: Vec<Option<(u8, bool)>> = pages.iter().flatten().map(|l| l.heading).collect();
        assert_eq!(levels, [Some((1, true)), None, Some((2, true)), None, None]);

        let mut pages = vec![
            vec![
                line("3.2 Encoder settings", 10.0, false),
                line("text", 10.0, false),
            ],
            vec![],
        ];
        let marks = [
            Mark {
                level: 2,
                title: "3.2 Encoder settings".into(),
                page: 0,
            },
            Mark {
                level: 1,
                title: "Appendix".into(),
                page: 1,
            },
        ];
        assert_eq!(apply_marks(&mut pages, &marks), 1);
        assert_eq!(
            pages
                .first()
                .and_then(|p| p.first())
                .and_then(|l| l.heading),
            Some((2, false))
        );
        assert_eq!(
            pages
                .get(1)
                .and_then(|p| p.first())
                .map(|l| l.text.as_str()),
            Some("Appendix")
        );
    }

    #[test]
    fn paragraphs_join_hyphenated_words_and_tables_form() {
        let mut a = line("The connec-", 10.0, false);
        a.top = 700.0;
        a.bottom = 690.0;
        let mut b = line("tion is made here.", 10.0, false);
        b.top = 688.0;
        b.bottom = 678.0;
        let rows: Vec<Line> = ["Name\tValue\tUnit", "Gain\t12\tdB", "Time\t5\tms"]
            .iter()
            .map(|t| line(t, 10.0, false))
            .collect();
        let mut page = vec![a, b];
        page.extend(rows);
        let out = blocks(&[page]);
        assert_eq!(
            out.first().map(|b| b.text.as_str()),
            Some("The connection is made here.")
        );
        assert_eq!(out.get(1).map(|b| b.kind), Some(Kind::Table));
        assert!(
            out.get(1)
                .is_some_and(|b| b.text.contains("| Gain | 12 | dB |"))
        );
    }

    #[test]
    fn contents_lines_with_leaders_after_the_number() {
        assert!(contents_line(
            "1.1\tProper use\t1\u{2013}1 . . . . . . . . . . ."
        ));
        assert!(contents_line("Safety Instructions ........ 12"));
        assert!(contents_line("2 Overview\t2-1"));
        assert!(!contents_line("The value is set to 12 in the example."));
        assert_eq!(
            heading_text("2.3\tKns_AnwenderInit").as_deref(),
            Some("2.3 Kns_AnwenderInit")
        );
        assert_eq!(heading_text("Name\tValue\tUnit"), None);
        assert_eq!(ref_key("Overview\t2\u{2013}3").as_deref(), Some("overview"));
    }

    #[test]
    fn page_numbers_are_recognised() {
        for t in [
            "12",
            "- 12 -",
            "Page 3 of 10",
            "Seite 4",
            "iv",
            "3/12",
            "12. oldal",
        ] {
            assert!(is_page_number(t), "{t}");
        }
        for t in ["Introduction", "12 Parameters", "Page layout"] {
            assert!(!is_page_number(t), "{t}");
        }
    }
}

#[cfg(test)]
mod probe {
    use super::*;

    /// `KAPU_PDF=<file> KAPU_PAGE=<n> cargo test -p kapu-create probe -- --ignored --nocapture`
    #[test]
    #[ignore = "a probe for a real file"]
    fn probe_page_lines() {
        let (Some(file), Some(page)) = (
            std::env::var_os("KAPU_PDF"),
            std::env::var("KAPU_PAGE").ok(),
        ) else {
            return;
        };
        let options = Options {
            pdfium: std::env::var_os("KAPU_PDFIUM").map(PathBuf::from),
            ..Options::default()
        };
        let Some(lib) = pdfium(&options) else { return };
        let bytes = std::fs::read(file).unwrap_or_default();
        let Ok(pdf) = lib.load_pdf_from_byte_slice(&bytes, None) else {
            return;
        };
        let n: i32 = page.parse().unwrap_or(1);
        let Ok(p) = pdf.pages().get(n - 1) else {
            return;
        };
        let (lines, _) = page_lines(&p);
        for l in lines {
            println!(
                "{:5.1} b{:.1} z{} top{:6.1} | {}",
                l.size,
                l.bold,
                u8::from(l.zone),
                l.top,
                l.text.replace('\t', " ⇥ ")
            );
        }
    }
}
