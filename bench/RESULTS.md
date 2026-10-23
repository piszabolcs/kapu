# KAPU measurements

English summary of the measurements. The full log of every measurement, including the earlier synthetic-corpus runs and the Hungarian-question track, is in [REPORT.md](REPORT.md) (Hungarian). Raw numbers: [results/2026-10-23-english-corpus.json](results/2026-10-23-english-corpus.json).

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

On English text with English questions, keyword search alone is already strong, and the hybrid ranking keeps that level. The vector model matters when the question and the text are in different languages. That is a separate track, measured in REPORT.md chapter 6: Hungarian questions over English text reach a hybrid recall@10 of 0.45.

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

An earlier run on a mixed English and Hungarian corpus (REPORT.md chapters 5-8) led to specification v1.0.2:

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

## 8. Reproduce

```bash
python tools/collect_skills.py bench/korpusz/rooms           # the installed skills as rooms (local only)
kapu import <rooms folder> corpus.kapu --model models/potion-multilingual-128M-i8
kapu validate corpus.kapu --full
kapu bench corpus.kapu questions.jsonl --json                # recall@5/10, MRR@10 per mode
python tools/mcp_probe.py corpus.kapu questions.jsonl        # MCP latency and response sizes
python tools/kapu_mcp_call.py call kapu_search '{"file": "...", "query": "..."}'
```
