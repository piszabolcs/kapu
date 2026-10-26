"""Collect the format 1.2 measurements into bench/results/2026-10-26-format-1.2.json.

    tools/.venv/Scripts/python tools/study/collect_v12.py <study-data> <en11.kapu> <out.json>

Reads, from the study-data folder (outside the repository): the 1.1 and 1.2
files of the corpora, the `kapu bench --json` outputs in bench12/ (1.1 float
vectors, 1.2 sign bits alone, 1.2 sign bits with rescoring), the MCP
measurement measure-v12-final.json (tools/study/measure_v12.py) and the scale
probe scale-v12.json (tools/study/measure_scale.py, one JSON line per file).
"""
import json
import sys
from pathlib import Path

CORPORA = {
    "rust_book": ("s-v11.kapu", "s-v12.kapu", "s12", "vecexp/s-base", "questions-s.jsonl"),
    "kubernetes": ("m-v11.kapu", "m-v12.kapu", "m12", "vecexp/m-base", "questions-m.jsonl"),
    "mdn": ("l-v11.kapu", "l-v12.kapu", "l12", "bench12/l11", "questions-l.jsonl"),
    "skills_en": (None, "en-v12.kapu", "en12-en", "bench12/en11-en", "en-questions.jsonl"),
    "skills_hu_questions": (None, "en-v12.kapu", "en12-hu", "bench12/en11-hu", "q-hu.jsonl"),
    "skills_hu_translated": (None, "en-v12.kapu", "en12-huen", None, "q-hu-en.jsonl"),
}


def scores(d):
    keep = ("recall@10", "room_recall@10", "room_mrr@10")
    return {mode: {k: round(d[mode][k], 3) for k in keep} for mode in ("bm25", "vector", "hybrid") if d.get(mode)}


def main():
    data, en11, out = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
    mcp = json.loads((data / "measure-v12-final.json").read_text(encoding="utf-8"))
    scale = [json.loads(l) for l in (data / "scale-v12.json").read_text(encoding="utf-8").splitlines() if l.strip()]
    result = {
        "date": "2026-10-26",
        "version": "tools 0.2.0, specification v1.2, file format 1.2",
        "machine": "Windows 11 Pro, release build (lto, codegen-units = 1)",
        "model": "potion-multilingual-128M, int8",
        "rescore_candidates": 200,
        "tool_list_tokens": {"v1.1_five_tools": 1174, "v1.2_six_tools": 797, "method": "names, descriptions and input schemas as compact JSON, o200k_base"},
        "scale": scale,
        "corpora": {},
    }
    measured = {"rust_book": "s", "kubernetes": "m", "mdn": "l", "skills_en": "en"}
    for name, (old, new, bench, base, questions) in CORPORA.items():
        entry = {"questions_file": questions}
        if old:
            a, b = (data / old).stat().st_size, (data / new).stat().st_size
            entry["bytes"] = {"v1.1": a, "v1.2": b, "growth": round(b / a - 1, 4)}
        elif name == "skills_en":
            a, b = en11.stat().st_size, (data / new).stat().st_size
            entry["bytes"] = {"v1.1": a, "v1.2": b, "growth": round(b / a - 1, 4)}
        if base:
            entry["v1.1_float"] = scores(json.loads((data / f"{base}.json").read_text(encoding="utf-8")))
        entry["v1.2_sign_bits_only"] = scores(json.loads((data / f"bench12/{bench}.json").read_text(encoding="utf-8")))
        rescored = json.loads((data / f"bench12/{bench}-rs.json").read_text(encoding="utf-8"))
        entry["v1.2"] = scores(rescored)
        entry["questions"] = rescored["questions"]
        if name in measured:
            entry["mcp"] = mcp[measured[name]]
        result["corpora"][name] = entry
    out.write_text(json.dumps(result, indent=1, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
