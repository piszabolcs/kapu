# KAPU maker specification  v0.1  (2026-11-03)

The maker (`kapu-create`, crate `crates/kapu-create`, and the window's Create view) turns web sites, documents and folders into a KAPU file in one step, without an agent. The rooms it writes are ordinary rooms of [SPEC-KAPU.md](SPEC-KAPU.md); the file is built by `kapu-core` like any other. This document fixes what the maker reads, how it cuts rooms, and the source list it keeps for a refresh. Decisions are `KD1`..`KD17`.

## 1. Scope

1. **Sources:** files, folders (recursive, sorted by path, without hidden folders, `node_modules`, `target`, `__pycache__`, `venv`, `.kapu` and `.sources.json` files) and http(s) addresses (KD2).
2. **File types**, recognised by their content first and their extension second:

| Type | Reader | Structure |
|---|---|---|
| PDF | PDFium (`pdfium-render` 0.9, PDFium chromium/8125); without it `pdf-extract` 0.12 and `lopdf` 0.46 | bookmarks; headings from type size, weight and section numbers |
| Word, Excel, PowerPoint (DOC, DOCX, XLS, XLSX, PPT, PPTX) | `office_oxide` 0.1 | heading styles, slide titles, sheet names |
| ODT, ODS, ODP, RTF, EPUB, CSV | `anydoc` 0.2 (no network code; its hosted OCR is never used) | headings |
| HTML | `scraper` 0.29 (main content), `htmd` 0.5 (Markdown) | `h1`..`h6` |
| Markdown, text, source code | own, `pulldown-cmark` 0.13; `chardetng` and `encoding_rs` for old code pages | Markdown headings; code as one code block |
| E-mail (EML, MSG) | `mail-parser` 1.0, `msg_parser` 0.3 | subject as title; attachments as documents |
| ZIP, 7z | `zip` 9, `sevenz-rust2` 0.24 | every member as a file |
| CHM | `hh.exe -decompile` (Windows only), the `.hhc` order | the pages' HTML |
| Images (PNG, JPEG, TIFF, BMP, GIF, WebP) | Tesseract 5 (tested with 5.4.0) as a separate process | none |
| RAR | not unpacked (licence) | reported |

3. **Not done:** running anything from the content; JavaScript of web pages (reported); password prompts; RAR.

## 2. Reading

1. **Fault isolation:** every file is read on its own thread; an error or a panic of a parser fails that file only. Nothing is skipped silently: a file that is not read is in the report with the reason (KD2).
2. **Archives and mails** (KD7, KD8): a member's key is `<archive key>!<member path>`, its name `<archive name> > <member path>`; at most 5 levels deep; a member over 1 GiB or with a compression ratio over 200 above 64 MiB is not unpacked; an archive stops after 4 GiB unpacked. `__MACOSX`, `._*`, `.DS_Store`, `Thumbs.db`, `desktop.ini` are left out. A mail lists its attachments as links to their rooms.
3. **PDF** (KD4):
   1. A line is a run of characters in PDFium's reading order; its size is the mean size of its letters and digits (a symbol glyph does not count); bold comes from the font weight or, when the font reports none, from its name (`Bold`, `Black`, `Heavy`, `Semibold`, `Demi`). A gap wider than 1.6 em between two letters is a column break (a tab). A character in U+0080..U+009F is read as Windows-1252.
   2. **Running heads:** a line in the zone of a page (its first or last three lines, or the top or bottom 7%) is dropped when, with its digits as `#`, it repeats on at least 30% of the pages (at least 3), or when it carries a page reference (`12`, `3-12`, `A-4`) and its words repeat on 3 pages and its type is not larger than the body's by 15%, or when it is a page number (`12`, `- 12 -`, `Page 3 of 10`, `Seite 4`, `12. oldal`, roman numerals).
   3. **Contents and index pages:** a page in the first 15% (at least 10 pages) is a contents page when at least 40% of its lines are contents lines (a dot leader with a page reference, or a page reference after a column gap). After an index title (`Index`, `Stichwortverzeichnis`, `Register`, `Tárgymutató`, ...) in the last 15%, a page is an index page when 60% of its lines end in page numbers. Both are dropped.
   4. **Bookmarks:** every bookmark marks the line of its page (or the next page) that starts with its title as a heading of its level; a title wrapped onto the next line takes that line too; a bookmark not found is inserted at the top of its page.
   5. **Inferred headings,** when there are fewer than 3 bookmarks or more than 10 pages per bookmark: a line of 2..120 characters and at most 15 words, not ending in `.`, `,` or `;`, starting with a capital or a digit, mostly letters, with paired brackets, whose size is at least 1.15 times the body's, or bold at body size with a section number or at most 10 words. A number followed by `:` or `)` is not a section number; a one-level number allows at most 8 words. The level is the section number's depth (`3.2.1` is 3); without a number, the size's rank among the heading sizes outside the first two pages (title pages larger than all ranks are level 1). When more than a quarter of 50 or more lines would be headings, only lines 1.3 times the body size are kept.
   6. **Paragraphs:** a new paragraph starts at a heading, a bullet (`•` and the like, private-use symbol glyphs, `-`, `–` or `*` with a space), a size change of 15%, a vertical gap of 0.9 line, or a new page (unless the sentence goes on in lower case). A word hyphenated at a line end is joined when the next line goes on in lower case. Three or more consecutive lines with column gaps form a Markdown table.
   7. **Image pages:** a page with fewer than 20 letters and an image is rendered at 300 dpi in grey and read by Tesseract (KD5): a separate process per page, 120 s each, on half the cores, `--psm 3`, languages `eng+deu+hun` by default; without Tesseract the pages are reported. With several languages the document's one language is used when its frequent words show it (at least 20, twice as many as of the next language): from the document's text pages, else from the first image page read with all the languages; an image file is read again in its language. A language without a word list (only `eng`, `deu`, `hun` have one) keeps the languages as given.
   8. **Title:** the metadata title when it reads as one (two words of letters, mostly letters, no `*` or `_`, not a file name), else the file name; a file name that is not a title gives way to the first readable heading of the first two pages.
4. **HTML:** the first of `main`, `[role=main]`, `article`, `#content`, `.content`, `.markdown-body` with at least 30% of the body's visible text, else `body` without `header`; `script`, `style`, `noscript`, `svg`, `template`, `iframe`, `nav`, `aside`, `footer`, `form`, `button`, `select`, `input`, `canvas`, `dialog` are dropped. The title is the main part's `h1`, else `<title>`; the description the meta description. A page with less than 200 characters of visible text, scripts, and an application root (`#root`, `#app`, `#__next`, ...) or a `noscript` asking for JavaScript is reported as built by JavaScript.
5. **Text normalisation** (all readers): LF line ends, NFC, the characters the body forbids or warns about removed (FR-02), ligatures spelled out, soft hyphens and no-break spaces resolved.

## 3. Web sites (KD9)

1. **Scope:** the address's origin and first path segment (`https://elevenlabs.io/docs/overview/intro` → `https://elevenlabs.io/docs/`; a file name in the path keeps its folder), or `--scope`.
2. **The ladder,** first that gives pages: (1) `llms.txt` in the scope, then at the origin: its `[title](url): description` lines inside the scope, at least 2, each fetched (a `.md` link is read as Markdown); (2) `llms-full.txt`, one Markdown document; (3) the sitemaps of `robots.txt` and `/sitemap.xml` (indexes followed 2 levels, at most 50 maps), the URLs inside the scope, at least 2; (4) a crawl from the address, following links inside the scope (assets left out).
3. **Politeness:** `robots.txt` is obeyed (agent `KAPU`, its crawl delay up to 30 s, else 100 ms between requests), 4 requests at a time, 429 and 5xx retried 3 times after `Retry-After` or 2, 4, 8 s, redirects only on the same host, 30 s and 20 MiB per page, at most `--max-pages` (5000) pages. User-Agent: `KAPU/<version> (kapu-create; +https://github.com/piszabolcs/kapu)`.
4. **Addresses:** fragments and tracking parameters (`utm_*`, `ref`, `fbclid`, `gclid`) are dropped. A page's Markdown twin (`intro.md`) has the page's own address as key (`intro`).
5. 401 and 403 answers are reported (login or a bot check).

## 4. Rooms (KD11-KD14)

1. **Boilerplate:** on a site of at least 4 pages, a non-heading block of at most 600 characters that is on at least half of the pages (and at least 3) is dropped.
2. **Section tree:** the headings of a document; a document whose text is all under one top heading has that heading as its title.
3. **Copies** (KD13): a section of at least `--min-section` tokens whose text, with the titles of its sub-sections, repeats an earlier section word for word becomes the line `Same text as in [title](#id).` (`--no-dedupe` keeps them).
4. **Cutting** (KD11), `o200k_base` tokens as on the map: a section that fits the frame (`--room-max`, 4000) is one room, its sub-sections `##` sub-doors and the deeper ones `###`..`######`; a larger one gives its own text before the first sub-section a room and goes on with its sub-sections; sections under `--min-section` (200) are packed with their neighbours into one room (at most one larger section a room), titled `first - last`; a long section without sub-sections is split at block boundaries, a block larger than the frame by rows (a table, its header repeated), lines (code, its fence repeated) or sentences, titled `title, p. 12-15` for a paged source, else `title (2/5)`. A web page is never merged with another page.
5. **Overview** (KD12): a document cut into at least 2 rooms whose own text before the first section fits gets an overview room: that text, then `## Contents` with links to its top-level rooms within a fifth of the frame (at most 700 tokens).
6. **Room template** (KD14):

```
# <title>
> <the source's description, in the document's first room only>

Source: <name>[, p. <label>-<label> | , <n> pages]
Path: [<document>](#<overview>) > [<chapter>](#<room>)

<content>
```

7. **Titles:** a title that repeats in the file gets its parent section, then its document, in brackets.
8. **Ids:** the prefix is the slug of the document title (at most 24 characters) or of a web page's path without extension (48); a document of one room, and an overview, have the prefix as id; other rooms `<prefix>-<section number or title slug (40)>`; collisions get `-2`, `-3` (FR-04/4). The id of a section key (`<document key>#<section path>`, `#intro` for a section's own text, `#<k>` for a piece, `#overview`) is kept from the source list when it is free.
9. **Links:** a Markdown link to another document of the run (a relative path, a web address inside the run, or `kapu-key:<key>`) becomes `[text](#<its first room>)`; on a web page the other relative links become absolute.
10. The planner is deterministic: the same documents give the same rooms byte for byte.

## 5. Source list and refresh (KD15, KD16)

1. Beside `x.kapu`, `x.sources.json`: `version` (1), `tool`, `sources`, the planner settings (`room_max`, `sub_max`, `min_section`, `overview`, `dedupe`), `ocr_langs`, `max_pages`, `scope`, `ids` (section key → room id) and `rooms` (the ids the maker wrote).
2. `kapu-create refresh x.kapu` reads the list, reads the sources again with its settings, keeps the rooms of `x.kapu` that are not in `rooms` (written by hand or by `kapu_write`), and gives every section the id it had.
3. **Cache:** a file's documents are stored as JSON under the KAPU cache folder (`create/<BLAKE3>.json`; key: the reader version, the OCR languages and resolution, and the BLAKE3 of the bytes), so an unchanged file is not read again. `--no-cache` turns it off. Identical files in one run are read once.
4. The file is built like `kapu import` does (D173): with more than 200 vector windows to compute it is written first without vectors, searchable by keyword, then with them.

## 6. Interfaces

1. **CLI:** `kapu-create new <file> <source>...`, `refresh <file>`, `preview <source>... [--rooms-dir <folder>]`; switches `--json`, `--model`, `--no-model`, `--no-cache`, `--pdfium`, `--tesseract`, `--ocr-lang`, `--room-max`, `--sub-max`, `--min-section`, `--no-overview`, `--no-dedupe`, `--max-pages`, `--scope`. Progress on stderr, the report on stdout. The exit code is 1 when no text could be read or the file cannot be written.
2. **Window:** the Create view (sources by button, address or drop, the file to write, options, preview, progress, report, open the file) and Refresh from sources in the Check view; the commands `create` and `create_pick`, progress as `create-progress` events.
3. **Programs found next to the maker:** PDFium (`--pdfium`, `KAPU_PDFIUM`, the program's folder or its `pdfium` folder, the system) and Tesseract (`--tesseract`, `KAPU_TESSERACT`, the program's folder or its `tesseract` folder, `Tesseract-OCR` under Program Files and LocalAppData, `PATH`).

## 7. Decisions

- **KD1** A separate crate and program; only the maker and the window use the network (SPEC-KAPU chapter 7/6). No MCP tool fetches anything (prompt injection could send data out).
- **KD2** Every file type above in the first version; nothing is skipped silently.
- **KD3** Rust only, no Python, no GPL or AGPL. PDFium (BSD/Apache) and Tesseract (Apache-2.0) are separate programs. PDFium is in the release packages; Tesseract is not: the libraries its Windows build (UB Mannheim 5.4.0) loads include GPL ones (`libjbig` GPL-2.0+, the GCC runtime `libgcc_s` and `libstdc++` GPL-3.0 with the runtime exception), so the user installs it and the maker finds it. The licences `bzip2-1.0.6` (libbz2-rs-sys) and `CDLA-Permissive-2.0` (webpki-roots) are permissive and allowed.
- **KD4** PDFium first (on the 14-PDF measurement: text F1 0.98-1.00, reading order 0.98-1.00, the 728-page manual in 0.8 s), `pdf-extract` without it (F1 0.98-1.00, slower, no bookmarks), `pdf_oxide` not (reading order 0.57-0.97).
- **KD5** Tesseract, not `ocrs` (on synthetic scans `ocrs` reads no accented letter; Tesseract with the right language: recall German 0.90-1.00, Hungarian 0.81-1.00, English 0.91-0.96). Measured through `kapu-create` with Tesseract 5.4.0 on scans made at 200 dpi from born-digital pages (half with noise and a tilt), word recall against their text layer: German law text 0.991 (accented words 1.000), Hungarian article 0.957 (0.957), English two-column paper 0.945, a datasheet image 0.960; 15 pages and an image in 6 s on 6 processes, a second run from the cache in under a second. One language beats several together: on the Hungarian pages `eng+deu+hun` gives 0.886 (accented 0.801) and takes 1.5 s a page against 0.9 s, hence the language detection.
- **KD6** `office_oxide` for the Microsoft formats (more headings in the old formats than `anydoc`: 22 against 7 in a DOC, 58 and 19 against 29 and 0 in two PPTs), `anydoc` for ODF, RTF, EPUB and CSV.
- **KD7** Mails with `mail-parser` and `msg_parser`; attachments as documents. An attached Outlook message inside an MSG is not read yet (reported).
- **KD8** ZIP and 7z in memory, with depth and size limits. `anydoc` 0.2.4 pins `zip` 8.6, whose advisories RUSTSEC-2026-0367, -0368, -0369 and -0429 concern `extract()` to disk, which neither `anydoc` nor the maker calls (ignored in `deny.toml` and the audit job).
- **KD9** The web ladder `llms.txt`, `llms-full.txt`, sitemap, crawl. Of 24 documentation sites tried, every reachable page had its text in the HTML; JavaScript pages are reported, a WebView reader may come later.
- **KD10** CHM through `hh.exe` on Windows.
- **KD11** Rooms of at most 4000 tokens, sub-doors about 1500 (one `kapu_open` part at the default budget), sections under 200 packed. Defaults, to be tuned on question sets.
- **KD12** An overview room per document of two or more rooms.
- **KD13** Sections copied word for word kept once.
- **KD14** The room template with `Source:` and `Path:`, English keywords.
- **KD15** The source list beside the file, not inside it, so the file format does not change.
- **KD16** The document cache by content.
- **KD17** Rendering PDF pages for OCR needs the `image` crate (PNG); the window links the maker (`kapu-gui` depends on `kapu-create`).
