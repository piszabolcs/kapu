# KAPU study pilot: measurement plan

> **For agentic workers:** executed natively in the session (the user asked for the measurements to run now, and prefers lean execution with no per-task review loop). Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run as much of the study design as we can without an API budget or a human labeller, and record what each step shows.

**Architecture:**
- Public corpora are downloaded outside the repo to `../study-data/`. They are untrusted third-party data and are never committed.
- Small Python scripts in `tools/study/` turn the corpora into flat room folders and collect real questions. They measure retrieval and run the agent tools through logging wrappers.
- Agents are Claude subagents (the Agent tool), the same method as REPORT chapters 8 and 10.
- Results go to `bench/results/` and to a new REPORT chapter.

**Tech stack:** the existing `kapu` / `kapu-mcp` release build, Python 3.13 in `tools/.venv` (tiktoken, model2vec, numpy, mcp; psutil added), git sparse clones, the Stack Exchange API v2.3 without a key (300 requests/day).

**Spec:** `docs/research/2026-10-23-arxiv-study-design.md`

## Global constraints

- Third-party text (docs, Stack Overflow posts) stays in `../study-data/`. Only ids, counts and measurements are committed.
- No API keys and no paid calls. Agents are Claude subagents only, so there is no non-Claude judge or generator. This deviation is recorded.
- Room ids are written by the prepare step as ASCII slugs of at most 64 bytes, so `kapu import`'s own slug keeps them unchanged (`text.rs` `slug`).
- Commits: noreply author, no AI attribution, no push.
- Measured with spec v1.0.3 and the current release build. No KAPU code changes during the pilot.

## Review focus

1. A docs URL in a Stack Overflow answer that points at an old path (a redirect) or an anchor is still mapped to the right room. Redirect files are used where the corpus has one, and anchors are dropped.
2. A question whose accepted answer links to several doc pages is left out, so every kept question has exactly one gold room.
3. Corpus macros (mdBook `{{#include}}`, Hugo `{{< >}}`, MDN `{{macro}}`) do not leave raw template syntax that inflates tokens or breaks titles.
4. Two source files that slug to the same id get distinct ids, and the manifest matches what `kapu import` produced (checked with `kapu export`).
5. The L build can run out of memory or time. It runs in the background with peak memory recorded, and a failure is reported as a finding, not hidden.

---

### Task 1: Corpus preparation

**Files:** Create `tools/study/prepare_corpus.py`, `tools/study/check_prepare.py`

**Interfaces:**
- Produces: `prepare_corpus.py <kind> <src> <out_dir>` where kind is `rust-book`, `k8s` or `mdn`. It writes flat `<id>.md` rooms (first line `# <title>`, front matter removed), `manifest.json` (`{id: {"url", "src", "title"}}`) and `corpus.txt` (all rooms in id order, for the full-context run).
- `slug(s) -> str`: ASCII lower-case, every other run becomes `-`, trimmed, cut to 64 bytes and trimmed again. Collisions get `-2`, `-3`.
- `canonical_url(kind, url) -> str | None`: maps a docs URL to the manifest key form (redirects applied, anchor and query dropped, trailing slash and `.html` normalised, MDN case-insensitive).

- [ ] Step 1: `check_prepare.py` asserts:
  - `slug("ch04-01-what-is-ownership") == "ch04-01-what-is-ownership"`;
  - a 70-character slug is cut to 64 bytes without a trailing `-`;
  - `canonical_url("rust-book", "https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html#the-stack")` equals the manifest URL of that room;
  - an old `second-edition` book URL maps to the same room;
  - an MDN URL with different case maps to one room.
- [ ] Step 2: Implement until `check_prepare.py` passes on the real downloaded corpora.
- [ ] Step 3: Import each room folder; `kapu export` the result; the exported file names equal the manifest ids.

### Task 2: Fetch and scale probe (F0.5)

**Files:** Create `tools/study/scale_probe.py`

- [ ] Step 1: Sparse, depth-1 clones into `../study-data/src-*`:
  - `rust-lang/book`: `src`, `listings`;
  - `kubernetes/website`: `content/en/docs`, `static/_redirects`;
  - `mdn/content`: `files/en-us`.
  Record each commit hash.
- [ ] Step 2: Prepare S (Rust book), M (Kubernetes en docs) and L (MDN en-US; if it exceeds 25 million tokens, cut it to the largest top-level sections, about 10 million). Record the body tokens (o200k_base).
- [ ] Step 3: `scale_probe.py <rooms_dir> <file.kapu>` runs `kapu import` and records:
  - wall time, peak working set (psutil), file size;
  - `kapu validate --full` time;
  - over one MCP session: first and later map tokens and ms, largest group list tokens, 20 searches (median and p90 ms, tokens), 20 opens (median ms).
  It writes `scale-<size>.json`.
- [ ] Step 4: Run it on S, M and L. L runs in the background. A failure is recorded with its error.

### Task 3: Real questions (Q-valós)

**Files:** Create `tools/study/so_questions.py`

- [ ] Step 1: For each corpus, take the tag (`rust`, `kubernetes`, `javascript`+`css`+`html`) and the docs host. Fetch up to 2,000 top-voted questions with an accepted answer (`filter=withbody`), then the accepted answers. Keep a question when the answer links exactly one distinct page of the corpus that is in the manifest. Respect `backoff`.
- [ ] Step 2: Write `questions-<size>.jsonl` in the `kapu bench` format (`kerdes` = title, `valasz_fajl` = `<id>.md`, `valasz_szakasz` = "", `nyelv` = "en"). Write `questions-<size>-full.jsonl` with the question id, title, body text, the accepted answer text and the gold id.
- [ ] Step 3: Commit only the question ids and the gold ids (`bench/results/study-pilot-questions.json`); the bodies stay local.

### Task 4: Retrieval without an agent

**Files:** Create `tools/study/chunk_baseline.py`

- [ ] Step 1: `kapu bench --json` on each size, giving BM25, vector and hybrid, strict and lenient. Lenient (room-level) is the comparable number.
- [ ] Step 2: `chunk_baseline.py <rooms_dir> <questions.jsonl>` cuts rooms into 512-token chunks (o200k_base, 64 overlap) and ranks chunks by:
  - potion (f32) cosine;
  - BM25 (k1 1.2, b 0.75).
  It maps chunks to rooms in rank order and reports room-level recall@5, recall@10 and MRR@10, the same numbers as `kapu bench` lenient.
- [ ] Step 3: Table: size × method.

### Task 5: Agent tool wrappers

**Files:**
- Create `tools/study/fs_call.py`, `tools/study/llms_call.py`, `tools/study/rag_call.py`;
- Modify `tools/kapu_mcp_call.py`: an optional `KAPU_CALL_LOG` environment variable sets the log path.

- [ ] Step 1: Each wrapper appends `{"tag", "tool", "args", "ms", "tokens", "error"}` to the log in `KAPU_CALL_LOG`, using the same token counter as `kapu_mcp_call.py`.
  - `fs_call.py <dir>`: `ls`, `grep <regex>` (at most 50 lines, `file:line: text`), `read <file> [--start N --lines M]` (at most 400 lines).
  - `llms_call.py <dir>`: `index` (llms.txt: `- [title](file): first sentence`), `read <file>`.
  - `rag_call.py <dir>`: `search "<query>" [--k 8]`, returning chunk text with its file name. The index is built once and cached as `.npz`.
- [ ] Step 2: Smoke-run each wrapper once on S and check that the log line is written.

### Task 6: Agentic pilot on S and judging

- [ ] Step 1: Draw 30 questions from `questions-s-full.jsonl` (fixed seed 20261023, spread over the book's chapters). Each agent gets the title and the body (code kept, cut to 1,500 characters).
- [ ] Step 2: Run each system with Sonnet subagents, 10 questions per agent:
  - C0 closed-book: 1 agent, 30 questions;
  - C1 full context: 1 agent reads `corpus.txt` once and then answers. It runs only if the corpus fits the agent; otherwise it is recorded as not run;
  - C2b RAG tool, C3 file tools, C4 llms.txt, C5 KAPU: 3 agents each.
  Tags `sp-<system>-<q>`. Each agent writes its answers to `../study-data/pilot/answers-<system>-<n>.md`.
- [ ] Step 3: Judge agents (Sonnet; a non-Claude judge is not available) grade every answer against the accepted Stack Overflow answer and the gold page: `correct` yes/partly/no, plus a short reason. The output is `../study-data/pilot/judge-*.json`.
- [ ] Step 4: Compile, per system: accuracy, read tokens per question from the logs (median, mean), calls per question, and billed tokens per question (agent total ÷ questions, approximate).

### Task 7: Write-up

- [ ] Step 1: REPORT chapter 11 (Hungarian), with each deviation from the design stated.
- [ ] Step 2: `bench/results/2026-10-23-study-pilot.json`.
- [ ] Step 3: Commit locally.
