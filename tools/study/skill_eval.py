"""Skill finding: native Claude Code skills, the same skills in one KAPU file, and
a folder of Markdown files (protocol v2, chapter 4, J1; D153).

    python tools/study/skill_eval.py prepare <pool-dir> <work-dir> [--small 45]
    python tools/study/skill_eval.py run <work-dir> [--model opus] [--jobs 3]
    python tools/study/skill_eval.py baseline <work-dir> [--model opus]
    python tools/study/skill_eval.py report <work-dir> <out.json>

`prepare` builds two skill sets from the pool of `skill_pool.py`: `s` (every
gold skill of `skill_tasks.json` and random others up to --small) and `l` (the
whole pool), each in three forms: a project with the skills in
`.claude/skills/` (native), a folder `skills/` of skill folders (md), and
`skills.kapu` made with `kapu import --skills` (kapu).

`run` gives every task to an isolated `claude -p` per form and set, with a
prompt that points at the skills (the user's design): native may only use
the Skill tool, md only Read, Glob and Grep, kapu only the kapu MCP tools.
A skill counts as loaded when the Skill tool was called with it, one of its
files was read, or one of its rooms was opened. Runs are appended to
`<work-dir>/runs.jsonl`; a second `run` skips the finished ones. `baseline`
measures the fixed cost of a session in each form: one turn of a prompt that
needs no skill (the native skill list is in every session's system prompt).

Needs a logged-in `claude` CLI; uses the user's Claude Code account.
"""

import argparse
import json
import os
import random
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TASKS = Path(__file__).with_name("skill_tasks.json")
_EXE = Path(os.environ.get("APPDATA", "")) / "npm" / "node_modules" / "@anthropic-ai" / "claude-code" / "bin" / "claude.exe"
CLAUDE = [str(_EXE)] if _EXE.is_file() else ["claude"]
FORMS = ["native", "kapu", "md"]
KAPU_TOOLS = ["kapu_map", "kapu_search", "kapu_describe", "kapu_open", "kapu_ask"]
ROOM = re.compile(r'<kapu-room file="[^"]*" id="([^"]+)"')

ENDING = ("Do not do the task itself: when you have the relevant skills, stop and list their names, "
          "comma separated, between <skills> and </skills>.")
PROMPTS = {
    "native": "Task: {task}\n\nBefore you start, load every skill that is relevant to this task with the Skill tool. " + ENDING,
    "kapu": ("Task: {task}\n\nMy skills are in the KAPU knowledge file {file}; reach it with the kapu MCP tools. "
             "Before you start, find and open every skill in it that is relevant to this task. " + ENDING),
    "md": ("Task: {task}\n\nMy skills are in the folder {dir}, one folder per skill with a SKILL.md. "
           "Before you start, find and read every skill there that is relevant to this task. " + ENDING),
}


def tasks():
    return json.loads(TASKS.read_text(encoding="utf-8"))["tasks"]


def gold_names(task):
    return {n for group in task["gold"] for n in group}


def prepare(pool: Path, work: Path, small: int, kapu: Path, seed: int = 26):
    names = sorted(json.loads((pool / "pool.json").read_text(encoding="utf-8")))
    gold = sorted(set().union(*(gold_names(t) for t in tasks())))
    missing = [g for g in gold if g not in names]
    if missing:
        sys.exit(f"gold skills not in the pool: {missing}")
    others = [n for n in names if n not in gold]
    sets = {"s": sorted(gold + random.Random(seed).sample(others, max(0, small - len(gold)))), "l": names}
    for key, members in sets.items():
        base = work / key
        if base.exists():
            shutil.rmtree(base)
        for name in members:
            shutil.copytree(pool / name, base / "native" / ".claude" / "skills" / name)
            shutil.copytree(pool / name, base / "md" / "skills" / name)
        (base / "kapu").mkdir(parents=True)
        out = subprocess.run([str(kapu), "import", "--skills", str(base / "md" / "skills"), str(base / "kapu" / "skills.kapu")],
                             capture_output=True, text=True, encoding="utf-8")
        if out.returncode:
            sys.exit(out.stderr)
        (base / "members.json").write_text(json.dumps(members), encoding="utf-8")
        print(f"set {key}: {len(members)} skills, {(base / 'kapu' / 'skills.kapu').stat().st_size / 1e6:.2f} MB kapu")


def clean_env():
    # Inside the Claude desktop app the parent session's proxy and bare-mode
    # variables would stop the child from using the CLI login.
    return {k: v for k, v in os.environ.items()
            if not k.startswith(("CLAUDE", "ANTHROPIC")) and k not in {"USE_LOCAL_OAUTH", "USE_STAGING_OAUTH"}}


def command(form, model, mcp_config):
    cmd = [*CLAUDE, "-p", "--model", model, "--setting-sources", "project", "--no-session-persistence",
           "--output-format", "stream-json", "--verbose", "--strict-mcp-config"]
    if form == "native":
        return cmd + ["--tools", "Skill", "--allowedTools", "Skill"]
    if form == "md":
        return cmd + ["--tools", "Read,Glob,Grep", "--allowedTools", "Read", "Glob", "Grep"]
    return cmd + ["--mcp-config", str(mcp_config), "--tools", "",
                  "--allowedTools", " ".join(f"mcp__kapu__{t}" for t in KAPU_TOOLS)]


def owner(text, members):
    """The skill a room id, a path or a written name belongs to (longest name first)."""
    text = text.strip().strip("`*'\" ").lower().replace("\\", "/")
    text = text.split(":")[-1] if ":" in text and "/" not in text else text
    for name in sorted(members, key=len, reverse=True):
        if text == name or re.search(rf"(^|/){re.escape(name)}(/|-|$)", text):
            return name
    return None


def run_one(task, form, key, work: Path, model, mcp_config):
    base = work / key
    members = json.loads((base / "members.json").read_text(encoding="utf-8"))
    cwd = base / form
    prompt = PROMPTS[form].format(task=task["task"], file=(cwd / "skills.kapu").as_posix(), dir=(cwd / "skills").as_posix())
    started = time.perf_counter()
    proc = subprocess.run(command(form, model, mcp_config), cwd=cwd, input=prompt, capture_output=True, text=True,
                          encoding="utf-8", timeout=900, env=clean_env())
    seconds = time.perf_counter() - started
    calls, loaded, outside, result, init = [], [], [], {}, {}
    pending = {}
    for line in proc.stdout.splitlines():
        try:
            event = json.loads(line)
        except ValueError:
            continue
        kind = event.get("type")
        if kind == "system" and event.get("subtype") == "init":
            init = event
        elif kind == "assistant":
            for block in event.get("message", {}).get("content", []):
                if block.get("type") != "tool_use":
                    continue
                name, args = block.get("name", "").replace("mcp__kapu__", ""), block.get("input", {})
                calls.append({"tool": name, "input": args})
                pending[block.get("id")] = name
                if name == "Skill":
                    skill = str(args.get("skill", ""))
                    hit = owner(skill, members)
                    (loaded if hit else outside).append(hit or skill)
                elif name == "Read":
                    hit = owner(str(args.get("file_path", "")).split("/skills/", 1)[-1], members)
                    if hit:
                        loaded.append(hit)
        elif kind == "user":
            for block in event.get("message", {}).get("content", []) or []:
                if not isinstance(block, dict) or block.get("type") != "tool_result":
                    continue
                if pending.get(block.get("tool_use_id")) not in ("kapu_open", "kapu_ask"):
                    continue
                content = block.get("content")
                text = content if isinstance(content, str) else " ".join(c.get("text", "") for c in content or [] if isinstance(c, dict))
                for room in ROOM.findall(text):
                    hit = owner(room, members)
                    if hit:
                        loaded.append(hit)
        elif kind == "result":
            result = event
    text = result.get("result", "") or ""
    m = re.search(r"<skills>(.*?)</skills>", text, re.S)
    named = [owner(n, members) for n in re.split(r"[,\n]", m.group(1))] if m else []
    usage = result.get("usage", {}) or {}
    return {
        "task": task["id"], "form": form, "set": key, "model": model, "set_size": len(members),
        "claude_code": init.get("claude_code_version"),
        "native_skills_listed": len(init.get("skills", [])) if form == "native" else None,
        "loaded": sorted(set(loaded)), "outside_pool": sorted(set(outside)), "named": sorted({n for n in named if n}),
        "tool_calls": len(calls), "tools": [c["tool"] for c in calls], "turns": result.get("num_turns"),
        "seconds": round(seconds, 1),
        "input_tokens": sum(usage.get(k) or 0 for k in ("input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens")),
        "output_tokens": usage.get("output_tokens") or 0, "cost_usd": result.get("total_cost_usd"),
        "answer": text[-600:], "error": None if result and not result.get("is_error") else (text or proc.stderr.strip()[-400:] or "no result"),
    }


def run(work: Path, model: str, jobs: int, kapu_mcp: Path, model_dir: Path, only=None,
        forms=FORMS, runs_name="runs.jsonl"):
    out = work / runs_name
    done = set()
    if out.exists():
        for line in out.read_text(encoding="utf-8").splitlines():
            r = json.loads(line)
            if not r.get("error"):
                done.add((r["task"], r["form"], r["set"], r["model"]))
    todo = [(t, f, k) for k in ("s", "l") for t in tasks() for f in forms
            if (t["id"], f, k, model) not in done and (not only or t["id"] in only)]
    # Absolute paths: every run starts in its own folder.
    config = {"mcpServers": {"kapu": {"command": str(kapu_mcp.resolve()), "env": {"KAPU_MODEL_DIR": str(model_dir.resolve())}}}}
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False, encoding="utf-8") as f:
        json.dump(config, f)
        mcp_config = f.name
    lock = threading.Lock()
    print(f"{len(todo)} runs, {jobs} at a time", flush=True)

    def one(item):
        task, form, key = item
        try:
            r = run_one(task, form, key, work, model, mcp_config)
        except subprocess.TimeoutExpired:
            r = {"task": task["id"], "form": form, "set": key, "model": model, "error": "timeout"}
        gold = task["gold"]
        found = sum(any(n in r.get("loaded", []) for n in g) for g in gold)
        with lock:
            with out.open("a", encoding="utf-8") as f:
                f.write(json.dumps(r, ensure_ascii=False) + "\n")
            print(f"{key} {form:6} {task['id']:18} gold {found}/{len(gold)} loaded={r.get('loaded')} "
                  f"in={r.get('input_tokens')} {r.get('seconds')}s {('ERROR ' + str(r['error'])[:120]) if r.get('error') else ''}", flush=True)

    with ThreadPoolExecutor(jobs) as pool:
        list(pool.map(one, todo))
    os.unlink(mcp_config)


def baseline(work: Path, model: str, kapu_mcp: Path, model_dir: Path, forms=FORMS, name="baseline.json"):
    """The fixed cost of a session in each form and set: one turn of a prompt that needs no skill."""
    config = {"mcpServers": {"kapu": {"command": str(kapu_mcp.resolve()), "env": {"KAPU_MODEL_DIR": str(model_dir.resolve())}}}}
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False, encoding="utf-8") as f:
        json.dump(config, f)
        mcp_config = f.name
    rows = {}
    for key in ("s", "l"):
        for form in forms:
            proc = subprocess.run([*command(form, model, mcp_config), "--max-turns", "1"], cwd=work / key / form,
                                  input="Reply with the single word OK.", capture_output=True, text=True,
                                  encoding="utf-8", timeout=300, env=clean_env())
            events = [json.loads(line) for line in proc.stdout.splitlines() if line.startswith("{")]
            usage = next((e.get("usage", {}) for e in events if e.get("type") == "result"), {})
            rows[f"{key}/{form}"] = sum(usage.get(k) or 0 for k in ("input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"))
            print(key, form, rows[f"{key}/{form}"], flush=True)
    os.unlink(mcp_config)
    old = json.loads((work / name).read_text(encoding="utf-8")) if (work / name).exists() else {}
    (work / name).write_text(json.dumps({**old, **rows}, indent=1), encoding="utf-8")


def median(values):
    values = sorted(v for v in values if v is not None)
    if not values:
        return None
    mid = len(values) // 2
    return values[mid] if len(values) % 2 else (values[mid - 1] + values[mid]) / 2


def report(work: Path, out: Path, runs_names=("runs.jsonl",), baselines=("baseline.json",)):
    by_task = {t["id"]: t for t in tasks()}
    # a later file's run of the same task, form, set and model replaces an earlier one
    runs = [json.loads(line) for name in runs_names for line in (work / name).read_text(encoding="utf-8").splitlines()]
    latest = {}
    for r in runs:
        if not r.get("error"):
            latest[(r["task"], r["form"], r["set"], r["model"])] = r
    rows = []
    for r in latest.values():
        task = by_task[r["task"]]
        allowed = gold_names(task) | set(task["ok"])
        r["gold"] = len(task["gold"])
        r["gold_loaded"] = sum(any(n in r["loaded"] for n in g) for g in task["gold"])
        r["gold_named"] = sum(any(n in r["named"] for n in g) for g in task["gold"])
        r["wrong_loaded"] = sorted(set(r["loaded"]) - allowed)
        rows.append(r)
    fixed = {}
    for name in baselines:
        if (work / name).exists():
            fixed.update(json.loads((work / name).read_text(encoding="utf-8")))
    summary = {}
    for key in ("s", "l"):
        for form in FORMS:
            sel = [r for r in rows if r["set"] == key and r["form"] == form]
            if not sel:
                continue
            gold = sum(r["gold"] for r in sel)
            summary[f"{key}/{form}"] = {
                "set_size": sel[0]["set_size"], "runs": len(sel),
                "gold_loaded": sum(r["gold_loaded"] for r in sel), "gold_named": sum(r["gold_named"] for r in sel), "gold": gold,
                "recall_loaded": round(sum(r["gold_loaded"] for r in sel) / gold, 3),
                "recall_named": round(sum(r["gold_named"] for r in sel) / gold, 3),
                "tasks_all_gold_loaded": sum(r["gold_loaded"] == r["gold"] for r in sel),
                "wrong_loaded": sum(len(r["wrong_loaded"]) for r in sel),
                "outside_pool_loaded": sum(len(r.get("outside_pool", [])) for r in sel),
                "input_tokens_median": median(r["input_tokens"] for r in sel),
                "output_tokens_median": median(r["output_tokens"] for r in sel),
                # what the CLI charges, cache reads at their lower price
                "cost_usd_median": round(median(r["cost_usd"] for r in sel), 4),
                "tool_calls_median": median(r["tool_calls"] for r in sel),
                "seconds_median": median(r["seconds"] for r in sel),
                "native_skills_listed": sel[0].get("native_skills_listed"),
                "session_fixed_tokens": fixed.get(f"{key}/{form}"),
            }
    keep = ("task", "form", "set", "set_size", "gold", "gold_loaded", "gold_named", "loaded", "named", "wrong_loaded",
            "outside_pool", "tool_calls", "tools", "turns", "seconds", "input_tokens", "output_tokens", "cost_usd")
    data = {"date": time.strftime("%Y-%m-%d"), "model": sorted({r["model"] for r in rows}),
            "claude_code": sorted({r.get("claude_code") for r in rows if r.get("claude_code")}),
            "tasks": tasks(), "summary": summary,
            "runs": sorted(({k: r.get(k) for k in keep} for r in rows), key=lambda r: (r["set"], r["task"], r["form"]))}
    out.write_text(json.dumps(data, indent=1, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")
    for name, s in summary.items():
        print(f"{name:9} n={s['set_size']:3} loaded {s['gold_loaded']}/{s['gold']} ({s['recall_loaded']:.0%}) "
              f"named {s['recall_named']:.0%} all-gold {s['tasks_all_gold_loaded']}/{s['runs']} wrong {s['wrong_loaded']} "
              f"outside {s['outside_pool_loaded']} in-tokens {s['input_tokens_median']} ${s['cost_usd_median']} "
              f"calls {s['tool_calls_median']} {s['seconds_median']}s")


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("prepare")
    p.add_argument("pool", type=Path)
    p.add_argument("work", type=Path)
    p.add_argument("--small", type=int, default=45)
    p.add_argument("--kapu", type=Path, default=ROOT / "target" / "release" / "kapu.exe")
    p = sub.add_parser("run")
    p.add_argument("work", type=Path)
    p.add_argument("--model", default="opus")
    p.add_argument("--jobs", type=int, default=3)
    p.add_argument("--kapu-mcp", type=Path, default=ROOT / "target" / "release" / "kapu-mcp.exe")
    p.add_argument("--model-dir", type=Path, default=ROOT / "models" / "potion-multilingual-128M-i8")
    p.add_argument("--only", nargs="*")
    p.add_argument("--forms", nargs="*", default=FORMS, choices=FORMS)
    p.add_argument("--runs", default="runs.jsonl", help="file in the work dir the runs are appended to")
    p = sub.add_parser("baseline")
    p.add_argument("work", type=Path)
    p.add_argument("--model", default="opus")
    p.add_argument("--kapu-mcp", type=Path, default=ROOT / "target" / "release" / "kapu-mcp.exe")
    p.add_argument("--model-dir", type=Path, default=ROOT / "models" / "potion-multilingual-128M-i8")
    p.add_argument("--forms", nargs="*", default=FORMS, choices=FORMS)
    p.add_argument("--out", default="baseline.json", help="file in the work dir")
    p = sub.add_parser("report")
    p.add_argument("work", type=Path)
    p.add_argument("out", type=Path)
    p.add_argument("--runs", nargs="*", default=["runs.jsonl"], help="run files, later ones replace earlier runs")
    p.add_argument("--baselines", nargs="*", default=["baseline.json"])
    a = ap.parse_args()
    if a.cmd == "prepare":
        prepare(a.pool, a.work.resolve(), a.small, a.kapu)
    elif a.cmd == "run":
        run(a.work.resolve(), a.model, a.jobs, a.kapu_mcp, a.model_dir, a.only, a.forms, a.runs)
    elif a.cmd == "baseline":
        baseline(a.work.resolve(), a.model, a.kapu_mcp, a.model_dir, a.forms, a.out)
    else:
        report(a.work.resolve(), a.out, a.runs, a.baselines)


if __name__ == "__main__":
    main()
