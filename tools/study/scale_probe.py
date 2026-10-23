"""F0.5 scale probe (study pilot): build one size and measure it.

    python tools/study/scale_probe.py <rooms_dir> <file.kapu> <out.json>

Records the import (wall time, peak working set, warnings), the file size, a
full validation, a CLI map call (peak memory: the reader holds the whole
file), and over one MCP session the map, every group list, 20 searches and
their opens (tools/mcp_probe.py). Queries are 20 room titles (seed 21).
"""

import asyncio
import json
import os
import random
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import psutil

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from mcp import ClientSession, StdioServerParameters  # noqa: E402
from mcp.client.stdio import stdio_client  # noqa: E402
from mcp_probe import stats, timed  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent.parent
BIN = Path(os.environ.get("KAPU_BIN_DIR") or ROOT / "target" / "release")
KAPU = BIN / "kapu.exe"
MODEL = ROOT / "models" / "potion-multilingual-128M-i8"


def run_measured(cmd):
    """Runs a command; returns (seconds, peak working set in MB, exit code, stdout)."""
    # output goes to temp files: a full pipe would block the child while we poll its memory
    with tempfile.TemporaryFile() as fo, tempfile.TemporaryFile() as fe:
        start = time.perf_counter()
        proc = subprocess.Popen(cmd, stdout=fo, stderr=fe)
        ps, peak = psutil.Process(proc.pid), 0
        while proc.poll() is None:
            try:
                info = ps.memory_info()
                peak = max(peak, getattr(info, "peak_wset", info.rss))
            except psutil.Error:
                pass
            time.sleep(0.05)
        secs = round(time.perf_counter() - start, 2)
        fo.seek(0)
        fe.seek(0)
        return secs, round(peak / 2**20, 1), proc.returncode, fo.read().decode("utf-8", "replace"), fe.read().decode("utf-8", "replace")


async def mcp_part(file: str, queries):
    env = {"KAPU_MODEL_DIR": str(MODEL), "SYSTEMROOT": "C:\\Windows"}
    report = {}
    async with stdio_client(StdioServerParameters(command=str(BIN / "kapu-mcp.exe"), env=env)) as (r, w):
        async with ClientSession(r, w) as s:
            await s.initialize()
            ms, tok, err, first = await timed(s, "kapu_map", {"file": file})
            report["map_first"] = {"ms": round(ms, 1), "tokens": tok, "error": err}
            maps = [await timed(s, "kapu_map", {"file": file}) for _ in range(3)]
            report["map"] = {"ms": stats([m[0] for m in maps]), "tokens": maps[0][1]}
            report["totals_line"] = next((l for l in maps[0][3].splitlines() if l.startswith("rooms=")), None)
            gids = [l.split(" ", 2)[1] for l in maps[0][3].splitlines() if l.startswith("G ")]
            groups = [await timed(s, "kapu_map", {"file": file, "group": g}) for g in gids]
            report["groups"] = {"n": len(gids), "tokens": stats([g[1] for g in groups]) if groups else None, "sum_tokens": sum(g[1] for g in groups)}
            searches, opens, first_ms = [], [], None
            for q in queries:
                ms, tok, err, text = await timed(s, "kapu_search", {"file": file, "query": q, "k": 5})
                if first_ms is None:
                    first_ms = round(ms, 1)
                else:
                    searches.append((ms, tok))
                top = text.split(" | ", 1)[0].strip() if " | " in text else None
                if top:
                    o = await timed(s, "kapu_open", {"file": file, "ids": [top]})
                    opens.append(o[:2])
            report["search_first_ms"] = first_ms
            report["search"] = {"ms": stats([x[0] for x in searches]), "tokens": stats([x[1] for x in searches])}
            report["open"] = {"ms": stats([x[0] for x in opens]), "tokens": stats([x[1] for x in opens])}
    return report


def main(rooms: Path, file: Path, out: Path):
    report = {"rooms_dir": str(rooms), "file": str(file)}
    secs, peak, code, stdout, stderr = run_measured([str(KAPU), "--json", "import", "--model", str(MODEL), str(rooms), str(file)])
    report["import"] = {"seconds": secs, "peak_mb": peak, "exit": code, "stderr_tail": stderr[-400:]}
    try:
        info = json.loads(stdout)
        report["import"]["warnings"] = len(info.get("warnings", [])) if isinstance(info, dict) else None
    except json.JSONDecodeError:
        report["import"]["stdout_tail"] = stdout[-400:]
    if code != 0:
        out.write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(json.dumps(report, indent=2))
        return 1
    report["file_mb"] = round(file.stat().st_size / 2**20, 1)
    secs, peak, code, _, err = run_measured([str(KAPU), "validate", "--full", "--model", str(MODEL), str(file)])
    report["validate_full"] = {"seconds": secs, "peak_mb": peak, "exit": code, "stderr_tail": err[-300:]}
    secs, peak, code, _, _ = run_measured([str(KAPU), "map", str(file)])
    report["cli_map"] = {"seconds": secs, "peak_mb": peak, "exit": code}
    titles = [json.loads(json.dumps(v))["title"] for v in json.loads((rooms.parent / f"{rooms.name}.meta" / "manifest.json").read_text(encoding="utf-8")).values()]
    random.Random(21).shuffle(titles)
    report["mcp"] = asyncio.run(mcp_part(str(file.resolve()), titles[:21]))
    out.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
    print(json.dumps(report, indent=2, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main(Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])))
