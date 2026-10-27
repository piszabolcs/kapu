# KAPU measurements

English summary of the measurements. Raw numbers: [results/2026-10-23-english-corpus.json](results/2026-10-23-english-corpus.json). Chapter 8 is a pilot on public documentation, with its own results file; chapter 9 measures it again with file format 1.1, chapter 10 with format 1.2 ([results/2026-10-26-format-1.2.json](results/2026-10-26-format-1.2.json)), chapter 11 compares native skills, a KAPU file and a Markdown folder at finding skills ([results/2026-10-26-skill-finding.json](results/2026-10-26-skill-finding.json), and after the catalog of tools 0.2.1 [results/2026-10-26-skill-finding-0.2.1.json](results/2026-10-26-skill-finding-0.2.1.json)), and chapter 12 measures every ranking change of plan v2 on a fixed dev/test split, against fair chunk baselines and with agents on a new corpus of PLC manuals ([results/2026-10-27-plan-v2.json](results/2026-10-27-plan-v2.json)). The [results page](../docs/results/index.html) shows the pilot, formats 1.1 and 1.2 and the skill finding with charts and tables; printed as PDF in [English](../docs/results/kapu-measurements-en.pdf) and [Hungarian](../docs/results/kapu-measurements-hu.pdf) (`tools/study/results_pdf.py`).

- **Date:** 2026-10-23
- **Version:** tools 0.1.0 with specification v1.0.2 (chapters 1-8); chapters 9 and 10 name their own
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
| `kapu bench` run time (s) | - → 1 | 187 → 8 | 1,774 → 127 |

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

## 10. File format 1.2 (specification v1.2)

Measured on 2026-10-26 with tools 0.2.0, the same corpora, questions and machine as chapter 9, after the changes of specification chapter 20 (D142-D150):
- **Search vectors in the file.** A `#VEC` section holds one sign bit per component of every window vector (43 base64 characters per window). The sign bits pick the 200 best candidates, and the model's float window vectors, computed for those at query time and kept in memory, order them.
- **Reply budgets.** Every MCP reply starts with its own price (`reply: N tokens`), every reading tool takes `max_tokens` (map 1,000, search 600, the others 2,000 by default), and `kapu_ask` searches and opens in one call.
- **Kinds and skills.** A room can name its kind (`kind: skill`, `project`, `pattern`, `note`); `kapu import --skills` imports a folder of agent skills.

| | Rust book (S) | Kubernetes (M) | MDN (L) | 244 skills and notes, English |
|---|---:|---:|---:|---:|
| File size, v1.1 → v1.2 (MB) | 2.31 → 2.35 | 27.5 → 28.1 | 134.6 → 139.0 | 5.83 → 5.95 |
| Growth from `#VEC` | 1.7% | 2.2% | 3.3% | 2.0% |
| Search over MCP (ms, median) | 32 | 50 | 282 | 26 |
| Opening a door over MCP (ms, median) | 3.6 | 7.9 | 48.0 | 2.9 |
| Map reply, first / next (tokens) | 328 / 159 | 967 / 983 | 969 / 980 | 600 / 224 |
| Search reply, 10 hits with excerpts (tokens, median / max) | 527 / 580 | 528 / 586 | 500 / 583 | 547 / 585 |
| `kapu_ask` reply (tokens, median) | 1,657 | 1,795 | 1,664 | 1,786 |
| `kapu_ask` opens the expected room in its one reply | 21% | 20% | 12% | 83% |

On a generated file of 3,000 rooms (12,024 doors) the first map reply is 306 tokens and opening a door takes 7.6 ms (median). The six tools' descriptions and input schemas are 797 tokens (names, descriptions and schemas as compact JSON, o200k_base); the five tools of format 1.1 were 1,174.

Retrieval, room-level recall@10 of the hybrid ranking (MRR@10 in brackets):

| | Rust book, 68 questions | Kubernetes, 201 | MDN, 395 | Skills, 77 English | Skills, 60 Hungarian | Skills, the 60 translated |
|---|---:|---:|---:|---:|---:|---:|
| v1.1, float vectors from the cache | 0.485 (0.281) | 0.458 (0.245) | 0.271 (0.128) | 0.948 (0.851) | 0.633 (0.464) | - |
| v1.2, sign bits alone | 0.500 (0.288) | 0.458 (0.263) | 0.246 (0.116) | 0.922 (0.783) | 0.583 (0.442) | 0.833 (0.735) |
| **v1.2, sign bits and float rescoring** | **0.485 (0.281)** | **0.458 (0.243)** | **0.278 (0.131)** | **0.961 (0.841)** | **0.633 (0.453)** | **0.833 (0.750)** |

- The sign bits alone were as good as the float vectors on the Rust book and Kubernetes, the corpora they were first tried on, but lost on MDN, on the skills corpus and most of all with Hungarian questions on English text (strict hybrid recall@10 0.55 to 0.45). With the rescoring they are back at the float level everywhere; on a file of at most 200 doors the ranking is exactly the float one.
- The price of the rescoring is search time: Kubernetes 33 to 50 ms, MDN 198 to 282 ms (median, sign bits alone versus rescored).
- Hungarian questions on English text: translated into English, as an agent does after reading `lang=en` on the map, strict hybrid recall@10 is 0.78 and room-level 0.83; without translation 0.55 and 0.63. The translations are in `tools/study/translate_hu.py`; Claude wrote them without looking at the expected files.
- `kapu_ask` answers in one call where retrieval is good (the skills corpus, 83%); on the Stack Overflow questions, where even recall@10 is 0.28-0.49, one call is rarely enough.
- The file grows by more than 3% only on MDN, with its 84,910 mostly short doors.

## 11. Finding skills: native skills, one KAPU file, a Markdown folder

Does an agent find the skills a task needs? Measured on 2026-10-26 with Claude Opus through `claude -p` (Claude Code 2.1.310), tools 0.2.0 ([results/2026-10-26-skill-finding.json](results/2026-10-26-skill-finding.json); the user's design, D153).

- **Skills:** every real skill on the test machine, 91 (Claude desktop skills, Claude Code plugins, skills synced from claude.ai and the user's own), and a set of 45: the 38 skills some task needs and 7 random others.
- **Tasks:** 12 realistic development tasks, for example a landing page with a pricing section and the author footer, an MCP server packaged for Claude Desktop, a Claude Code plugin with a command, a hook and settings, a flaky API, Excel, PowerPoint and PDF from a CSV, a Figma design system. Each task has 2 to 4 skills that clearly apply (39 in all), fixed before any run, and a list of skills that are reasonable but not required.
- **Forms:** the skills as native Claude Code skills in `.claude/skills/` (only the Skill tool allowed), as one `.kapu` file made with `kapu import --skills` (only the kapu MCP tools), and as a folder of skill folders (only Read, Glob and Grep). Every run is isolated: no user settings, no other MCP servers.
- **Prompt:** it points at the skills, as a user would: "load every relevant skill with the Skill tool", "find and open every relevant skill in the KAPU file", "find and read every relevant skill in this folder", then list them and stop without doing the task.
- **Found:** the Skill tool loaded it, one of its files was read, or one of its rooms was opened. **Wrong:** loaded although neither required nor reasonable. **Tokens per task:** every input token of the run, each turn counted. **Fixed cost:** the first turn of a session that needs no skill.

| Skills | Form | Relevant skills found | Tasks with all found | Wrong loaded | Tokens per task (median) | Calls per task | Fixed cost per session |
|---:|---|---:|---:|---:|---:|---:|---:|
| 45 | native skills | 38/39 | 11/12 | 0 | 42,574 | 4 | 12,652 |
| 45 | KAPU file | 37/39 | 10/12 | 0 | 77,215 | 7.5 | 4,774 |
| 45 | Markdown folder | 39/39 | 12/12 | 14 | 93,176 | 10.5 | 5,430 |
| 91 | native skills | 39/39 | 12/12 | 0 | 44,953 | 4 | 14,081 |
| 91 | KAPU file | 38/39 | 11/12 | 0 | 88,418 | 9 | 4,774 |
| 91 | Markdown folder | 39/39 | 12/12 | 12 | 107,726 | 10.5 | 5,430 |

- **At this size the native skills win the task itself:** all relevant skills with 91 skills, nothing wrong, the fewest tokens and calls. Claude Code lists every skill with its description in the system prompt, so the model picks from the list and loads each skill with one call.
- **The KAPU file found almost as many and loaded nothing wrong, at about twice the tokens per task.** The agents looked at the map more than once, searched, and opened whole skills over 7 to 9 calls. Its misses: `frontend-expert` for the Figma task (the native skills missed it too), `skill-development` once, and with 91 skills `tanulo-mod`, a skill written in Hungarian, for an English task (the cross-language weakness of chapter 10).
- **The Markdown folder found everything but read 12 to 14 skills that the tasks did not need**, and used the most tokens.
- **KAPU's advantage is the fixed cost:** 4,774 tokens per session whatever the number of skills, against 12,652 and 14,081 for the native list. A session that needs no skill saves about 9,300 tokens with KAPU; a task that needs skills costs about 43,000 more. KAPU pays off when skills are needed in fewer than about one session in five, or when there are many more skills than here.
- **Not measured:** 250 skills (the machine has 91), and a prompt that does not mention skills, where the agent has to think of them itself. Every task ran once per form and size.

### After the catalog: tools 0.2.1 (D154, D155)

The KAPU agents above looked at the map two to four times and opened the skills one by one. Two changes: `kapu_map kind=skill` lists every skill with its price and the start of its description in one reply (91 skills: 3,282 tokens; a skill's reference rooms are folded into it), and several doors in one `kapu_open` share the budget. Only the KAPU runs were repeated, with the same tasks, model and skill sets ([results/2026-10-26-skill-finding-0.2.1.json](results/2026-10-26-skill-finding-0.2.1.json)).

| Skills | Form | Relevant skills found | Wrong loaded | Tokens per task | $ per task | Calls per task |
|---:|---|---:|---:|---:|---:|---:|
| 45 | native skills | 38/39 | 0 | 42,574 | 0.224 | 4 |
| 45 | KAPU file, 0.2.0 | 37/39 | 0 | 77,215 | 0.229 | 7.5 |
| 45 | KAPU file, 0.2.1 | 38/39 | 3 | 58,266 | 0.131 | 5.5 |
| 91 | native skills | 39/39 | 0 | 44,953 | 0.236 | 4 |
| 91 | KAPU file, 0.2.0 | 38/39 | 0 | 88,418 | 0.255 | 9 |
| 91 | KAPU file, 0.2.1 | 38/39 | 3 | 48,973 | 0.099 | 5 |
| 91 | Markdown folder | 39/39 | 12 | 107,726 | 0.336 | 10.5 |

- **The same finding rate in fewer calls:** with 91 skills 5 calls instead of 9 and 45% fewer input tokens, close to the native skills. The skill written in Hungarian (`tanulo-mod`) is now found from the catalog; `frontend-expert` is still missed in the Figma task (the native skills missed it too with 45 skills); in the learning-API task the agent opened three skills the task did not need (`signature`, `app-signature`, `verification-before-completion`).
- **Cost per task**, as Claude Code reports it at API list prices: the KAPU file now costs less than the native skills. The usage of one task (t03, 91 skills) shows why: the native run wrote 24,280 tokens to the prompt cache and read 15,767, the KAPU run wrote 6,465 and read 66,022; a cache write costs twice the input price, a read a tenth. The Skill tool loads a whole skill (`mcp-builder`: 6,901 tokens), `kapu_open` the first part of each, up to 2,000 tokens, with the rest on request.
- The six tools' descriptions and schemas are 798 tokens (K-01: at most 800).

## 12. Plan v2, phases 1 to 3: ablation, one fix that holds, honest baselines

Measured on 2026-10-27 with tools 0.3.0 (specification v1.2.2, file format 1.2), on one Windows 11 machine, after the user's plan v2 ([results/2026-10-27-plan-v2.json](results/2026-10-27-plan-v2.json); per-question ranks in [results/2026-10-27-plan-v2-ranks.json](results/2026-10-27-plan-v2-ranks.json), by question hash).

**How it was measured.**
- **Every change is a switch** (`RankOptions`; `kapu bench --config`, `kapu-mcp --rank`), off by default. Off, the ranking is bit for bit that of 0.2.1 (`tests/vectors/rankings-0.2.1.tsv`).
- **A fixed split** (`bench/splits/`): each question set is cut once, by a hash of the question, into a dev part (40%) and a test part (60%). The variant of a fix is chosen on the dev part, and only the test part is reported.
- **Seven question sets:** the Stack Overflow questions of chapter 8 on the Rust book (41 test questions), Kubernetes (122) and MDN (236); the skills corpus of chapter 10 in English (46), translated from Hungarian (42) and in Hungarian (42); and a new corpus of PLC manuals (48, below).
- **The M0 rule**, fixed before the runs: a fix is kept when, on the test part with all sets pooled, room-level recall@10 or MRR@10 improves with a 95% paired bootstrap interval above zero; no set loses more than 0.01 in either metric; and search is at most 1.5 times slower. The per-set constraint is also applied on the dev part when the variant is chosen. That step was added during the analysis: the first report chose by the dev gain alone, so the collapse group's pick (`collapse-1-beta-0.1`) failed the constraint on the test part while plain `collapse-1` met it (D158).
- **The fair baseline** (M2): chunks of 512 tokens with one hit per page (`dedup`). The README compared KAPU's room level with the undeduplicated chunk column until 0.2.1; that was not a fair comparison.

**The new corpus.** The public PLC documentation of six vendors, 623 pages, 1.01 million words, local only (`plc.kapu`, 12 MB):
- Beckhoff TwinCAT 3 user interface
- Bosch Rexroth ctrlX PLC
- Mitsubishi iQ-R, iQ-F and GX Works3
- Omron Sysmac
- Siemens S7-1200/1500, SCL and STL
- Emerson PACSystems

The Nolek TestIT wiki answers scripted requests with a bot check, so it is not in it. The questions are 72 answerable, 24 whose answer is not in the corpus, and 12 that need two pages. An agent wrote them while reading the pages; every quoted piece of evidence is verbatim in its page. Because the writer saw the pages, search is easy on this set; its use is the agent measurement below.

### Ranking fixes

**Phase 2, against 0.2.1 (m0)**

| Fix | Variants | Chosen on dev | Test Δ room recall@10 [95%] | Test Δ room MRR@10 [95%] | Worst set | Kept |
|---|---:|---|---:|---:|---:|---|
| stemming | 3 | stem-en | +0.002 [-0.012, +0.016] | -0.005 [-0.014, +0.004] | -0.021 | no |
| avgdl-split | 1 | avgdl-split | +0.021 [+0.005, +0.036] | +0.016 [+0.003, +0.028] | -0.022 | no |
| parent | 3 | parent-0.1 | -0.004 [-0.019, +0.012] | -0.009 [-0.018, -0.000] | -0.030 | no |
| collapse | 4 | collapse-1 | +0.022 [+0.010, +0.035] | +0.011 [+0.008, +0.014] | +0.000 | yes |
| rescore-bm25 | 1 | rescore-bm25-50 | +0.002 [-0.005, +0.010] | +0.001 [-0.002, +0.005] | -0.021 | no |
| min-door | 1 | min-door-40 | -0.002 [-0.009, +0.004] | +0.001 [-0.001, +0.004] | -0.009 | no |
| adaptive | 2 | adaptive-1.5 | +0.002 [-0.005, +0.009] | +0.000 [-0.003, +0.003] | -0.021 | no |
| rrf | 18 | rrf-b1-v0.7-k60 | +0.009 [-0.005, +0.024] | +0.006 [+0.001, +0.012] | -0.048 | no |
| convex | 3 | convex-b1-v1 | +0.002 [-0.017, +0.021] | +0.004 [-0.008, +0.016] | -0.024 | no |

**On top of the collapse (m1)**

| Fix | Variants | Chosen on dev | Test Δ room recall@10 [95%] | Test Δ room MRR@10 [95%] | Worst set | Kept |
|---|---:|---|---:|---:|---:|---|
| parent | 2 | parent-0.25 | -0.007 [-0.024, +0.010] | -0.010 [-0.022, +0.002] | -0.031 | no |
| avgdl-split | 1 | avgdl-split | +0.010 [-0.005, +0.026] | +0.011 [-0.000, +0.024] | -0.024 | no |
| rescore-bm25 | 1 | rescore-bm25-50 | +0.005 [-0.002, +0.014] | +0.002 [-0.001, +0.006] | -0.002 | no |
| min-door | 1 | min-door-40 | +0.005 [+0.000, +0.012] | +0.002 [-0.001, +0.005] | +0.000 | no |
| adaptive | 1 | adaptive-2 | +0.007 [+0.000, +0.016] | +0.004 [-0.001, +0.009] | -0.001 | no |
| rrf | 3 | rrf-b1.5-v1-k20 | +0.028 [+0.009, +0.047] | +0.015 [+0.004, +0.025] | -0.024 | no |
| graph | 6 | graph-sim-0.05 | +0.010 [-0.004, +0.024] | -0.004 [-0.017, +0.008] | -0.059 | no |

**J4b with J4c on top of the collapse, six sets without MDN (m2)**

| Fix | Variants | Chosen on dev | Test Δ room recall@10 [95%] | Test Δ room MRR@10 [95%] | Worst set | Kept |
|---|---:|---|---:|---:|---:|---|
| rrf | 3 | rrf-b1.5-v1-k20-adaptive-3 | +0.032 [+0.006, +0.059] | +0.025 [+0.011, +0.041] | -0.024 | no |

- **One fix holds: one hit per room (J1, D158).** It is the default in 0.3.0. On the test part, pooled: recall@10 +0.022, MRR@10 +0.011, both intervals above zero, and no set worse. Search time is unchanged (`--timing`, each configuration in its own process: Kubernetes p50 27.3 → 27.9 ms, MDN 313.5 → 313.8 ms).
- **Three changes come close but break the per-set constraint by one question on a small set:**
  - separate average lengths for rooms and sections (J2a), +0.021 and +0.016;
  - BM25 weighted 1.5 with k 20 on top of the collapse (J4b), +0.028 and +0.015;
  - the same with the query-dependent weight (J4b with J4c), +0.032 and +0.025.

  Each loses one question of 42 to 48 on a skills or PLC set, 0.021 to 0.024, while the rule allows 0.01. They stay switches, off. Whether the rule should allow one question on a small set is the user's decision, not one to take after seeing the numbers.
- **No gain from the others:** stemming with one stemmer or none, a share of the room's score for its sections (J2b), the first BM25 hits in the vector rescoring (J4a), min-max fusion (J4d), sections under 40 tokens only through their room (J10), and title weights 1, 2 and 5 instead of 3 (built with the hidden `--title-weight`, BM25 only, pooled differences between -0.002 and +0.000).
- **The graph step (J9 variant A, N2b) does not help** on top of the collapse: the best variant on dev (`sim` 0.05) gives recall@10 +0.010 [-0.004, +0.024] and MRR -0.004. Stronger graph weights cost up to 0.058 of MRR. As a ranking signal the graph gives nothing measurable; it is a navigation aid (N13 below).

### Retrieval against the chunk baselines

| Set | Questions | Ranking | recall@1 | recall@10 | recall@100 | MRR@10 | nDCG@10 |
|---|---:|---|---:|---:|---:|---:|---:|
| Rust book | 41 | KAPU 0.2.1 | 0.146 | 0.439 | 0.902 | 0.220 | 0.271 |
| Rust book | 41 | KAPU, collapsed (0.3.0) | 0.146 | 0.512 | 0.902 | 0.252 | 0.314 |
| Rust book | 41 | Chunks, BM25 | 0.146 | 0.512 | 0.902 | 0.255 | 0.315 |
| Rust book | 41 | Chunks, vectors | 0.122 | 0.439 | 0.854 | 0.223 | 0.274 |
| Rust book | 41 | Chunks, hybrid | 0.146 | 0.537 | 0.829 | 0.249 | 0.317 |
| Kubernetes | 122 | KAPU 0.2.1 | 0.139 | 0.418 | 0.713 | 0.238 | 0.282 |
| Kubernetes | 122 | KAPU, collapsed (0.3.0) | 0.139 | 0.451 | 0.713 | 0.253 | 0.302 |
| Kubernetes | 122 | Chunks, BM25 | 0.164 | 0.484 | 0.795 | 0.242 | 0.299 |
| Kubernetes | 122 | Chunks, vectors | 0.066 | 0.328 | 0.639 | 0.145 | 0.189 |
| Kubernetes | 122 | Chunks, hybrid | 0.164 | 0.467 | 0.738 | 0.250 | 0.301 |
| MDN | 236 | KAPU 0.2.1 | 0.076 | 0.271 | 0.525 | 0.127 | 0.161 |
| MDN | 236 | KAPU, collapsed (0.3.0) | 0.076 | 0.288 | 0.525 | 0.135 | 0.171 |
| MDN | 236 | Chunks, BM25 | 0.089 | 0.322 | 0.602 | 0.156 | 0.196 |
| MDN | 236 | Chunks, vectors | 0.064 | 0.203 | 0.517 | 0.095 | 0.120 |
| MDN | 236 | Chunks, hybrid | 0.064 | 0.356 | 0.631 | 0.145 | 0.195 |
| Skills, English | 46 | KAPU 0.2.1 | 0.783 | 0.978 | 1.000 | 0.846 | 0.878 |
| Skills, English | 46 | KAPU, collapsed (0.3.0) | 0.783 | 0.978 | 1.000 | 0.853 | 0.884 |
| Skills, translated | 42 | KAPU 0.2.1 | 0.714 | 0.833 | 0.833 | 0.756 | 0.775 |
| Skills, translated | 42 | KAPU, collapsed (0.3.0) | 0.714 | 0.833 | 0.833 | 0.759 | 0.777 |
| Skills, Hungarian | 42 | KAPU 0.2.1 | 0.405 | 0.619 | 0.762 | 0.487 | 0.520 |
| Skills, Hungarian | 42 | KAPU, collapsed (0.3.0) | 0.405 | 0.643 | 0.762 | 0.492 | 0.529 |
| PLC manuals | 48 | KAPU 0.2.1 | 0.625 | 0.917 | 0.979 | 0.720 | 0.767 |
| PLC manuals | 48 | KAPU, collapsed (0.3.0) | 0.625 | 0.938 | 0.979 | 0.734 | 0.783 |
| PLC manuals | 48 | Chunks, BM25 | 0.625 | 0.958 | 0.979 | 0.741 | 0.794 |
| PLC manuals | 48 | Chunks, vectors | 0.458 | 0.792 | 0.896 | 0.571 | 0.625 |
| PLC manuals | 48 | Chunks, hybrid | 0.583 | 0.938 | 0.979 | 0.686 | 0.746 |

**Why a question is missed (M3), test part**

| Set | Ranking | Missed | Duplication | Fusion | Rescoring | Ranking 11-100 | Vocabulary | Room hit, door missed |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| Rust book | KAPU 0.2.1 | 23 | 3 | 5 | 1 | 10 | 4 | 1 |
| Rust book | KAPU, collapsed (0.3.0) | 20 | 0 | 5 | 1 | 10 | 4 | 8 |
| Kubernetes | KAPU 0.2.1 | 71 | 4 | 11 | 6 | 15 | 35 | 9 |
| Kubernetes | KAPU, collapsed (0.3.0) | 67 | 0 | 11 | 6 | 15 | 35 | 26 |
| MDN | KAPU 0.2.1 | 172 | 4 | 17 | 15 | 24 | 112 | 8 |
| MDN | KAPU, collapsed (0.3.0) | 168 | 0 | 17 | 15 | 24 | 112 | 23 |
| Skills, English | KAPU 0.2.1 | 1 | 0 | 0 | 0 | 1 | 0 | 2 |
| Skills, English | KAPU, collapsed (0.3.0) | 1 | 0 | 0 | 0 | 1 | 0 | 15 |
| Skills, translated | KAPU 0.2.1 | 7 | 0 | 0 | 0 | 0 | 7 | 1 |
| Skills, translated | KAPU, collapsed (0.3.0) | 7 | 0 | 0 | 0 | 0 | 7 | 7 |
| Skills, Hungarian | KAPU 0.2.1 | 16 | 1 | 1 | 0 | 4 | 10 | 2 |
| Skills, Hungarian | KAPU, collapsed (0.3.0) | 15 | 0 | 1 | 0 | 4 | 10 | 7 |
| PLC manuals | KAPU 0.2.1 | 4 | 1 | 1 | 0 | 1 | 1 | 1 |
| PLC manuals | KAPU, collapsed (0.3.0) | 3 | 0 | 1 | 0 | 1 | 1 | 21 |

- **The deduplicated chunk baselines are still better than KAPU, now on fair terms.** Chunk BM25 beats it on Kubernetes (0.484 against 0.451) and MDN (0.322 against 0.288), and the chunk hybrid is ahead everywhere except PLC. KAPU 0.3.0 reaches chunk BM25 on the Rust book (0.512) and the chunk hybrid on PLC (0.938).
- **Ceiling (recall@100):** the Rust book reaches 0.90 at 100 but 0.51 at 10, so its misses are mostly ranking. MDN reaches only 0.53 even at 100, so half of its questions use words its pages do not.
- **Error classes (M3), MDN with the collapse:** 112 of 168 misses are vocabulary (the page is not in any top 100), 24 ranking, 17 fusion, 15 rescoring candidates. Duplication is gone after the collapse. This points at document and query expansion (J7, J8) and smaller vector windows (J3), the next phases of the plan, not at more fusion tuning.

### kapu_ask (J12, D159) and the two-line excerpt (J11)

"Answered" means that a frame of the expected page is among the frames the one reply opens (`ask_eval.py`, test part, default budget 2,000 tokens).

| Set | 0.2.1 (best hit) | 0.3.0 (start of 5 rooms) | Reply tokens, 0.2.1 → 0.3.0 | Tokens per question spent without the page |
|---|---:|---:|---:|---:|
| Rust book | 0.146 | 0.415 | 1,653 → 1,224 | 1,315 → 710 |
| Kubernetes | 0.172 | 0.393 | 1,800 → 1,130 | 1,391 → 678 |
| MDN | 0.114 | 0.216 | 1,614 → 1,124 | 1,331 → 854 |
| Skills, English | 0.826 | 0.957 | 1,788 → 1,145 | 295 → 51 |
| Skills, translated | 0.762 | 0.810 | 1,684 → 1,144 | 384 → 210 |

- **The shared budget wins everywhere.** The paired interval is above zero on every set except the translated one, and the reply is about a third smaller. The start of a room may not hold the answer line, so "answered" is lenient; the agent measurement below checks the answers themselves.
- **The confidence mode was the plan's proposal:** the best hit alone when it is found by both rankings and scores at least 1.3 times the second, otherwise the spread. It is not better on the dev part, and its signal is not calibrated: where it would say `confidence: high`, the expected room is there for only 11% of the Kubernetes and MDN questions, fewer than when it says low (42% and 22%). So 0.3.0 always spreads and prints no confidence line. The mode stays measurable (`kapu-mcp --rank '{"ask": "confidence"}'`).
- **The two-line excerpt (J11) costs hits and shows no gain** that this measurement can see. With it, 5.9 to 7.8 hits fit in the 600-token search reply instead of 7.2 to 9.5, and no answer quality was measured. It stays off (`excerpt_lines`).
- **The collapse in the search reply:** the line naming a room's other matching sections takes tokens, so 7.8 to 9.5 hits fit instead of 9.8 to 10. Every hit is now a different room.

### Search or reading: oracle, KAPU and closed book (N1), and abstention (N4)

On the PLC corpus, with Claude Sonnet through `claude -p` (isolated, no user settings) and Claude Haiku as the judge (`oracle_eval.py`, one run per question):
- 36 answerable questions, 6 per vendor;
- the 24 absent ones.

| | Correct | Partly | Wrong | Said not found | Input tokens (median) | $ (median) |
|---|---:|---:|---:|---:|---:|---:|
| Oracle: the expected page in the prompt | 97% | 3% | 0% | 0% | 5,461 | 0.015 |
| **KAPU tools** | **92%** | 8% | 0% | 0% | 12,892 | 0.019 |
| Closed book: the question alone | 28% | 17% | 8% | 47% | 2,906 | 0.006 |

| Absent questions | Said it is not there | Made an answer up | Input tokens (median) |
|---|---:|---:|---:|
| **KAPU tools** | **96%** | 4% (1 of 24) | 57,346 |
| Closed book | 54% | 46% | 2,895 |

- **Reading, not search, is the small loss here:** `retrieval_gap` (oracle - KAPU) is 0.06, `knowledge_gain` (KAPU - closed book) 0.64. The agent opened the expected room for 94% of the questions. Search is easy on this set, as noted above.
- **Abstention is KAPU's clearest win:** asked about something the manuals do not say, the model alone made an answer up for almost half of the questions, the KAPU agent for one of 24. A "not found" costs more, a median of 5.5 calls and 57,000 tokens, because the agent keeps searching.
- The confidence mode gave the same numbers (92% and 89% correct, 96% abstention), so `confidence: low` did not reduce made-up answers.
- **Limits:** one run per question; the plan asks for at least three (M5). The question writer saw the pages.

### The graph: followed neighbours (N2a) and topic groups (N13)

- **Neighbour lists are rarely followed.** In the logged live runs of chapters 5 and 8 (72 runs, `neighbour_follow.py`, the replies asked again from today's server), 5 of 27 later `kapu_open` calls opened a door from an earlier neighbour list: `link` 2 of 64 shown, `sim` 3 of 64, `term` 0 of 221. A neighbour list costs 70 tokens per opened door on average. This is the data for J16 (phase 4): `term` neighbours can go.

| Set | Rooms | Groups / folders | ARI | NMI (random same sizes) | k-means ARI | sim only | term only | link only | Largest group (rooms / tokens) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Kubernetes | 1,722 | 104 / 73 | 0.159 | 0.416 (0.150) | 0.137 | 0.155 | -0.005 | 0.000 | 166 / 1,509,243 |
| MDN | 14,719 | 99 / 50 | 0.040 | 0.390 (0.029) | 0.032 | 0.041 | -0.001 | -0.000 | 733 / 1,296,318 |

- **The groups carry structure, and the `sim` edges carry it.** Against the second folder level, the Kubernetes groups score ARI 0.159 and NMI 0.416 (random groups of the same sizes 0.150), about k-means on the room vectors (0.137). On MDN they score ARI 0.040 and NMI 0.390, against 0.029 for random groups and 0.032 for k-means.
- Leiden on one edge type at a time: `sim` alone gives what all edges give, `term` alone is at or below zero, and `link` edges are too few to form groups.
- The largest group is 166 rooms (1.5 M tokens) on Kubernetes and 733 rooms (1.3 M tokens) on MDN, which is why a group list does not fit a map reply (G16, J15).

### Found on the way
- **A memory bound (D157).** The float windows the vector rescoring keeps were bounded by doors only (20,000). The sign bits pick long doors with many windows, so on MDN a long run grew past 20 GB. They are now also bounded by windows (250,000, about 260 MB); a `kapu-mcp` server stays near 850 MB over 200 MDN searches.
- **Open (A55):** `kapu bench --config` keeps about 2 MB per run on a large file, so `ablation.py run --chunk 50` runs MDN in parts.
- **Two import problems** showed up while building the PLC corpus:
  - a file name longer than 64 characters gives an invalid room id (E008);
  - a control character left by PDF conversion stops the import (E004).

  The corpus was cleaned first; the import itself is unchanged.

### The tool list

With the new `kapu_search` and `kapu_ask` descriptions the six tools cost 799 tokens (K-01: at most 800); the first wording was 802, and a test now prices the list.

### Not done yet

These are phases 4 to 7 of the plan: J2c, J3, J5, J6, J7, J8, J13 to J16, M4, N3, N5 to N12.

## 13. Reproduce

```bash
python tools/collect_skills.py bench/korpusz/rooms           # the installed skills as rooms (local only)
kapu import <rooms folder> corpus.kapu --model models/potion-multilingual-128M-i8
kapu validate corpus.kapu --full
kapu bench corpus.kapu questions.jsonl --json                # recall@5/10, MRR@10 per mode
python tools/mcp_probe.py corpus.kapu questions.jsonl        # MCP latency and response sizes
python tools/kapu_mcp_call.py call kapu_search '{"file": "...", "query": "..."}'
```

Chapter 12: `tools/study/split_questions.py` (the dev/test split), `ablation.py run` and `report` (the matrices `bench/ablation/m0.json`, `m1.json`, `m2.json`; set `KAPU_MODEL_DIR` for a copied binary, `--chunk 50` for MDN), `chunk_baseline.py` (with `--chunks` in the report), `ask_eval.py` (`bench/ablation/ask-modes*.json`), `title_weight.py`, `neighbour_follow.py`, `group_quality.py`, `oracle_eval.py` (needs a logged-in `claude` CLI), `collect_v2.py` (the results file) and `v2_tables.py` (these tables). Chapter 11: `tools/study/skill_pool.py` collects the skills of the machine, `tools/study/skill_eval.py` prepares the three forms (`prepare`), runs the tasks of `tools/study/skill_tasks.json` (`run`), measures the fixed cost (`baseline`) and writes the results file (`report`); it needs a logged-in `claude` CLI. Chapter 10: `tools/study/measure_v12.py` (reply prices, `kapu_ask`, open times over MCP), `tools/study/measure_scale.py` (a file of a given size), `tools/study/translate_hu.py` (the translated questions) and `tools/study/collect_v12.py` (the results file). Chapters 8 and 9 (public documentation): `tools/study/prepare_corpus.py` and `so_questions.py` prepare the corpora and questions, `scale_probe.py` measures a size, `chunk_baseline.py` the chunk baselines, `collect_results.py` writes `results/2026-10-23-study-pilot.json`, and `results_page.py` turns that and the format 1.2 results file into the results page.
