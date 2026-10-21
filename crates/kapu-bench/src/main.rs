//! `kapu-bench`: synthetic corpus generator and measurement helpers (spec M8).
//!
//! M1 delivers the `gen` subcommand; the measurement runner arrives with M8.
#![forbid(unsafe_code)]

mod corpus;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

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
    }
}
