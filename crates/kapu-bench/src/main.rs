//! `kapu-bench` corpus generator and measurement helpers (spec M8). Arrives in M1/M8; M0 only wires the binary.
#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    println!(
        "kapu-bench {} (KAPU format {})",
        env!("CARGO_PKG_VERSION"),
        kapu_core::FORMAT_VERSION
    );
    ExitCode::SUCCESS
}
