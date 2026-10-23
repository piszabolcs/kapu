//! Author signature and app info card, shared by the CLI and the MCP server.
//!
//! The version comes from the workspace `Cargo.toml` (every crate inherits it), so
//! the signature and the card can never disagree with the build.

use std::fmt::Write as _;

use crate::FORMAT_VERSION;

/// Release version of the tools (workspace `Cargo.toml`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Author signature, the same mark on every app of the author.
pub const SIGNATURE: &str = concat!("psz // v", env!("CARGO_PKG_VERSION"));

/// Home of the project.
pub const REPOSITORY: &str = "https://github.com/piszabolcs/kapu";

/// One or two sentences on what the tools are for.
pub const PURPOSE: &str = "Navigable single-file knowledge container for AI agents. One .kapu file \
holds the Markdown body with a map, an index and a graph, so an agent opens only the rooms it needs.";

/// The layers the tools are built from, with their role.
pub const LAYERS: [(&str, &str); 2] = [
    (
        "core",
        "kapu-core: format, validator, builder, BM25 and vector search, graph, Leiden groups",
    ),
    (
        "tools",
        "kapu (command line) and kapu-mcp (MCP server over stdio)",
    ),
];

/// Languages, notable libraries and the vector model.
pub const BUILT_WITH: [&str; 3] = [
    "Rust",
    "blake3, tiktoken-rs, tokenizers, rust-stemmers, rmcp, clap",
    "static embeddings: potion-multilingual-128M (int8)",
];

/// The project repository and the third-party repositories it is built on.
pub const REPOSITORIES: [(&str, &str); 3] = [
    ("piszabolcs/kapu", REPOSITORY),
    (
        "modelcontextprotocol/rust-sdk",
        "https://github.com/modelcontextprotocol/rust-sdk",
    ),
    (
        "MinishLab/model2vec",
        "https://github.com/MinishLab/model2vec",
    ),
];

/// The version line that follows the program name: `0.1.0 (KAPU format 1.0)`.
#[must_use]
pub fn version_line() -> String {
    format!("{VERSION} (KAPU format {FORMAT_VERSION})")
}

/// The app info card printed by `--version`, after the program name.
#[must_use]
pub fn card() -> String {
    let mut out = version_line();
    out.push_str("\n\n");
    let mut width = 0;
    for word in PURPOSE.split(' ') {
        if width > 0 && width + 1 + word.len() > 78 {
            out.push('\n');
            width = 0;
        } else if width > 0 {
            out.push(' ');
            width += 1;
        }
        out.push_str(word);
        width += word.len();
    }
    out.push_str("\n\nArchitecture\n");
    for (name, role) in LAYERS {
        let _ = writeln!(out, "  {name:<7}{role}");
    }
    out.push_str("\nBuilt with\n");
    for line in BUILT_WITH {
        let _ = writeln!(out, "  {line}");
    }
    out.push_str("\nRepositories\n");
    for (name, url) in REPOSITORIES {
        let _ = writeln!(out, "  {name:<31}{url}");
    }
    let _ = write!(out, "\n{SIGNATURE}");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_carries_the_build_version() {
        assert_eq!(SIGNATURE, format!("psz // v{VERSION}"));
        assert!(card().starts_with(&version_line()));
        assert!(card().ends_with(SIGNATURE));
    }
}
