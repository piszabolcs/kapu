"""The title-weight axis of the M0 matrix (plan v2): BM25 on files built with
another weight of the title and tag words.

    python tools/study/title_weight.py <kapu> <splits-dir> <work-dir> <out.json> \
        <set>=<questions.jsonl>:<file-w1.kapu>,<file-w2.kapu>,... ...

Each file is built with `kapu --no-model --title-weight <w> import` (no
vectors: the vectors do not depend on the weight, so the BM25 ranking shows
the whole effect). The weight is read from the file name (`-tw<w>.kapu`); the
base is weight 3 (ASSUMPTION-05). Per set: room and strict BM25 metrics on
the dev and test parts, and the paired bootstrap of room recall@10 and MRR@10
against weight 3, per set and pooled.
"""

import json
import re
import subprocess
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ablation import SEED, bootstrap, hits, metrics, question_ranks  # noqa: E402


def main():
    kapu, splits, work, out = str(Path(sys.argv[1]).resolve()), Path(sys.argv[2]), Path(sys.argv[3]), Path(sys.argv[4])
    work.mkdir(parents=True, exist_ok=True)
    base_cfg = work / "base.json"
    base_cfg.write_text('[{"name": "base"}]\n', encoding="utf-8")
    rng = np.random.default_rng(SEED)
    result, pooled = {"sets": {}}, {}
    for spec in sys.argv[5:]:
        name, rest = spec.split("=", 1)
        qfile, files = rest.split(":", 1) if rest.count(":") == 1 else rest.rsplit(":", 1)
        parts = [p for _, p in json.loads((splits / f"{name}.json").read_text(encoding="utf-8"))["lines"]]
        ranks = {}
        for f in files.split(","):
            w = int(re.search(r"-tw(\d+)\.kapu$", f).group(1))
            rk = work / f"{name}-tw{w}.rankings.jsonl"
            subprocess.run([kapu, "--no-model", "--json", "bench", f, qfile, "--config", str(base_cfg),
                            "--rankings", str(rk)], capture_output=True, check=True)
            ranks[w] = [question_ranks(json.loads(l)) for l in rk.read_text(encoding="utf-8").splitlines()]
        entry = {}
        for w, qs in sorted(ranks.items()):
            entry[w] = {}
            for part in ("dev", "test"):
                sel = [i for i, p in enumerate(parts) if p == part]
                entry[w][part] = {"room": metrics([qs[i]["bm25"][1] for i in sel]),
                                  "strict": metrics([qs[i]["bm25"][0] for i in sel])}
                if w != 3:
                    entry[w][part]["vs_3"] = {}
                    for metric in ("r@10", "mrr@10"):
                        a = hits([qs[i]["bm25"][1] for i in sel], metric)
                        b = hits([ranks[3][i]["bm25"][1] for i in sel], metric)
                        entry[w][part]["vs_3"][metric] = bootstrap(a, b, rng)
                        acc = pooled.setdefault(w, {}).setdefault(part, {}).setdefault(metric, ([], []))
                        acc[0].append(a)
                        acc[1].append(b)
        result["sets"][name] = entry
    result["pooled"] = {w: {part: {m: bootstrap(np.concatenate(a), np.concatenate(b), rng) for m, (a, b) in by.items()}
                            for part, by in parts_.items()} for w, parts_ in pooled.items()}
    out.write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8")
    for w, by_part in sorted(result["pooled"].items()):
        for part in ("dev", "test"):
            p = by_part[part]
            print(f"weight {w} {part}: r@10 {p['r@10']['diff']:+.3f} [{p['r@10']['lo']:+.3f}, {p['r@10']['hi']:+.3f}], "
                  f"MRR {p['mrr@10']['diff']:+.3f} [{p['mrr@10']['lo']:+.3f}, {p['mrr@10']['hi']:+.3f}]")
    for name, entry in result["sets"].items():
        print(name, {w: round(e["test"]["room"]["r@10"], 3) for w, e in entry.items()},
              {w: round(e["test"]["room"]["mrr@10"], 3) for w, e in entry.items()})


if __name__ == "__main__":
    main()
