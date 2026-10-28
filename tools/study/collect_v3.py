"""Plan v3 results file (D170-D172) from the local measurement runs.

    python tools/study/collect_v3.py <study-data> <out.json> <ranks.json>

Reads, under `<study-data>/v3/`:
- `v1/report.json`: the vector-model candidates on the same 512-token chunks
  (`model_candidates.py`, D170), the difference to the packaged model;
- `v2store/report.json`, `v2store-c/report.json`, `v2store-r/report.json`: the
  storage forms of the e5 chunk vectors (`@int8`, `@sign`, ...), D171;
- `v2all.json`: `ablation.py report` of the files rebuilt with e5, 8 and 4
  bits, against the static model, in KAPU's own search (D171);
- `v2sse/*.bench.json`: the same search with an SSE2 (baseline) build;
- the file sizes of the set files, today's and rebuilt (`v2/`, `v2b4/`).

Writes the summary to `<out.json>` and the per-question ranks of the KAPU
runs, by question hash, to `<ranks.json>` (no question text).
"""

import json
import sys
from pathlib import Path

SETS = ["s", "m", "l", "en", "hu-en", "hu", "plc"]
FILES = {"s": "s-v12", "m": "m-v12", "l": "l-v12", "en": "en-v12", "plc": "plc"}
# seconds of the first build with e5 on 12 threads (AVX-512 build), from the build logs
BUILD_S = {"s": 71, "en": 124, "plc": 321, "m": 922, "l": 2986}
CONFIGS = {
    "ml-base": ("static", 1, "base"),
    "e5-base": ("e5", 8, "base"),
    "e5b4-base": ("e5", 4, "base"),
    "e5b4-passage-prefix-3-en": ("e5", 4, "passage"),
    "e5b4-rooms-vp-passage-prefix-3-en": ("e5", 4, "passage+rooms"),
}
FORMS = [  # storage form, bytes per window of 384 components
    ("float", 1536), ("cfloat", 1536), ("int8", 384), ("int4", 192), ("cint4", 192), ("rint4", 194), ("cint3", 144),
    ("ctern", 96), ("rtern", 98), ("csign", 48), ("rsign", 50), ("sign", 48), ("sign+int8", 432), ("sign+int4", 240),
    ("csign+cint4", 240),
]


def r4(x):
    return None if x is None else round(x, 6)


def ci(d):
    return {"diff": r4(d["diff"]), "lo": r4(d["lo"]), "hi": r4(d["hi"])}


def main(root: Path, out: Path, ranks_out: Path):
    v3 = root / "v3"
    v1 = json.loads((v3 / "v1/report.json").read_text(encoding="utf-8"))
    candidates = []
    for key, value in v1["pooled"].items():
        model, method = key.split(":", 1)
        candidates.append({"model": model, "method": method, "r10": ci(value["r@10"]), "mrr": ci(value["mrr@10"])})

    storage = []
    seen = set()
    for name in ("v2store", "v2store-c", "v2store-r"):
        rep = json.loads((v3 / name / "report.json").read_text(encoding="utf-8"))
        for key, value in rep["pooled"].items():
            model, method = key.split(":", 1)
            form = model.partition("@")[2]
            if not form or method != "kapu_bm25+vec:base" or form in seen:
                continue
            seen.add(form)
            size = dict(FORMS).get(form)
            storage.append({"form": form, "bytes": size, "r10": ci(value["r@10"]), "mrr": ci(value["mrr@10"])})
    storage.sort(key=lambda s: -(s["bytes"] or 0))

    rep = json.loads((v3 / "v2all.json").read_text(encoding="utf-8"))
    pooled = []
    for c, (model, bits, rank) in CONFIGS.items():
        if c == "ml-base":
            continue
        t = rep["pooled"][c]["test"]
        pooled.append({"config": c, "model": model, "bits": bits, "rank": rank, "r10": ci(t["r@10"]), "mrr": ci(t["mrr@10"]),
                       "worst_set": r4(rep["accepted"][c]["worst_set_diff"]), "p50_ratio": r4(rep["accepted"][c]["max_p50_ratio"])})
    sse = {}
    for s in SETS:
        f = v3 / "v2sse" / f"{s}.bench.json"
        if f.is_file():
            b = json.loads(f.read_text(encoding="utf-8"))
            sse[s] = next(c for c in b["configs"] if c["name"] == "base")["ms_p50"]
    sets = []
    for s in SETS:
        st = rep["sets"][s]
        row = {"set": s, "test": st["test"], "dev": st["dev"], "configs": {}}
        for c in CONFIGS:
            cfg = st["configs"][c]
            room = cfg["parts"]["test"]["metrics"]["hybrid"]["room"]
            strict = cfg["parts"]["test"]["metrics"]["hybrid"]["strict"]
            row["configs"][c] = {"r10": r4(room["r@10"]), "mrr": r4(room["mrr@10"]), "strict_r10": r4(strict["r@10"]),
                                 "p50_ms": round(cfg["ms_p50"], 1), "p95_ms": round(cfg["ms_p95"], 1)}
        row["e5b4_p50_sse2_ms"] = round(sse[s], 1) if s in sse else None
        # the chunked hybrid of chapter 12 with each model (V1, the same chunks)
        rk = v1["sets"][s]["rankings"]
        row["chunk_hybrid_r10"] = {m: r4(rk[f"{name}:chunk_hybrid"]["test"]["r@10"]) for m, name in
                                   (("static", "potion-multilingual-128M"), ("e5", "multilingual-e5-small-qint8"))}
        sets.append(row)
    sizes = []
    for s, f in FILES.items():
        today, b8, b4 = (root / f"{f}.kapu", v3 / "v2" / f"{f}.kapu", v3 / "v2b4" / f"{f}.kapu")
        sizes.append({"set": s, "today_bytes": today.stat().st_size, "e5_8bit_bytes": b8.stat().st_size,
                      "e5_4bit_bytes": b4.stat().st_size, "build_s": BUILD_S[s]})
    result = {
        "date": rep["meta"]["date"],
        "version": "tools 0.5.0, specification v1.3, format 1.3",
        "machine": rep["meta"]["machine"],
        "models": {"static": "potion-multilingual-128M-i8", "e5": "multilingual-e5-small-q8"},
        "candidates": candidates,
        "storage": storage,
        "pooled": pooled,
        "sets": sets,
        "sizes": sizes,
        "encoder": {"window_512_s_avx512": 0.45, "window_512_s_sse2": 1.3, "onnx_cosine_short_min": 0.9978,
                    "onnx_cosine_512": 0.9993, "same_bits": ["SSE2", "AVX2", "AVX-512"]},
    }
    out.write_text(json.dumps(result, ensure_ascii=False, indent=1) + "\n", encoding="utf-8", newline="\n")
    ranks = {s: {"hashes": rep["ranks"][s]["hashes"],
                 "configs": {c: rep["ranks"][s]["configs"][c] for c in CONFIGS}} for s in SETS}
    ranks_out.write_text(json.dumps(ranks, separators=(",", ":")) + "\n", encoding="utf-8", newline="\n")
    print("written", out, ranks_out)


if __name__ == "__main__":
    main(Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3]))
