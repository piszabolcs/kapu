"""Probe kapu-mcp over stdio with the official Python MCP client: tool list and
annotations, then the latency and the token size of every read tool on a file.

    python tools/mcp_probe.py <file.kapu> <questions.jsonl> [--runs 5] [--json out.json]

The searches use the questions of the benchmark file, the opens use the first
hit of each search. Prices are counted with o200k_base (tiktoken), like FR-08.
"""

import argparse
import asyncio
import json
import statistics
import time
from pathlib import Path

import tiktoken
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

ROOT = Path(__file__).resolve().parent.parent
ENC = tiktoken.get_encoding("o200k_base")


def text_of(result):
    return "".join(getattr(c, "text", "") for c in result.content)


async def timed(session, tool, args):
    start = time.perf_counter()
    result = await session.call_tool(tool, args)
    ms = (time.perf_counter() - start) * 1000
    text = text_of(result)
    return ms, len(ENC.encode(text)), bool(getattr(result, "is_error", None) or getattr(result, "isError", False)), text


def stats(values):
    values = sorted(values)
    return {
        "n": len(values),
        "median": round(statistics.median(values), 1),
        "p90": round(values[min(len(values) - 1, int(len(values) * 0.9))], 1),
        "max": round(values[-1], 1),
    }


async def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("file")
    ap.add_argument("questions")
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--json")
    a = ap.parse_args()
    file = str(Path(a.file).resolve())
    questions = [json.loads(l)["kerdes"] for l in Path(a.questions).read_text(encoding="utf-8").splitlines() if l.strip()]
    exe = ROOT / "target" / "release" / "kapu-mcp.exe"
    env = {"KAPU_MODEL_DIR": str(ROOT / "models" / "potion-multilingual-128M-i8"), "SYSTEMROOT": "C:\\Windows"}
    params = StdioServerParameters(command=str(exe), env=env)
    report = {}
    started = time.perf_counter()
    async with stdio_client(params) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            report["initialize_ms"] = round((time.perf_counter() - started) * 1000, 1)
            tools = (await session.list_tools()).tools
            report["tools"] = [
                {
                    "name": t.name,
                    "read_only": getattr(t.annotations, "read_only_hint", getattr(t.annotations, "readOnlyHint", None)),
                    "destructive": getattr(t.annotations, "destructive_hint", getattr(t.annotations, "destructiveHint", None)),
                    "description_tokens": len(ENC.encode(t.description or "")),
                }
                for t in tools
            ]
            ms, tok, err, _ = await timed(session, "kapu_map", {"file": file})
            report["map_first"] = {"ms": round(ms, 1), "tokens": tok, "error": err}
            maps = [await timed(session, "kapu_map", {"file": file}) for _ in range(a.runs)]
            report["map"] = {"ms": stats([m[0] for m in maps]), "tokens": maps[0][1]}
            ms, tok, err, _ = await timed(session, "kapu_map", {"file": file, "group": "g1"})
            report["map_group_g1"] = {"ms": round(ms, 1), "tokens": tok, "error": err}
            searches, opens, describes, first_search = [], [], [], None
            for i, q in enumerate(questions):
                ms, tok, err, text = await timed(session, "kapu_search", {"file": file, "query": q, "k": 5})
                if first_search is None:
                    first_search = round(ms, 1)
                else:
                    searches.append((ms, tok))
                top = text.split(" | ", 1)[0].strip() if " | " in text else None
                if top and i < 25:
                    o = await timed(session, "kapu_open", {"file": file, "ids": [top]})
                    opens.append(o[:2])
                    d = await timed(session, "kapu_describe", {"file": file, "ids": [top]})
                    describes.append(d[:2])
            report["search_first_ms"] = first_search
            report["search"] = {"ms": stats([s[0] for s in searches]), "tokens": stats([s[1] for s in searches])}
            report["open"] = {"ms": stats([o[0] for o in opens]), "tokens": stats([o[1] for o in opens])}
            report["describe"] = {"ms": stats([d[0] for d in describes]), "tokens": stats([d[1] for d in describes])}
            ms, tok, err, text = await timed(session, "kapu_open", {"file": file, "ids": ["no-such-door"]})
            report["error_example"] = text.strip()[:200]
    print(json.dumps(report, indent=2, ensure_ascii=False))
    if a.json:
        Path(a.json).write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")


if __name__ == "__main__":
    asyncio.run(main())
