"""Search or reading? Oracle, KAPU and closed-book answers (plan v2 N1, N4).

    python tools/study/oracle_eval.py run <work> <questions-dir> <rooms-dir> <file.kapu> <kapu-mcp> <model-dir> \
        [--model sonnet] [--jobs 3] [--forms closed,oracle,kapu,kapu_conf] [--per-vendor 6]
    python tools/study/oracle_eval.py judge <work> [--model haiku]
    python tools/study/oracle_eval.py report <work> <out.json>

Questions: `answerable.jsonl` (a gold room and answer) and `absent.jsonl`
(near the corpus, but not answered by it), from `<questions-dir>`;
`--per-vendor` takes the first N answerable questions of each vendor. Forms,
each an isolated `claude -p` run (no user settings, no other MCP servers):
- `closed`: the question alone, no tools;
- `oracle`: the gold room's text in the prompt, no tools (answerable only);
- `kapu`: the kapu MCP tools on the file (`--rank {"collapse":1}`, J1);
- `kapu_conf`: the same with `kapu-mcp --rank {"collapse":1,"ask":"confidence"}`
  (J12: does `confidence: low` lower the made-up answers?).
Every prompt allows the answer `NOT FOUND`. A judge model (`judge`) grades
each answer against the gold answer and evidence: correct, partly, wrong or
not found; for an absent question, not found is the right answer and any
other answer is made up. `report`: per form the share correct, partly, wrong
and not found, the correct abstentions and the made-up answers on the absent
questions, tokens, calls, seconds and dollars; `retrieval_gap = oracle -
kapu` and `knowledge_gain = kapu - closed` (N1) on the correct share.
"""

import json
import re
import statistics
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from skill_eval import CLAUDE, KAPU_TOOLS, clean_env  # noqa: E402

NOT_FOUND = "NOT FOUND"
ROOM = re.compile(r'<kapu-room file="[^"]*" id="([^"]+)"')
ORACLE_CHARS = 40_000
ENDING = (f"If the answer is not there or you are not sure, answer exactly {NOT_FOUND}. "
          "Give the answer in one to three sentences between <answer> and </answer>.")
PROMPTS = {
    "closed": "Question about industrial automation (PLC programming and engineering tools): {q}\n\n"
              "Answer from what you know. " + ENDING,
    "oracle": "Here is one page of vendor documentation.\n<doc>\n{doc}\n</doc>\n\n"
              "Question: {q}\n\nAnswer from this page only. " + ENDING,
    "kapu": "The KAPU knowledge file {file} holds vendor documentation for PLC systems; reach it with the kapu MCP tools. "
            "Question: {q}\n\nAnswer from the file only. " + ENDING,
}
PROMPTS["kapu_conf"] = PROMPTS["kapu"]
JUDGE = """You grade an answer to a question about PLC documentation.
Question: {q}
Gold answer: {gold}
Evidence from the documentation: {evidence}
Answer to grade: {answer}

Reply with one word: CORRECT (the answer gives the gold answer's key fact), PARTLY (part of it, or it with a wrong detail), WRONG (another or a made-up fact), or NOTFOUND (it says the answer was not found or declines)."""
JUDGE_ABSENT = """A question was asked whose answer is NOT in the documentation the answerer could read.
Question: {q}
Answer to grade: {answer}

Reply with one word: NOTFOUND (it says the answer was not found, or declines), or ANSWERED (it gives an answer anyway)."""


def load(path):
    return [json.loads(l) for l in Path(path).read_text(encoding="utf-8").splitlines() if l.strip()]


def questions(qdir, per_vendor):
    ans = load(Path(qdir) / "answerable.jsonl")
    if per_vendor:
        seen, picked = {}, []
        for q in ans:
            if seen.get(q["vendor"], 0) < per_vendor:
                seen[q["vendor"]] = seen.get(q["vendor"], 0) + 1
                picked.append(q)
        ans = picked
    out = [{**q, "kind": "answerable", "qid": f"a{i:03d}"} for i, q in enumerate(ans)]
    out += [{**q, "kind": "absent", "qid": f"n{i:03d}"} for i, q in enumerate(load(Path(qdir) / "absent.jsonl"))]
    return out


def command(form, model, configs):
    cmd = [*CLAUDE, "-p", "--model", model, "--setting-sources", "project", "--no-session-persistence",
           "--output-format", "stream-json", "--verbose", "--strict-mcp-config"]
    if form in ("closed", "oracle"):
        return cmd + ["--tools", ""]
    return cmd + ["--mcp-config", configs[form], "--tools", "",
                  "--allowedTools", " ".join(f"mcp__kapu__{t}" for t in KAPU_TOOLS)]


def run_one(q, form, model, configs, rooms, kfile, cwd):
    doc = ""
    if form == "oracle":
        doc = (Path(rooms) / q["valasz_fajl"]).read_text(encoding="utf-8")[:ORACLE_CHARS]
    prompt = PROMPTS[form].format(q=q["kerdes"], doc=doc, file=Path(kfile).as_posix())
    started = time.perf_counter()
    proc = subprocess.run(command(form, model, configs), cwd=cwd, input=prompt, capture_output=True, text=True,
                          encoding="utf-8", timeout=900, env=clean_env())
    seconds = time.perf_counter() - started
    calls, opened, result, pending = [], [], {}, {}
    for line in proc.stdout.splitlines():
        try:
            event = json.loads(line)
        except ValueError:
            continue
        kind = event.get("type")
        if kind == "assistant":
            for block in event.get("message", {}).get("content", []):
                if block.get("type") == "tool_use":
                    name = block.get("name", "").replace("mcp__kapu__", "")
                    calls.append(name)
                    pending[block.get("id")] = name
        elif kind == "user":
            for block in event.get("message", {}).get("content", []) or []:
                if isinstance(block, dict) and block.get("type") == "tool_result":
                    content = block.get("content")
                    text = content if isinstance(content, str) else " ".join(
                        c.get("text", "") for c in content or [] if isinstance(c, dict))
                    opened += ROOM.findall(text)
        elif kind == "result":
            result = event
    text = result.get("result", "") or ""
    m = re.search(r"<answer>(.*?)</answer>", text, re.S)
    answer = (m.group(1) if m else text[-800:]).strip()
    usage = result.get("usage", {}) or {}
    gold_room = Path(q["valasz_fajl"]).stem if q.get("valasz_fajl") else None
    return {"qid": q["qid"], "kind": q["kind"], "vendor": q.get("vendor"), "form": form, "model": model,
            "answer": answer, "said_not_found": NOT_FOUND in answer.upper(),
            "opened_gold_room": None if gold_room is None or form not in ("kapu", "kapu_conf")
            else any(o == gold_room or o.startswith(gold_room + "/") for o in opened),
            "tool_calls": len(calls), "tools": calls, "seconds": round(seconds, 1),
            "input_tokens": sum(usage.get(k) or 0 for k in ("input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens")),
            "output_tokens": usage.get("output_tokens") or 0, "cost_usd": result.get("total_cost_usd"),
            "error": None if result and not result.get("is_error") else (text or proc.stderr.strip()[-400:] or "no result")}


def run(argv):
    opts = {"--model": "sonnet", "--jobs": "3", "--forms": "closed,oracle,kapu,kapu_conf", "--per-vendor": "6"}
    pos = []
    it = iter(argv)
    for a in it:
        if a in opts:
            opts[a] = next(it)
        else:
            pos.append(a)
    work, qdir, rooms, kfile, mcp, model_dir = (Path(p) for p in pos)
    work.mkdir(parents=True, exist_ok=True)
    out = work / "runs.jsonl"
    done = set()
    if out.exists():
        for line in out.read_text(encoding="utf-8").splitlines():
            r = json.loads(line)
            if not r.get("error"):
                done.add((r["qid"], r["form"], r["model"]))
    qs = questions(qdir, int(opts["--per-vendor"]))
    (work / "questions.json").write_text(json.dumps(qs, ensure_ascii=False, indent=1), encoding="utf-8")
    forms = opts["--forms"].split(",")
    configs = {}
    for form, args in (("kapu", ["--rank", '{"collapse": 1}']),
                       ("kapu_conf", ["--rank", '{"collapse": 1, "ask": "confidence"}'])):
        cfg = {"mcpServers": {"kapu": {"command": str(mcp.resolve()), "args": args,
                                       "env": {"KAPU_MODEL_DIR": str(model_dir.resolve())}}}}
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False, encoding="utf-8") as f:
            json.dump(cfg, f)
            configs[form] = f.name
    todo = [(q, f) for q in qs for f in forms
            if (q["qid"], f, opts["--model"]) not in done and not (f == "oracle" and q["kind"] != "answerable")]
    cwd = work / "cwd"
    cwd.mkdir(exist_ok=True)
    print(f"{len(todo)} runs", flush=True)

    def one(job):
        q, f = job
        try:
            return run_one(q, f, opts["--model"], configs, rooms, kfile.resolve(), cwd)
        except subprocess.TimeoutExpired:
            return {"qid": q["qid"], "kind": q["kind"], "form": f, "model": opts["--model"], "error": "timeout"}

    with ThreadPoolExecutor(int(opts["--jobs"])) as pool, open(out, "a", encoding="utf-8") as sink:
        for i, r in enumerate(pool.map(one, todo), 1):
            sink.write(json.dumps(r, ensure_ascii=False) + "\n")
            sink.flush()
            if i % 10 == 0:
                print(f"{i}/{len(todo)}", flush=True)


def judge(argv):
    work = Path(argv[0])
    model = argv[argv.index("--model") + 1] if "--model" in argv else "haiku"
    qs = {q["qid"]: q for q in json.loads((work / "questions.json").read_text(encoding="utf-8"))}
    runs = [json.loads(l) for l in (work / "runs.jsonl").read_text(encoding="utf-8").splitlines()]
    out = work / "judged.jsonl"
    done = {(j["qid"], j["form"], j["model"]) for j in load(out)} if out.exists() else set()
    todo = [r for r in runs if not r.get("error") and (r["qid"], r["form"], r["model"]) not in done]

    def one(r):
        q = qs[r["qid"]]
        prompt = (JUDGE.format(q=q["kerdes"], gold=q["answer"], evidence=q["evidence"], answer=r["answer"])
                  if q["kind"] == "answerable" else JUDGE_ABSENT.format(q=q["kerdes"], answer=r["answer"]))
        proc = subprocess.run([*CLAUDE, "-p", "--model", model, "--setting-sources", "project", "--no-session-persistence",
                               "--tools", "", "--strict-mcp-config"], input=prompt, capture_output=True, text=True,
                              encoding="utf-8", timeout=300, env=clean_env())
        word = re.search(r"\b(CORRECT|PARTLY|WRONG|NOTFOUND|ANSWERED)\b", proc.stdout.upper())
        return {"qid": r["qid"], "form": r["form"], "model": r["model"], "judge": model,
                "grade": word.group(1).lower() if word else "unclear"}

    with ThreadPoolExecutor(4) as pool, open(out, "a", encoding="utf-8") as sink:
        for j in pool.map(one, todo):
            sink.write(json.dumps(j) + "\n")
            sink.flush()
    print(f"judged {len(todo)}")


def report(argv):
    work, out = Path(argv[0]), Path(argv[1])
    qs = {q["qid"]: q for q in json.loads((work / "questions.json").read_text(encoding="utf-8"))}
    runs = {(r["qid"], r["form"]): r for r in load(work / "runs.jsonl") if not r.get("error")}
    grades = {(j["qid"], j["form"]): j["grade"] for j in load(work / "judged.jsonl")}
    forms = sorted({f for _, f in runs})
    result = {"questions": {"answerable": sum(q["kind"] == "answerable" for q in qs.values()),
                            "absent": sum(q["kind"] == "absent" for q in qs.values())}, "forms": {}}
    for f in forms:
        e = {}
        for kind in ("answerable", "absent"):
            keys = [(qid, f) for qid, q in qs.items() if q["kind"] == kind and (qid, f) in runs]
            if not keys:
                continue
            g = [grades.get(k, "unjudged") for k in keys]
            n = len(keys)
            rs = [runs[k] for k in keys]
            row = {"n": n,
                   "tokens_median": statistics.median(r["input_tokens"] for r in rs),
                   "calls_median": statistics.median(r["tool_calls"] for r in rs),
                   "seconds_median": statistics.median(r["seconds"] for r in rs),
                   "cost_usd_median": statistics.median(r["cost_usd"] or 0 for r in rs)}
            if kind == "answerable":
                for grade in ("correct", "partly", "wrong", "notfound"):
                    row[grade] = g.count(grade) / n
                opened = [r["opened_gold_room"] for r in rs if r.get("opened_gold_room") is not None]
                if opened:
                    row["opened_gold_room"] = sum(opened) / len(opened)
            else:
                row["abstained"] = g.count("notfound") / n
                row["made_up"] = g.count("answered") / n
            e[kind] = row
        result["forms"][f] = e
    c = {f: result["forms"][f].get("answerable", {}).get("correct") for f in forms}
    if c.get("oracle") is not None and c.get("kapu") is not None:
        result["retrieval_gap"] = c["oracle"] - c["kapu"]
    if c.get("kapu") is not None and c.get("closed") is not None:
        result["knowledge_gain"] = c["kapu"] - c["closed"]
    out.write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=1))


if __name__ == "__main__":
    {"run": run, "judge": judge, "report": report}[sys.argv[1]](sys.argv[2:])
