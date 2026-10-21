# KAPU

Navigable single-file knowledge container for AI agents.

A `.kapu` file is one UTF-8 text file: a fixed 512-byte header, a map of "doors" with their token prices, long descriptions, a graph, a BM25 index, byte positions with BLAKE3 fingerprints, and the Markdown body itself. The body is the only source of truth; every other section is derived from it by the builder, byte for byte. A reader (the `kapu` CLI or the `kapu-mcp` MCP server) looks at the map, searches, and opens only the rooms a task needs, so an agent never has to read the whole file.

**Status:** v1 in development. Milestone M0 (repository, toolchain, quality gates) is done. See `ALLAPOT.md` for the current state.

Hungarian documents: the specification (`SPEC-KAPU.md`, the source of truth), the milestone plan (`TERV.md`), the status log (`ALLAPOT.md`) and the decision log (`DONTESNAPLO.md`). The code, its comments and commit messages are in English.

## Layout

```
crates/kapu-core    library: read, validate, build, index, graph, groups, search
crates/kapu-cli     `kapu` command-line tool
crates/kapu-mcp     `kapu-mcp` MCP server (stdio)
crates/kapu-bench   synthetic corpus generator and measurement helpers
tests/vectors       hand-built sample file and invalid files per error code (M1)
bench/              corpora, questions and REPORT.md (M8)
fuzz/               cargo-fuzz targets (M2)
tools/              developer-only tools, e.g. the Python distillation script
Archive/            v0.1 drafts, kept for reference
```

## Build and check

Requires Rust; `rustup` installs the pinned toolchain from `rust-toolchain.toml` automatically.

```
cargo build --workspace
scripts/check.ps1        # Windows: fmt, clippy -D warnings, test, deny, audit
scripts/check.sh         # Linux / macOS
```

The check scripts need `cargo-deny` and `cargo-audit` (`cargo install --locked cargo-deny cargo-audit`).

## Licence

Code: MIT. Specification: CC BY 4.0. Author and originator: Pintér Szabolcs.
