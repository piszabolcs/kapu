//! `kapu-mcp` MCP server over stdio (FR-15). Tools arrive in M7; M0 only wires the binary.
#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    println!(
        "kapu-mcp {} (KAPU format {})",
        env!("CARGO_PKG_VERSION"),
        kapu_core::FORMAT_VERSION
    );
    ExitCode::SUCCESS
}
