//! File type detection (plan L1/2): the content's magic bytes first, the
//! extension second, so a file with a wrong extension still reaches the
//! right reader.

/// What a file is, as far as the readers care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// PDF.
    Pdf,
    /// A Microsoft Office file (`office_oxide`).
    Office(office_oxide::DocumentFormat),
    /// ODF, RTF, EPUB or another format of `anydoc`.
    Anydoc(anydoc::Format),
    /// HTML.
    Html,
    /// Markdown.
    Markdown,
    /// Plain text.
    Text,
    /// Delimiter-separated values.
    Csv,
    /// Source code or a data file shown as code.
    Code(&'static str),
    /// ZIP archive (not an Office or ODF package).
    Zip,
    /// 7z archive.
    SevenZ,
    /// RAR archive: detected, not unpacked (KD8).
    Rar,
    /// A mail (`.eml`, RFC 5322).
    Eml,
    /// An Outlook message (`.msg`).
    Msg,
    /// Compiled HTML help (`.chm`).
    Chm,
    /// A raster image, read by OCR.
    Image,
    /// Anything else: reported, not read.
    Unknown,
}

const CODE: &[(&str, &str)] = &[
    ("rs", "rust"), ("py", "python"), ("js", "javascript"), ("mjs", "javascript"),
    ("ts", "typescript"), ("tsx", "tsx"), ("jsx", "jsx"), ("c", "c"), ("h", "c"),
    ("cpp", "cpp"), ("cc", "cpp"), ("hpp", "cpp"), ("cs", "csharp"), ("java", "java"),
    ("kt", "kotlin"), ("go", "go"), ("rb", "ruby"), ("php", "php"), ("swift", "swift"),
    ("sh", "bash"), ("bash", "bash"), ("ps1", "powershell"), ("bat", "bat"), ("cmd", "bat"),
    ("sql", "sql"), ("json", "json"), ("yaml", "yaml"), ("yml", "yaml"), ("toml", "toml"),
    ("xml", "xml"), ("ini", "ini"), ("cfg", "ini"), ("lua", "lua"), ("r", "r"),
    ("scala", "scala"), ("dart", "dart"), ("vue", "vue"), ("svelte", "svelte"),
    ("css", "css"), ("scss", "scss"), ("st", "iecst"), ("scl", "iecst"), ("awl", "text"),
    ("vhd", "vhdl"), ("v", "verilog"), ("m", "matlab"), ("pl", "perl"), ("ex", "elixir"),
    ("zig", "zig"), ("nim", "nim"), ("hs", "haskell"), ("ml", "ocaml"), ("cmake", "cmake"),
    ("gradle", "groovy"), ("dockerfile", "dockerfile"), ("makefile", "makefile"),
];

/// The type of a file named `name` with content `bytes`.
#[must_use]
pub fn detect(bytes: &[u8], name: &str) -> FileType {
    let ext = extension(name);
    if let Some(t) = by_magic(bytes, &ext) {
        return t;
    }
    by_extension(&ext, bytes)
}

/// The lower-case extension of `name` (the whole name for `Makefile`-like
/// names without one).
#[must_use]
pub fn extension(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or(name);
    match last.rfind('.') {
        Some(i) if i > 0 => last.get(i + 1..).unwrap_or_default().to_lowercase(),
        _ => last.to_lowercase(),
    }
}

fn by_magic(bytes: &[u8], ext: &str) -> Option<FileType> {
    let head = bytes.get(..bytes.len().min(1024)).unwrap_or(bytes);
    if find(head, b"%PDF-").is_some() {
        return Some(FileType::Pdf);
    }
    if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") {
        return Some(zip_package(bytes, ext));
    }
    if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        return Some(ole(bytes, ext));
    }
    if bytes.starts_with(&[b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C]) {
        return Some(FileType::SevenZ);
    }
    if bytes.starts_with(b"Rar!\x1A\x07") {
        return Some(FileType::Rar);
    }
    if bytes.starts_with(b"ITSF") {
        return Some(FileType::Chm);
    }
    if bytes.starts_with(b"{\\rtf") {
        return Some(FileType::Anydoc(anydoc::Format::Rtf));
    }
    let image = bytes.starts_with(b"\x89PNG")
        || bytes.starts_with(&[0xFF, 0xD8, 0xFF])
        || bytes.starts_with(b"GIF8")
        || bytes.starts_with(b"II*\0")
        || bytes.starts_with(b"MM\0*")
        || (bytes.starts_with(b"BM") && matches!(ext, "bmp" | "dib"))
        || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"));
    if image {
        return Some(FileType::Image);
    }
    None
}

fn zip_package(bytes: &[u8], ext: &str) -> FileType {
    match office_format(ext) {
        Some(f @ (office_oxide::DocumentFormat::Docx
        | office_oxide::DocumentFormat::Xlsx
        | office_oxide::DocumentFormat::Pptx)) => return FileType::Office(f),
        _ => {}
    }
    match anydoc::Format::from_bytes(bytes) {
        Some(anydoc::Format::Docx) => FileType::Office(office_oxide::DocumentFormat::Docx),
        Some(anydoc::Format::Pptx) => FileType::Office(office_oxide::DocumentFormat::Pptx),
        Some(anydoc::Format::Excel) if ext != "xlsb" => {
            FileType::Office(office_oxide::DocumentFormat::Xlsx)
        }
        Some(f) => FileType::Anydoc(f),
        None => FileType::Zip,
    }
}

fn ole(bytes: &[u8], ext: &str) -> FileType {
    if ext == "msg" {
        return FileType::Msg;
    }
    match anydoc::Format::from_bytes(bytes) {
        Some(anydoc::Format::Doc) => FileType::Office(office_oxide::DocumentFormat::Doc),
        Some(anydoc::Format::Ppt) => FileType::Office(office_oxide::DocumentFormat::Ppt),
        Some(anydoc::Format::Excel) => FileType::Office(office_oxide::DocumentFormat::Xls),
        Some(f) => FileType::Anydoc(f),
        None => match office_format(ext) {
            Some(f) => FileType::Office(f),
            // An OLE file with a `__substg1.0_` stream is an Outlook message.
            None if find(bytes, &utf16("__substg1.0_")).is_some() => FileType::Msg,
            None => FileType::Unknown,
        },
    }
}

fn office_format(ext: &str) -> Option<office_oxide::DocumentFormat> {
    use office_oxide::DocumentFormat as F;
    Some(match ext {
        "docx" | "docm" | "dotx" | "dotm" => F::Docx,
        "xlsx" | "xlsm" | "xltx" | "xltm" => F::Xlsx,
        "pptx" | "pptm" | "ppsx" | "ppsm" | "potx" => F::Pptx,
        "doc" | "dot" => F::Doc,
        "xls" | "xlt" => F::Xls,
        "ppt" | "pps" | "pot" => F::Ppt,
        _ => return None,
    })
}

fn by_extension(ext: &str, bytes: &[u8]) -> FileType {
    match ext {
        "html" | "htm" | "xhtml" | "shtml" => return FileType::Html,
        "md" | "markdown" | "mdx" | "mkd" => return FileType::Markdown,
        "txt" | "text" | "log" | "rst" | "adoc" | "asciidoc" | "org" | "nfo" => return FileType::Text,
        "csv" | "tsv" => return FileType::Csv,
        "eml" | "mht" | "mhtml" => return FileType::Eml,
        "msg" => return FileType::Msg,
        "odt" => return FileType::Anydoc(anydoc::Format::Odt),
        "ods" => return FileType::Anydoc(anydoc::Format::Ods),
        "odp" => return FileType::Anydoc(anydoc::Format::Odp),
        "epub" => return FileType::Anydoc(anydoc::Format::Epub),
        "rtf" => return FileType::Anydoc(anydoc::Format::Rtf),
        _ => {}
    }
    if let Some((_, lang)) = CODE.iter().find(|(e, _)| *e == ext) {
        return FileType::Code(lang);
    }
    if !crate::text::looks_like_text(bytes) {
        return FileType::Unknown;
    }
    let head = String::from_utf8_lossy(bytes.get(..bytes.len().min(512)).unwrap_or(bytes)).to_lowercase();
    let head = head.trim_start();
    if head.starts_with("<!doctype html") || head.starts_with("<html") {
        return FileType::Html;
    }
    let mail_headers = ["from:", "received:", "return-path:", "message-id:", "mime-version:"];
    if mail_headers.iter().any(|h| head.starts_with(h)) && head.contains("\nsubject:") {
        return FileType::Eml;
    }
    FileType::Text
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_wins_over_the_extension() {
        assert_eq!(detect(b"%PDF-1.7\n...", "notes.txt"), FileType::Pdf);
        assert_eq!(detect(b"\x89PNG\r\n\x1a\n", "x.dat"), FileType::Image);
        assert_eq!(detect(b"{\\rtf1 hello}", "a.doc"), FileType::Anydoc(anydoc::Format::Rtf));
        assert_eq!(detect(b"Rar!\x1A\x07\x00", "a.zip"), FileType::Rar);
    }

    #[test]
    fn text_types_come_from_the_extension() {
        assert_eq!(detect(b"# hi", "a.md"), FileType::Markdown);
        assert_eq!(detect(b"fn main() {}", "main.rs"), FileType::Code("rust"));
        assert_eq!(detect(b"<!DOCTYPE html><p>x", "page"), FileType::Html);
        assert_eq!(detect(b"From: a@b\nSubject: x\n\nbody", "mail"), FileType::Eml);
        assert_eq!(detect(b"plain words", "README"), FileType::Text);
        assert_eq!(detect(&[0u8, 1, 2, 3, 0, 0], "blob.bin"), FileType::Unknown);
    }
}
