"""Results file of chapter 15 (plan v4 and v5) from the local measurement runs.

    python tools/study/md_eval.py report <study-data>/m1/pixellab <study-data>/m1/pixellab/report-v5.json
    python tools/study/collect_m1.py <study-data>/m1 <out.json>

Reads `plc/report-v5.json` and `pixellab/report-v5.json`, the reports of
`md_eval.py` (numbers only, no question text), and writes one summary per form:
the dollars of every run, the fully correct answers, the made up ones and the
tool calls.
"""

import json
import sys
from pathlib import Path

PLC_SINGLE = {"md": "One Markdown file, Grep and Read", "kapu": "KAPU 0.5.1", "kapu-tail3": "KAPU 0.6.0"}
PLC_HOOK = {"kapu-hook-h0": "The hook", "kapu-hook-h1": "The hook with its stop line",
            "kapu-hook-c1": "The hook and at most one tool call (left out)"}
PLC_SESSION = {"md": "One Markdown file", "kapu": "KAPU 0.5.1", "kapu-d178": "KAPU 0.6.0"}
PIXEL_SINGLE = {"md": "One Markdown file", "kapu": "KAPU 0.5.1", "kapu-tail3": "KAPU 0.6.0"}
PIXEL_SESSION = {"md": "One Markdown file", "kapu": "KAPU 0.5.1", "kapu-v4": "KAPU 0.6.0 before the passage tail",
                 "pasted": "The whole Markdown file pasted"}


def form(entry, name, answerable, absent):
    """`answerable` and `absent`: the questions of one run."""
    runs = entry["runs"]
    dollars = [round(runs[r]["spend"]["cost_usd"], 3) for r in sorted(runs)]
    pooled = entry["pooled"]
    a = pooled["answerable"]
    n = len(dollars)
    out = {
        "name": name,
        "runs": n,
        "dollars": dollars,
        "dollars_mean": round(sum(dollars) / n, 3),
        "answerable": answerable * n,
        "fully_correct": round(a["correct"] * answerable * n),
        "partly": round(a["partly"] * answerable * n),
        "wrong": round(a["wrong"] * answerable * n),
        "not_found": round(a["notfound"] * answerable * n),
        "calls_answerable_median": a["tool_calls_median"],
    }
    if absent:
        b = pooled["absent"]
        out.update(absent=absent * n, made_up=round(b["made_up"] * absent * n), calls_absent_median=b["tool_calls_median"])
        first = runs["1"]
        out["run_1"] = {"dollars": dollars[0], "fully_correct": round(first["answerable"]["correct"] * answerable),
                        "made_up": round(first["absent"]["made_up"] * absent)}
    return out


def forms(modes, mode, names, answerable, absent):
    return [form(modes[mode][key], name, answerable, absent) for key, name in names.items() if key in modes[mode]]


def main(work, out):
    plc = json.loads((work / "plc" / "report-v5.json").read_text(encoding="utf-8"))["modes"]
    pixel = json.loads((work / "pixellab" / "report-v5.json").read_text(encoding="utf-8"))["modes"]
    result = {
        "date": "2026-11-05",
        "study": "One Markdown file with grep against KAPU (plan v4 and v5), bench/RESULTS.md chapter 15",
        "model": "Claude Sonnet through claude -p, no user settings",
        "judge": "Claude Haiku",
        "tools": "0.6.0; the forms name their own",
        "plc_single": forms(plc, "single", PLC_SINGLE, 36, 24),
        "plc_hook": forms(plc, "single", PLC_HOOK, 36, 24),
        "plc_session": forms(plc, "session", PLC_SESSION, 20, 5),
        "pixellab_single": forms(pixel, "single", PIXEL_SINGLE, 12, 0),
        "pixellab_session": forms(pixel, "session", PIXEL_SESSION, 12, 0),
    }
    out.write_text(json.dumps(result, ensure_ascii=False, indent=1) + "\n", encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main(Path(sys.argv[1]), Path(sys.argv[2]))
