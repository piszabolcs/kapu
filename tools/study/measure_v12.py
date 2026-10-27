"""Reply budgets and one-call answers of the MCP server (spec v1.2, D145, D146).

    tools/.venv/Scripts/python tools/study/measure_v12.py <kapu-mcp> <out.json> \
        <name>=<file.kapu>:<questions.jsonl> ...

One persistent `kapu-mcp` session per file, over stdio. Per file:
  - K-02: price of the first `kapu_map` (with the guide) and of the next one;
  - K-05: price and time of `kapu_search` (k=10, excerpts) per question;
  - K-07: `kapu_ask` per question: price, time, and whether a door of the
    expected room is among the opened frames (answerable in one call);
  - K-22: time of `kapu_open` on the expected room's first door.
Prices are the `reply: N tokens` line (K-08). Expected room as `kapu bench`:
the slug of the answer file's stem (D49).
"""
import json
import re
import statistics
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from build_vectors import slug  # noqa: E402

FRAME_ID = re.compile(r'<kapu-room file="[^"]*" id="([^"]+)"')


class Server:
    def __init__(self, exe):
        # `exe` is the server, or a list: the server and its arguments
        self.p = subprocess.Popen(exe if isinstance(exe, list) else [exe], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        self.n = 0
        self.rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "measure", "version": "0"}})
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def send(self, msg):
        self.p.stdin.write((json.dumps(msg) + "\n").encode())
        self.p.stdin.flush()

    def rpc(self, method, params):
        self.n += 1
        self.send({"jsonrpc": "2.0", "id": self.n, "method": method, "params": params})
        while True:
            msg = json.loads(self.p.stdout.readline())
            if msg.get("id") == self.n:
                return msg

    def call(self, tool, args):
        t = time.perf_counter()
        msg = self.rpc("tools/call", {"name": tool, "arguments": args})
        ms = (time.perf_counter() - t) * 1000
        text = msg["result"]["content"][0]["text"]
        m = re.match(r"reply: (\d+) tokens\n", text)
        return text, (int(m.group(1)) if m else None), ms

    def close(self):
        self.p.stdin.close()
        self.p.wait()


def summary(values):
    values = [v for v in values if v is not None]
    if not values:
        return None
    values = sorted(values)
    return {"n": len(values), "median": statistics.median(values), "p90": values[int(0.9 * (len(values) - 1))], "max": values[-1]}


def measure(exe, file, questions):
    s = Server(exe)
    out = {"file": Path(file).name, "bytes": Path(file).stat().st_size}
    text, first, ms = s.call("kapu_map", {"file": file})
    out["map_first"] = {"tokens": first, "ms": ms, "truncated": "more map lines" in text}
    text, second, ms = s.call("kapu_map", {"file": file})
    out["map_next"] = {"tokens": second, "ms": ms}
    qs = [json.loads(l) for l in Path(questions).read_text(encoding="utf-8").splitlines() if l.strip()]
    search_t, search_ms, ask_t, ask_ms, open_ms, answered = [], [], [], [], [], 0
    for q in qs:
        room = slug(Path(q["valasz_fajl"]).stem)
        _, t, ms = s.call("kapu_search", {"file": file, "query": q["kerdes"]})
        search_t.append(t)
        search_ms.append(ms)
        text, t, ms = s.call("kapu_ask", {"file": file, "query": q["kerdes"]})
        ask_t.append(t)
        ask_ms.append(ms)
        opened = FRAME_ID.findall(text)
        if any(i == room or i.startswith(room + "/") for i in opened):
            answered += 1
        _, _, ms = s.call("kapu_open", {"file": file, "ids": [room]})
        open_ms.append(ms)
    s.close()
    out.update({
        "questions": len(qs),
        "search_tokens": summary(search_t),
        "search_ms": summary(search_ms),
        "ask_tokens": summary(ask_t),
        "ask_ms": summary(ask_ms),
        "ask_answered": answered / len(qs) if qs else None,
        "open_ms": summary(open_ms),
    })
    return out


def main():
    exe, out_path = sys.argv[1], sys.argv[2]
    results = {}
    for spec in sys.argv[3:]:
        name, rest = spec.split("=", 1)
        file, questions = rest.rsplit(":", 1) if rest.count(":") > 1 else rest.split(":", 1)
        results[name] = measure(exe, file, questions)
        r = results[name]
        print(f"{name}: map {r['map_first']['tokens']}/{r['map_next']['tokens']} t; search median {r['search_tokens']['median']} max {r['search_tokens']['max']} t, "
              f"{r['search_ms']['median']:.0f} ms; ask median {r['ask_tokens']['median']} t, answered {r['ask_answered']:.0%}; "
              f"open median {r['open_ms']['median']:.1f} ms p90 {r['open_ms']['p90']:.1f} ms", flush=True)
    Path(out_path).write_text(json.dumps(results, indent=1), encoding="utf-8")


if __name__ == "__main__":
    main()
