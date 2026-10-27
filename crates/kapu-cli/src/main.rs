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
    about = "KAPU: a personal knowledge base in one text file, for AI agents"
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
    /// Measurement only (plan v2 M0): weight of the title and tag words in
    /// the index instead of 3; the file does not validate.
    #[arg(long, global = true, hide = true)]
    title_weight: Option<u64>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a file with one empty room (FR-05 template; `--kind project`
    /// gives the project card).
    New {
        file: PathBuf,
        /// skill, project, pattern or note.
        #[arg(long)]
        kind: Option<String>,
    },
    /// Print the room template of a kind (`--kind project`: the project card).
    Template {
        #[arg(long)]
        kind: Option<String>,
    },
    /// Build a file from a folder of Markdown files, one room per `.md`;
    /// `--skills`: a folder of agent skills (SKILL.md, scripts, references).
    Import {
        dir: PathBuf,
        file: PathBuf,
        #[arg(long)]
        skills: bool,
    },
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
    /// Show the map: groups, the door lines of one group, or the rooms of a kind.
    Map {
        file: PathBuf,
        #[arg(long)]
        group: Option<String>,
        #[arg(long)]
        kind: Option<String>,
    },
    /// Hybrid search (BM25 + vectors, RRF).
    Search {
        file: PathBuf,
        query: String,
        #[arg(long, default_value_t = 10)]
        k: usize,
        /// Leave out doors priced above this many tokens.
        #[arg(long, alias = "max-tokens")]
        max_price: Option<u64>,
        /// Only doors of rooms of this kind.
        #[arg(long)]
        kind: Option<String>,
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
    Bench {
        file: PathBuf,
        questions: PathBuf,
        /// Ranking switches: a JSON object, or a list of them run one after
        /// the other (the ablation matrix).
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write every question's rankings, per configuration, as JSON lines.
        #[arg(long)]
        rankings: Option<PathBuf>,
    },
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
    model: Option<&Model>,
    mut warnings: Vec<Warning>,
    extra: Value,
) -> Result<Output, Error> {
    write_atomic(path, &built.bytes)?;
    let s = kapu_core::Sections::parse(&built.bytes)?;
    // D141: the search vectors are computed now, not at the first search
    if let Some(model) = model {
        kapu_core::search::warm_in(&s, &VectorCache::new(model))?;
    }
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
        Command::New { file, kind } => {
            if file.exists() {
                return Err(Error::new(
                    ErrorCode::E012,
                    format!("{} already exists", file.display()),
                ));
            }
            let model = writer_model(cli, &mut warnings)?;
            let base = kapu_core::edit::new_file_of(kind.as_deref())?;
            let built = rebuild_with(&base.bytes, model.as_ref())?;
            written(file, built, model.as_ref(), warnings, json!({}))
        }
        Command::Template { kind } => {
            let text = kapu_core::build::template_for(kind.as_deref());
            Ok(Output::new(json!({ "template": text }), text))
        }
        Command::Import { dir, file, skills } => {
            let model = writer_model(cli, &mut warnings)?;
            let imported = if *skills {
                kapu_core::skills::import_skills(dir)?
            } else {
                kapu_core::edit::import_dir(dir)?
            };
            warnings.extend(imported.warnings);
            let built = rebuild_with(&imported.bytes, model.as_ref())?;
            written(file, built, model.as_ref(), warnings, json!({}))
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
            let mut out = written(file, built, model.as_ref(), warnings, json!({ "id": room }))?;
            out.text = format!("room `{room}` saved; {}", out.text);
            Ok(out)
        }
        Command::Rm { file, id } => {
            let model = writer_model(cli, &mut warnings)?;
            let removed = kapu_core::edit::remove_room(&read(file)?, id)?;
            let built = rebuild_with(&removed.bytes, model.as_ref())?;
            written(
                file,
                built,
                model.as_ref(),
                warnings,
                json!({ "removed": id }),
            )
        }
        Command::Build { file } => {
            let model = writer_model(cli, &mut warnings)?;
            let built = rebuild_with(&read(file)?, model.as_ref())?;
            written(file, built, model.as_ref(), warnings, json!({}))
        }
        Command::Validate { file, full } => validate(cli, file, *full),
        Command::Map { file, group, kind } => map(file, group.as_deref(), kind.as_deref()),
        Command::Search {
            file,
            query,
            k,
            max_price,
            kind,
        } => {
            let options = SearchOptions {
                k: *k,
                max_price: *max_price,
                kind: kind.clone(),
                ..SearchOptions::default()
            };
            search(cli, file, query, &options)
        }
        Command::Describe { file, ids } => describe(file, ids),
        Command::Open { file, ids } => open(file, ids),
        Command::Bench {
            file,
            questions,
            config,
            rankings,
        } => bench_any(cli, file, questions, config.as_deref(), rankings.as_deref()),
    }
}

/// `kapu bench`: the matrix with `--config`, otherwise the plain table.
fn bench_any(
    cli: &Cli,
    file: &Path,
    questions: &Path,
    config: Option<&Path>,
    rankings: Option<&Path>,
) -> Result<Output, Error> {
    match config {
        Some(config) => bench_matrix(cli, file, questions, config, rankings),
        None => bench(cli, file, questions),
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
    out.warnings.extend(report.warnings.iter().cloned());
    if full && !report.rebuild_checked {
        out.warnings.push(Warning {
            code: "W001",
            message: "model mismatch, rebuild check skipped (D50)".to_owned(),
        });
        out.fail_with = Some("W001");
    }
    Ok(out)
}

fn map(file: &Path, group: Option<&str>, kind: Option<&str>) -> Result<Output, Error> {
    let data = read(file)?;
    if let Some(kind) = kind {
        // D144: the rooms of one kind, across groups
        let s = kapu_core::Sections::parse_with(
            &data,
            &kapu_core::Limits::default(),
            kapu_core::BodyLength::Tolerate,
        )?;
        // D154: the catalog, one line per room with its description
        let catalog = kapu_core::view::kind_catalog_in(&s, kind)?;
        let text = if catalog.is_empty() {
            format!("no room of kind `{kind}`")
        } else {
            catalog
                .iter()
                .map(kapu_core::view::KindEntry::line)
                .collect::<Vec<_>>()
                .join("\n")
        };
        let rooms: Vec<Value> = catalog
            .iter()
            .map(|e| json!({ "id": e.id, "tokens": e.tokens, "description": e.description, "folded": e.folded }))
            .collect();
        return Ok(Output::new(json!({ "kind": kind, "rooms": rooms }), text));
    }
    let v = kapu_core::view::map_view(&data, group)?;
    let groups: Vec<Value> = v
        .groups
        .iter()
        .map(|g| json!({ "gid": g.gid, "name": g.name, "rooms": g.rooms, "tokens": g.tokens }))
        .collect();
    let doors: Vec<Value> = v
        .doors
        .iter()
        .map(|d| json!({ "id": d.id, "title": d.title, "tokens": d.tokens, "group": d.gid, "tags": d.tags, "room": d.is_room, "kind": d.kind }))
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

fn search(cli: &Cli, file: &Path, query: &str, options: &SearchOptions) -> Result<Output, Error> {
    let data = read(file)?;
    let model = reader_model(cli, &data)?;
    let cache = model.as_ref().map(VectorCache::new);
    let r = kapu_core::search::search(&data, query, options, cache.as_ref())?;
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

/// The configurations of `kapu bench --config`: one object or a list, each
/// with an optional `name` (default `c<N>`).
fn bench_configs(path: &Path) -> Result<Vec<(String, kapu_core::search::RankOptions)>, Error> {
    let text = std::fs::read_to_string(path).map_err(|e| io_err(path, &e))?;
    let value: Value = serde_json::from_str(&text).map_err(|e| {
        Error::new(
            ErrorCode::E012,
            format!("{}: not JSON: {e}", path.display()),
        )
    })?;
    let list = match value {
        Value::Array(list) => list,
        one => vec![one],
    };
    list.iter()
        .enumerate()
        .map(|(i, v)| {
            let name = v
                .get("name")
                .and_then(Value::as_str)
                .map_or_else(|| format!("c{i}"), str::to_owned);
            Ok((name, kapu_core::search::RankOptions::from_json(v)?))
        })
        .collect()
}

/// Median and 95th percentile of `ms`.
fn percentiles(ms: &[f64]) -> (f64, f64) {
    let mut sorted = ms.to_vec();
    sorted.sort_by(f64::total_cmp);
    let at = |q: f64| {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let i = ((sorted.len().saturating_sub(1)) as f64 * q).round() as usize;
        sorted.get(i).copied().unwrap_or(0.0)
    };
    (at(0.5), at(0.95))
}

/// `kapu bench --config` (plan v2 M0): every configuration on the same
/// questions, the scores and search times of each, and with `--rankings`
/// every question's rankings.
fn bench_matrix(
    cli: &Cli,
    file: &Path,
    questions: &Path,
    config: &Path,
    rankings: Option<&Path>,
) -> Result<Output, Error> {
    use std::io::Write as _;
    let configs = bench_configs(config)?;
    let data = read(file)?;
    let text = std::fs::read_to_string(questions).map_err(|e| io_err(questions, &e))?;
    let qs = kapu_core::evaluate::parse_questions(&text)?;
    let model = reader_model(cli, &data)?;
    let cache = model.as_ref().map(VectorCache::new);
    let s = kapu_core::Sections::parse_with(
        &data,
        &kapu_core::Limits::default(),
        kapu_core::BodyLength::Tolerate,
    )?;
    let mut sink = match rankings {
        Some(path) => Some(std::io::BufWriter::new(
            std::fs::File::create(path).map_err(|e| io_err(path, &e))?,
        )),
        None => None,
    };
    let mut lines = vec![
        format!("{} questions, {} configurations", qs.len(), configs.len()),
        "| Configuration | BM25 room r@10 | vector room r@10 | hybrid room r@10 | hybrid room MRR@10 | hybrid r@10 | ms p50 |".to_owned(),
        "|---|---:|---:|---:|---:|---:|---:|".to_owned(),
    ];
    let mut items = Vec::new();
    for (name, rank) in &configs {
        let (e, runs) = kapu_core::evaluate::evaluate_with(&s, &qs, rank, cache.as_ref())?;
        let ms: Vec<f64> = runs.iter().map(|r| r.ms).collect();
        let (p50, p95) = percentiles(&ms);
        if let Some(out) = sink.as_mut() {
            for (i, run) in runs.iter().enumerate() {
                let line = json!({
                    "config": name, "q": i, "expected": run.expected, "ms": run.ms,
                    "bm25": run.rankings.bm25, "vector": run.rankings.vector, "hybrid": run.rankings.hybrid,
                });
                writeln!(out, "{line}").map_err(|e| io_err(rankings.unwrap_or(file), &e))?;
            }
        }
        lines.push(format!(
            "| {name} | {:.3} | {} | {:.3} | {:.3} | {:.3} | {p50:.1} |",
            e.bm25.room_recall10,
            e.vector
                .as_ref()
                .map_or_else(|| "-".to_owned(), |v| format!("{:.3}", v.room_recall10)),
            e.hybrid.room_recall10,
            e.hybrid.room_mrr10,
            e.hybrid.recall10,
        ));
        items.push(json!({
            "name": name, "options": rank.to_json(),
            "bm25": scores_json(&e.bm25), "vector": e.vector.as_ref().map(scores_json), "hybrid": scores_json(&e.hybrid),
            "ms_p50": p50, "ms_p95": p95,
        }));
    }
    if let (Some(out), Some(path)) = (sink.as_mut(), rankings) {
        out.flush().map_err(|e| io_err(path, &e))?;
    }
    let embhash: String = s.header.embhash.0.iter().fold(String::new(), |mut h, b| {
        let _ = write!(h, "{b:02x}");
        h
    });
    Ok(Output::new(
        json!({
            "questions": qs.len(),
            "tool": env!("CARGO_PKG_VERSION"),
            "format": s.header.version,
            "emb": s.header.emb, "embhash": embhash,
            "vector": model.is_some(),
            "configs": items,
        }),
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

/// One line to stdout. A reader that closed the pipe early (`kapu map | head`)
/// is not an error (A40): the caller exits 0 without a panic.
fn emit(text: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut out = std::io::stdout().lock();
    writeln!(out, "{text}")?;
    out.flush()
}

/// Exit code after a failed write: success for a closed pipe, failure otherwise.
fn write_failed(e: &std::io::Error) -> ExitCode {
    if e.kind() == std::io::ErrorKind::BrokenPipe {
        ExitCode::SUCCESS
    } else {
        eprintln!("kapu: cannot write the output: {e}");
        ExitCode::FAILURE
    }
}

fn main() -> ExitCode {
    let cli = parse_cli();
    if let Some(weight) = cli.title_weight {
        kapu_core::build::set_title_weight_for_measurement(weight);
    }
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
                if let Err(e) = emit(&json.to_string()) {
                    return write_failed(&e);
                }
            } else {
                if !out.text.is_empty()
                    && let Err(e) = emit(out.text.trim_end_matches('\n'))
                {
                    return write_failed(&e);
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
                let line = json!({ "error": { "code": e.code.as_str(), "detail": e.detail }, "timing_ms": elapsed });
                if let Err(w) = emit(&line.to_string()) {
                    return write_failed(&w);
                }
            } else {
                eprintln!("{e}");
            }
            ExitCode::FAILURE
        }
    }
}
