"""Ablation runs and their analysis (plan v2 M0-M3).

    python tools/study/ablation.py run <kapu> <matrix.json> <work-dir> \
        <set>=<file.kapu>:<questions.jsonl> ...
    python tools/study/ablation.py report <work-dir> <splits-dir> <out.json> \
        [--base base] [--chunks <dir of chunk-<set>.json>] <set> ...

`run` calls `kapu bench --json --config <matrix> --rankings` once per question
set (`<work-dir>/<set>.bench.json`, `<set>.rankings.jsonl`); a set whose two
files exist is not run again (`--force` reruns).

`report` reads them with the fixed dev/test split (M1, `split_questions.py`)
and writes per set, configuration, part and ranking (BM25, vector, hybrid):
room-level and strict recall@1/5/10/50/100, MRR@10 and nDCG@10 (M2; with one
expected door P@1 equals recall@1); for the hybrid the difference to the base
configuration with a paired bootstrap 95% interval (10 000 draws, fixed seed)
on room recall@10 and room MRR@10, per set and pooled over all sets; the error
classes of every missed question (M3); search time p50 and p95; and the M0
acceptance rule. Per question it keeps only the ranks of the expected door
and room, from which every metric above can be computed again.
"""

import datetime
import json
import math
import platform
import subprocess
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
DEPTH = 100
SEED = 20261027
DRAWS = 10_000
KS = (1, 5, 10, 50, 100)
CLASSES = ("duplication", "fusion", "rescore", "ranking", "vocabulary")


def room(door):
    return door.split("/", 1)[0]


def run(argv):
    force = "--force" in argv
    argv = [a for a in argv if a != "--force"]
    chunk = 0
    if "--chunk" in argv:
        # A40-like: `kapu bench --config` keeps about 2 MB per run of a large
        # file, so MDN runs in parts of this many questions, one process each
        i = argv.index("--chunk")
        chunk = int(argv[i + 1])
        del argv[i:i + 2]
    kapu, matrix, work = str(Path(argv[0]).resolve()), Path(argv[1]), Path(argv[2])
    work.mkdir(parents=True, exist_ok=True)
    for spec in argv[3:]:
        name, rest = spec.split("=", 1)
        kfile, questions = rest.rsplit(":", 1)
        bench, ranks = work / f"{name}.bench.json", work / f"{name}.rankings.jsonl"
        if bench.is_file() and ranks.is_file() and not force:
            print(f"{name}: already run")
            continue
        print(f"{name}: {kfile}", flush=True)
        lines = Path(questions).read_text(encoding="utf-8").splitlines()
        lines = [l for l in lines if l.strip()]
        size = chunk or len(lines)
        docs, rows = [], []
        for start in range(0, len(lines), size):
            part_q = work / f"{name}.part.jsonl"
            part_r = work / f"{name}.part.rankings.jsonl"
            part_q.write_text("\n".join(lines[start:start + size]) + "\n", encoding="utf-8")
            out = subprocess.run([kapu, "--json", "bench", kfile, str(part_q), "--config", str(matrix),
                                  "--rankings", str(part_r)], capture_output=True, check=True)
            doc = json.loads(out.stdout)
            if doc["emb"] != "none" and not doc["vector"]:
                # a copied binary finds no packaged model: set KAPU_MODEL_DIR
                sys.exit(f"{name}: the file has vectors ({doc['emb']}) but kapu found no model")
            docs.append(doc)
            for line in part_r.read_text(encoding="utf-8").splitlines():
                row = json.loads(line)
                row["q"] += start
                rows.append(row)
            part_q.unlink()
            part_r.unlink()
            if chunk:
                print(f"  {min(start + size, len(lines))}/{len(lines)}", flush=True)
        doc = docs[0]
        doc["questions"] = len(lines)
        for c in doc["configs"]:
            ms = sorted(r["ms"] for r in rows if r["config"] == c["name"])
            c["ms_p50"] = ms[round((len(ms) - 1) * 0.5)]
            c["ms_p95"] = ms[round((len(ms) - 1) * 0.95)]
        if len(docs) > 1:
            doc["parts"] = len(docs)
        with open(ranks, "w", encoding="utf-8") as f:
            for row in sorted(rows, key=lambda r: (r["config"], r["q"])):
                f.write(json.dumps(row) + "\n")
        bench.write_text(json.dumps(doc), encoding="utf-8")


def rank_of(lst, hit):
    for i, door in enumerate(lst[:DEPTH]):
        if hit(door):
            return i + 1
    return None


def distinct_room_rank(lst, gold_room):
    seen = []
    for door in lst[:DEPTH]:
        r = room(door)
        if r not in seen:
            seen.append(r)
            if r == gold_room:
                return len(seen)
    return None


def question_ranks(line):
    """Ranks of one question: strict and room rank per ranking, the room's
    rank among distinct rooms of the hybrid, and whether a door of the room
    got a vector score at all."""
    gold = line["expected"]
    gold_room = room(gold)
    out = {}
    for mode in ("bm25", "vector", "hybrid"):
        lst = line.get(mode)
        if lst is None:
            out[mode] = None
            continue
        out[mode] = (rank_of(lst, lambda d: d == gold), rank_of(lst, lambda d: room(d) == gold_room))
    out["droom"] = distinct_room_rank(line["hybrid"], gold_room)
    vec = line.get("vector")
    out["vec_scored"] = None if vec is None else any(room(d) == gold_room for d in vec)
    return out


def metrics(ranks):
    """recall@k, MRR@10 and nDCG@10 of a list of ranks (None: not in the top 100)."""
    n = len(ranks)
    if n == 0:
        return {}
    m = {f"r@{k}": sum(1 for r in ranks if r is not None and r <= k) / n for k in KS}
    m["mrr@10"] = sum(1 / r for r in ranks if r is not None and r <= 10) / n
    m["ndcg@10"] = sum(1 / math.log2(r + 1) for r in ranks if r is not None and r <= 10) / n
    return m


def classify(q, has_vec):
    """M3: the error class of a question whose room is not in the hybrid top 10."""
    hybrid_room = q["hybrid"][1]
    if hybrid_room is not None and hybrid_room <= 10:
        return None
    if q["droom"] is not None and q["droom"] <= 10:
        return "duplication"
    bm25_room = q["bm25"][1] if q["bm25"] else None
    vec_room = q["vector"][1] if q["vector"] else None
    if (bm25_room is not None and bm25_room <= 10) or (vec_room is not None and vec_room <= 10):
        return "fusion"
    if has_vec and bm25_room is not None and bm25_room <= 50 and not q["vec_scored"]:
        return "rescore"
    if hybrid_room is not None:
        return "ranking"
    return "vocabulary"


def hits(ranks, metric):
    if metric == "r@10":
        return np.array([1.0 if r is not None and r <= 10 else 0.0 for r in ranks])
    return np.array([1 / r if r is not None and r <= 10 else 0.0 for r in ranks])


def bootstrap(a, b, rng):
    """Paired bootstrap of mean(a - b): the difference and its 95% interval."""
    d = a - b
    if len(d) == 0:
        return None
    idx = rng.integers(0, len(d), size=(DRAWS, len(d)))
    means = d[idx].mean(axis=1)
    return {"diff": float(d.mean()), "lo": float(np.percentile(means, 2.5)), "hi": float(np.percentile(means, 97.5))}


def git_commit():
    try:
        return subprocess.run(["git", "-C", str(ROOT), "rev-parse", "--short", "HEAD"], capture_output=True,
                              text=True, check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def report(argv):
    base, chunks = "base", None
    if "--base" in argv:
        i = argv.index("--base")
        base = argv[i + 1]
        del argv[i:i + 2]
    if "--chunks" in argv:
        i = argv.index("--chunks")
        chunks = Path(argv[i + 1])
        del argv[i:i + 2]
    work, splits, out = Path(argv[0]), Path(argv[1]), Path(argv[2])
    names = argv[3:]
    rng = np.random.default_rng(SEED)
    result = {"sets": {}, "pooled": {}, "ranks": {}}
    meta = None
    pooled = {}  # config -> part -> metric -> (config hits, base hits)
    for name in names:
        bench = json.loads((work / f"{name}.bench.json").read_text(encoding="utf-8"))
        split = json.loads((splits / f"{name}.json").read_text(encoding="utf-8"))
        parts = [p for _, p in split["lines"]]
        hashes = [h for h, _ in split["lines"]]
        if meta is None:
            meta = {k: bench[k] for k in ("tool", "format", "emb", "embhash")}
        by_config = {}
        with open(work / f"{name}.rankings.jsonl", encoding="utf-8") as f:
            for line in f:
                row = json.loads(line)
                by_config.setdefault(row["config"], []).append(row)
        has_vec = bench["vector"]
        entry = {"questions": bench["questions"], "dev": parts.count("dev"), "test": parts.count("test"),
                 "vector": has_vec, "configs": {}}
        qranks = {c: [question_ranks(r) for r in rows] for c, rows in by_config.items()}
        result["ranks"][name] = {"hashes": hashes, "parts": parts, "configs": {
            c: [[q["hybrid"][0], q["hybrid"][1], q["droom"],
                 q["bm25"][1] if q["bm25"] else None,
                 q["vector"][1] if q["vector"] else None, q["vec_scored"]] for q in qs]
            for c, qs in qranks.items()}}
        for c in bench["configs"]:
            cname = c["name"]
            qs = qranks[cname]
            assert len(qs) == len(parts), (name, cname)
            ms = [r["ms"] for r in by_config[cname]]
            cfg = {"options": c["options"], "ms_p50": c["ms_p50"], "ms_p95": c["ms_p95"], "parts": {}}
            for part in ("dev", "test", "all"):
                sel = [i for i, p in enumerate(parts) if part == "all" or p == part]
                m = {}
                for mode in ("bm25", "vector", "hybrid"):
                    if qs[0][mode] is None:
                        continue
                    m[mode] = {"room": metrics([qs[i][mode][1] for i in sel]),
                               "strict": metrics([qs[i][mode][0] for i in sel])}
                errors = {k: 0 for k in CLASSES}
                merge = 0
                for i in sel:
                    k = classify(qs[i], has_vec)
                    if k:
                        errors[k] += 1
                    elif qs[i]["hybrid"][0] is None or qs[i]["hybrid"][0] > 10:
                        merge += 1
                errors["merge (room hit, door missed)"] = merge
                cfg["parts"][part] = {"metrics": m, "errors": errors}
                if cname != base:
                    b = qranks[base]
                    cfg["parts"][part]["vs_base"] = {}
                    for metric in ("r@10", "mrr@10"):
                        a_h = hits([qs[i]["hybrid"][1] for i in sel], metric)
                        b_h = hits([b[i]["hybrid"][1] for i in sel], metric)
                        cfg["parts"][part]["vs_base"][metric] = bootstrap(a_h, b_h, rng)
                        acc = pooled.setdefault(cname, {}).setdefault(part, {}).setdefault(metric, ([], []))
                        acc[0].append(a_h)
                        acc[1].append(b_h)
            cfg["ms_p50_ratio"] = c["ms_p50"] / max(next(x["ms_p50"] for x in bench["configs"] if x["name"] == base), 1e-9)
            del ms
            entry["configs"][cname] = cfg
        chunk_file = chunks / f"chunk-{name}.json" if chunks else None
        if chunk_file and chunk_file.is_file():
            # M2: the fair baseline is the chunk ranking with one hit per room
            doc = json.loads(chunk_file.read_text(encoding="utf-8"))
            entry["chunk"] = {}
            for mode in ("bm25", "vector", "hybrid"):
                if mode not in doc or "ranks" not in doc[mode]:
                    continue
                entry["chunk"][mode] = {}
                for unit in ("units", "dedup"):
                    ranks = doc[mode]["ranks"][unit]
                    entry["chunk"][mode][unit] = {
                        part: metrics([r for r, p in zip(ranks, parts) if part == "all" or p == part])
                        for part in ("dev", "test", "all")}
        result["sets"][name] = entry
    for cname, by_part in pooled.items():
        result["pooled"][cname] = {}
        for part, by_metric in by_part.items():
            result["pooled"][cname][part] = {}
            for metric, (a, b) in by_metric.items():
                result["pooled"][cname][part][metric] = bootstrap(np.concatenate(a), np.concatenate(b), rng)
    result["accepted"] = {c: accept(result, c) for c in result["pooled"]}
    result["selection"] = select(result, base)
    result["meta"] = {
        **(meta or {}),
        "date": datetime.date.today().isoformat(),
        "commit": git_commit(),
        "machine": {"system": platform.system(), "release": platform.release(), "machine": platform.machine(),
                    "processor": platform.processor(), "cpus": __import__("os").cpu_count()},
        "base": base,
        "sets": names,
        "bootstrap": {"draws": DRAWS, "seed": SEED, "interval": "2.5-97.5 percentile of the mean paired difference"},
        "split": "bench/splits (M1): sha256 of the question, 40% dev, 60% test",
        "ranks_columns": ["strict rank", "room rank", "room rank among distinct rooms", "BM25 room rank",
                          "vector room rank", "room has a vector score"],
        "acceptance": "M0: on the test part room recall@10 or room MRR@10 improves with the pooled 95% interval "
                      "above zero, no set loses more than 0.01 in either, search p50 at most 1.5x the base",
    }
    out.write_text(json.dumps(result, ensure_ascii=False, separators=(",", ":")) + "\n", encoding="utf-8")
    print_table(result, base)


def accept(result, cname):
    pooled = result["pooled"][cname]["test"]
    better = any(v and v["lo"] > 0 for v in pooled.values())
    worst, slow = 0.0, 1.0
    for entry in result["sets"].values():
        cfg = entry["configs"][cname]
        for v in cfg["parts"]["test"]["vs_base"].values():
            if v:
                worst = min(worst, v["diff"])
        slow = max(slow, cfg["ms_p50_ratio"])
    return {"accepted": bool(better and worst >= -0.01 and slow <= 1.5), "pooled_better": better,
            "worst_set_diff": worst, "max_p50_ratio": slow}


def groups_of(configs, base):
    """The variants of each fix: the configuration names up to their last
    `-<value>` part (`parent-0.25` -> `parent`), the fusion grids by kind."""
    out = {}
    for c in configs:
        if c == base or c.startswith("ref-"):
            continue
        if c.startswith("rrf-"):
            key = "rrf"
        elif c.startswith("convex-"):
            key = "convex"
        elif c.startswith("collapse-"):
            key = "collapse"
        elif c.startswith("stem-"):
            key = "stemming"
        elif c.startswith("graph-"):
            key = "graph"
        else:
            key = c.rsplit("-", 1)[0] if c[-1].isdigit() else c
        out.setdefault(key, []).append(c)
    return out


def dev_worst(result, c):
    """The largest loss of one set on the dev part, in either metric."""
    worst = 0.0
    for entry in result["sets"].values():
        for v in entry["configs"][c]["parts"]["dev"]["vs_base"].values():
            if v:
                worst = min(worst, v["diff"])
    return worst


def select(result, base):
    """M1: per fix, among the variants that keep the M0 constraint on the dev
    part too (no set loses more than 0.01), the one with the best pooled dev
    gain (room recall@10 plus room MRR@10); if none keeps it, the best gain.
    Then its test numbers and the M0 rule."""
    configs = list(next(iter(result["sets"].values()))["configs"])
    picks = {}
    for group, variants in groups_of(configs, base).items():
        def dev_gain(c):
            p = result["pooled"][c]["dev"]
            return p["r@10"]["diff"] + p["mrr@10"]["diff"]
        fair = [c for c in variants if dev_worst(result, c) >= -0.01] or variants
        best = max(fair, key=lambda c: (dev_gain(c), c))
        picks[group] = {"variant": best, "variants": variants, "dev_gain": dev_gain(best),
                        "dev_worst_set": dev_worst(result, best),
                        "test": result["pooled"][best]["test"], **result["accepted"][best]}
    return picks


def print_table(result, base):
    sets = list(result["sets"])
    print("| config | " + " | ".join(f"{s} r@10 / MRR" for s in sets) + " | pooled test d r@10 [95%] | d MRR [95%] | p50x | ok |")
    print("|---|" + "---:|" * (len(sets) + 4))
    configs = list(next(iter(result["sets"].values()))["configs"])
    for c in configs:
        cells = []
        for s in sets:
            m = result["sets"][s]["configs"][c]["parts"]["test"]["metrics"]["hybrid"]["room"]
            cells.append(f"{m['r@10']:.3f} / {m['mrr@10']:.3f}")
        if c == base:
            tail = ["", "", "1.00", ""]
        else:
            p = result["pooled"][c]["test"]
            a = result["accepted"][c]
            fmt = lambda v: f"{v['diff']:+.3f} [{v['lo']:+.3f}, {v['hi']:+.3f}]"
            tail = [fmt(p["r@10"]), fmt(p["mrr@10"]), f"{a['max_p50_ratio']:.2f}", "yes" if a["accepted"] else ""]
        print(f"| {c} | " + " | ".join(cells + tail) + " |")
    chunked = [s for s in sets if "chunk" in result["sets"][s]]
    if chunked:
        print()
        print("| set (test) | KAPU base hybrid | chunk BM25 dedup | chunk vector dedup | chunk hybrid dedup | chunk BM25 units |")
        print("|---|---:|---:|---:|---:|---:|")
        for s in chunked:
            e = result["sets"][s]
            k = e["configs"][base]["parts"]["test"]["metrics"]["hybrid"]["room"]
            c = e["chunk"]
            cell = lambda m: f"{m['r@10']:.3f} / {m['mrr@10']:.3f}"
            print(f"| {s} | {cell(k)} | {cell(c['bm25']['dedup']['test'])} | {cell(c['vector']['dedup']['test'])} | "
                  f"{cell(c['hybrid']['dedup']['test']) if 'hybrid' in c else '-'} | {cell(c['bm25']['units']['test'])} |")
    print()
    print("| fix | chosen on dev | dev gain | test d r@10 [95%] | test d MRR [95%] | worst set | accepted |")
    print("|---|---|---:|---:|---:|---:|---|")
    for group, p in result["selection"].items():
        fmt = lambda v: f"{v['diff']:+.3f} [{v['lo']:+.3f}, {v['hi']:+.3f}]"
        print(f"| {group} | {p['variant']} | {p['dev_gain']:+.3f} | {fmt(p['test']['r@10'])} | {fmt(p['test']['mrr@10'])} | "
              f"{p['worst_set_diff']:+.3f} | {'yes' if p['accepted'] else 'no'} |")


if __name__ == "__main__":
    {"run": run, "report": report}[sys.argv[1]](sys.argv[2:])
