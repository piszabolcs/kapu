"""The results file of plan v2, phases 1 and 2 (`bench/results/`).

    python tools/study/collect_v2.py <study-data/v2> <out.json> <ranks.json>

Reads the reports of `ablation.py` (`m0-all.json`), `ask_eval.py`
(`ask-all.json`), `neighbour_follow.py` (`n2a.json`), `group_quality.py`
(`n13.json`) and `title_weight.py` (`tw-all.json`) and writes one summary
without question text, and the per-question ranks (question hashes only) to
`<ranks.json>`, from which every retrieval metric can be computed again.
"""

import json
import sys
from pathlib import Path

NEW = "collapse-1"  # the accepted configuration of phase 2 (D157)
SETS = ["s", "m", "l", "en", "hu-en", "hu"]


def load(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def pick(m):
    return {k: round(m[k], 4) for k in ("r@1", "r@5", "r@10", "r@50", "r@100", "mrr@10", "ndcg@10")}


def main():
    work, out, ranks_out = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
    m0 = load(work / "m0-all.json")
    sets = [s for s in SETS if s in m0["sets"]]
    retrieval, errors = [], []
    for s in sets:
        e = m0["sets"][s]
        for cname in ("base", NEW):
            part = e["configs"][cname]["parts"]["test"]
            row = {"set": s, "config": cname, "n": e["test"],
                   "hybrid_room": pick(part["metrics"]["hybrid"]["room"]),
                   "hybrid_strict": pick(part["metrics"]["hybrid"]["strict"]),
                   "bm25_room": pick(part["metrics"]["bm25"]["room"]),
                   "vector_room": pick(part["metrics"]["vector"]["room"]) if "vector" in part["metrics"] else None,
                   "ms_p50": round(e["configs"][cname]["ms_p50"], 1)}
            retrieval.append(row)
            errors.append({"set": s, "config": cname, "misses": sum(part["errors"][k] for k in
                           ("duplication", "fusion", "rescore", "ranking", "vocabulary")), **part["errors"]})
        if "chunk" in e:
            for mode in ("bm25", "vector", "hybrid"):
                if mode in e["chunk"]:
                    retrieval.append({"set": s, "config": f"chunk_{mode}", "n": e["test"],
                                      "hybrid_room": pick(e["chunk"][mode]["dedup"]["test"]),
                                      "units": pick(e["chunk"][mode]["units"]["test"])})
    fixes = []
    for group, p in m0["selection"].items():
        t = p["test"]
        fixes.append({"fix": group, "variant": p["variant"], "variants": len(p["variants"]),
                      "dev_gain": round(p["dev_gain"], 4),
                      "d_r10": round(t["r@10"]["diff"], 4), "lo_r10": round(t["r@10"]["lo"], 4), "hi_r10": round(t["r@10"]["hi"], 4),
                      "d_mrr": round(t["mrr@10"]["diff"], 4), "lo_mrr": round(t["mrr@10"]["lo"], 4), "hi_mrr": round(t["mrr@10"]["hi"], 4),
                      "worst_set": round(p["worst_set_diff"], 4), "p50_ratio": round(p["max_p50_ratio"], 2),
                      "accepted": p["accepted"]})
    configs = {c: {"options": next(iter(m0["sets"].values()))["configs"][c]["options"],
                   "pooled_test": m0["pooled"].get(c, {}).get("test"), "pooled_dev": m0["pooled"].get(c, {}).get("dev"),
                   "accepted": m0["accepted"].get(c)}
               for c in next(iter(m0["sets"].values()))["configs"]}
    doc = {"date": m0["meta"]["date"], "meta": m0["meta"], "sets": {s: {k: m0["sets"][s][k] for k in ("questions", "dev", "test", "vector")} for s in sets},
           "fixes": fixes, "retrieval": retrieval, "errors": errors, "configs": configs}
    for name, key in (("ask-all.json", "ask"), ("n2a.json", "neighbours"), ("n13.json", "groups"), ("tw-all.json", "title_weight"),
                      ("timing.json", "timing")):
        if (work / name).is_file():
            doc[key] = load(work / name)
    out.write_text(json.dumps(doc, ensure_ascii=False, indent=1) + "\n", encoding="utf-8", newline="\n")
    ranks_out.write_text(json.dumps({"columns": m0["meta"]["ranks_columns"], "sets": m0["ranks"]}, separators=(",", ":")) + "\n",
                         encoding="utf-8", newline="\n")
    print("written", out, ranks_out)


if __name__ == "__main__":
    main()
