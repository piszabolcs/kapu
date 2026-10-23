"""Shared call log of the study-pilot agent tools: one JSON line per call, the
same shape as tools/kapu_mcp_call.py (tag, tool, args, ms, tokens, error).
The path is `KAPU_CALL_LOG`, else bench/korpusz/mcp-calls.jsonl."""

import json
import os
import sys
import time
from pathlib import Path

import tiktoken

ROOT = Path(__file__).resolve().parent.parent.parent
ENC = tiktoken.get_encoding("o200k_base")


def log_path() -> Path:
    return Path(os.environ.get("KAPU_CALL_LOG") or ROOT / "bench" / "korpusz" / "mcp-calls.jsonl")


def pop_tag(argv):
    if "--tag" in argv:
        i = argv.index("--tag")
        return argv[i + 1], argv[:i] + argv[i + 2:]
    return None, argv


def run(tool: str, args: dict, tag, fn) -> int:
    """Runs `fn() -> str`, prints the text and logs the call; an exception is a logged error."""
    start = time.perf_counter()
    try:
        text, error = fn(), False
    except Exception as e:  # noqa: BLE001 - every failure goes back to the agent as text
        text, error = f"error: {e}", True
    ms = (time.perf_counter() - start) * 1000
    sys.stdout.reconfigure(encoding="utf-8")
    print(text)
    path = log_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as f:
        f.write(json.dumps({"tag": tag, "tool": tool, "args": args, "ms": round(ms, 1),
                            "tokens": len(ENC.encode(text, disallowed_special=())), "error": error}, ensure_ascii=False) + "\n")
    return 1 if error else 0
