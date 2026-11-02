//! The run (layers L4 and L6): sources to documents, documents to rooms,
//! rooms to a KAPU file, and the source list beside it for a refresh.
//!
//! Every file is read on its own thread with its failures (and panics of a
//! parser) caught, so one bad file never stops the others; every failure is
//! in the report. A document read once is cached by its bytes (KD16).

use std::collections::{BTreeMap, BTreeSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use kapu_core::build::{PENDING_WINDOWS, RoomSource, missing_windows, rebuild_pending, rebuild_with_progress, write_atomic};
use kapu_core::embed::Model;
use serde::{Deserialize, Serialize};

use crate::doc::Document;
use crate::readers::{self, Ctx};
use crate::rooms::{self, DocPlan, Stats};
use crate::{Error, Result, web};

/// Bumped when a reader's output changes, so the cache is not reused.
pub const READER_VERSION: u32 = 1;

/// Everything a run can be told.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// Reader options (OCR, PDFium, archive limits).
    pub read: readers::Options,
    /// Room planner settings (KD11-KD13).
    pub rooms: rooms::Settings,
    /// Crawl limits (KD9).
    pub web: web::Options,
    /// Do not use or fill the document cache.
    pub no_cache: bool,
}

/// What the run reports while it works.
#[derive(Debug, Clone)]
pub enum Progress<'a> {
    /// A web page fetched (`done` of `total`; `total` 0 while crawling).
    Fetched { done: usize, total: usize, url: &'a str },
    /// A file read.
    Read { done: usize, total: usize, name: &'a str },
    /// The rooms are planned.
    Planned { rooms: usize },
    /// Vector windows computed.
    Vectors { done: usize, total: usize },
    /// The file is written, searchable by keyword; vectors may follow.
    Written { path: &'a Path, pending: bool },
}

/// A progress callback.
pub type OnProgress<'a> = &'a (dyn Fn(Progress<'_>) + Sync);

/// One document in the report.
#[derive(Debug, Clone, Serialize)]
pub struct DocReport {
    /// Its key.
    pub key: String,
    /// The name shown.
    pub name: String,
    /// The reader, or `none` for a failure.
    pub reader: String,
    /// Pages of a paged source.
    pub pages: u32,
    /// Characters of text.
    pub chars: usize,
    /// Lines dropped as running heads, contents and index pages.
    pub removed: usize,
    /// Warnings and the failure, if any.
    pub warnings: Vec<String>,
}

/// The report of a run (plan L3 and L6: nothing skipped silently).
#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    /// The file written.
    pub file: String,
    /// How the web sources were found (`llms.txt`, `sitemap`, ...).
    pub web: Vec<String>,
    /// Every document read or failed.
    pub documents: Vec<DocReport>,
    /// The rooms per document.
    pub plan: Vec<DocPlan>,
    /// Planner counts.
    pub stats: Stats,
    /// Rooms written (with the kept hand-written ones).
    pub rooms: usize,
    /// Rooms kept from the earlier file on a refresh.
    pub kept: usize,
    /// Documents unchanged on a refresh (read from the cache).
    pub cached: usize,
    /// The vector model, or `none`.
    pub model: String,
    /// Seconds of the whole run.
    pub seconds: f64,
}

impl Report {
    /// The documents that failed.
    pub fn failures(&self) -> impl Iterator<Item = &DocReport> {
        self.documents.iter().filter(|d| d.reader == "none")
    }
}

/// The source list beside the file (KD15): what to read again on a
/// refresh, and which section got which room id.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Format of this file.
    pub version: u32,
    /// The program that wrote it.
    pub tool: String,
    /// The sources as given: paths and URLs.
    pub sources: Vec<String>,
    /// The planner settings used.
    pub room_max: usize,
    /// See [`rooms::Settings::sub_max`].
    pub sub_max: usize,
    /// See [`rooms::Settings::min_section`].
    pub min_section: usize,
    /// See [`rooms::Settings::overview`].
    pub overview: bool,
    /// See [`rooms::Settings::dedupe`].
    pub dedupe: bool,
    /// The OCR languages.
    pub ocr_langs: String,
    /// The crawl limit.
    pub max_pages: usize,
    /// The crawl scope, if set.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scope: String,
    /// Section key to room id.
    pub ids: BTreeMap<String, String>,
    /// The rooms this program wrote (the others are kept on a refresh).
    pub rooms: Vec<String>,
}

/// The source list's path: `x.kapu` → `x.sources.json`.
#[must_use]
pub fn manifest_path(file: &Path) -> PathBuf {
    file.with_extension("sources.json")
}

impl Manifest {
    /// Reads the source list of `file`.
    ///
    /// # Errors
    /// When it is missing or not valid.
    pub fn read(file: &Path) -> Result<Self> {
        let path = manifest_path(file);
        let bytes = std::fs::read(&path).map_err(|e| Error::new(format!("{}: {e}", path.display())))?;
        serde_json::from_slice(&bytes).map_err(|e| Error::new(format!("{}: {e}", path.display())))
    }

    /// The settings it was made with, on top of `base`.
    #[must_use]
    pub fn settings(&self, base: &Settings) -> Settings {
        let mut s = base.clone();
        s.rooms = rooms::Settings {
            room_max: self.room_max,
            sub_max: self.sub_max,
            min_section: self.min_section,
            overview: self.overview,
            dedupe: self.dedupe,
        };
        s.read.ocr.langs.clone_from(&self.ocr_langs);
        s.web.max_pages = self.max_pages;
        s.web.scope.clone_from(&self.scope);
        s
    }
}

/// Whether `source` is a web address.
#[must_use]
pub fn is_url(source: &str) -> bool {
    let lower = source.to_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Makes `out` from `sources`. `model` computes the vectors (the packaged
/// model by default); without one the file has keyword search only.
///
/// # Errors
/// When no document could be read, or the file cannot be built or written.
pub fn make(
    sources: &[String],
    out: &Path,
    settings: &Settings,
    model: Option<&Model>,
    progress: OnProgress<'_>,
) -> Result<Report> {
    run(sources, out, settings, model, progress, &Manifest::default(), &[])
}

/// Reads the sources of `file` again and rebuilds it: unchanged documents
/// come from the cache, sections keep their ids, hand-written rooms stay.
///
/// # Errors
/// When the source list is missing, or as [`make`].
pub fn refresh(file: &Path, base: &Settings, model: Option<&Model>, progress: OnProgress<'_>) -> Result<Report> {
    let manifest = Manifest::read(file)?;
    let settings = manifest.settings(base);
    let made: BTreeSet<&String> = manifest.rooms.iter().collect();
    let kept: Vec<RoomSource> = std::fs::read(file)
        .ok()
        .and_then(|b| kapu_core::edit::rooms_of(&b).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|r| !made.contains(&r.id))
        .collect();
    run(&manifest.sources, file, &settings, model, progress, &manifest, &kept)
}

fn run(
    sources: &[String],
    out: &Path,
    settings: &Settings,
    model: Option<&Model>,
    progress: OnProgress<'_>,
    previous: &Manifest,
    kept: &[RoomSource],
) -> Result<Report> {
    let start = Instant::now();
    let mut report = Report { file: out.display().to_string(), ..Report::default() };
    let docs = collect(sources, settings, progress, &mut report);
    if docs.iter().all(|d| d.blocks.is_empty()) && kept.is_empty() {
        let why: Vec<String> = report.failures().take(5).map(|d| format!("{}: {}", d.name, d.warnings.join("; "))).collect();
        return Err(Error::new(format!("no text was read from the sources. {}", why.join(" | "))));
    }
    let reserved: BTreeSet<String> = kept.iter().map(|r| r.id.clone()).collect();
    let plan = rooms::plan(&docs, &settings.rooms, &previous.ids, &reserved);
    progress(Progress::Planned { rooms: plan.rooms.len() });
    let mut all = plan.rooms.clone();
    all.extend(kept.iter().cloned());
    report.kept = kept.len();
    report.rooms = all.len();
    report.plan = plan.docs;
    report.stats = plan.stats;
    report.model = model.map_or_else(|| kapu_core::build::NO_MODEL.to_owned(), |m| m.name().to_owned());
    write(out, &all, model, progress)?;
    let manifest = Manifest {
        version: 1,
        tool: format!("kapu-create {}", env!("CARGO_PKG_VERSION")),
        sources: sources.to_vec(),
        room_max: settings.rooms.room_max,
        sub_max: settings.rooms.sub_max,
        min_section: settings.rooms.min_section,
        overview: settings.rooms.overview,
        dedupe: settings.rooms.dedupe,
        ocr_langs: settings.read.ocr.langs.clone(),
        max_pages: settings.web.max_pages,
        scope: settings.web.scope.clone(),
        ids: plan.ids,
        rooms: plan.rooms.iter().map(|r| r.id.clone()).collect(),
    };
    let json = serde_json::to_vec_pretty(&manifest).map_err(|e| Error::new(e.to_string()))?;
    write_atomic(&manifest_path(out), &json)?;
    report.seconds = start.elapsed().as_secs_f64();
    Ok(report)
}

/// Builds and writes the file; a model that has many windows to compute
/// first writes the file without vectors (D173), then the whole one.
fn write(out: &Path, rooms: &[RoomSource], model: Option<&Model>, progress: OnProgress<'_>) -> Result<()> {
    let base = kapu_core::build::build(rooms)?;
    let Some(model) = model else {
        write_atomic(out, &base.bytes)?;
        progress(Progress::Written { path: out, pending: false });
        return Ok(());
    };
    if model.is_contextual() {
        let (missing, _) = missing_windows(&base.bytes, model)?;
        if missing > PENDING_WINDOWS {
            let first = rebuild_pending(&base.bytes, model)?;
            write_atomic(out, &first.bytes)?;
            progress(Progress::Written { path: out, pending: true });
        }
    }
    let report = |done: usize, total: usize| progress(Progress::Vectors { done, total });
    let built = rebuild_with_progress(&base.bytes, Some(model), Some(&report))?;
    write_atomic(out, &built.bytes)?;
    progress(Progress::Written { path: out, pending: false });
    Ok(())
}

/// A file to read: its path and the key and name it gets.
struct Job {
    path: PathBuf,
    key: String,
    name: String,
}

/// Reads every source into documents; failures go into the report as
/// documents without blocks.
fn collect(sources: &[String], settings: &Settings, progress: OnProgress<'_>, report: &mut Report) -> Vec<Document> {
    let mut docs: Vec<Document> = Vec::new();
    let mut jobs: Vec<Job> = Vec::new();
    for source in sources {
        if is_url(source) {
            let on_page = |done: usize, total: usize, url: &str| progress(Progress::Fetched { done, total, url });
            match web::crawl(source, &settings.web, &on_page) {
                Ok(crawl) => {
                    report.web.push(format!("{source}: {} pages via {}", crawl.pages.len(), crawl.method));
                    for w in &crawl.warnings {
                        docs.push(readers::archive::failed(source, source, w));
                    }
                    docs.extend(crawl.pages.iter().map(page_document));
                }
                Err(e) => docs.push(readers::archive::failed(source, source, &e.0)),
            }
            continue;
        }
        let path = PathBuf::from(source);
        if path.is_dir() {
            let mut files = Vec::new();
            walk(&path, &mut files);
            for file in files {
                let rel = file.strip_prefix(&path).map_or_else(|_| file.clone(), Path::to_path_buf);
                let name = format!(
                    "{}/{}",
                    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                    rel.to_string_lossy().replace('\\', "/")
                );
                jobs.push(Job { key: key_of(&file), name, path: file });
            }
        } else if path.is_file() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| source.clone());
            jobs.push(Job { key: key_of(&path), name, path });
        } else {
            docs.push(readers::archive::failed(source, source, "no such file or folder"));
        }
    }
    let done = AtomicUsize::new(0);
    let total = jobs.len();
    let cached = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
    let results = readers::ocr::parallel(&jobs, workers, |job| {
        let r = read_job(job, settings, &cached);
        let n = done.fetch_add(1, Ordering::Relaxed) + 1;
        progress(Progress::Read { done: n, total, name: &job.name });
        r
    });
    report.cached = cached.into_inner();
    let mut seen_hashes: BTreeMap<String, String> = BTreeMap::new();
    for (job, result) in jobs.iter().zip(results) {
        match result {
            Ok((hash, read)) => {
                if let Some(first) = seen_hashes.get(&hash) {
                    let mut doc = readers::archive::failed(&job.key, &job.name, &format!("the same file as {first}, read once"));
                    doc.reader = "same".into();
                    docs.push(doc);
                    continue;
                }
                seen_hashes.insert(hash, job.name.clone());
                docs.extend(read);
            }
            Err(e) => docs.push(readers::archive::failed(&job.key, &job.name, &e.0)),
        }
    }
    for doc in &docs {
        report.documents.push(DocReport {
            key: doc.key.clone(),
            name: doc.name.clone(),
            reader: doc.reader.clone(),
            pages: doc.pages,
            chars: doc.text_len(),
            removed: doc.removed,
            warnings: doc.warnings.clone(),
        });
    }
    docs.retain(|d| !d.blocks.is_empty());
    docs
}

/// A fetched page as a document; its site is the host (the boilerplate
/// family).
fn page_document(page: &web::Page) -> Document {
    let name = page.url.clone();
    let mut doc = match page.kind {
        web::PageKind::Markdown => readers::text::markdown(&page.bytes, &page.url, &name),
        web::PageKind::Html => readers::html::read(&page.bytes, &page.url, &name),
    };
    if !page.title.is_empty() {
        doc.title.clone_from(&page.title);
    }
    if !page.description.is_empty() {
        doc.description.clone_from(&page.description);
    }
    doc.site = url::Url::parse(&page.url).ok().and_then(|u| u.host_str().map(str::to_owned)).unwrap_or_default();
    doc
}

/// The key of a file: its absolute path with `/`.
fn key_of(path: &Path) -> String {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    abs.to_string_lossy().replace('\\', "/")
}

/// The files under `dir`, sorted, without hidden folders and build output.
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        if name.starts_with('.') || name.starts_with("~$") {
            continue;
        }
        if path.is_dir() {
            if matches!(name.as_str(), "node_modules" | "target" | "__pycache__" | "venv" | ".venv") {
                continue;
            }
            walk(&path, out);
        } else if !matches!(name.as_str(), "thumbs.db" | "desktop.ini")
            && !name.ends_with(".kapu")
            && !name.ends_with(".sources.json")
        {
            out.push(path);
        }
    }
}

#[derive(Serialize, Deserialize)]
struct CacheEntry {
    key: String,
    name: String,
    docs: Vec<Document>,
}

/// Reads one file, from the cache when its bytes were read before with the
/// same reader version and options. A panic of a parser is caught.
fn read_job(job: &Job, settings: &Settings, cached: &AtomicUsize) -> Result<(String, Vec<Document>)> {
    let bytes = std::fs::read(&job.path).map_err(|e| Error::new(e.to_string()))?;
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let fingerprint = format!(
        "{READER_VERSION}|{}|{}|{}",
        settings.read.ocr.langs, settings.read.ocr.dpi, hash
    );
    let cache = (!settings.no_cache)
        .then(|| kapu_core::embed::cache_root())
        .flatten()
        .map(|root| root.join("create").join(format!("{}.json", blake3::hash(fingerprint.as_bytes()).to_hex())));
    if let Some(entry) = cache.as_ref().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice::<CacheEntry>(&b).ok()) {
        cached.fetch_add(1, Ordering::Relaxed);
        let docs = entry.docs.into_iter().map(|d| rekey(d, &entry.key, &job.key, &entry.name, &job.name)).collect();
        return Ok((hash, docs));
    }
    let ctx = Ctx { options: &settings.read, depth: 0 };
    let read = catch_unwind(AssertUnwindSafe(|| readers::read(&bytes, &job.key, &job.name, &ctx)))
        .map_err(|_| Error::new("the reader stopped on this file (an internal error of its parser)"))??;
    if let Some(path) = cache {
        let entry = CacheEntry { key: job.key.clone(), name: job.name.clone(), docs: read.clone() };
        if let (Some(dir), Ok(json)) = (path.parent(), serde_json::to_vec(&entry)) {
            let _ = std::fs::create_dir_all(dir);
            let _ = write_atomic(&path, &json);
        }
    }
    Ok((hash, read))
}

/// A cached document of a file now at another place.
fn rekey(mut doc: Document, old_key: &str, new_key: &str, old_name: &str, new_name: &str) -> Document {
    if old_key != new_key {
        if let Some(rest) = doc.key.strip_prefix(old_key) {
            doc.key = format!("{new_key}{rest}");
        }
    }
    if old_name != new_name {
        if let Some(rest) = doc.name.strip_prefix(old_name) {
            doc.name = format!("{new_name}{rest}");
        }
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_sits_beside_the_file() {
        assert_eq!(manifest_path(Path::new("d/x.kapu")), PathBuf::from("d/x.sources.json"));
    }

    #[test]
    fn rekey_moves_archive_members_too() {
        let mut d = Document::new("a/p.zip!x.md", "p.zip > x.md", "markdown");
        d = rekey(d, "a/p.zip", "b/p.zip", "p.zip", "q.zip");
        assert_eq!(d.key, "b/p.zip!x.md");
        assert_eq!(d.name, "q.zip > x.md");
    }

    #[test]
    fn a_folder_becomes_a_file_and_refreshes() {
        let dir = std::env::temp_dir().join(format!("kapu-create-test-{}", std::process::id()));
        let src = dir.join("docs");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(std::fs::create_dir_all(src.join("sub")).is_ok());
        assert!(std::fs::write(src.join("a.md"), "# Alpha\n\nSee [beta](sub/b.md).\n").is_ok());
        assert!(std::fs::write(src.join("sub/b.md"), "# Beta\n\nThe beta text.\n").is_ok());
        assert!(std::fs::write(src.join("broken.pdf"), "%PDF-1.4 nothing").is_ok());
        let out = dir.join("docs.kapu");
        let settings = Settings { no_cache: true, ..Settings::default() };
        let sources = vec![src.to_string_lossy().into_owned()];
        let report = make(&sources, &out, &settings, None, &|_| {});
        let report = match report {
            Ok(r) => r,
            Err(e) => panic!("{e}"),
        };
        assert_eq!(report.rooms, 2);
        assert_eq!(report.failures().count(), 1, "{:?}", report.documents);
        let bytes = std::fs::read(&out).unwrap_or_default();
        let rooms = kapu_core::edit::rooms_of(&bytes).unwrap_or_default();
        let alpha = rooms.iter().find(|r| r.id == "alpha").map(|r| r.content.clone()).unwrap_or_default();
        assert!(alpha.contains("[beta](#beta)"), "{alpha}");
        // a hand-written room survives a refresh, and the ids stay
        let mut with_note = rooms.clone();
        with_note.push(RoomSource { id: "my-note".into(), content: "# My note\n\nkept\n".into() });
        let built = kapu_core::build::build(&with_note).map(|b| b.bytes).unwrap_or_default();
        assert!(write_atomic(&out, &built).is_ok());
        assert!(std::fs::write(src.join("c.md"), "# Gamma\n\nNew.\n").is_ok());
        let again = refresh(&out, &settings, None, &|_| {});
        assert!(again.is_ok(), "{:?}", again.err());
        let bytes = std::fs::read(&out).unwrap_or_default();
        let ids: Vec<String> = kapu_core::edit::rooms_of(&bytes).unwrap_or_default().into_iter().map(|r| r.id).collect();
        assert!(ids.contains(&"my-note".to_owned()), "{ids:?}");
        assert!(ids.contains(&"alpha".to_owned()) && ids.contains(&"gamma".to_owned()), "{ids:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
