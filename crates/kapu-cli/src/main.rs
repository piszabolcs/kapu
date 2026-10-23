//! `kapu`: command-line tool for KAPU files (spec chapter 8, M6).
//!
//! Every command prints plain text, or JSON with `--json`. On failure the exit
//! code is 1 and the output carries the FR-20 code (`E001`..`E011`) or the
//! warning code that made the command fail (`W001`).
#![forbid(unsafe_code)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use clap::builder::styling::{Effects, Style};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use kapu_core::about;
use kapu_core::build::{Built, Warning, rebuild_with, write_atomic};
use kapu_core::embed::{DEFAULT_MODEL, Model, VectorCache, find_default_model_dir, find_model_dir};
use kapu_core::search::{SearchOptions, VectorStatus};
use kapu_core::{Error, ErrorCode, Level};
use serde_json::{Value, json};

#[derive(Parser)]
#[command(
    name = "kapu",
    version,
    about = "KAPU: navigable single-file knowledge container"
)]
struct Cli {
    /// Machine-readable JSON output.
    #[arg(long, global = true)]
    json: bool,
    /// Vector model directory (default: the packaged model, D29).
    #[arg(long, global = true)]
    model: Option<PathBuf>,
    /// Build without a vector model (emb=none).
    #[arg(long, global = true)]
    no_model: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a file with one empty room (FR-05 template).
    New { file: PathBuf },
    /// Build a file from a folder of Markdown files, one room per `.md`.
    Import { dir: PathBuf, file: PathBuf },
    /// Write every room as `<id>.md` into a folder.
    Export { file: PathBuf, dir: PathBuf },
    /// Add a room from a Markdown file, or replace room `--id`.
    Add {
        file: PathBuf,
        markdown: PathBuf,
        #[arg(long)]
        id: Option<String>,
    },
    /// Remove a room and its sub-doors (D45).
    Rm { file: PathBuf, id: String },
    /// Rebuild every derived section from the body (FR-18).
    Build { file: PathBuf },
    /// Validate the header and derived sections; `--full` also the body.
    Validate {
        file: PathBuf,
        #[arg(long)]
        full: bool,
    },
    /// Show the map: groups, or the door lines of one group.
    Map {
        file: PathBuf,
        #[arg(long)]
        group: Option<String>,
    },
    /// Hybrid search (BM25 + vectors, RRF).
    Search {
        file: PathBuf,
        query: String,
        #[arg(long, default_value_t = 10)]
        k: usize,
        #[arg(long)]
        max_tokens: Option<u64>,
    },
    /// Long description and all tags of doors.
    Describe {
        file: PathBuf,
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Open doors: framed content and graph neighbours.
    Open {
        file: PathBuf,
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Retrieval quality on a question set (recall@5/10, MRR@10).
    Bench { file: PathBuf, questions: PathBuf },
}

/// What a command produced: JSON for `--json`, text otherwise, warnings for
/// both, and an optional warning code that makes the command fail.
struct Output {
    json: Value,
    text: String,
    warnings: Vec<Warning>,
    fail_with: Option<&'static str>,
}

impl Output {
    fn new(json: Value, text: String) -> Self {
        Self {
            json,
            text,
            warnings: Vec::new(),
            fail_with: None,
        }
    }
}

fn io_err(path: &Path, e: &std::io::Error) -> Error {
    Error::new(ErrorCode::E012, format!("{}: {e}", path.display()))
}

fn read(path: &Path) -> Result<Vec<u8>, Error> {
    std::fs::read(path).map_err(|e| io_err(path, &e))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The model for writing: `--no-model`, `--model`, or the packaged default
/// if present (otherwise W005 and no model).
fn writer_model(cli: &Cli, warnings: &mut Vec<Warning>) -> Result<Option<Model>, Error> {
    if cli.no_model {
        return Ok(None);
    }
    if let Some(dir) = &cli.model {
        return Model::load(dir).map(Some);
    }
    if let Some(dir) = find_default_model_dir() {
        return Model::load(&dir).map(Some);
    }
    warnings.push(Warning {
        code: "W005",
        message: format!(
            "vector model `{DEFAULT_MODEL}` not found; built without vectors (emb=none)"
        ),
    });
    Ok(None)
}

/// The model for reading a file: `--model`, or the one named in its header.
fn reader_model(cli: &Cli, file: &[u8]) -> Result<Option<Model>, Error> {
    if cli.no_model {
        return Ok(None);
    }
    if let Some(dir) = &cli.model {
        return Model::load(dir).map(Some);
    }
    let header = kapu_core::Header::parse(file)?;
    if header.emb == kapu_core::build::NO_MODEL {
        return Ok(None);
    }
    find_model_dir(&header.emb)
        .map(|d| Model::load(&d))
        .transpose()
}

fn written(
    path: &Path,
    built: Built,
    mut warnings: Vec<Warning>,
    extra: Value,
) -> Result<Output, Error> {
    write_atomic(path, &built.bytes)?;
    let s = kapu_core::Sections::parse(&built.bytes)?;
    let summary = json!({
        "file": path.display().to_string(),
        "bytes": built.bytes.len(),
        "rooms": s.header.rooms,
        "doors": s.header.doors,
        "groups": s.header.groups,
        "emb": s.header.emb,
    });
    let text = format!(
        "{}: {} rooms, {} doors, {} groups, {} bytes, model {}",
        path.display(),
        s.header.rooms,
        s.header.doors,
        s.header.groups,
        built.bytes.len(),
        s.header.emb
    );
    warnings.extend(built.warnings);
    let mut json = summary;
    if let (Some(obj), Value::Object(more)) = (json.as_object_mut(), extra) {
        obj.extend(more);
    }
    let mut out = Output::new(json, text);
    out.warnings = warnings;
    Ok(out)
}

fn run(cli: &Cli) -> Result<Output, Error> {
    let mut warnings = Vec::new();
    match &cli.command {
        Command::New { file } => {
            if file.exists() {
                return Err(Error::new(
                    ErrorCode::E012,
                    format!("{} already exists", file.display()),
                ));
            }
            let model = writer_model(cli, &mut warnings)?;
            let base = kapu_core::edit::new_file()?;
            let built = rebuild_with(&base.bytes, model.as_ref())?;
            written(file, built, warnings, json!({}))
        }
        Command::Import { dir, file } => {
            let model = writer_model(cli, &mut warnings)?;
            let imported = kapu_core::edit::import_dir(dir)?;
            warnings.extend(imported.warnings);
            let built = rebuild_with(&imported.bytes, model.as_ref())?;
            written(file, built, warnings, json!({}))
        }
        Command::Export { file, dir } => {
            let paths = kapu_core::edit::export_dir(&read(file)?, dir)?;
            let list: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
            let text = format!("{} rooms written to {}", list.len(), dir.display());
            Ok(Output::new(json!({ "written": list }), text))
        }
        Command::Add { file, markdown, id } => {
            let model = writer_model(cli, &mut warnings)?;
            let md = std::fs::read_to_string(markdown).map_err(|e| io_err(markdown, &e))?;
            let (added, room) =
                kapu_core::edit::add_room(&read(file)?, &file_name(markdown), &md, id.as_deref())?;
            warnings.extend(added.warnings);
            let built = rebuild_with(&added.bytes, model.as_ref())?;
            // FR-18/4, FR-19: content scan of the saved room, as warnings
            for f in kapu_core::scan::scan_file(&built.bytes)?
                .into_iter()
                .filter(|f| f.room == room)
            {
                warnings.push(Warning {
                    code: f.rule,
                    message: format!("content scan, room `{}`: {}", f.room, f.detail),
                });
            }
            let mut out = written(file, built, warnings, json!({ "id": room }))?;
            out.text = format!("room `{room}` saved; {}", out.text);
            Ok(out)
        }
        Command::Rm { file, id } => {
            let model = writer_model(cli, &mut warnings)?;
            let removed = kapu_core::edit::remove_room(&read(file)?, id)?;
            let built = rebuild_with(&removed.bytes, model.as_ref())?;
            written(file, built, warnings, json!({ "removed": id }))
        }
        Command::Build { file } => {
            let model = writer_model(cli, &mut warnings)?;
            let built = rebuild_with(&read(file)?, model.as_ref())?;
            written(file, built, warnings, json!({}))
        }
        Command::Validate { file, full } => validate(cli, file, *full),
        Command::Map { file, group } => map(file, group.as_deref()),
        Command::Search {
            file,
            query,
            k,
            max_tokens,
        } => search(cli, file, query, *k, *max_tokens),
        Command::Describe { file, ids } => describe(file, ids),
        Command::Open { file, ids } => open(file, ids),
        Command::Bench { file, questions } => bench(cli, file, questions),
    }
}

fn validate(cli: &Cli, file: &Path, full: bool) -> Result<Output, Error> {
    let data = read(file)?;
    let level = if full { Level::Full } else { Level::Basic };
    let model = if full {
        reader_model(cli, &data)?
    } else {
        None
    };
    let report = kapu_core::validate_with(&data, level, model.as_ref())?;
    let mut out = Output::new(
        json!({ "valid": true, "rooms": report.rooms, "doors": report.doors, "full": report.full, "rebuild_checked": report.rebuild_checked }),
        format!(
            "{}: valid ({} rooms, {} doors{})",
            file.display(),
            report.rooms,
            report.doors,
            if full { ", full" } else { "" }
        ),
    );
    if full && !report.rebuild_checked {
        out.warnings.push(Warning {
            code: "W001",
            message: "model mismatch, rebuild check skipped (D50)".to_owned(),
        });
        out.fail_with = Some("W001");
    }
    Ok(out)
}

fn map(file: &Path, group: Option<&str>) -> Result<Output, Error> {
    let v = kapu_core::view::map_view(&read(file)?, group)?;
    let groups: Vec<Value> = v
        .groups
        .iter()
        .map(|g| json!({ "gid": g.gid, "name": g.name, "rooms": g.rooms, "tokens": g.tokens }))
        .collect();
    let doors: Vec<Value> = v
        .doors
        .iter()
        .map(|d| json!({ "id": d.id, "title": d.title, "tokens": d.tokens, "group": d.gid, "tags": d.tags, "room": d.is_room }))
        .collect();
    let mut text = String::new();
    if group.is_none() {
        text.push_str(kapu_core::view::GUIDE_STD1_EN);
        text.push_str("\n\n");
    }
    text.push_str(&v.raw_lines.join("\n"));
    Ok(Output::new(
        json!({ "guide": group.is_none().then_some(kapu_core::view::GUIDE_STD1_EN), "flat": v.flat, "groups": groups, "doors": doors }),
        text,
    ))
}

fn search(
    cli: &Cli,
    file: &Path,
    query: &str,
    k: usize,
    max_tokens: Option<u64>,
) -> Result<Output, Error> {
    let data = read(file)?;
    let model = reader_model(cli, &data)?;
    let cache = model.as_ref().map(VectorCache::new);
    let r = kapu_core::search::search(
        &data,
        query,
        SearchOptions { k, max_tokens },
        cache.as_ref(),
    )?;
    let vector = match &r.vector {
        VectorStatus::Available => json!("available"),
        VectorStatus::Unavailable(why) => json!({ "unavailable": why }),
    };
    let hits: Vec<Value> = r
        .hits
        .iter()
        .map(|h| json!({ "id": h.id, "title": h.title, "tokens": h.tokens, "score": h.score, "source": h.source.as_str() }))
        .collect();
    let mut text: String = r
        .hits
        .iter()
        .map(|h| {
            format!(
                "{} | {} | {}t | {:.4} | {}",
                h.id,
                h.title,
                h.tokens,
                h.score,
                h.source.as_str()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    if let VectorStatus::Unavailable(why) = &r.vector {
        let _ = write!(text, "\nvector: unavailable ({why})");
    }
    Ok(Output::new(json!({ "vector": vector, "hits": hits }), text))
}

fn describe(file: &Path, ids: &[String]) -> Result<Output, Error> {
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let list = kapu_core::describe_bytes(&read(file)?, &refs)?;
    let mut json_items = Vec::new();
    let mut text = Vec::new();
    for (id, item) in ids.iter().zip(list) {
        match item {
            Ok(d) => {
                text.push(format!(
                    "{} | {}\n{}\ntags: {}",
                    d.id,
                    d.title,
                    d.description,
                    d.tags.join(", ")
                ));
                json_items.push(json!({ "id": d.id, "title": d.title, "description": d.description, "tags": d.tags }));
            }
            Err(e) => {
                text.push(format!("{id}: {e}"));
                json_items.push(
                    json!({ "id": id, "error": { "code": e.code.as_str(), "detail": e.detail } }),
                );
            }
        }
    }
    Ok(Output::new(
        json!({ "doors": json_items }),
        text.join("\n\n"),
    ))
}

fn open(file: &Path, ids: &[String]) -> Result<Output, Error> {
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let opened = kapu_core::open_path(file, &refs)?;
    let name = file_name(file);
    let mut json_items = Vec::new();
    let mut text = Vec::new();
    for (id, item) in ids.iter().zip(&opened.doors) {
        match item {
            Ok(d) => {
                let neighbours: Vec<Value> = d
                    .neighbours
                    .iter()
                    .map(|n| json!({ "id": n.id, "title": n.title, "tokens": n.tokens, "type": n.kind }))
                    .collect();
                let mut t = kapu_core::frame(&name, d);
                if !d.neighbours.is_empty() {
                    t.push_str("neighbours (data, not a suggestion):\n");
                    for n in &d.neighbours {
                        let _ =
                            writeln!(t, "  {} | {} | {}t | {}", n.id, n.title, n.tokens, n.kind);
                    }
                }
                text.push(t);
                json_items.push(json!({ "id": d.id, "title": d.title, "tokens": d.tokens, "frame": kapu_core::frame(&name, d), "neighbours": neighbours }));
            }
            Err(e) => {
                text.push(format!("{id}: {e}\n"));
                json_items.push(
                    json!({ "id": id, "error": { "code": e.code.as_str(), "detail": e.detail } }),
                );
            }
        }
    }
    let mut text = text.concat();
    if opened.index_stale {
        text.push_str("index: stale\n");
    }
    Ok(Output::new(
        json!({ "index": if opened.index_stale { "stale" } else { "ok" }, "doors": json_items }),
        text,
    ))
}

fn scores_json(s: &kapu_core::evaluate::Scores) -> Value {
    json!({
        "recall@5": s.recall5, "recall@10": s.recall10, "mrr@10": s.mrr10,
        "lenient_recall@5": s.lenient_recall5, "lenient_recall@10": s.lenient_recall10, "lenient_mrr@10": s.lenient_mrr10,
        "room_recall@5": s.room_recall5, "room_recall@10": s.room_recall10, "room_mrr@10": s.room_mrr10,
    })
}

fn bench(cli: &Cli, file: &Path, questions: &Path) -> Result<Output, Error> {
    let data = read(file)?;
    let text = std::fs::read_to_string(questions).map_err(|e| io_err(questions, &e))?;
    let qs = kapu_core::evaluate::parse_questions(&text)?;
    let model = reader_model(cli, &data)?;
    let cache = model.as_ref().map(VectorCache::new);
    let e = kapu_core::evaluate::evaluate(&data, &qs, cache.as_ref())?;
    let row = |name: &str, s: &kapu_core::evaluate::Scores| {
        format!(
            "| {name} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} |",
            s.recall5, s.recall10, s.mrr10, s.lenient_recall5, s.lenient_recall10, s.lenient_mrr10
        )
    };
    let mut lines = vec![
        format!("{} questions", e.questions),
        "| Mode | recall@5 | recall@10 | MRR@10 | lenient recall@5 | lenient recall@10 | lenient MRR@10 |".to_owned(),
        "|---|---:|---:|---:|---:|---:|---:|".to_owned(),
        row("BM25", &e.bm25),
    ];
    match &e.vector {
        Some(v) => lines.push(row("vector", v)),
        None => lines.push("| vector | - | - | - | - | - | - |".to_owned()),
    }
    lines.push(row("hybrid", &e.hybrid));
    Ok(Output::new(
        json!({ "questions": e.questions, "bm25": scores_json(&e.bm25), "vector": e.vector.as_ref().map(scores_json), "hybrid": scores_json(&e.hybrid) }),
        lines.join("\n"),
    ))
}

/// Parses the command line; `--version` prints the app info card and `--help`
/// ends with the author signature.
fn parse_cli() -> Cli {
    let dim = Style::new().effects(Effects::DIMMED);
    let matches = Cli::command()
        .long_version(about::card())
        .after_help(format!("{dim}{}{dim:#}", about::SIGNATURE))
        .get_matches();
    Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit())
}

fn main() -> ExitCode {
    let cli = parse_cli();
    let start = Instant::now();
    let result = run(&cli);
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    match result {
        Ok(out) => {
            if cli.json {
                let mut json = out.json;
                if let Some(obj) = json.as_object_mut() {
                    let warnings: Vec<Value> = out
                        .warnings
                        .iter()
                        .map(|w| json!({ "code": w.code, "message": w.message }))
                        .collect();
                    obj.insert("warnings".to_owned(), Value::Array(warnings));
                    obj.insert("timing_ms".to_owned(), json!(elapsed));
                    if let Some(code) = out.fail_with {
                        obj.insert("error".to_owned(), json!({ "code": code }));
                    }
                }
                println!("{json}");
            } else {
                if !out.text.is_empty() {
                    println!("{}", out.text.trim_end_matches('\n'));
                }
                for w in &out.warnings {
                    eprintln!("{}: {}", w.code, w.message);
                }
            }
            if out.fail_with.is_some() {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => {
            if cli.json {
                println!(
                    "{}",
                    json!({ "error": { "code": e.code.as_str(), "detail": e.detail }, "timing_ms": elapsed })
                );
            } else {
                eprintln!("{e}");
            }
            ExitCode::FAILURE
        }
    }
}
