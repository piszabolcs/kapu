"""Live evaluation of kapu-mcp with Claude Code (mcp-builder evaluation format).

Every question of the XML file runs as `claude -p` with no built-in tools, only
the four read tools of the KAPU server, and the answer is compared with the
expected one. Records the tool calls, the tokens and the time of every run.

    python tools/mcp_eval.py <evaluation.xml> <file.kapu> [--model sonnet] [--out report.json]

Needs a logged-in `claude` CLI; uses the user's Claude Code account.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
READ_TOOLS = ["kapu_map", "kapu_search", "kapu_describe", "kapu_open"]
# The npm shim is a .cmd file; run the package's own executable (older
# versions: its script with node) so the prompt, sent on stdin, never passes
# through cmd.exe quoting.
_PKG = Path(os.environ.get("APPDATA", "")) / "npm" / "node_modules" / "@anthropic-ai" / "claude-code"
CLAUDE = ([str(_PKG / "bin" / "claude.exe")] if (_PKG / "bin" / "claude.exe").is_file()
          else ["node", str(_PKG / "cli.js")] if (_PKG / "cli.js").is_file() else ["claude"])

PROMPT = """Answer the question below using only the KAPU tools. The knowledge file is:
{file}

Look at the map or search first, then open only the doors you need. When you are done,
give your final answer, as short as possible, between <response> and </response>. If the
file does not hold the answer, give <response>NOT_FOUND</response>.

Question: {question}"""


def normalise(text):
    return re.sub(r"[\s`'\".]+", " ", text).strip().lower()


def run(question, file, model, mcp_config):
    cmd = [
        *CLAUDE, "-p",
        "--model", model,
        "--mcp-config", str(mcp_config), "--strict-mcp-config",
        "--tools", "",
        "--allowedTools", " ".join(f"mcp__kapu__{t}" for t in READ_TOOLS),
        "--setting-sources", "project",
        "--no-session-persistence",
        "--output-format", "stream-json", "--verbose",
    ]
    # A clean environment: inside the Claude desktop app the parent session's proxy
    # and bare-mode variables would stop the child from using the CLI login.
    env = {
        k: v for k, v in os.environ.items()
        if not k.startswith(("CLAUDE", "ANTHROPIC")) and k not in {"USE_LOCAL_OAUTH", "USE_STAGING_OAUTH"}
    }
    started = time.perf_counter()
    with tempfile.TemporaryDirectory() as cwd:
        proc = subprocess.run(
            cmd, cwd=cwd, input=PROMPT.format(file=file, question=question),
            capture_output=True, text=True, encoding="utf-8", timeout=900, env=env,
        )
    seconds = time.perf_counter() - started
    calls, result = [], {}
    for line in proc.stdout.splitlines():
        try:
            event = json.loads(line)
        except ValueError:
            continue
        if event.get("type") == "assistant":
            for block in event.get("message", {}).get("content", []):
                if block.get("type") == "tool_use":
                    calls.append({"tool": block.get("name", "").replace("mcp__kapu__", ""), "input": block.get("input", {})})
        elif event.get("type") == "result":
            result = event
    text = result.get("result", "") or ""
    m = re.search(r"<response>(.*?)</response>", text, re.S)
    usage = result.get("usage", {}) or {}
    return {
        "answer": (m.group(1).strip() if m else text.strip())[:300],
        "tool_calls": calls,
        "turns": result.get("num_turns"),
        "seconds": round(seconds, 1),
        "input_tokens": (usage.get("input_tokens") or 0) + (usage.get("cache_read_input_tokens") or 0) + (usage.get("cache_creation_input_tokens") or 0),
        "output_tokens": usage.get("output_tokens") or 0,
        "cost_usd": result.get("total_cost_usd"),
        "error": None if result else (proc.stderr.strip()[-400:] or "no result event"),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("evaluation")
    ap.add_argument("file")
    ap.add_argument("--model", default="sonnet")
    ap.add_argument("--out")
    ap.add_argument("--only", type=int, help="run only the n-th question (1-based)")
    a = ap.parse_args()
    file = str(Path(a.file).resolve())
    pairs = [(q.findtext("question").strip(), q.findtext("answer").strip()) for q in ET.parse(a.evaluation).getroot().iter("qa_pair")]
    if a.only:
        pairs = [pairs[a.only - 1]]
    config = {
        "mcpServers": {
            "kapu": {
                "command": str(ROOT / "target" / "release" / "kapu-mcp.exe"),
                "env": {"KAPU_MODEL_DIR": str(ROOT / "models" / "potion-multilingual-128M-i8")},
            }
        }
    }
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False, encoding="utf-8") as f:
        json.dump(config, f)
        mcp_config = f.name
    rows = []
    for i, (question, expected) in enumerate(pairs, 1):
        r = run(question, file, a.model, mcp_config)
        r["question"], r["expected"] = question, expected
        r["correct"] = normalise(expected) in normalise(r["answer"])
        rows.append(r)
        tools = ",".join(c["tool"].replace("kapu_", "") for c in r["tool_calls"])
        print(f"{i:2} {'OK  ' if r['correct'] else 'MISS'} {r['seconds']:6.1f}s calls={len(r['tool_calls']):2} [{tools}] answer={r['answer'][:60]!r} expected={expected!r}", flush=True)
        if r["error"]:
            print("   error:", r["error"], file=sys.stderr)
    os.unlink(mcp_config)
    summary = {
        "model": a.model,
        "questions": len(rows),
        "correct": sum(r["correct"] for r in rows),
        "tool_calls_median": sorted(len(r["tool_calls"]) for r in rows)[len(rows) // 2] if rows else 0,
        "rows": rows,
    }
    print(f"accuracy {summary['correct']}/{summary['questions']}")
    if a.out:
        Path(a.out).write_text(json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8")


if __name__ == "__main__":
    main()
