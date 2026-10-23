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
use kapu_core::build::{RoomSource, build};

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
        Command::Measure { rooms, seed, runs } => match measure(&rooms, seed, runs.max(1)) {
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
fn measure(sizes: &[usize], seed: u64, runs: usize) -> kapu_core::Result<String> {
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
            bytes = build(&rooms)?.bytes;
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
    }
    Ok(out)
}
