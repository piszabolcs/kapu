"""Call one kapu-mcp tool over stdio (official Python MCP client) and print the
answer, for an agent that may only reach KAPU through the MCP server.

    python tools/kapu_mcp_call.py list
    python tools/kapu_mcp_call.py call kapu_search '{"file": "...", "query": "..."}' [--tag q3]

Every call is appended to bench/korpusz/mcp-calls.jsonl (tag, tool, arguments,
time, response tokens) so a live run can be measured afterwards.
"""

import asyncio
import json
import sys
import time
from pathlib import Path

import tiktoken
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

ROOT = Path(__file__).resolve().parent.parent
LOG = ROOT / "bench" / "korpusz" / "mcp-calls.jsonl"
ENC = tiktoken.get_encoding("o200k_base")


async def main(argv):
    tag = None
    if "--tag" in argv:
        i = argv.index("--tag")
        tag = argv[i + 1]
        argv = argv[:i] + argv[i + 2:]
    exe = ROOT / "target" / "release" / "kapu-mcp.exe"
    env = {"KAPU_MODEL_DIR": str(ROOT / "models" / "potion-multilingual-128M-i8"), "SYSTEMROOT": "C:\\Windows"}
    async with stdio_client(StdioServerParameters(command=str(exe), env=env)) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            if argv[:1] == ["list"]:
                for t in (await session.list_tools()).tools:
                    print(f"## {t.name}\n{t.description}\ninput schema: {json.dumps(t.input_schema if hasattr(t, 'input_schema') else t.inputSchema, ensure_ascii=False)}\n")
                return 0
            if len(argv) != 3 or argv[0] != "call":
                print(__doc__)
                return 2
            tool, args = argv[1], json.loads(argv[2])
            start = time.perf_counter()
            result = await session.call_tool(tool, args)
            ms = (time.perf_counter() - start) * 1000
            text = "".join(getattr(c, "text", "") for c in result.content)
            print(text)
            LOG.parent.mkdir(parents=True, exist_ok=True)
            with LOG.open("a", encoding="utf-8") as log:
                log.write(json.dumps({"tag": tag, "tool": tool, "args": args, "ms": round(ms, 1), "tokens": len(ENC.encode(text)), "error": bool(getattr(result, "is_error", False))}, ensure_ascii=False) + "\n")
            return 1 if getattr(result, "is_error", False) else 0


if __name__ == "__main__":
    sys.exit(asyncio.run(main(sys.argv[1:])))
