# Changelog

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versioning: [SemVer](https://semver.org/).
The file format has its own version (`%KAPU 1.0`, see `SPEC-KAPU.md`); the tools below are versioned separately.

## [0.1.0] - 2026-10-23

First portfolio release. Specification v1.0.1, milestones M0-M7 and M10 done, M8 and M9 in part (see `TERV.md`).

### Added
- `kapu-core`: reader, validator (error codes E001-E012), builder with BM25 index, token prices (o200k_base), YAKE tags, BLAKE3 fingerprints, graph (`link`, `term`, `sim` edges), Leiden groups and hybrid search (BM25 + static embeddings, RRF).
- `kapu` command line tool: `new`, `import`, `export`, `add`, `rm`, `build`, `map`, `search`, `describe`, `open`, `validate`, `bench`; every command with `--json`.
- `kapu-mcp`: MCP server over stdio with `kapu_map`, `kapu_search`, `kapu_describe`, `kapu_open`, `kapu_write`, the std-1 guide and the content scan (FR-19).
- `kapu-gui`: desktop window (Tauri 2) with the map and token prices, room reader, search with a token budget, graph view and file checks.
- Demo file `examples/kapu-spec.kapu`: the specification packed as a KAPU file.
- Author signature `psz // v0.1.0`: last line of `kapu --help`, app info card on `--version` and in the window.
- Packed vector model: `potion-multilingual-128M` in int8 (124 MB), portable Windows package, draft release workflow for Linux and macOS.
- Byte-level Python reference builder, test vectors, cross-checks, fuzz targets, CI on Windows, Linux and macOS.
