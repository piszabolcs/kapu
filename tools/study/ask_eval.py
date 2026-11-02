"""`kapu_ask` and `kapu_search` replies per server setting (plan v2 J11, J12).

    python tools/study/ask_eval.py <kapu-mcp> <splits-dir> <modes.json> <out.json> \
        [--max-tokens N] <set>=<file.kapu>:<questions.jsonl> ...

`modes.json` is a list of `{"name", "rank"}`; `rank` goes to `kapu-mcp --rank`
(the ranking switches and `ask`, `ask_ratio`, `excerpt_lines`). Per mode, set
and question: `kapu_ask` (default budget 2000) and `kapu_search` (k 10, budget
600). Reported per part (M1): the share of asks with a frame of the expected
room (answered, K-07), the reply tokens, the tokens spent on asks without
it, the confidence line when there is one, and for the search the reply
tokens and the hits that fit. The difference of the answered share to the
first mode gets a paired bootstrap 95% interval (as `ablation.py`).

`--max-tokens` is the `kapu_ask` budget (J2a: 600 is what a client gives a
small model at 8k context). A question with an `evidence` text (the PLC set)
also gets `evidence`: the share of the evidence's words (3 letters or more)
that are in the reply, and `evidence_found` when that is at least 0.6: is the
answer itself in the reply, not only a frame of its room. Both get the
interval to the first mode too.
"""

import json
import re
import statistics
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ablation import bootstrap, SEED  # noqa: E402
from measure_v12 import FRAME_ID, Server  # noqa: E402

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from build_vectors import slug  # noqa: E402

WORD = re.compile(r"[\w.]{3,}")


def evidence_share(evidence, text):
    words = {w.lower().strip(".") for w in WORD.findall(evidence)} - {""}
    if not words:
        return None
    have = {w.lower().strip(".") for w in WORD.findall(text)}
    return len(words & have) / len(words)


HIT_LINE = re.compile(r"^\S+ \| .* \| \d+t \| [\d.]+ \| (bm25|vec|both)$", re.M)


def ask_runs(exe, rank, kfile, questions, max_tokens=None):
    server = Server([exe, "--rank", json.dumps(rank)]) if rank else Server(exe)
    rows = []
    for q in questions:
        room = slug(Path(q["valasz_fajl"]).stem)
        args = {"file": kfile, "query": q["kerdes"]}
        if max_tokens:
            args["max_tokens"] = max_tokens
        text, tokens, ms = server.call("kapu_ask", args)
        share = evidence_share(q["evidence"], text) if q.get("evidence") else None
        opened = FRAME_ID.findall(text)
        conf = re.search(r"^confidence: (high|low)$", text, re.M)
        s_text, s_tokens, s_ms = server.call("kapu_search", {"file": kfile, "query": q["kerdes"]})
        rows.append({
            "answered": any(i == room or i.startswith(room + "/") for i in opened),
            "tokens": tokens, "ms": ms, "frames": len(opened),
            "confidence": conf.group(1) if conf else None,
            "evidence": share, "evidence_found": None if share is None else share >= 0.6,
            "search_tokens": s_tokens, "search_hits": len(HIT_LINE.findall(s_text)), "search_ms": s_ms,
        })
    server.close()
    return rows


def summarise(rows):
    n = len(rows)
    if not n:
        return {}
    out = {
        "n": n,
        "answered": sum(r["answered"] for r in rows) / n,
        "tokens_mean": statistics.mean(r["tokens"] for r in rows),
        "tokens_median": statistics.median(r["tokens"] for r in rows),
        "wasted_tokens_per_ask": sum(r["tokens"] for r in rows if not r["answered"]) / n,
        "frames_mean": statistics.mean(r["frames"] for r in rows),
        "ask_ms_median": statistics.median(r["ms"] for r in rows),
        "search_tokens_median": statistics.median(r["search_tokens"] for r in rows),
        "search_tokens_max": max(r["search_tokens"] for r in rows),
        "search_hits_mean": statistics.mean(r["search_hits"] for r in rows),
    }
    ev = [r for r in rows if r.get("evidence") is not None]
    if ev:
        out["evidence"] = statistics.mean(r["evidence"] for r in ev)
        out["evidence_found"] = sum(r["evidence_found"] for r in ev) / len(ev)
    high = [r for r in rows if r["confidence"] == "high"]
    low = [r for r in rows if r["confidence"] == "low"]
    if high or low:
        out["high_share"] = len(high) / n
        out["answered_when_high"] = sum(r["answered"] for r in high) / len(high) if high else None
        out["answered_when_low"] = sum(r["answered"] for r in low) / len(low) if low else None
    return out


def main():
    argv = sys.argv[1:]
    max_tokens = None
    if "--max-tokens" in argv:
        at = argv.index("--max-tokens")
        max_tokens = int(argv[at + 1])
        del argv[at:at + 2]
    exe, splits, modes_path, out = argv[0], Path(argv[1]), Path(argv[2]), Path(argv[3])
    exe = str(Path(exe).resolve())
    modes = json.loads(modes_path.read_text(encoding="utf-8"))
    rng = np.random.default_rng(SEED)
    result = {"modes": [m["name"] for m in modes], "max_tokens": max_tokens, "sets": {}}
    for spec in argv[4:]:
        name, rest = spec.split("=", 1)
        kfile, qfile = rest.rsplit(":", 1)
        qs = [json.loads(l) for l in Path(qfile).read_text(encoding="utf-8").splitlines() if l.strip()]
        parts = [p for _, p in json.loads((splits / f"{name}.json").read_text(encoding="utf-8"))["lines"]]
        per_mode = {}
        for m in modes:
            print(f"{name}: {m['name']}", flush=True)
            per_mode[m["name"]] = ask_runs(exe, m.get("rank"), str(Path(kfile).resolve()), qs, max_tokens)
        entry = {}
        first = modes[0]["name"]
        for mname, rows in per_mode.items():
            entry[mname] = {}
            for part in ("dev", "test", "all"):
                sel = [i for i, p in enumerate(parts) if part == "all" or p == part]
                entry[mname][part] = summarise([rows[i] for i in sel])
                if mname != first:
                    a = np.array([float(rows[i]["answered"]) for i in sel])
                    b = np.array([float(per_mode[first][i]["answered"]) for i in sel])
                    entry[mname][part]["answered_vs_first"] = bootstrap(a, b, rng)
                    ev = [i for i in sel if rows[i].get("evidence") is not None]
                    if ev:
                        for key in ("evidence", "evidence_found"):
                            a = np.array([float(rows[i][key]) for i in ev])
                            b = np.array([float(per_mode[first][i][key]) for i in ev])
                            entry[mname][part][f"{key}_vs_first"] = bootstrap(a, b, rng)
        result["sets"][name] = entry
        for mname in per_mode:
            t = entry[mname]["test"]
            ev = f", evidence {t['evidence']:.3f} found {t['evidence_found']:.3f}" if "evidence" in t else ""
            print(f"  {mname}: test answered {t['answered']:.3f}{ev}, ask {t['tokens_median']:.0f} t, wasted {t['wasted_tokens_per_ask']:.0f} t/ask, "
                  f"search {t['search_tokens_median']:.0f} t, {t['search_hits_mean']:.1f} hits", flush=True)
    out.write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
