# Changelog

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versioning: [SemVer](https://semver.org/).
The file format has its own version (`%KAPU 1.1`, see `SPEC-KAPU.md`); the tools below are versioned separately.

## [Unreleased]

### Changed
- Search by meaning uses one vector per window of a door's text (at most 512 tokens, overlapping by an eighth); the best window counts (D141). Room-level recall@10 on real questions, hybrid: Rust book 0.46 to 0.49, Kubernetes docs 0.44 to 0.46; on MDN, with its many short sections, 0.28 to 0.27. The vector cache (`.vecs2`) keeps the windows of unchanged doors after a write, and `kapu import`, `build`, `add` and `rm` compute them right away.
- File format 1.1 (specification v1.1, D140): an `#INDEX` posting is `<gap>:<tf>`, the gap between the doors' line numbers in `#DOCS`, instead of the door id. Files are about 4.5 times smaller (Kubernetes docs: 117 MB to 26 MB, MDN: 542 MB to 128 MB) and the 13-million-token MDN corpus builds within the 1 MiB line limit. Format 1.0 files are still read; `kapu build` rewrites them as 1.1.
- Specification v1.0.5 (D139): one call parses and checks the file once, a term's `#INDEX` line is found by binary search, and the MCP server keeps a checked copy of an unchanged file. Search on the Kubernetes docs: 657 ms to 19 ms (median), on MDN 3.2 s to 158 ms; opening a door: 216 ms to 5 ms.
- Specification v1.0.4 (D131-D138): group names leave out parameter-table words; the search excerpt prefers rarer query words; the map's totals line names the content language (`lang=`) and the guide asks for queries in that language; at most 10 content-scan lines on the map (`scan=true` lists all); `kapu_open` serves long doors in parts (`max_tokens`, `part`); the MCP server keeps unchanged files and door vectors in memory.
- Specification v1.0.2: in-page anchors link to the room's own sections and no longer warn (D123); group names leave out stop words (D121).
- `kapu-mcp`: tool annotations, at most 50 hits, a next step in every error (D120); an excerpt line under each search hit (D124); a group lists its rooms only and `room=` lists one room's sub-doors (D125); at most 8 neighbours per opened door (D126); a totals line on the map (D127).

### Removed
- The internal planning documents (status log, decision log, open questions, milestone plan, research plans, early concept archive) left the repository; decisions are still cited by their D-numbers in the specification and the code.

### Fixed
- Invisible characters (U+200B-U+200D, U+2060, U+FEFF) in a room's title or first sentence no longer produce a file the reader rejects (D135).
- `kapu ... | head` no longer panics when the reader closes the pipe early (D138).
- The S5 content scan no longer matches a command name inside another word (`async`, `confirm`) (D122).

### Added
- `examples/kapu-docs.kapu`: this documentation as a KAPU file, the quickstart example (`tools/make_examples.py`).
- The results page `docs/results/index.html` (English and Hungarian, written by `tools/study/results_page.py` from `bench/results/`) and the animated terminal demo `docs/assets/demo.svg` (`tools/make_demo_svg.py`).
- Real-corpus measurements: `bench/RESULTS.md`, `bench/results/`.
- `kapu bench --json`: room-level recall@5/10 and MRR@10, where any door of the expected room is a hit (D130).
- Study pilot tools in `tools/study/`: corpus preparation from public documentation, Stack Overflow questions with a gold page, chunk-RAG baselines, scale probe, logged file-search, llms.txt and RAG tools for agents.
- Tools: `tools/collect_skills.py`, `tools/mcp_probe.py`, `tools/kapu_mcp_call.py`, `tools/mcp_eval.py`.

## [0.1.0] - 2026-10-23

First portfolio release. Specification v1.0.1, milestones M0-M7 and M10 done, M8 and M9 in part.

### Added
- `kapu-core`: reader, validator (error codes E001-E012), builder with BM25 index, token prices (o200k_base), YAKE tags, BLAKE3 fingerprints, graph (`link`, `term`, `sim` edges), Leiden groups and hybrid search (BM25 + static embeddings, RRF).
- `kapu` command line tool: `new`, `import`, `export`, `add`, `rm`, `build`, `map`, `search`, `describe`, `open`, `validate`, `bench`; every command with `--json`.
- `kapu-mcp`: MCP server over stdio with `kapu_map`, `kapu_search`, `kapu_describe`, `kapu_open`, `kapu_write`, the std-1 guide and the content scan (FR-19).
- `kapu-gui`: desktop window (Tauri 2) with the map and token prices, room reader, search with a token budget, graph view and file checks.
- Demo file `examples/kapu-spec.kapu`: the specification packed as a KAPU file.
- Author signature `psz // v0.1.0`: last line of `kapu --help`, app info card on `--version` and in the window.
- Packed vector model: `potion-multilingual-128M` in int8 (124 MB), portable Windows package, draft release workflow for Linux and macOS.
- Byte-level Python reference builder, test vectors, cross-checks, fuzz targets, CI on Windows, Linux and macOS.
