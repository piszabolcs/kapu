//! `kapu-create`: a KAPU file from web sites, documents and folders
//! (KD1-KD16). Unlike `kapu`, this program fetches web pages when a source
//! is an http(s) address (KD1).
#![forbid(unsafe_code)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Mutex;
use std::time::Instant;

use clap::{Args, Parser, Subcommand};
use kapu_core::embed::{DEFAULT_MODEL, Model, find_default_model_dir};
use kapu_create::create::{self, Progress, Report, Settings};
use kapu_create::rooms;

#[derive(Parser)]
#[command(
    name = "kapu-create",
    version,
    about = "Make a KAPU file from web sites, documents (PDF, Office, mail, archives, images) and folders"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Make FILE from the sources: files, folders and http(s) addresses.
    New {
        /// The KAPU file to write.
        file: PathBuf,
        /// Files, folders, web addresses.
        #[arg(required = true)]
        sources: Vec<String>,
        #[command(flatten)]
        options: Opts,
    },
    /// Read the sources of FILE again (its .sources.json) and rebuild it;
    /// hand-written rooms stay, sections keep their ids.
    Refresh {
        /// The KAPU file made by `kapu-create new`.
        file: PathBuf,
        #[command(flatten)]
        options: Opts,
    },
    /// Show the rooms the sources would give, without writing a file.
    Preview {
        /// Files, folders, web addresses.
        #[arg(required = true)]
        sources: Vec<String>,
        /// Also write every room as `<id>.md` into this folder.
        #[arg(long)]
        rooms_dir: Option<PathBuf>,
        #[command(flatten)]
        options: Opts,
    },
}

// Command-line switches are bools by nature.
#[allow(clippy::struct_excessive_bools)]
#[derive(Args, Clone)]
struct Opts {
    /// Machine-readable JSON output.
    #[arg(long)]
    json: bool,
    /// Vector model folder (default: the packaged model).
    #[arg(long)]
    model: Option<PathBuf>,
    /// Build without a vector model: keyword search only.
    #[arg(long)]
    no_model: bool,
    /// Read every file again, without the document cache.
    #[arg(long)]
    no_cache: bool,
    /// The `PDFium` library or its folder (default: next to the program).
    #[arg(long)]
    pdfium: Option<PathBuf>,
    /// The tesseract program (default: next to the program, then PATH).
    #[arg(long)]
    tesseract: Option<PathBuf>,
    /// OCR languages, `+`-joined.
    #[arg(long, default_value = "eng+deu+hun")]
    ocr_lang: String,
    /// The most tokens of a room.
    #[arg(long, default_value_t = 4000)]
    room_max: usize,
    /// The target tokens of a `##` section.
    #[arg(long, default_value_t = 1500)]
    sub_max: usize,
    /// Sections under this many tokens are packed together.
    #[arg(long, default_value_t = 200)]
    min_section: usize,
    /// No overview room per document.
    #[arg(long)]
    no_overview: bool,
    /// Keep sections copied word for word in every document.
    #[arg(long)]
    no_dedupe: bool,
    /// The most web pages fetched per address.
    #[arg(long, default_value_t = 5000)]
    max_pages: usize,
    /// Web pages must start with this URL (default: the host and the first
    /// path segment of the address).
    #[arg(long)]
    scope: Option<String>,
}

impl Opts {
    fn settings(&self) -> Settings {
        let mut s = Settings {
            no_cache: self.no_cache,
            ..Settings::default()
        };
        s.read.pdfium.clone_from(&self.pdfium);
        s.read.ocr.tesseract.clone_from(&self.tesseract);
        s.read.ocr.langs.clone_from(&self.ocr_lang);
        s.rooms = rooms::Settings {
            room_max: self.room_max.max(300),
            sub_max: self.sub_max.max(100),
            min_section: self.min_section,
            overview: !self.no_overview,
            dedupe: !self.no_dedupe,
        };
        s.web.max_pages = self.max_pages.max(1);
        s.web.scope = self.scope.clone().unwrap_or_default();
        s
    }

    /// The model for writing: `--no-model`, `--model`, or the packaged one.
    fn model(&self, notes: &mut Vec<String>) -> Result<Option<Model>, String> {
        if self.no_model {
            return Ok(None);
        }
        if let Some(dir) = &self.model {
            return Model::load(dir).map(Some).map_err(|e| e.to_string());
        }
        if let Some(dir) = find_default_model_dir() {
            return Model::load(&dir).map(Some).map_err(|e| e.to_string());
        }
        notes.push(format!(
            "vector model `{DEFAULT_MODEL}` not found; keyword search only (emb=none)"
        ));
        Ok(None)
    }
}

/// Progress on stderr, at most every two seconds per kind.
struct Reporter {
    last: Mutex<(Instant, &'static str)>,
    quiet: bool,
}

impl Reporter {
    fn show(&self, p: &Progress<'_>) {
        if self.quiet {
            return;
        }
        let (kind, line, last_one) = match p {
            Progress::Fetched { done, total, url } => {
                let of = if *total > 0 {
                    format!("/{total}")
                } else {
                    String::new()
                };
                (
                    "fetch",
                    format!("fetched {done}{of}: {url}"),
                    *total > 0 && done == total,
                )
            }
            Progress::Read { done, total, name } => (
                "read",
                format!("read {done}/{total}: {name}"),
                done == total,
            ),
            Progress::Planned { rooms } => ("plan", format!("{rooms} rooms planned"), true),
            Progress::Vectors { done, total } => {
                ("vec", format!("vectors {done}/{total}"), done == total)
            }
            Progress::Written { path, pending } => {
                let more = if *pending {
                    ", keyword search ready; computing the vectors"
                } else {
                    ""
                };
                ("write", format!("written {}{more}", path.display()), true)
            }
        };
        let Ok(mut last) = self.last.lock() else {
            return;
        };
        let now = Instant::now();
        if last_one || last.1 != kind || now.duration_since(last.0).as_secs() >= 2 {
            *last = (now, kind);
            eprintln!("kapu-create: {line}");
        }
    }
}

fn summary(report: &Report, notes: &[String]) -> String {
    let mut out = String::new();
    let read = report
        .documents
        .iter()
        .filter(|d| d.reader != "none" && d.reader != "same")
        .count();
    let failed = report.failures().count();
    let _ = write!(
        out,
        "{}: {} rooms from {read} documents",
        if report.file.is_empty() {
            "preview"
        } else {
            &report.file
        },
        report.rooms
    );
    if failed > 0 {
        let _ = write!(out, " ({failed} not read)");
    }
    let _ = writeln!(out, ", model {}, {:.0} s", report.model, report.seconds);
    if report.kept > 0 || report.cached > 0 {
        let _ = writeln!(
            out,
            "  kept hand-written rooms: {}, documents from the cache: {}",
            report.kept, report.cached
        );
    }
    for w in &report.web {
        let _ = writeln!(out, "  web: {w}");
    }
    let removed: usize = report.documents.iter().map(|d| d.removed).sum();
    let _ = writeln!(
        out,
        "  dropped lines (running heads, contents, index): {removed}; web boilerplate blocks: {}; copied sections kept once: {}; links to rooms: {}",
        report.stats.boilerplate, report.stats.duplicates, report.stats.links
    );
    for d in &report.plan {
        let tokens: usize = d.rooms.iter().map(|r| r.tokens).sum();
        let _ = writeln!(
            out,
            "  {} [{:?}]: {} rooms, {tokens} tokens",
            d.title,
            d.tier,
            d.rooms.len()
        );
    }
    let problems: Vec<String> = report
        .documents
        .iter()
        .filter(|d| !d.warnings.is_empty())
        .flat_map(|d| d.warnings.iter().map(move |w| format!("{}: {w}", d.name)))
        .collect();
    if !problems.is_empty() {
        let _ = writeln!(out, "  notes:");
        for p in problems.iter().take(50) {
            let _ = writeln!(out, "    {p}");
        }
        if problems.len() > 50 {
            let _ = writeln!(
                out,
                "    ... and {} more (--json lists all)",
                problems.len() - 50
            );
        }
    }
    for n in notes {
        let _ = writeln!(out, "  note: {n}");
    }
    out
}

fn json(report: &Report, notes: &[String]) -> String {
    let mut v = serde_json::to_value(report).unwrap_or_default();
    if let Some(obj) = v.as_object_mut() {
        obj.insert("notes".into(), serde_json::json!(notes));
    }
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

fn write_rooms(dir: &Path, rooms: &[kapu_core::build::RoomSource]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for room in rooms {
        let path = dir.join(format!("{}.md", room.id));
        std::fs::write(&path, &room.content).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

fn run(cli: &Cli) -> Result<String, String> {
    let (options, json_out) = match &cli.command {
        Command::New { options, .. }
        | Command::Refresh { options, .. }
        | Command::Preview { options, .. } => (options, options.json),
    };
    let reporter = Reporter {
        last: Mutex::new((Instant::now(), "")),
        quiet: false,
    };
    let progress = |p: Progress<'_>| reporter.show(&p);
    let settings = options.settings();
    let mut notes = Vec::new();
    let report = match &cli.command {
        Command::New { file, sources, .. } => {
            let model = options.model(&mut notes)?;
            create::make(sources, file, &settings, model.as_ref(), &progress).map_err(|e| e.0)?
        }
        Command::Refresh { file, .. } => {
            let model = options.model(&mut notes)?;
            create::refresh(file, &settings, model.as_ref(), &progress).map_err(|e| e.0)?
        }
        Command::Preview {
            sources, rooms_dir, ..
        } => {
            let (report, rooms) =
                create::preview(sources, &settings, &progress).map_err(|e| e.0)?;
            if let Some(dir) = rooms_dir {
                write_rooms(dir, &rooms)?;
                notes.push(format!(
                    "{} rooms written to {}",
                    rooms.len(),
                    dir.display()
                ));
            }
            report
        }
    };
    Ok(if json_out {
        json(&report, &notes)
    } else {
        summary(&report, &notes)
    })
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("kapu-create: {e}");
            ExitCode::FAILURE
        }
    }
}
