"""Results file of chapter 14 from the qwen3:8b study, with KAPU 0.6.0.

    python tools/study/collect_j2.py <study-data>/v3-qwen/report.json <out.json>

`report.json` is what the study's `report.py` writes from the Vellum harness
output and the judge (numbers only, no question text). The forms without KAPU
and the 0.5 forms are the runs of 2026-10-28 to 2026-11-02; a form that has no
run yet is left out.
"""

import json
import sys
from pathlib import Path

# form, KAPU version, context, knowledge budget, condition of the report, in the chart
PLC = [
    ("closed", None, "8k", None, "L0", True),
    ("prefetch", "0.5", "8k", 600, "L2p_v05", True),
    ("tools", "0.6", "8k", 600, "L2_v06", True),
    ("prefetch", "0.6", "8k", 600, "L2p_v06", True),
    ("prefetch", "0.6", "16k", 4000, "L2p_16k_v06", True),
    ("prefetch", "0.5", "32k", 6000, "L2p_32k_b6000_v05", True),
    ("prefetch", "0.6", "32k", 8000, "L2p_32k_v06", True),
    ("grep", None, "8k", None, "L3", True),
    ("grep", None, "32k", None, "L3_32k", True),
    ("pasted", None, "8k", None, "L4t", False),
    ("oracle", None, "8k", None, "Lo", False),
    ("oracle", None, "32k", None, "Lo_32k", True),
]
# column of the page, condition of the report
PUBLIC = {"closed": "L0", "tools": "L2_v06", "prefetch": "L2p_v06", "prefetch32": "L2p_32k_v06", "grep": "L3"}
SETS = ["rust", "k8s", "mdn", "pixellab"]


def plc_row(form, tools, ctx, budget, chart, entry):
    a = entry.get("answerable", {})
    b = entry.get("absent", {})
    return {"form": form, "tools": tools, "ctx": ctx, "budget": budget, "chart": chart, "n": a.get("n"),
            "correct": a.get("correct"), "partly": a.get("partly"), "wrong": a.get("wrong"), "not_found": a.get("notfound"),
            "right_room": a.get("right_page_read"), "absent": b.get("n"), "made_up": b.get("made_up"),
            "seconds": a.get("seconds_median"), "tokens": a.get("input_tokens_median")}


def main(report, out):
    r = json.loads(report.read_text(encoding="utf-8"))
    q = r["qwen3:8b"]
    plc = [plc_row(form, tools, ctx, budget, chart, q["plc"][cond])
           for form, tools, ctx, budget, cond, chart in PLC if cond in q["plc"]]
    claude = r["claude"]["plc_n1_sonnet"]["forms"]
    for form, key in (("claude_closed", "closed"), ("claude_kapu", "kapu")):
        a, b = claude[key]["answerable"], claude[key]["absent"]
        plc.append({"form": form, "tools": "0.3" if key == "kapu" else None, "ctx": None, "budget": None, "chart": True, "n": 36,
                    "correct": round(a["correct"], 3), "partly": round(a["partly"], 3), "wrong": round(a["wrong"], 3),
                    "not_found": round(a["notfound"], 3), "right_room": None, "absent": 24,
                    "made_up": round(b["made_up"], 3), "seconds": None, "tokens": None})
    public = []
    for name in SETS:
        row = {"set": name, "n": q[name]["L0"]["answerable"]["n"]}
        for column, cond in PUBLIC.items():
            a = q[name].get(cond, {}).get("answerable")
            row[column] = {k: a[k] for k in ("correct", "partly", "wrong", "notfound")} if a else None
        public.append(row)
    result = {
        "date": "2026-11-05",
        "study": "J2: a small local model with KAPU, the way a user runs it, bench/RESULTS.md chapter 14",
        "model": "qwen3:8b (Ollama, 8 GB GPU; at 32k about 40% of the model on the CPU)",
        "client": "Vellum, branch feat/kapu-knowledge (815c6a9); not on its public main yet",
        "runs": "one per question, temperature 0",
        "judge": "Claude Haiku; on PLC the JUDGE and JUDGE_ABSENT prompts of chapter 12 word for word",
        "tools": "0.6.0 where a row says 0.6, each question in its own kapu-mcp session; 0.5.0 where it says 0.5; the client's knowledge budget",
        "plc": plc,
        "public": public,
    }
    out.write_text(json.dumps(result, ensure_ascii=False, indent=1) + "\n", encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main(Path(sys.argv[1]), Path(sys.argv[2]))
