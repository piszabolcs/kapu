# KAPU measurements

English summary of the measurements. Raw numbers: [results/2026-10-23-english-corpus.json](results/2026-10-23-english-corpus.json). Chapter 8 is a pilot on public documentation, with its own results file; chapter 9 measures it again with file format 1.1. The [results page](../docs/results/index.html) shows the pilot and format 1.1 with charts.

- **Date:** 2026-10-23
- **Version:** tools 0.1.0 with specification v1.0.2
- **Machine:** Windows 11 Pro, release build (`lto`, `codegen-units = 1`)
- **Vector model:** `potion-multilingual-128M`, int8 (124 MB)

## Summary

| What | Result |
|---|---|
| Knowledge in the test file | 2.35 million characters, 573,733 tokens of Markdown, 244 rooms |
| Live run: Claude agents that could only use the KAPU tools | 10 of 10 questions answered correctly |
| Tokens an agent read per question in the live run (median) | 842 (1.5 tool calls) |
| Map call, the agent's first look at the file | 220 tokens (777 on the first call of a session, with the guide) |
| Search with 5 hits and an excerpt line under each (median) | 333 tokens, 280 ms |
| Retrieval, 77 English questions, hybrid search | recall@5 0.84, recall@10 0.90, MRR@10 0.67 |
| Build with the vector model / full check with rebuild | 4.6 s / 3.3 s; the second build is byte-identical |

An agent never reads the `.kapu` file itself: 33.0 million characters, 9.8 million tokens, 90% of it the search index that the tools read. The fair comparison is with the knowledge itself, the 573,733 tokens of Markdown, which do not fit into any current context window. In the live run an agent found its answer after reading a median of 842 tokens, about 1/680 of the knowledge.

## 1. Test corpus

A local corpus, not published, because most of it is third-party text:

- **216 rooms:** the Claude skills installed on the author's machine. These are Anthropic's skills, the superpowers, Figma and claude-md-management plugins, and the author's own skills, each as a room, with its reference documents as separate rooms. `tools/collect_skills.py` collects them, taking the newest version of each plugin.
- **28 rooms:** modern frontend sample code (CSS, the web platform, React 19, Tailwind 4, accessibility, performance). An agent wrote them, checking browser support against MDN, web.dev and caniuse.

Hungarian rooms were left out, so this measures an English knowledge file with English questions.

| Part of the file | Characters | Tokens (o200k_base) |
|---|---:|---:|
| Body: the Markdown knowledge | 2,350,156 | 573,733 |
| Map, descriptions, graph, positions | 1,056,712 | 347,852 |
| Search index | 29,569,515 | 8,887,658 |
| Whole `.kapu` file | 32,976,901 | 9,809,442 |

Build: 244 rooms, 1,916 doors (rooms and their `##` sections), 10 groups, 4.6 s with the vector model. A second build gives the same bytes, the full check (fingerprints plus a rebuild comparison) passes in 3.3 s, and there are no warnings.

## 2. What an agent pays

Measured through the official Python MCP client in one session (`tools/mcp_probe.py`):

| Call | Tokens returned |
|---|---:|
| `kapu_map`, first call of a session (guide, totals line, 10 groups, content-scan notes) | 777 |
| `kapu_map`, later calls (totals line and 10 groups) | 220 |
| `kapu_map group=...`, one group's rooms | 399 - 2,005 (all 10 groups: 9,698) |
| `kapu_map room=...`, one room's sections | 760 (example) |
| `kapu_search`, 5 hits with excerpts (median / p90) | 333 / 382 |
| `kapu_open`, one door (median / p90) | 1,655 / 5,401 |
| `kapu_describe`, one door (median) | 53 |

## 3. Retrieval quality

There are 77 English questions:

- 49 point at skill rooms and 28 at frontend rooms.
- 18 of them point at one `##` section, not a whole room.

They are phrased as a developer's need, and where a paraphrase works they avoid the room's title and the API's name. Their authors knew the content; see Limitations.

- **Strict:** the expected door is in the top k.
- **Lenient:** the expected door or its room is in the top k.

| Mode | recall@5 | recall@10 | MRR@10 | lenient recall@10 |
|---|---:|---:|---:|---:|
| BM25 (keywords, Snowball stemming) | 0.83 | 0.90 | 0.68 | 0.95 |
| Vector (static embeddings) | 0.64 | 0.73 | 0.47 | 0.79 |
| Hybrid (reciprocal rank fusion) | 0.84 | 0.90 | 0.67 | 0.94 |

| Questions | n | Hybrid recall@5 | Hybrid recall@10 | Hybrid MRR@10 |
|---|---:|---:|---:|---:|
| Skill rooms | 49 | 0.80 | 0.86 | 0.65 |
| Frontend rooms | 28 | 0.93 | 0.96 | 0.71 |
| Whole room expected | 59 | 0.92 | 0.93 | 0.76 |
| One section expected | 18 | 0.61 | 0.78 | 0.40 |

On English text with English questions, keyword search alone is already strong, and the hybrid ranking keeps that level. The vector model matters when the question and the text are in different languages. That is a separate track: Hungarian questions over English text reach a hybrid recall@10 of 0.45.

## 4. MCP server

| Tool | Median | p90 | Notes |
|---|---:|---:|---|
| initialize | 31 ms | | |
| `kapu_map` | 80 ms | 83 ms | the whole 33 MB file is read and checked on every call |
| `kapu_search` (k = 5, with excerpts) | 280 ms | 293 ms | the first search of a session is 1.5 s, while the vector model loads |
| `kapu_open` | 71 ms | 91 ms | |
| `kapu_describe` | 70 ms | 91 ms | |

All five tools carry MCP annotations: the four read tools are `readOnlyHint`, and `kapu_write` is `destructiveHint`. Errors name the next step, for example: `E009: unknown door ... (next: take door ids from kapu_map door lines or from kapu_search)`.

## 5. Live evaluation

The evaluation followed the format of the mcp-builder skill: 10 questions, each with one verifiable answer. The topics and every call are in the results JSON.

- **Agents:** five Claude Sonnet agents, two questions each.
- **Tool access:** only the four read tools, through `tools/kapu_mcp_call.py`, which speaks real MCP to `kapu-mcp` and logs every call. The agents were not allowed to read the file.

| # | Topic | Correct | Calls | Tools | Tokens read |
|---:|---|:---:|---:|---|---:|
| 1 | design reference: recommended button colour | yes | 3 | search, search, open | 1,785 |
| 2 | debugging: phase before any fix | yes | 2 | search, open | 2,714 |
| 3 | spreadsheet formula check script | yes | 2 | search, open | 907 |
| 4 | MCP pagination default | yes | 3 | search, map room, open | 1,280 |
| 5 | README badge limit | yes | 3 | search, open, search | 1,210 |
| 6 | TypeScript MCP SDK package name | yes | 1 | search | 373 |
| 7 | total number of rooms | yes | 1 | map | 777 |
| 8 | CSS anchor positioning: browser version | yes | 1 | search | 355 |
| 9 | `scheduler.yield()`: missing browser | yes | 1 | search | 337 |
| 10 | `field-sizing`: Firefox version | yes | 1 | search | 334 |

Five of the ten answers came from a search excerpt alone, without opening a door. The agents asked for two things:

- an excerpt that shows the answer line more often (questions 4 and 5);
- fewer neighbour lines when a large room is opened.

## 6. Changes made from these measurements

An earlier run on a mixed English and Hungarian corpus led to specification v1.0.2:

| Change | Before | After |
|---|---|---|
| Group names left out stop words (D121) | "the, and, css" | "css, rem, element" |
| Content-scan network rule matches whole commands only (D122) | 83 false warnings, first map 2,971 tokens | 4 real warnings, first map 777 tokens |
| In-page anchors link to their own section (D123) | 274 false "missing door" warnings | 0 warnings, 113 new link edges |
| An excerpt line under each search hit (D124) | median 2,238 tokens per live question | median 842 (different corpus, 8 of 10 questions the same) |
| A group lists its rooms; `room=` lists one room's sections (D125) | largest group list 15,756 tokens | 2,005 tokens |
| At most 8 neighbours per opened door (D126) | up to 31 neighbour lines | 8 lines and a count |
| Totals line on the map (D127) | the room count took two calls | one call |

## 7. Limitations

- The questions were written by people (and an agent) who had seen the content. This is a first measurement, not a benchmark; an independent question set is still needed.
- No baselines yet: full-context reading where it fits, chunk-based RAG, `grep`-style agentic file search and `llms.txt`.
- One machine, one vector model, one agent model (Claude Sonnet), ten live questions.
- The wrapper of the live run starts a new server for every call, so the guide and the content-scan notes appear more often than in a real session. Token counts are therefore slightly high.
- The runs used specification v1.0.2, whose tool output still had a few Hungarian labels ("szomszédok", "alajtó"); since v1.0.3 all agent-facing output is English.

## 8. Pilot on public documentation

This is a first run of the study design. Raw numbers are in [results/2026-10-23-study-pilot.json](results/2026-10-23-study-pilot.json).

**Corpora:**
- S: The Rust Programming Language, 0.32M tokens;
- M: the Kubernetes docs, 3.9M tokens;
- L: MDN Web Docs, 13.0M tokens.

**Questions:** real Stack Overflow questions whose accepted answer links exactly one page of the corpus. There are 68, 201 and 395 of them.

**Run setup:** Claude Sonnet agents, Sonnet and Haiku judges, one run.

### Scale

| | S | M | L |
|---|---:|---:|---:|
| Rooms / doors / groups | 111 / 231 / 7 | 1,722 / 6,370 / 104 | 14,719 / 84,910 / 99 |
| Build, peak memory | 3.4 s, 0.7 GB | 24 s, 1.4 GB | 190 s, 4.0 GB * |
| File size | 7.6 MB | 117 MB | 542 MB * |
| Search response (tokens, ms) | 249, 58 | 265, 666 | 213, 3,192 * |
| Map (tokens) | 151 | 1,948 | 1,919 * |
| Largest group list (tokens) | 804 | 6,957 | 30,118 * |

\* L does not build with the released tools: one search-index line of a common word exceeds the 1 MiB line limit. The L column comes from an experimental build with a 64 MiB limit.

What this shows:
- A search response stays around 200-270 tokens at every size.
- The map and the latency grow with the file, because the server reads the whole file on every call.
- These are open items for the next specification version.

### Retrieval without an agent

The table gives room-level recall@10. A hit is any door or chunk of the gold page in the top 10 units.

| | KAPU hybrid | KAPU BM25 | KAPU vector | 512-token chunks, BM25 | 512-token chunks, vector |
|---|---:|---:|---:|---:|---:|
| S (68) | 0.46 | 0.44 | 0.32 | 0.49 | 0.43 |
| M (201) | 0.44 | 0.45 | 0.28 | 0.45 | 0.32 |
| L (395) | 0.28 | 0.28 | 0.15 | 0.30 | 0.19 |

- KAPU's hybrid search is on par with BM25 over chunks.
- Its vector search is weaker than chunk vectors, because one static vector per long door blurs the content.

### Agents on S, 30 questions

| System | Correct (Sonnet / Haiku judge) | Tokens read per question (median) |
|---|---:|---:|
| Closed book, no tools | 30 / 28 | 0 |
| Agentic chunk RAG | 30 / 27 | 4,150 |
| grep/read on the Markdown folder | 29 / 28 | 1,290 |
| llms.txt | 30 / 29 | 8,739 |
| KAPU | 30 / 29 | 3,983 |

The model answers these popular questions from memory, so accuracy cannot separate the systems. On a small, well-known corpus, grep reads the fewest tokens.

### Same content as KAPU, one Markdown file and a PDF

This test uses the PixelLab knowledge file, content the model does not know: 0.21M tokens and 12 beginner requests. The answers were graded blind against the documentation.

| Format | Correct | Supported claims | Tokens read per request (median) |
|---|---:|---:|---:|
| KAPU | 12/12 | 122/123 | 2,624 |
| One `.md` file, with search and line reads | 12/12 | 134/138 | 8,280 |
| PDF (219 pages), with search and page reads | 11/12 | 146/151 | 8,207 |
| Whole `.md` pasted into the conversation | - | - | 206,899 |

Accuracy is the same. KAPU read about a third of the tokens of the searchable file formats, and about 1/79 of pasting the whole file.

### After specification v1.0.4

A first set of fixes followed the pilot (D131-D138, specification chapter 17). The most visible ones:
- long doors open in parts of at most 2,000 tokens;
- at most 10 content-scan lines on the map;
- the content language on the totals line;
- an in-memory file and vector cache in the server.

Measured again with the same corpora and questions:

| | Before | After |
|---|---:|---:|
| KAPU agents on S, tokens read per question (median), correct | 3,983, 30/30 | 2,331, 30/30 |
| KAPU agents on S, tokens read for all 30 questions | 152,151 | 74,916 |
| First map call on M / L (tokens) | 4,989 / 4,139 | 2,318 / 2,303 |
| Search on M / L (ms, median) | 666 / 3,192 | 657 / 3,172 |

The PixelLab run was repeated with the same prompt for all three formats:

| Format | Correct | Supported claims | Tokens read per request (median) |
|---|---:|---:|---:|
| KAPU | 12/12 | 153/157 | 4,104 |
| One `.md` file | 12/12 | 140/145 | 8,280 |
| PDF | 11/12 | 147/151 | 8,207 |

- Splitting long doors halved what the agents read on the Rust book, at the same accuracy.
- The cache did not make search faster. The time goes to parsing the sections three times per search and to a linear scan of the search index, not to reading the file. That is the next fix.

## 9. File format 1.1 (specification v1.1)

Three changes after the pilot (D139-D141, specification chapters 18 and 19), measured on 2026-10-24 with the same corpora and questions:
- **Compact index (format 1.1).** An `#INDEX` posting names a door by the gap between line numbers in `#DOCS` instead of its id. Files are about 4.5 times smaller, and the longest index line stays under the 1 MiB line limit, so MDN builds with the released tool.
- **One parse per call.** A call parses and checks the file once (it was three times per search), finds a term's index line by binary search, and the MCP server keeps a checked copy of an unchanged file.
- **Window vectors.** Search by meaning uses one vector per 512-token window of a door's text instead of one vector for its first 512 tokens; the best window counts. `kapu import` and `kapu build` compute them, so the first search does not wait.

| | Rust book (S) | Kubernetes (M) | MDN (L) |
|---|---:|---:|---:|
| File size (MB), v1.0.4 → v1.1 | 7.6 → 2.2 | 117.4 → 26.2 | 542.2* → 128.3 |
| Builds with the released tool | yes | yes | no (E005) → yes |
| Import with the vector model (s) | 3.3 → 4.6 | 23.9 → 38.0 | 357.8* → 240.3 |
| Peak memory of the import (GB) | 0.70 → 0.69 | 1.35 → 1.03 | 3.87* → 2.68 |
| Search over MCP (ms, median) | 56 → 12.8 | 657 → 19.2 | 3,172 → 158.2 |
| Opening a door over MCP (ms, median) | 17 → 2.4 | 216 → 4.9 | 1,040 → 44.1 |
| Map over MCP (ms, median) | 18 → 3.0 | 253 → 46.6 | 1,400 → 390.9 |
| First search of a session (s) | 1.1 → 1.1 | 1.7 → 1.0 | 4.3 → 1.3 |
| `kapu bench` run time (s) | – → 1 | 187 → 8 | 1,774 → 127 |

\* MDN with v1.0.4: an experimental build with a 64 MiB line limit, and its import time was not measured on an idle machine. The import now includes the window vectors.

Retrieval on the same Stack Overflow questions, room-level recall@10 (MRR@10 in brackets):

| | Rust book, 68 questions | Kubernetes, 201 | MDN, 395 |
|---|---:|---:|---:|
| KAPU hybrid, v1.0.4 | 0.456 (0.207) | 0.438 (0.227) | 0.281 (0.131) |
| **KAPU hybrid, v1.1** | **0.485 (0.281)** | **0.458 (0.245)** | **0.271 (0.128)** |
| KAPU vectors only, v1.0.4 → v1.1 | 0.324 → 0.426 | 0.279 → 0.318 | 0.154 → 0.142 |
| KAPU BM25 only | 0.441 | 0.448 | 0.284 |
| Chunks (512 tokens, 64 overlap), BM25 | 0.485 | 0.448 | 0.299 |
| Chunks, vectors | 0.426 | 0.318 | 0.190 |

- Window vectors bring the KAPU vector ranking to the chunk baseline on the Rust book and Kubernetes, and the hybrid ranking to or above chunked BM25 there.
- On MDN they do not help: its 84,910 doors are mostly short sections, and a room and its own sections now match with the same window and take several of the first 10 places. Merging a room with its sections in the vector ranking is the next thing to try.
- The BM25 ranking and every result of format 1.0 files are unchanged: the summing order of the scores is the same.

## 10. Reproduce

```bash
python tools/collect_skills.py bench/korpusz/rooms           # the installed skills as rooms (local only)
kapu import <rooms folder> corpus.kapu --model models/potion-multilingual-128M-i8
kapu validate corpus.kapu --full
kapu bench corpus.kapu questions.jsonl --json                # recall@5/10, MRR@10 per mode
python tools/mcp_probe.py corpus.kapu questions.jsonl        # MCP latency and response sizes
python tools/kapu_mcp_call.py call kapu_search '{"file": "...", "query": "..."}'
```

Chapters 8 and 9 (public documentation): `tools/study/prepare_corpus.py` and `so_questions.py` prepare the corpora and questions, `scale_probe.py` measures a size, `chunk_baseline.py` the chunk baselines, `collect_results.py` writes `results/2026-10-23-study-pilot.json`, and `results_page.py` turns that into the results page.
