# Changelog

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versioning: [SemVer](https://semver.org/).
The file format has its own version (`%KAPU 1.3`, see `SPEC-KAPU.md`); the tools below are versioned separately.

## [Unreleased]

### Changed
- The README, the results page and the screenshots brought up to 0.6.0: the comparison with one Markdown file and grep, a small local model and the file maker at the front, and the screenshots taken from the 0.6.0 window (the old ones showed 0.1).
- Chapter 14 of `bench/RESULTS.md` measured again with 0.6.0: `qwen3:8b` on an 8 GB GPU, on the PLC manuals, 12 of 36 fully correct at 8k context (2 with 0.5, 1 alone, 0 with grep over the Markdown) and 21 at 32k; it makes up 4 of 24 absent answers at 8k and 6 at 32k (17 alone, 1 with 0.5 at 8k). Where the model half knows the subject (Rust book, Kubernetes, MDN) KAPU changes little.

### Fixed
- The desktop window: the address fields of Create had the browser's default look, and the graph view threw an error when it was resized before it had its data.
- The one-line installers find the newest release again: they asked GitHub for `releases/latest`, which skips pre-releases, and every KAPU release is one, so they stopped with 404 unless `KAPU_VERSION` was set.
- The minimum Rust 1.90 builds the workspace again: `kapu-create` uses `mail-parser` 0.11 and `sevenz-rust2` 0.20 (`mail-parser` 1.0 asks for Rust 1.98, `sevenz-rust2` 0.24 for 1.93). The source at the tag v0.6.0 needs Rust 1.98; its prebuilt packages are not affected.

## [0.6.0] - 2026-11-05

Against one Markdown file with grep, measured with Claude Sonnet on PLC manuals: $1.42 against $1.85 on 60 single questions, $0.58 against $0.72 in a session of 25, at about the same accuracy (98 and 99 of 108, 17 and 19 of 20 fully correct). The target of 2/5 of its dollars at the same accuracy is not met ([chapter 15](bench/RESULTS.md#15-one-markdown-file-with-grep-against-kapu-plan-v4-and-v5)).

### Changed
- Specification v1.5, in English and Hungarian (plan v5): FR-21 describes the hook, chapter 26 lists the changes and the measurements.
- Specification v1.4, in English and Hungarian (plan v4 V4-7): FR-15 describes the passages of `kapu_ask`, `kapu_grep` in the place of `kapu_describe`, `kapu_open` `line` and 3 neighbours, the `<kapu-passages>` frame and three acceptance points; FR-17 says why the guide stays std-1; chapter 25 lists the changes and the measurement. The file format stays 1.2 and 1.3. The README's tool table follows.
- `kapu_ask` answers with passages (plan v4, D175, D176): every door is cut into passages of consecutive lines (a heading starts one, about 700 to 1,100 characters), and the reply is the best passage of each of the best rooms of the search, ranked by the passage's keyword score and its room's rank, in one `<kapu-passages>` frame of about 1,000 tokens, each passage with its door and line range. On the PLC manuals the evidence of the answer is in a 700-token reply for 85% of the questions (the shared starts of 0.5.1: 2%). The earlier forms stay as switches (`kapu-mcp --rank '{"ask": "spread"}'`, `top`, `focus`).
- In one session `kapu_ask` leaves out the passages it showed before and says how many (D176), so a follow-up question gets new text instead of the same; `kapu_open` still shows a door whole.
- `kapu_grep` takes the place of `kapu_describe` (D176): the lines that hold the words (`a|b` for either, any case) as `door:Lnn: line`, or `no match`, within 300 tokens by default, for a cheap check that something is or is not in the file. The tool list stays under 800 tokens. `kapu describe` on the command line is unchanged.
- With the three changes, Claude answers the 60 PLC questions (36 answerable, 24 not in the manuals) for 1.68 dollars instead of 2.56 with 0.5.1 and 1.85 with one Markdown file and Grep and Read (one run each, 3 for the Markdown file; accuracy within the noise: 92% correct, 96% of the absent ones declined). 64% of the answerable questions take one call.
- `kapu_grep` reads its pattern as a regex, in any case and line by line, as grep does (D177); a pattern that is not a valid regex is read as `a|b` words, and the reply says so. Agents write their grep patterns as regexes (`retain.{0,40}(bytes|KB)`, `\d{4,6}`); the literal check of D176 answered them `no match`, and the agent kept trying. Three runs on the 60 PLC questions: 1.53 dollars on average (1.37 to 1.63) against 1.85 for the Markdown file with Grep and Read (1.64 to 2.02), the 24 questions not in the manuals 0.91 instead of 1.10. The trade: 86% of the answerable questions fully correct instead of 92% (the rest partly correct, none wrong), mostly where the deciding sentence is just past the passage. PixelLab, 12 requests: 0.27 dollars against 0.39.
- A `kapu_ask` passage goes on for up to 3 more non-blank lines of its door (D178), up to the next heading or 600 characters: the sentence just after a passage often decides the answer (that the run-time meters must be restarted after STOP, what a watchdog does when it expires). Three runs on the 60 PLC questions: 1.42 dollars on average (1.33 to 1.50) against 1.53 without the tail and 1.85 for the Markdown file with Grep and Read; 90.7% of the answerable questions fully correct (86.1% without the tail, 91.7% for the Markdown file), none wrong; 26 of the 36 answered in one call instead of 21. PixelLab, 12 requests, three runs: 0.23 dollars against 0.27 without the tail and 0.39 for the Markdown file, accuracy within the judge's noise. `kapu-mcp --rank '{"passage_tail": 0}'` gives the passages of D177.
- `kapu-core` depends on `regex` (already in the lock file through the desktop window; pure Rust, linear time, the compiled pattern bounded to 1 MB).
- `kapu_ask` with 1,000 tokens or more shows each of its rooms at the part where the query's words are, not at its start (D174): on the PLC manuals the evidence of the answer is in the reply for 50% instead of 2% of the questions at 1,000 tokens and 87.5% instead of 12.5% at 2,000; no question set loses a room. Below 1,000 tokens the starts stay, so all rooms still fit.
- Shorter lists (D174): `kapu_ask` names 3 other matches instead of 8, `kapu_open` 3 neighbours instead of 8, and a shared ask's part line no longer repeats the door id. With the change above, Claude answers the PLC questions for 21% fewer dollars (one run, accuracy within the noise). `kapu-mcp --rank '{"lean": false, "spread_best": false}'` gives the replies of 0.5.1.

### Added
- `kapu-mcp hook <file.kapu>`, a Claude Code `UserPromptSubmit` hook (plan v5 V5-1, D180): it asks the file the prompt as `kapu_ask` would and hands the passages to the agent together with the prompt (`additionalContext`), so a question can be answered without a tool call. It speaks only when the prompt's best vector similarity to the file reaches 0.845 (switch `hook_similarity`; measured with the e5 model: 59 of the 60 PLC questions and all 12 PixelLab requests pass, and about a third of 30 off-topic prompts too), stays under Claude Code's 10,000-character limit, and on an error, a timeout (5 s), a short prompt or a slash command it prints nothing and exits 0, so it never holds up a prompt. A cold start on the 13 MB PLC file takes about 1.5 s. `hook_stop` adds a line on when to stop looking (to be measured). `kapu_core::search::SearchResult` carries the best vector similarity (`similarity`).
- The hook is optional and off unless set up: it measured no cheaper than the tools alone ($1.53 and with a stop line $1.52 against $1.50, 33 and 35 against 31 of 36 fully correct, one run each, D182). A limit of one tool call per question was measured and left out ($0.90, but 24 of 36, D184).
- `kapu_open` `line` (plan v4 V4-5, D177): only the lines from `Lnn` of a passage or a grep line (and two before it), 600 tokens a door by default, in a passages frame with the line range, without neighbours; the reply names the next line.
- Switches `guide_answer` (a guide for one `kapu_ask`, an exact check and a point to stop at) and `hints` (a next-step line under `kapu_ask` and `kapu_grep`), measured and left off (D177): on the PLC questions 1.81 dollars instead of 1.68 at slightly lower accuracy.
- `kapu-create`, the KAPU maker ([SPEC-KAPU-CREATE.md](SPEC-KAPU-CREATE.md)): a file from web sites, documents and folders in one step, without an agent. `kapu-create new <file> <source>...`, `refresh <file>` (reads the sources again from `<file>.sources.json`, keeps hand-written rooms and the ids of sections), `preview <source>... [--rooms-dir]`. Reads PDF (PDFium: bookmarks, headings from type size and section numbers, running heads, contents and index pages dropped, tables, image pages through Tesseract OCR; `pdf-extract` without PDFium), Word, Excel and PowerPoint (old and new), OpenDocument, RTF, EPUB, CSV, HTML, Markdown, text, code, e-mail with attachments, ZIP and 7z, CHM (Windows), images. Web sites through `llms.txt`, `llms-full.txt`, the sitemap or a crawl, inside the address's first folder, with `robots.txt` and at most 5000 pages. Rooms of at most 4000 tokens cut at the headings, with `Source:` and `Path:` lines, an overview per larger document, word-for-word copies kept once, links between documents as room links; a report names every file not read. Seven IndraDrive and IndraLogic manuals of 1550 pages: rooms in 4 s, the file with vectors in 2.5 min. Scanned pages through Tesseract in the document's own language, recognised from its text (word recall on test scans: German 0.99, Hungarian 0.96, English 0.95). Only `kapu-create` and the window use the network; `kapu`, `kapu-mcp` and the core stay offline (spec chapter 7/6).
- The window's **Create** view (sources by button, address or drop, options, preview, progress, report) and **Refresh from sources** in the Check view.
- Server switches for measurement (`kapu-mcp --rank`): `ask` `focus` (only the best door, at its best part), `focus_below`, `spread_rooms`, `spread_best`, `best_from`, `ask_tokens`, `open_tokens`, `ask_others`, `neighbours`, `short_parts`, `lean`, and `tail` (`kapu_search` also names the ids of the next hits; off by default, it fits only with about 900 tokens).
- `md_eval.py` records the input and the reply size of every tool call and warms the prompt cache with one unrecorded call before a run, so the first calls of a new tool list do not cost more (D177).
- `md_eval.py`: the form `kapu-hook`, what the hook did on every call, and the cache tokens of each API call (D181, D182).
- `tools/study/md_eval.py` (plan v3 M1): Claude with one Markdown file and Grep and Read, against the KAPU tools, single questions and long sessions, with the cache writes and reads and the dollars of every call; `ask_eval.py --max-tokens` and the evidence of the answer in the reply.

### Fixed
- `md_eval.py report` summed the running total that `claude --resume` reports on every turn; a session's dollars are those of its last turn (D185). No published figure used the wrong ones.

## [0.5.1] - 2026-10-29

Specification v1.3.1; the file format stays 1.2 and 1.3. A large file can be searched within seconds of `kapu import`, and its first build with the vector model is 2.5 times faster (D173).

### Changed
- The x86-64 release builds need a processor with AVX2 (x86-64-v3: Intel from 2013, AMD from 2015); on an older one they stop with an illegal-instruction error. macOS on Apple silicon is unchanged.
- The encoder of the vector model is faster and gives the same vectors: integer products in 32 lanes, rounding without a library call, the attention head by head. One 512-token window on one thread: 1.10 s → 0.66 s with a baseline build, 0.44 s with AVX2; on 12 threads 5.8 → 14.3 windows a second. The ElevenLabs documentation (1,399 rooms, 14,942 windows) builds in 14.5 minutes instead of 33 minutes on 12 threads. Files and window logs of 0.5.0 stay valid.

### Added
- When a build has more than 200 window vectors to compute, `kapu import`, `build`, `add`, `rm` and `new` first write the file without vectors (header field `pending=<model>`), so it can be searched by keyword at once (the ElevenLabs file after 37 seconds), then compute the vectors and write the whole file. Stopping is safe: `kapu build` goes on from the window log. A search on such a file says that the vectors are still being computed.
- `kapu_write` with more than 200 windows to compute saves without vectors the same way and says so, instead of computing for minutes inside the tool call.
- Tests: the bits of long windows, the count of missing windows, a file with pending vectors, and `encoder_speed` (ignored: `cargo test --release -p kapu-core --test bert -- --ignored --nocapture`).

## [0.5.0] - 2026-10-28

Specification v1.3: a file whose `#VEC` holds 4- or 8-bit integers is format 1.3, every other file stays 1.2. Plan v3 (D168-D172): the bundled vector model is now `multilingual-e5-small`, a contextual encoder run in plain Rust (`bench/RESULTS.md` chapter 13, `bench/results/2026-10-28-plan-v3.json`, the results page with a new section and its PDFs). There is no 0.4.0.

### Changed
- The bundled vector model is `multilingual-e5-small` in its int8 form (`models/multilingual-e5-small-q8`, 135 MB) instead of `potion-multilingual-128M` (D172). The release workflow downloads the int8 ONNX file at a fixed commit, checks its SHA-256, converts it with `tools/onnx_to_kapu.py` and checks the converted weights, so every platform has the same `embhash`; it also runs the encoder's golden test on Windows, Linux and macOS.
- A file built with the static model of the earlier releases keeps its vector search as long as that model is installed (the installers leave it in place); otherwise its search is BM25 only, and the reply says that `kapu build` rebuilds it with the new model. `kapu_write` on such a file rebuilds it with the bundled model instead of dropping its vectors. A first build with the new model takes minutes on a large file (Kubernetes 15 minutes, MDN 50 minutes on 12 threads), a rebuild only computes new text.
- The trade, measured and accepted (D171, D172): room-level recall@10 +0.062 and MRR@10 +0.028 on the test part of seven question sets; Hungarian questions on English skills lose MRR (0.49 to 0.44), a small file is searched about 35 ms slower (the query's vector, release build), a large one faster (MDN 408 to 320 ms), and files are 9 to 16% larger.

### Added
- A contextual vector model (D171): `multilingual-e5-small` in the int8 form of ONNX Runtime's dynamic quantization, run in plain Rust (`kapu --model <folder>`), with no new dependency and no download at run time. `tools/onnx_to_kapu.py` makes the model folder from the ONNX file. Every product is exact in integers and every float sum has a fixed order, so a vector has the same bits on SSE2, AVX2 and AVX-512 builds. Its windows are stored whole in `#VEC`, 4 bits a component, and scored without read-time rescoring. On the test part of seven question sets, against the bundled model: room-level recall@10 +0.062 (95% interval +0.035 to +0.090), MRR@10 +0.028; Hungarian questions on English skills lose MRR (0.49 to 0.44), a small file is searched 12 to 35 ms slower (the query's vector), MDN faster (408 to 156 ms), and the file is 9 to 16% larger.
- A contextual model builds on every core, the longest windows first, and keeps the windows it computed in a log in the cache folder, so an interrupted build goes on where it stopped and a rebuild computes only new windows. `kapu` reports the progress. A window that cannot be computed is warning W009, not a failed build.
- Ranking switches for measurement, off by default: passage BM25 with heading prefix, room-level fusion and vector prefix (D168), pseudo-relevance feedback (D169).

### Fixed
- Importing a folder no longer stops at a long file name or a control character in a file (F1, F2).

## [0.3.0] - 2026-10-27

Specification v1.2.2; the file format stays 1.2. Phases 1 to 3 of plan v2: every ranking change was measured as a switch on a fixed dev/test split of the questions, and only what passed the M0 rule became a default (`bench/RESULTS.md` chapter 12, `bench/results/2026-10-27-plan-v2.json`).

### Changed
- The README starts with a 30-second quickstart (install, pack, connect), then a short before-and-after of the measurements; the full benchmark tables are folded, and how it works and the desktop window come before the comparisons.
- `kapu_search` gives one hit per room (D158, J1): the best door stays the hit, and a line under it names the room's other matching sections (`+3 in this room: syntax, examples, ...`). On the test part of seven question sets: room-level recall@10 +0.022 (95% interval +0.010 to +0.035), MRR@10 +0.011, no set worse, search time unchanged. The `kapu bench` hybrid ranking collapses the same way, and `kapu search` names up to five other matching sections under a hit (all in its JSON, `also`). The cost: when a whole room outranks its sections, the hit is the room and the answering section is only named under it (A57).
- `kapu_ask` opens the start of the best door of each of the first 5 rooms, the budget shared, instead of the best hit up to its whole budget (D159, J12). The expected room is in the reply for 42% instead of 15% of the Rust book questions, 39% instead of 17% on Kubernetes and 22% instead of 12% on MDN, in about 1,130 instead of 1,800 tokens.
- The float windows the vector rescoring keeps in memory are also bounded by their number, 250,000 (D157): on an MDN-sized file the memory bounded by doors alone grew past 20 GB in a long run.

### Added
- One-line installs from the release archives, no Rust or Python needed: `install.sh` (Linux x64, macOS arm64) and `install.ps1` (Windows x64) download the archive of the latest release, check its SHA-256 and put `kapu`, `kapu-mcp` and `kapu-gui` with the embedding model under the user's folder. The release workflow writes a `.sha256` file next to each archive, and the archives include `examples/kapu-docs.kapu`.
- The specification in English (`SPEC-KAPU.md`); the Hungarian original is `SPEC-KAPU.hu.md`. The translation also fixed one cross-reference (the standard guide is FR-17, not FR-15). The example `examples/kapu-spec.kapu` is now the English text.
- `docs/assets/social-preview.png`, the repository's preview image for links (`tools/make_social.py`).
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
