"""K-01, K-02, K-22 at a given size: tool-list price, map price and open times of
the MCP server on one file (spec v1.2, D145).

    tools/.venv/Scripts/python tools/study/measure_scale.py <kapu-mcp> <file.kapu> [n]

Opens `n` (default 50) doors spread over the file one by one and prints the
median and 90th percentile time, and the price of the first and second map.
"""
import json
import statistics
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from measure_v12 import Server  # noqa: E402


def main():
    exe, file = sys.argv[1], sys.argv[2]
    n = int(sys.argv[3]) if len(sys.argv) > 3 else 50
    s = Server(exe)
    tools = s.rpc("tools/list", {})["result"]["tools"]
    _, first, _ = s.call("kapu_map", {"file": file})
    _, second, _ = s.call("kapu_map", {"file": file})
    head = Path(file).read_bytes()[:512].decode()
    fields = dict(l.split("=", 1) for l in head.split("\n")[1:] if "=" in l)
    start, length = map(int, fields["docs"].split("+"))
    with open(file, "rb") as f:
        f.seek(start)
        docs = f.read(length).decode().split("\n")[1:-2]
    ids = [line.split("\t", 1)[0] for line in docs]
    picked = ids[:: max(1, len(ids) // n)][:n]
    times = []
    for door in picked:
        _, _, ms = s.call("kapu_open", {"file": file, "ids": [door]})
        times.append(ms)
    s.close()
    times.sort()
    print(json.dumps({
        "file": Path(file).name,
        "rooms": int(fields["rooms"]),
        "doors": int(fields["doors"]),
        "tools": len(tools),
        "map_first_tokens": first,
        "map_next_tokens": second,
        "open_ms_median": round(statistics.median(times), 2),
        "open_ms_p90": round(times[int(0.9 * (len(times) - 1))], 2),
    }))


if __name__ == "__main__":
    main()
