//! `kapu-bench`: synthetic corpus generator and measurement helpers (spec M8).
//!
//! `gen` writes a deterministic corpus (M1); `measure` builds generated
//! corpora of several sizes in memory and reports build time, file size and
//! the map/file token ratio (NFR-05, TERV.md M3 first measurement).
#![forbid(unsafe_code)]

mod corpus;

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use clap::{Parser, Subcommand};
use kapu_core::build::{RoomSource, build_with};

#[derive(Parser)]
#[command(
    name = "kapu-bench",
    version,
    about = "KAPU corpus generator and benchmark helpers"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a deterministic synthetic corpus of Markdown rooms into a directory.
    Gen {
        /// Number of rooms to generate.
        #[arg(long, default_value_t = 30)]
        rooms: usize,
        /// Seed of the generator; the same seed always gives the same files.
        #[arg(long, default_value_t = 42)]
        seed: u64,
        /// Output directory (created if missing); one `<room-id>.md` per room.
        #[arg(long)]
        out: PathBuf,
    },
    /// Build generated corpora and print a Markdown table of the measurements.
    Measure {
        /// Corpus sizes, comma separated.
        #[arg(long, value_delimiter = ',', default_value = "30,300,3000")]
        rooms: Vec<usize>,
        /// Seed of the generator.
        #[arg(long, default_value_t = 42)]
        seed: u64,
        /// Builds per size; the median time is reported.
        #[arg(long, default_value_t = 3)]
        runs: usize,
        /// Vector model directory; without it the build runs without a model (D57).
        #[arg(long)]
        model: Option<PathBuf>,
        /// Writes the room graph of every size to `<dir>/graph-<rooms>.tsv`
        /// (`a b weight`, room indices) for the leidenalg comparison (D43).
        #[arg(long)]
        dump_graph: Option<PathBuf>,
    },
    /// Runs the Rust Leiden on room graphs dumped by `measure --dump-graph`
    /// and prints the modularity, to compare with `tools/leiden_check.py` (D43).
    Leiden {
        /// `a b weight` files.
        graphs: Vec<PathBuf>,
    },
    /// Times `search` on generated corpora: BM25 only, then hybrid with a
    /// cold and a warm vector cache (NFR-05, M5).
    Search {
        /// Corpus sizes, comma separated.
        #[arg(long, value_delimiter = ',', default_value = "30,300,3000")]
        rooms: Vec<usize>,
        /// Seed of the generator.
        #[arg(long, default_value_t = 42)]
        seed: u64,
        /// Vector model directory.
        #[arg(long)]
        model: PathBuf,
        /// The query.
        #[arg(long, default_value = "üvegszerű homályos gomb")]
        query: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Gen { rooms, seed, out } => match corpus::generate(rooms, seed, &out) {
            Ok(n) => {
                println!(
                    "kapu-bench {} (KAPU format {}): wrote {n} rooms to {}",
                    env!("CARGO_PKG_VERSION"),
                    kapu_core::FORMAT_VERSION,
                    out.display()
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("kapu-bench gen: {e}");
                ExitCode::FAILURE
            }
        },
        Command::Leiden { graphs } => {
            for g in &graphs {
                match leiden_on(g) {
                    Ok(line) => println!("{line}"),
                    Err(e) => {
                        eprintln!("kapu-bench leiden: {}: {e}", g.display());
                        return ExitCode::FAILURE;
                    }
                }
            }
            ExitCode::SUCCESS
        }
        Command::Search {
            rooms,
            seed,
            model,
            query,
        } => match search_times(&rooms, seed, &model, &query) {
            Ok(table) => {
                print!("{table}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("kapu-bench search: {e}");
                ExitCode::FAILURE
            }
        },
        Command::Measure {
            rooms,
            seed,
            runs,
            model,
            dump_graph,
        } => match measure(
            &rooms,
            seed,
            runs.max(1),
            model.as_deref(),
            dump_graph.as_deref(),
        ) {
            Ok(table) => {
                print!("{table}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("kapu-bench measure: {e}");
                ExitCode::FAILURE
            }
        },
    }
}

/// One row per corpus size: rooms, doors, file size, build time (median),
/// body tokens, map tokens and their ratio.
fn measure(
    sizes: &[usize],
    seed: u64,
    runs: usize,
    model_dir: Option<&std::path::Path>,
    dump: Option<&std::path::Path>,
) -> kapu_core::Result<String> {
    let model = model_dir.map(kapu_core::embed::Model::load).transpose()?;
    let mut graph_table = String::from(
        "
| Szobák | link él | term él | sim él | Csoportok | Legnagyobb csoport | Medián csoport | Modularitás |
|---:|---:|---:|---:|---:|---:|---:|---:|
",
    );
    let mut out = String::from(
        "| Szobák | Ajtók | Fájl (KiB) | Build (ms, medián) | Törzs (token) | Térkép (token) | Térkép/törzs |\n|---:|---:|---:|---:|---:|---:|---:|\n",
    );
    for &n in sizes {
        let rooms: Vec<RoomSource> = corpus::markdown_rooms(n, seed)
            .into_iter()
            .map(|(id, content)| RoomSource { id, content })
            .collect();
        let mut times = Vec::with_capacity(runs);
        let mut bytes = Vec::new();
        for _ in 0..runs {
            let start = Instant::now();
            bytes = build_with(&rooms, model.as_ref())?.bytes;
            times.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(f64::total_cmp);
        let median = times.get(times.len() / 2).copied().unwrap_or(0.0);
        let s = kapu_core::Sections::parse(&bytes)?;
        let body_tokens = kapu_core::terms::tokens(std::str::from_utf8(s.body).unwrap_or(""))?;
        let map_tokens = kapu_core::terms::tokens(s.map)?;
        #[allow(clippy::cast_precision_loss)]
        let ratio = map_tokens as f64 / body_tokens.max(1) as f64;
        #[allow(clippy::cast_precision_loss)]
        let kib = bytes.len() as f64 / 1024.0;
        let _ = writeln!(
            out,
            "| {n} | {} | {kib:.1} | {median:.0} | {body_tokens} | {map_tokens} | {ratio:.3} |",
            s.header.doors
        );
        let _ = writeln!(graph_table, "{}", graph_row(n, &s, dump)?);
    }
    out.push_str(&graph_table);
    Ok(out)
}

/// Edge counts by type, group sizes and the modularity of the grouping, from
/// the built file alone (the room graph as Leiden saw it, D92).
fn graph_row(
    n: usize,
    s: &kapu_core::Sections<'_>,
    dump: Option<&std::path::Path>,
) -> kapu_core::Result<String> {
    use std::collections::BTreeMap;
    let map = kapu_core::catalog::parse_map(s.map)?;
    let edges = kapu_core::catalog::parse_graph(s.graph)?;
    let rooms: Vec<&str> = map
        .doors
        .iter()
        .filter(|d| d.is_room)
        .map(|d| d.id)
        .collect();
    let index: BTreeMap<&str, usize> = rooms.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let mut graph: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for e in &edges {
        *counts.entry(e.kind).or_insert(0) += 1;
        let room = |id: &str| id.split('/').next().and_then(|r| index.get(r)).copied();
        if let (Some(a), Some(b)) = (room(e.from), room(e.to))
            && a != b
        {
            *graph.entry((a.min(b), a.max(b))).or_insert(0.0) += e.weight;
        }
    }
    // group of each room from the R lines
    let mut gid_of: Vec<usize> = vec![0; rooms.len()];
    for line in s.map.lines().filter(|l| l.starts_with("R ")) {
        let mut f = line.split(" | ");
        let id = f.next().and_then(|h| h.get(2..)).unwrap_or("");
        let gid = f
            .nth(2)
            .and_then(|g| g.trim_start_matches('g').parse::<usize>().ok())
            .unwrap_or(0);
        if let Some(&i) = index.get(id)
            && let Some(slot) = gid_of.get_mut(i)
        {
            *slot = gid;
        }
    }
    let q = kapu_core::leiden::modularity(rooms.len(), &graph, &gid_of);
    let mut sizes: BTreeMap<usize, usize> = BTreeMap::new();
    for g in &gid_of {
        *sizes.entry(*g).or_insert(0) += 1;
    }
    let mut size_list: Vec<usize> = sizes.values().copied().collect();
    size_list.sort_unstable();
    let largest = size_list.last().copied().unwrap_or(0);
    let median = size_list.get(size_list.len() / 2).copied().unwrap_or(0);
    if let Some(dir) = dump {
        let mut text = String::new();
        for ((a, b), w) in &graph {
            let _ = writeln!(text, "{a} {b} {w}");
        }
        let _ = std::fs::create_dir_all(dir);
        let path = dir.join(format!("graph-{n}.tsv"));
        std::fs::write(&path, text).map_err(|e| {
            kapu_core::Error::new(
                kapu_core::ErrorCode::E012,
                format!("{}: {e}", path.display()),
            )
        })?;
    }
    Ok(format!(
        "| {n} | {} | {} | {} | {} | {largest} | {median} | {q:.4} |",
        counts.get("link").copied().unwrap_or(0),
        counts.get("term").copied().unwrap_or(0),
        counts.get("sim").copied().unwrap_or(0),
        sizes.len()
    ))
}

/// Median of three timings in milliseconds.
fn time3(mut f: impl FnMut() -> kapu_core::Result<()>) -> kapu_core::Result<f64> {
    let mut t = Vec::with_capacity(3);
    for _ in 0..3 {
        let start = Instant::now();
        f()?;
        t.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    t.sort_by(f64::total_cmp);
    Ok(t.get(1).copied().unwrap_or(0.0))
}

fn search_times(
    sizes: &[usize],
    seed: u64,
    model_dir: &std::path::Path,
    query: &str,
) -> kapu_core::Result<String> {
    use kapu_core::embed::{Model, VectorCache};
    use kapu_core::search::{SearchOptions, search};
    let start = Instant::now();
    let model = Model::load(model_dir)?;
    let load = start.elapsed().as_secs_f64() * 1000.0;
    let mut out = format!(
        "Modell: {} (betöltés és embhash: {load:.0} ms)

| Szobák | Ajtók | Térkép (ms) | 4 ajtó nyitása (ms) | BM25 (ms) | Hibrid, hideg (ms) | Hibrid, meleg (ms) |
|---:|---:|---:|---:|---:|---:|---:|
",
        model.name()
    );
    for &n in sizes {
        let rooms: Vec<RoomSource> = corpus::markdown_rooms(n, seed)
            .into_iter()
            .map(|(id, content)| RoomSource { id, content })
            .collect();
        let bytes = build_with(&rooms, Some(&model))?.bytes;
        let s = kapu_core::Sections::parse(&bytes)?;
        let doors = s.header.doors;
        let ids: Vec<String> = s
            .doc_entries()?
            .iter()
            .step_by(usize::try_from(doors / 4).unwrap_or(1).max(1))
            .take(4)
            .map(|e| e.id.to_owned())
            .collect();
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let map = time3(|| kapu_core::view::map_view(&bytes, None).map(|_| ()))?;
        let open = time3(|| kapu_core::open_bytes(&bytes, &refs).map(|_| ()))?;
        let bm25 = time3(|| search(&bytes, query, SearchOptions::default(), None).map(|_| ()))?;
        let dir = std::env::temp_dir().join(format!("kapu-bench-cache-{}-{n}", std::process::id()));
        let cold = time3(|| {
            let _ = std::fs::remove_dir_all(&dir);
            let cache = VectorCache::in_dir(&model, &dir);
            search(&bytes, query, SearchOptions::default(), Some(&cache)).map(|_| ())
        })?;
        let cache = VectorCache::in_dir(&model, &dir);
        search(&bytes, query, SearchOptions::default(), Some(&cache))?;
        let warm =
            time3(|| search(&bytes, query, SearchOptions::default(), Some(&cache)).map(|_| ()))?;
        let _ = std::fs::remove_dir_all(&dir);
        let _ = writeln!(
            out,
            "| {n} | {doors} | {map:.1} | {open:.1} | {bm25:.0} | {cold:.0} | {warm:.0} |"
        );
    }
    Ok(out)
}

/// Modularity and group count of the Rust Leiden on one dumped graph.
fn leiden_on(path: &std::path::Path) -> std::io::Result<String> {
    use std::collections::BTreeMap;
    let text = std::fs::read_to_string(path)?;
    let mut edges: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    let mut nodes = 0usize;
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let (Some(from), Some(to), Some(weight)) = (
            fields.first().and_then(|x| x.parse::<usize>().ok()),
            fields.get(1).and_then(|x| x.parse::<usize>().ok()),
            fields.get(2).and_then(|x| x.parse::<f64>().ok()),
        ) else {
            continue;
        };
        nodes = nodes.max(from + 1).max(to + 1);
        *edges.entry((from.min(to), from.max(to))).or_insert(0.0) += weight;
    }
    let start = Instant::now();
    let labels = kapu_core::leiden::leiden(nodes, &edges);
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    let q = kapu_core::leiden::modularity(nodes, &edges, &labels);
    let groups = labels.iter().copied().max().map_or(0, |m| m + 1);
    Ok(format!(
        "{}: Rust Q={q:.4} ({groups} groups, {ms:.0} ms)",
        path.display()
    ))
}
