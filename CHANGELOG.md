# Changelog

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versioning: [SemVer](https://semver.org/).
The file format has its own version (`%KAPU 1.2`, see `SPEC-KAPU.md`); the tools below are versioned separately.

## [Unreleased]

## [0.3.0] - 2026-10-27

Specification v1.2.2; the file format stays 1.2. Phases 1 to 3 of plan v2: every ranking change was measured as a switch on a fixed dev/test split of the questions, and only what passed the M0 rule became a default (`bench/RESULTS.md` chapter 12, `bench/results/2026-10-27-plan-v2.json`).

### Changed
- `kapu_search` gives one hit per room (D158, J1): the best door stays the hit, and a line under it names the room's other matching sections (`+3 in this room: syntax, examples, ...`). On the test part of seven question sets: room-level recall@10 +0.022 (95% interval +0.010 to +0.035), MRR@10 +0.011, no set worse, search time unchanged. The `kapu bench` hybrid ranking collapses the same way.
- `kapu_ask` opens the start of the best door of each of the first 5 rooms, the budget shared, instead of the best hit up to its whole budget (D159, J12). The expected room is in the reply for 42% instead of 15% of the Rust book questions, 39% instead of 17% on Kubernetes and 22% instead of 12% on MDN, in about 1,130 instead of 1,800 tokens.
- The float windows the vector rescoring keeps in memory are also bounded by their number, 250,000 (D157): on an MDN-sized file the memory bounded by doors alone grew past 20 GB in a long run.

### Added
- The ablation framework (D156): `RankOptions` with every switch of plan v2 (stemming, separate average lengths, parent score, BM25 hits in the rescoring, weighted, query-dependent and score-based fusion, collapse, short-door cut-off, a graph step), off by default; off, the ranking is bit for bit that of 0.2.1 (`tests/vectors/rankings-0.2.1.tsv`). `kapu bench --config <json>` runs one configuration or a matrix and `--rankings` writes every question's rankings; `kapu-mcp --rank <json>` runs the server with them.
- Study tools: `split_questions.py` (the fixed 40/60 dev/test split, `bench/splits/`), `ablation.py` (metrics, paired bootstrap, error classes, the M0 rule), `ask_eval.py`, `neighbour_follow.py`, `group_quality.py`, `title_weight.py`, `oracle_eval.py` (oracle, KAPU and closed-book answers, and abstention), `collect_v2.py`, `v2_tables.py`; `chunk_baseline.py` keeps per-question ranks and adds a chunk hybrid.
- A new measurement corpus: public PLC manuals of six vendors (623 pages, local only), with 72 answerable, 24 absent and 12 two-page questions. With the KAPU tools Claude Sonnet answered 92% correctly (97% with the right page in the prompt, 28% alone) and said "not found" for 23 of 24 absent questions, where alone it made up 11 answers (N1, N4).
- The tool-list test prices the six tools (K-01: at most 800 tokens; now 799).

### Measured and not changed
- Separate average lengths for rooms and sections (J2a), a share of the room's score for its sections (J2b), the first 50 BM25 hits in the vector rescoring (J4a), weighted, query-dependent and min-max fusion (J4b-J4d), no own hits for sections under 40 tokens (J10), a graph step from the hits (J9), stemming with one stemmer or none, and title weights 1, 2 and 5: none passed the rule; they stay as switches, off.

## [0.2.1] - 2026-10-26

Specification v1.2.1; the file format stays 1.2.

### Changed
- `kapu_map kind=` and `kapu map --kind` give the kind's catalog (D154): one line per room with its price and the start of its description, so an agent can choose skills from one reply; a skill's reference rooms are folded into the skill (`+5 rooms`). Its default budget is 4000 tokens, and the first map names the catalog of the most common kind.
- `kapu_open` with several ids shares the budget evenly (D155), so one call shows the start of every door; without `max_tokens` each door gets 2000 tokens, for up to five doors.

### Added
- A new app icon from the artwork of 2026-10-26: the arched gate and centred map lines in a framed dark tile (`tools/make_icons.py`): the window's icons, the interface, the README logo and the results page.
- The results page as PDF, in English and Hungarian (`docs/results/kapu-measurements-{en,hu}.pdf`, `tools/study/results_pdf.py`).
- The skill-finding measurement (D153): `tools/study/skill_pool.py`, `skill_eval.py` and `skill_tasks.json`, results in `bench/RESULTS.md` chapter 11 and on the results page.

## [0.2.0] - 2026-10-26

Specification v1.2 and file format 1.2, from the two review records of 2026-10-26. KAPU is now aimed at two cases: a personal knowledge base and memory for coding agents on any platform, and knowledge for local models with a small context.

### Added
- Search vectors in the file (D143): an optional `#VEC` section holds the sign bits of every window vector, 43 base64 characters per window. The sign bits pick 200 candidates and the model's float windows order them, so the first search needs no cache, works on any machine and ranks as the float vectors did: room-level recall@10, hybrid, Rust book 0.485 (same), Kubernetes docs 0.458 (same), MDN 0.271 to 0.278, the English skills corpus 0.948 to 0.961. The file grows by 1.7-3.3% (Rust book 1.7%, Kubernetes docs 2.2%, the English skills corpus 2.0%, MDN 3.3%); float or int8 vectors would have added 13-25%. Search takes a little longer for the rescoring (Kubernetes docs 33 to 48 ms, MDN 198 to 277 ms, median). The `dim=` of a `#VEC` section is at most 4096 (D151, found by the new `vec` fuzz target: a file could make the reader allocate a scoring table of any size).
- `kind:` on a room (D144): `skill`, `project`, `pattern` or `note` (others allowed with warning W006), in the room's head. The map (`kapu_map kind=`, `kapu map --kind`) and the search (`kind`) filter by it, and the totals line counts the kinds (`kinds=skill:30`).
- Project cards (D144): `kapu new --kind project` and `kapu template --kind project` give a card with Goal, Period, Stack, Key decisions, What worked, What did not and Where the code is; W007 warns above 600 tokens.
- `kapu_ask` (D146): search and open in one call, within `max_tokens` (default 2000): the best door (or its first part), more whole doors if they fit, and the other matches.
- `kapu import --skills <folder> <file>` (D147): every `SKILL.md` folder is a room with the skill's name as title and its description as the `>` line, `kind: skill`, `has_scripts: yes` and the scripts as code blocks; reference Markdown files are rooms of their own, linked from the skill. Nothing is run.
- A `max_tokens` reply budget on every reading tool (D145): `kapu_map` 1000, `kapu_search` 600, `kapu_describe`, `kapu_open` and `kapu_ask` 2000 by default; a reply never goes over it.
- The first line of every MCP reply is its own price: `reply: 523 tokens` (D145).
- `kapu` warns (W008) when a file is above 50 MiB, GitHub's warning size (D148).
- A CI job that builds and tests with the declared minimum Rust (D142).
- `tests/compat/minta-1.1.kapu`, a format 1.1 file the reader must keep reading.
- `tools/study/measure_v12.py` (reply budgets, one-call answers) and `tools/study/translate_hu.py` (the Hungarian questions as an agent writes them in English).
- `examples/kapu-docs.kapu`: this documentation as a KAPU file, the quickstart example (`tools/make_examples.py`).
- The results page `docs/results/index.html` (English and Hungarian, written by `tools/study/results_page.py` from `bench/results/`) and the animated terminal demo `docs/assets/demo.svg` (`tools/make_demo_svg.py`).
- Real-corpus measurements: `bench/RESULTS.md`, `bench/results/`.
- `kapu bench --json`: room-level recall@5/10 and MRR@10, where any door of the expected room is a hit (D130).
- Study pilot tools in `tools/study/`: corpus preparation from public documentation, Stack Overflow questions with a gold page, chunk-RAG baselines, scale probe, logged file-search, llms.txt and RAG tools for agents.
- Tools: `tools/collect_skills.py`, `tools/mcp_probe.py`, `tools/kapu_mcp_call.py`, `tools/mcp_eval.py`.


### Changed
- The minimum Rust is 1.90 (`rust-version`, measured: Tauri needs 1.90), not 1.99. `rust-toolchain.toml` asks for `stable`; CI and the release workflow pin 1.99.1 (D142).
- `kapu_open`'s `max_tokens` is the budget of the whole reply (default 2000), not the part size; a part is cut so the reply fits, and `part=N` of a later call cuts at the same places (D145).
- `kapu_search`'s door-price filter is `max_price` (`--max-price` on the command line; `--max-tokens` still works there); `max_tokens` is the reply budget (default 600). The `excerpts` switch is gone: every hit has its excerpt line, at most 100 characters instead of 160 (D145).
- The tool list costs 797 tokens instead of 1174, with six tools instead of five: shorter descriptions, compact input schemas, and the std-1 guide only in the server's instructions and the first map (D145).
- A map is flat (every door line without a group) up to 800 tokens instead of 2000 (D145).
- The specification starts with the two use cases and what KAPU is not for; the desktop window is in scope (J14).
- Search by meaning uses one vector per window of a door's text (at most 512 tokens, overlapping by an eighth); the best window counts (D141). Room-level recall@10 on real questions, hybrid: Rust book 0.46 to 0.49, Kubernetes docs 0.44 to 0.46; on MDN, with its many short sections, 0.28 to 0.27. The vector cache (`.vecs2`) keeps the windows of unchanged doors after a write, and `kapu import`, `build`, `add` and `rm` compute them right away.
- File format 1.1 (specification v1.1, D140): an `#INDEX` posting is `<gap>:<tf>`, the gap between the doors' line numbers in `#DOCS`, instead of the door id. Files are about 4.5 times smaller (Kubernetes docs: 117 MB to 26 MB, MDN: 542 MB to 128 MB) and the 13-million-token MDN corpus builds within the 1 MiB line limit. Format 1.0 files are still read; `kapu build` rewrites them as 1.1.
- Specification v1.0.5 (D139): one call parses and checks the file once, a term's `#INDEX` line is found by binary search, and the MCP server keeps a checked copy of an unchanged file. Search on the Kubernetes docs: 657 ms to 19 ms (median), on MDN 3.2 s to 158 ms; opening a door: 216 ms to 5 ms.
- Specification v1.0.4 (D131-D138): group names leave out parameter-table words; the search excerpt prefers rarer query words; the map's totals line names the content language (`lang=`) and the guide asks for queries in that language; at most 10 content-scan lines on the map (`scan=true` lists all); `kapu_open` serves long doors in parts (`max_tokens`, `part`); the MCP server keeps unchanged files and door vectors in memory.
- Specification v1.0.2: in-page anchors link to the room's own sections and no longer warn (D123); group names leave out stop words (D121).
- `kapu-mcp`: tool annotations, at most 50 hits, a next step in every error (D120); an excerpt line under each search hit (D124); a group lists its rooms only and `room=` lists one room's sub-doors (D125); at most 8 neighbours per opened door (D126); a totals line on the map (D127).


### Removed
- The internal planning documents (status log, decision log, open questions, milestone plan, research plans, early concept archive) left the repository; decisions are still cited by their D-numbers in the specification and the code.


### Fixed
- The MCP server runs every call on a blocking thread, so a large file no longer stalls the stdio loop (A24, D149).
- Invisible characters (U+200B-U+200D, U+2060, U+FEFF) in a room's title or first sentence no longer produce a file the reader rejects (D135).
- `kapu ... | head` no longer panics when the reader closes the pipe early (D138).
- The S5 content scan no longer matches a command name inside another word (`async`, `confirm`) (D122).


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
