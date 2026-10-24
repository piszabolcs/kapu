"""Writes the measurement results page from the committed results file.

    python tools/study/results_page.py bench/results/2026-10-23-study-pilot.json docs/results/index.html

The page is one self-contained HTML file (English and Hungarian, d3 from
cdnjs). Every number on it comes from the results file; the template is
`results_template.html` next to this script.
"""

import json
import sys
from pathlib import Path

TOPICS = [
    ("How do I use it", "Hogyan használható"),
    ("8-direction dog", "8 irányú kutya"),
    ("Walk animation", "Séta animáció"),
    ("Tiles", "Csempék"),
    ("Consistent style", "Egységes stílus"),
    ("Aseprite extension", "Aseprite-bővítmény"),
    ("Claude Code + Godot", "Claude Code + Godot"),
    ("Attack animation", "Ütés animáció"),
    ("Pixel art from a photo", "Fotóból pixel art"),
    ("Transparent background", "Átlátszó háttér"),
    ("UI elements", "UI-elemek"),
    ("50 items from the API", "50 tárgy API-ból"),
]
CORPORA = {"s": "Rust book", "m": "Kubernetes", "l": "MDN"}


def form_row(form, c):
    return {
        "form": form,
        "correct": c["yes"],
        "partly": c["partly"],
        "claims_supported": c["claims_supported"],
        "claims_total": c["claims_total"],
        "helpfulness": c["helpfulness_mean"],
        "read_tokens_median": round(c["read_tokens_median"]),
        "read_tokens_total": c["read_tokens_total"],
        "calls_median": c["calls_median"],
    }


def scale_row(size, d, experimental=False):
    m = d["mcp"]
    totals = dict(kv.split("=") for kv in m["totals_line"].split())
    return {
        "size": size.upper(),
        "corpus": CORPORA[size],
        "experimental": experimental,
        "knowledge_tokens": int(totals["tokens"]),
        "rooms": int(totals["rooms"]),
        "doors": int(totals["doors"]),
        "groups": int(totals["groups"]),
        "build_s": round(d["import"]["seconds"], 1),
        "peak_gb": round(d["import"]["peak_mb"] / 1024, 2),
        "file_mb": d["file_mb"],
        "map_tokens": m["map"]["tokens"],
        "map_ms": round(m["map"]["ms"]["median"]),
        "first_map_tokens": m["map_first"]["tokens"],
        "group_list_max": m["groups"]["tokens"]["max"],
        "search_tokens": round(m["search"]["tokens"]["median"]),
        "search_ms": round(m["search"]["ms"]["median"], 1),
        "first_search_s": round(m["search_first_ms"] / 1000, 1),
        "open_tokens": round(m["open"]["tokens"]["median"]),
        "open_ms": round(m["open"]["ms"]["median"], 1),
    }


def retrieval_rows(size, n, bench, chunks):
    rows = []
    for method, src in (("kapu_hybrid", bench["hybrid"]), ("kapu_bm25", bench["bm25"]), ("kapu_vector", bench["vector"])):
        rows.append({"size": size.upper(), "n": n, "method": method, "r5": round(src["room_recall@5"], 3),
                     "r10": round(src["room_recall@10"], 3), "mrr": round(src["room_mrr@10"], 3)})
    for method, src in (("chunk_bm25", chunks["bm25"]["units"]), ("chunk_vector", chunks["vector"]["units"])):
        rows.append({"size": size.upper(), "n": n, "method": method, "r5": round(src["recall@5"], 3),
                     "r10": round(src["recall@10"], 3), "mrr": round(src["mrr@10"], 3)})
    return rows


def main(results: Path, out: Path):
    r = json.loads(results.read_text(encoding="utf-8"))
    px = r["format_comparison_pixellab"]
    v104 = r["remeasure_v104"]
    v11 = r["remeasure_v11"]
    sizes = r["sizes"]

    split = px.get("kapu_tokens_by_tool_per_request") or {}
    requests = []
    for i, (en, hu) in enumerate(TOPICS):
        p = f"p{i + 1}"
        tokens = {f: px["result"][f]["per_request"][i][3] for f in ("kapu", "md", "pdf")}
        requests.append({"request": p, "topic_en": en, "topic_hu": hu, **tokens, **split.get(p, {})})

    before_scale = {"s": v104["scale"]["s"], "m": v104["scale"]["m"], "l": v104["scale_l_experimental"]}
    before_bench = {"s": sizes["s"]["kapu_bench"], "m": v104["kapu_bench"]["m"], "l": v104["kapu_bench"]["l"]}
    after_scale, after_bench = v11["scale"], v11["kapu_bench"]
    seconds = v11.get("kapu_bench_seconds") or {}
    v104_seconds = {"m": 187, "l": 1774}  # bench run times measured for v1.0.4 (RESULTS.md chapter 8)

    changes = []
    for s in ("s", "m", "l"):
        changes.append({"metric": "file_mb", "size": s, "before": before_scale[s]["file_mb"], "after": after_scale[s]["file_mb"]})
    changes.append({"metric": "l_builds", "size": "l", "before": "no", "after": "yes"})
    for s in ("s", "m", "l"):
        changes.append({"metric": "search_ms", "size": s, "before": round(before_scale[s]["mcp"]["search"]["ms"]["median"]),
                        "after": round(after_scale[s]["mcp"]["search"]["ms"]["median"], 1)})
    for s in ("s", "m", "l"):
        changes.append({"metric": "open_ms", "size": s, "before": round(before_scale[s]["mcp"]["open"]["ms"]["median"]),
                        "after": round(after_scale[s]["mcp"]["open"]["ms"]["median"], 1)})
    for mode in ("hybrid", "vector"):
        for s in ("s", "m", "l"):
            changes.append({"metric": f"r10_{mode}", "size": s, "before": round(before_bench[s][mode]["room_recall@10"], 3),
                            "after": round(after_bench[s][mode]["room_recall@10"], 3)})
    for s in ("m", "l"):
        if s in seconds:
            changes.append({"metric": "bench_s", "size": s, "before": v104_seconds[s], "after": seconds[s]})

    tokens = r["agentic_pilot_s"]["tokens"]
    agents = [{"system": s, "correct_sonnet": r["agentic_pilot_s"]["judge_sonnet"][s]["yes"],
               "correct_haiku": r["agentic_pilot_s"]["judge_haiku"][s]["yes"], "questions": 30,
               "read_tokens_median": round(tokens[s]["read_tokens_median"]), "calls_median": tokens[s]["calls_median"],
               "agent_tokens_per_q": tokens[s]["agent_tokens_per_q"]} for s in ("c0", "c2b", "c3", "c4", "c5")]

    old_kapu, new_kapu = tokens["c5"], v104["agents_s_kapu"]
    v104_rows = [
        {"metric": "rust_tokens_median", "before": round(old_kapu["read_tokens_median"]), "after": round(new_kapu["read_tokens_median"])},
        {"metric": "rust_correct", "before": 30, "after": 30},
        {"metric": "first_map_m", "before": sizes["m"]["scale"]["mcp"]["map_first"]["tokens"], "after": v104["scale"]["m"]["mcp"]["map_first"]["tokens"]},
        {"metric": "first_map_l", "before": sizes["l"]["scale_experimental_line_limit_64mib"]["mcp"]["map_first"]["tokens"],
         "after": v104["scale_l_experimental"]["mcp"]["map_first"]["tokens"]},
    ]

    data = {
        "date": v11.get("date"),
        "spec": v11.get("specification"),
        "knowledge_tokens": px["full_file_in_context_tokens_per_request"],
        "pixellab1": [form_row(f, px["result"][f]) for f in ("kapu", "md", "pdf")],
        "pixellab2": [form_row("kapu", v104["pixellab_same_prompt"]["k104"]), form_row("md", v104["pixellab_same_prompt"]["md"]),
                      form_row("pdf", v104["pixellab_same_prompt"]["pdf"])],
        "requests": requests,
        "changes": changes,
        "scale": [scale_row(s, after_scale[s]) for s in ("s", "m", "l")],
        "retrieval": [row for s in ("s", "m", "l") for row in retrieval_rows(s, sizes[s]["questions"], after_bench[s], sizes[s]["chunk_baselines"])],
        "agents": agents,
        "v104": v104_rows,
    }
    template = (Path(__file__).parent / "results_template.html").read_text(encoding="utf-8")
    page = template.replace("/*__DATA__*/null", json.dumps(data, ensure_ascii=False, separators=(",", ":")))
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(page, encoding="utf-8", newline="\n")
    print("written", out, len(page.encode("utf-8")), "bytes")


if __name__ == "__main__":
    main(Path(sys.argv[1]), Path(sys.argv[2]))
