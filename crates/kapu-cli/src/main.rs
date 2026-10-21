//! `kapu` command-line tool (spec chapter 8). Commands arrive in M6; M0 only wires the binary.
#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    println!(
        "kapu {} (KAPU format {})",
        env!("CARGO_PKG_VERSION"),
        kapu_core::FORMAT_VERSION
    );
    ExitCode::SUCCESS
}
