# Changelog

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versioning: [SemVer](https://semver.org/).
The file format has its own version (`%KAPU 1.0`, see `SPEC-KAPU.md`); the tools below are versioned separately.

## [Unreleased]

### Changed
- Specification v1.0.2: in-page anchors link to the room's own sections and no longer warn (D123); group names leave out stop words (D121).
- `kapu-mcp`: tool annotations, at most 50 hits, a next step in every error (D120); an excerpt line under each search hit (D124); a group lists its rooms only and `room=` lists one room's sub-doors (D125); at most 8 neighbours per opened door (D126); a totals line on the map (D127).

### Fixed
- The S5 content scan no longer matches a command name inside another word (`async`, `confirm`) (D122).

### Added
- Real-corpus measurements: `bench/REPORT.md` chapters 5-11, `bench/results/`.
- `kapu bench --json`: room-level recall@5/10 and MRR@10, where any door of the expected room is a hit (D130).
- Study pilot tools in `tools/study/`: corpus preparation from public documentation, Stack Overflow questions with a gold page, chunk-RAG baselines, scale probe, logged file-search, llms.txt and RAG tools for agents.
- Tools: `tools/collect_skills.py`, `tools/mcp_probe.py`, `tools/kapu_mcp_call.py`, `tools/mcp_eval.py`.

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
