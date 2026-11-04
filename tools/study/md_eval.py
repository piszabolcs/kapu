"""The file or the knowledge? Claude with one Markdown file against KAPU (plan v3 M1).

    py tools/study/md_eval.py prepare <work> <rooms-dir>
    py tools/study/md_eval.py pixellab-questions <requests.jsonl> <answers-dir> <out.json>
    py tools/study/md_eval.py run <work> <questions.json> <file.kapu> <kapu-mcp> <model-dir> \
        [--domain plc|pixellab] [--forms kapu,md] [--runs 3] [--model sonnet] [--jobs 3] \
        [--rank <json> --tag <suffix>]
    py tools/study/md_eval.py session <work> <questions.json> <file.kapu> <kapu-mcp> <model-dir> \
        [--domain plc|pixellab] [--forms kapu,md,pasted] [--runs 1] [--turns 25] [--model sonnet]         [--rank <json> --tag <suffix>]
    py tools/study/md_eval.py judge <work> [--model haiku]
    py tools/study/md_eval.py report <work> <out.json>

`prepare` joins the rooms of a corpus (`<id>.md`, in file name order, the
order of `kapu import`) into one Markdown file, `<work>/docs.md`, and keeps
the line range of each room (`docs-index.json`). The questions are a JSON
list of `{qid, kind, kerdes, ...}`: the N1 set (`study-data/v2/n1/questions.json`,
with gold answer and evidence) or the PixelLab requests with a checked
reference answer each (`pixellab-questions`).

Forms, each an isolated `claude -p` run with no user settings and no other
MCP servers:
- `kapu`: the kapu MCP tools on the file (tools of this repository);
- `kapu-hook`: the same tools and prompt, and `kapu-mcp hook` as a
  UserPromptSubmit hook in the folder's project settings, so the passages are
  there before the first turn (plan v5); `--rank` goes to the hook as well, and
  each call records what the hook did (`hook`: similarity, why, chars, ms, tokens);
- `md`: the one Markdown file in an otherwise empty folder, with the Grep and
  Read tools only;
- `pasted` (`session` only): the whole Markdown file in the first message,
  no tools.
`run` asks every question on its own, `--runs` times, after one unrecorded call
per form that warms the prompt cache; `--rank` goes to
`kapu-mcp --rank` (the server switches) and `--tag` names that variant of the
form (`kapu` + tag) in the records. `session` asks the
first `--turns` questions one after the other in one conversation
(`--session-id`, then `--resume`), so the knowledge read before stays in
the context. Each call records its input tokens without cache, the cache
writes and reads, the output tokens and the dollars `claude` reports, and
the rooms it read (`kapu`: the rooms of the tool replies, `md`: the line
ranges of `Read` against `docs-index.json`). `judge` grades every answer with
the N1 judge prompts of `oracle_eval.py` word for word (PLC), or against the
reference answer (PixelLab); `report` gives per form and run the shares,
their spread over the runs, and the tokens and dollars.
"""

import json
import re
import statistics
import subprocess
import sys
import tempfile
import time
import uuid
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from oracle_eval import ENDING, JUDGE, JUDGE_ABSENT, NOT_FOUND, ROOM, load  # noqa: E402
from skill_eval import CLAUDE, KAPU_TOOLS, clean_env  # noqa: E402

JUDGE_REF = """You grade an answer to a beginner's request about PixelLab (a pixel-art tool).
Request: {q}
Reference answer, written from the documentation and checked against it: {ref}
Answer to grade: {answer}

Reply with one word: CORRECT (it gives the reference's key facts for this request and nothing that contradicts it), PARTLY (some of them, or with a wrong detail), WRONG (other or made-up facts), or NOTFOUND (it says the answer was not found or declines)."""
ENDING_PX = (f"If the documentation does not cover it, answer exactly {NOT_FOUND}. "
             "Give the answer between <answer> and </answer>, as short as the request allows.")
DOMAIN = {
    "plc": ("vendor documentation for PLC systems", "Question", ENDING),
    "pixellab": ("the PixelLab documentation (a pixel-art tool)", "Request", ENDING_PX),
}
READ_DEFAULT_LINES = 2000
WARM_PROMPT = "Reply with the word OK and nothing else."
REPLY = re.compile(r"reply: (\d+) tokens")


def first_prompt(form, domain, q, kfile, lines, doc):
    what, label, ending = DOMAIN[domain]
    if form in ("kapu", "kapu-hook"):
        lead = f"The KAPU knowledge file {kfile} holds {what}; reach it with the kapu MCP tools."
    elif form == "md":
        lead = (f"The Markdown file docs.md in the current folder ({lines} lines) holds {what}; "
                "search it with Grep and read the parts you need with Read.")
    else:
        lead = f"Here is {what}.\n<doc>\n{doc}\n</doc>"
    return f"{lead}\n\n{label}: {q}\n\nAnswer from the documentation only. {ending}"


def next_prompt(domain, q):
    _, label, ending = DOMAIN[domain]
    return f"Next {label.lower()}: {q}\n\nAnswer from the documentation only. {ending}"


def command(form, model, config, session=None, resume=False):
    cmd = [*CLAUDE, "-p", "--model", model, "--setting-sources", "project",
           "--output-format", "stream-json", "--verbose", "--strict-mcp-config", "--max-budget-usd", "3"]
    if session is None:
        cmd.append("--no-session-persistence")
    elif resume:
        cmd += ["--resume", session]
    else:
        cmd += ["--session-id", session]
    if form == "kapu-hook":
        cmd.append("--include-hook-events")
    if form in ("kapu", "kapu-hook"):
        return cmd + ["--mcp-config", config, "--tools", "",
                      "--allowedTools", " ".join(f"mcp__kapu__{t}" for t in KAPU_TOOLS)]
    if form == "md":
        return cmd + ["--tools", "Read,Grep", "--allowedTools", "Read Grep"]
    return cmd + ["--tools", ""]


def rooms_in(index, start, count):
    """The rooms whose lines overlap `start` (1-based) and the `count` lines after it."""
    end = start + count
    return [r for a, b, r in index if a < end and b >= start]


def hook_record(event):
    """What the hook of a `kapu-hook` call did: its `--explain` line and the tokens it put in."""
    try:
        record = json.loads((event.get("stderr") or "").strip().splitlines()[-1])
    except (IndexError, ValueError):
        # a killed hook must not stop the run; the record shows it
        record = {"why": "no explain line"}
    # the reply sits inside the JSON of the hook's stdout, its newlines escaped
    m = re.search(r"\\nreply: (\d+) tokens", event.get("stdout") or "")
    return {**record, "tokens": int(m.group(1)) if m else 0, "exit": event.get("exit_code")}


def call(prompt, cmd, cwd, index):
    started = time.perf_counter()
    proc = subprocess.run(cmd, cwd=cwd, input=prompt, capture_output=True, text=True, encoding="utf-8",
                          timeout=1800, env=clean_env())
    seconds = time.perf_counter() - started
    tools, opened, result, inputs, replies, hook, turns = [], [], {}, [], [], None, {}
    for line in proc.stdout.splitlines():
        try:
            event = json.loads(line)
        except ValueError:
            continue
        kind = event.get("type")
        if kind == "assistant":
            turns[event.get("message", {}).get("id")] = event.get("message", {}).get("usage") or {}
            for block in event.get("message", {}).get("content", []):
                if block.get("type") == "tool_use":
                    name = block.get("name", "").replace("mcp__kapu__", "")
                    tools.append(name)
                    shown = {k: v for k, v in (block.get("input") or {}).items() if k not in ("file", "file_path")}
                    inputs.append(json.dumps(shown, ensure_ascii=False)[:200])
                    if name == "Read":
                        args = block.get("input", {}) or {}
                        start = int(args.get("offset") or 1)
                        opened += rooms_in(index, start, int(args.get("limit") or READ_DEFAULT_LINES))
        elif kind == "user":
            for block in event.get("message", {}).get("content", []) or []:
                if isinstance(block, dict) and block.get("type") == "tool_result":
                    content = block.get("content")
                    text = content if isinstance(content, str) else " ".join(
                        c.get("text", "") for c in content or [] if isinstance(c, dict))
                    opened += ROOM.findall(text)
                    m = REPLY.match(text)
                    replies.append(int(m.group(1)) if m else len(text) // 4)
        elif kind == "system" and event.get("subtype") == "hook_response":
            hook = hook_record(event)
        elif kind == "result":
            result = event
    text = result.get("result", "") or ""
    m = re.search(r"<answer>(.*?)</answer>", text, re.S)
    usage = result.get("usage", {}) or {}
    return {
        "answer": (m.group(1) if m else text[-1500:]).strip(),
        "tools": tools, "tool_calls": len(tools), "opened": sorted(set(opened)),
        "inputs": inputs, "replies": replies, "hook": hook,
        # per API call: cache writes, cache reads (the sums are the fields below)
        "turns": [[u.get("cache_creation_input_tokens") or 0, u.get("cache_read_input_tokens") or 0]
                  for u in turns.values()],
        "seconds": round(seconds, 1),
        "uncached": usage.get("input_tokens") or 0,
        "cache_write": usage.get("cache_creation_input_tokens") or 0,
        "cache_read": usage.get("cache_read_input_tokens") or 0,
        "output_tokens": usage.get("output_tokens") or 0,
        "cost_usd": result.get("total_cost_usd"),
        "error": None if result and not result.get("is_error") else (text or proc.stderr.strip()[-400:] or "no result"),
    }


def gold_read(q, opened):
    room = Path(q["valasz_fajl"]).stem if q.get("valasz_fajl") else None
    return None if room is None else any(o == room or o.startswith(room + "/") for o in opened)


def options(argv, defaults):
    opts, pos = dict(defaults), []
    it = iter(argv)
    for a in it:
        if a in opts:
            opts[a] = next(it)
        else:
            pos.append(a)
    return opts, pos


def hook_settings(cwd, kfile, mcp, model_dir, rank):
    """The project settings of the `kapu-hook` form; the hook line goes through Claude Code's shell."""
    line = f'"{Path(mcp).resolve().as_posix()}" hook "{kfile.resolve().as_posix()}" --explain'
    if rank:
        line += f" --rank '{rank}'"
    settings = {"env": {"KAPU_MODEL_DIR": str(Path(model_dir).resolve())},
                "hooks": {"UserPromptSubmit": [{"hooks": [{"type": "command", "command": line, "timeout": 10}]}]}}
    (cwd / ".claude").mkdir(exist_ok=True)
    (cwd / ".claude/settings.json").write_text(json.dumps(settings, indent=1) + "\n", encoding="utf-8")


def hook_check(kfile, mcp, model_dir, rank, prompt):
    """A quiet hook (no model, no vectors) would pay for a plain `kapu` run: stop before the first call."""
    args = ["--rank", rank] if rank else []
    proc = subprocess.run([str(mcp), "hook", str(kfile), "--explain", *args], input=json.dumps({"prompt": prompt}),
                          capture_output=True, text=True, encoding="utf-8", timeout=60,
                          env={**clean_env(), "KAPU_MODEL_DIR": str(Path(model_dir).resolve())})
    line = json.loads(proc.stderr.strip().splitlines()[-1])
    if not line.get("context"):
        sys.exit(f"the hook stays quiet on the first question: {line}")
    print(f"hook: {line}", flush=True)


def setup(work, kfile, mcp, model_dir, rank="", tag=""):
    """The MCP config, the empty folders the forms run in, the docs and its index."""
    if not Path(mcp).is_file():
        sys.exit(f"no kapu-mcp at {mcp}")
    args = ["--rank", rank] if rank else []
    cfg = {"mcpServers": {"kapu": {"command": str(Path(mcp).resolve()), "args": args,
                                   "env": {"KAPU_MODEL_DIR": str(Path(model_dir).resolve())}}}}
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False, encoding="utf-8") as f:
        json.dump(cfg, f)
    docs = work / "docs.md"
    index = json.loads((work / "docs-index.json").read_text(encoding="utf-8"))
    # a folder per tag: the settings carry the tag's --rank
    cwds = {"kapu": work / "cwd-kapu", "kapu-hook": work / f"cwd-kapu-hook{tag}", "md": work / "cwd-md",
            "pasted": work / "cwd-pasted"}
    for c in cwds.values():
        c.mkdir(exist_ok=True)
    hook_settings(cwds["kapu-hook"], kfile, mcp, model_dir, rank)
    md_copy = cwds["md"] / "docs.md"
    if not md_copy.exists() or md_copy.stat().st_size != docs.stat().st_size:
        md_copy.write_bytes(docs.read_bytes())
    text = docs.read_text(encoding="utf-8")
    return f.name, cwds, index, text, text.count("\n") + 1


def prepare(argv):
    work, rooms = Path(argv[0]), Path(argv[1])
    work.mkdir(parents=True, exist_ok=True)
    parts, index, line = [], [], 1
    for f in sorted(rooms.glob("*.md")):
        text = f.read_text(encoding="utf-8").replace("\r\n", "\n").strip("\n") + "\n"
        n = text.count("\n")
        index.append((line, line + n - 1, f.stem))
        parts.append(text)
        line += n + 1
    (work / "docs.md").write_text("\n".join(parts), encoding="utf-8", newline="\n")
    (work / "docs-index.json").write_text(json.dumps(index), encoding="utf-8")
    print(f"{len(index)} rooms, {line - 1} lines")


def pixellab_questions(argv):
    reqs, answers, out = Path(argv[0]), Path(argv[1]), Path(argv[2])
    refs = {}
    for f in sorted(answers.glob("answers-k104-*.md")):
        for m in re.finditer(r"^## (p\d+)\n(.*?)(?=^## p\d+\n|\Z)", f.read_text(encoding="utf-8"), re.M | re.S):
            refs.setdefault(m.group(1), m.group(2).strip()[:4000])
    qs = [{"qid": r["id"], "kind": "answerable", "kerdes": r["question"], "ref": refs[r["id"]]}
          for r in load(reqs)]
    out.write_text(json.dumps(qs, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"{len(qs)} requests")


def run(argv):
    opts, pos = options(argv, {"--domain": "plc", "--forms": "kapu,md", "--runs": "3", "--model": "sonnet",
                               "--jobs": "3", "--rank": "", "--tag": ""})
    work, qfile, kfile, mcp, model_dir = (Path(p) for p in pos)
    config, cwds, index, _, lines = setup(work, kfile, mcp, model_dir, opts["--rank"], opts["--tag"])
    qs = json.loads(qfile.read_text(encoding="utf-8"))
    if "kapu-hook" in opts["--forms"].split(","):
        hook_check(kfile, mcp, model_dir, opts["--rank"],
                   first_prompt("kapu-hook", opts["--domain"], qs[0]["kerdes"], kfile.resolve().as_posix(), lines, ""))
    (work / "questions.json").write_text(json.dumps(qs, ensure_ascii=False, indent=1), encoding="utf-8")
    out = work / "runs.jsonl"
    done = {(r["qid"], r["form"], r["run"]) for r in load(out) if not r.get("error")} if out.exists() else set()
    todo = [(q, f, n) for n in range(1, int(opts["--runs"]) + 1) for q in qs for f in opts["--forms"].split(",")
            if (q["qid"], f + opts["--tag"], n) not in done]
    print(f"{len(todo)} calls", flush=True)

    def one(job):
        q, f, n = job
        prompt = first_prompt(f, opts["--domain"], q["kerdes"], kfile.resolve().as_posix(), lines, "")
        try:
            r = call(prompt, command(f, opts["--model"], config), cwds[f], index)
        except subprocess.TimeoutExpired:
            r = {"error": "timeout"}
        return {"mode": "single", "qid": q["qid"], "kind": q["kind"], "form": f + opts["--tag"], "run": n,
                "model": opts["--model"], "rank": opts["--rank"], **r,
                "gold_read": gold_read(q, r.get("opened", []))}

    # the first calls of a new tool list write its prefix to the cache; one
    # unrecorded call does that first, so every recorded run starts warm
    for f in sorted({f for _, f, _ in todo}):
        r = call(WARM_PROMPT, command(f, opts["--model"], config), cwds[f], index)
        print(f"warm-up {f}: {r.get('cost_usd')} $", flush=True)
    with ThreadPoolExecutor(int(opts["--jobs"])) as pool, open(out, "a", encoding="utf-8") as sink:
        for i, r in enumerate(pool.map(one, todo), 1):
            sink.write(json.dumps(r, ensure_ascii=False) + "\n")
            sink.flush()
            if i % 10 == 0:
                print(f"{i}/{len(todo)}", flush=True)


def session(argv):
    opts, pos = options(argv, {"--domain": "plc", "--forms": "kapu,md,pasted", "--runs": "1", "--turns": "25",
                               "--model": "sonnet", "--rank": "", "--tag": ""})
    work, qfile, kfile, mcp, model_dir = (Path(p) for p in pos)
    config, cwds, index, text, lines = setup(work, kfile, mcp, model_dir, opts["--rank"], opts["--tag"])
    tag = opts["--tag"]
    qs = json.loads(qfile.read_text(encoding="utf-8"))[: int(opts["--turns"])]
    if "kapu-hook" in opts["--forms"].split(","):
        hook_check(kfile, mcp, model_dir, opts["--rank"],
                   first_prompt("kapu-hook", opts["--domain"], qs[0]["kerdes"], kfile.resolve().as_posix(), lines, ""))
    out = work / "sessions.jsonl"
    done = {(r["form"], r["run"]) for r in load(out) if r.get("turn") == len(qs) and not r.get("error")} \
        if out.exists() else set()

    def one(job):
        f, n = job
        sid, rows = str(uuid.uuid4()), []
        for t, q in enumerate(qs, 1):
            prompt = (first_prompt(f, opts["--domain"], q["kerdes"], kfile.resolve().as_posix(), lines, text)
                      if t == 1 else next_prompt(opts["--domain"], q["kerdes"]))
            try:
                r = call(prompt, command(f, opts["--model"], config, sid, t > 1), cwds[f], index)
            except subprocess.TimeoutExpired:
                r = {"error": "timeout"}
            rows.append({"mode": "session", "session": sid, "turn": t, "qid": q["qid"], "kind": q["kind"],
                         "form": f + tag, "run": n, "model": opts["--model"], **r,
                         "gold_read": gold_read(q, r.get("opened", []))})
            print(f"{f}{tag} run {n} turn {t}: {r.get('cost_usd')} $, error {r.get('error')}", flush=True)
            if r.get("error"):
                break
        return rows

    jobs = [(f, n) for n in range(1, int(opts["--runs"]) + 1) for f in opts["--forms"].split(",")
            if (f + tag, n) not in done]
    with ThreadPoolExecutor(len(jobs) or 1) as pool, open(out, "a", encoding="utf-8") as sink:
        for rows in pool.map(one, jobs):
            for r in rows:
                sink.write(json.dumps(r, ensure_ascii=False) + "\n")
            sink.flush()


def judge(argv):
    work = Path(argv[0])
    model = argv[argv.index("--model") + 1] if "--model" in argv else "haiku"
    qs = {q["qid"]: q for q in json.loads((work / "questions.json").read_text(encoding="utf-8"))}
    rows = [r for name in ("runs.jsonl", "sessions.jsonl") if (work / name).exists() for r in load(work / name)]
    out = work / "judged.jsonl"
    key = lambda r: (r["mode"], r["qid"], r["form"], r["run"])  # noqa: E731
    done = {key(j) for j in load(out)} if out.exists() else set()
    todo = [r for r in rows if not r.get("error") and key(r) not in done]

    def one(r):
        q = qs[r["qid"]]
        if "ref" in q:
            prompt = JUDGE_REF.format(q=q["kerdes"], ref=q["ref"], answer=r["answer"])
        elif q["kind"] == "answerable":
            prompt = JUDGE.format(q=q["kerdes"], gold=q["answer"], evidence=q["evidence"], answer=r["answer"])
        else:
            prompt = JUDGE_ABSENT.format(q=q["kerdes"], answer=r["answer"])
        proc = subprocess.run([*CLAUDE, "-p", "--model", model, "--setting-sources", "project",
                               "--no-session-persistence", "--tools", "", "--strict-mcp-config"],
                              input=prompt, capture_output=True, text=True, encoding="utf-8", timeout=300,
                              env=clean_env())
        word = re.search(r"\b(CORRECT|PARTLY|WRONG|NOTFOUND|ANSWERED)\b", proc.stdout.upper())
        return {"mode": r["mode"], "qid": r["qid"], "form": r["form"], "run": r["run"], "judge": model,
                "grade": word.group(1).lower() if word else "unclear"}

    with ThreadPoolExecutor(4) as pool, open(out, "a", encoding="utf-8") as sink:
        for j in pool.map(one, todo):
            sink.write(json.dumps(j) + "\n")
            sink.flush()
    print(f"judged {len(todo)}")


def shares(rows, grades, kind):
    g = [grades.get((r["mode"], r["qid"], r["form"], r["run"]), "unjudged") for r in rows]
    n = len(rows)
    if kind == "answerable":
        out = {grade: g.count(grade) / n for grade in ("correct", "partly", "wrong", "notfound")}
        read = [r["gold_read"] for r in rows if r.get("gold_read") is not None]
        if read:
            out["gold_read"] = sum(read) / len(read)
        return out
    return {"abstained": g.count("notfound") / n, "made_up": g.count("answered") / n}


def spend(rows):
    return {k: sum(r.get(k) or 0 for r in rows) for k in ("uncached", "cache_write", "cache_read", "output_tokens",
                                                       "cost_usd", "tool_calls", "seconds")}


def per_turn(rows):
    """`claude --resume` reports the dollars of the whole session so far: each session row gets its turn's own."""
    so_far = {}
    for r in sorted((r for r in rows if r["mode"] == "session"), key=lambda r: r["turn"]):
        total = r.get("cost_usd") or 0
        r["cost_usd"] = total - so_far.get(r["session"], 0)
        so_far[r["session"]] = total


def report(argv):
    work, out = Path(argv[0]), Path(argv[1])
    qs = {q["qid"]: q for q in json.loads((work / "questions.json").read_text(encoding="utf-8"))}
    rows = [r for name in ("runs.jsonl", "sessions.jsonl") if (work / name).exists() for r in load(work / name)
            if not r.get("error")]
    per_turn(rows)
    grades = {(j["mode"], j["qid"], j["form"], j["run"]): j["grade"] for j in load(work / "judged.jsonl")}
    result = {"questions": {"answerable": sum(q["kind"] == "answerable" for q in qs.values()),
                            "absent": sum(q["kind"] == "absent" for q in qs.values())},
              "errors": sum(1 for name in ("runs.jsonl", "sessions.jsonl") if (work / name).exists()
                            for r in load(work / name) if r.get("error")),
              "modes": {}}
    for mode in sorted({r["mode"] for r in rows}):
        result["modes"][mode] = {}
        for form in sorted({r["form"] for r in rows if r["mode"] == mode}):
            mine = [r for r in rows if r["mode"] == mode and r["form"] == form]
            runs = sorted({r["run"] for r in mine})
            per_run = {}
            for n in runs:
                rr = [r for r in mine if r["run"] == n]
                entry = {"spend": spend(rr), "calls": len(rr)}
                for kind in ("answerable", "absent"):
                    k = [r for r in rr if r["kind"] == kind]
                    if k:
                        entry[kind] = shares(k, grades, kind)
                per_run[n] = entry
            pooled = {}
            for kind in ("answerable", "absent"):
                k = [r for r in mine if r["kind"] == kind]
                if not k:
                    continue
                pooled[kind] = shares(k, grades, kind)
                key = "correct" if kind == "answerable" else "made_up"
                values = [per_run[n][kind][key] for n in runs if kind in per_run[n]]
                pooled[kind][f"{key}_runs"] = values
                pooled[kind]["input_tokens_median"] = statistics.median(
                    r["uncached"] + r["cache_write"] + r["cache_read"] for r in k)
                pooled[kind]["cost_usd_median"] = statistics.median(r["cost_usd"] or 0 for r in k)
                pooled[kind]["tool_calls_median"] = statistics.median(r["tool_calls"] for r in k)
            result["modes"][mode][form] = {"runs": per_run, "pooled": pooled,
                                           "spend_per_run_mean": {key: statistics.mean(per_run[n]["spend"][key]
                                                                                        for n in runs)
                                                                  for key in spend(mine)}}
    out.write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=1)[:6000])


if __name__ == "__main__":
    {"prepare": prepare, "pixellab-questions": pixellab_questions, "run": run, "session": session,
     "judge": judge, "report": report}[sys.argv[1]](sys.argv[2:])
