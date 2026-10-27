"""How well the Leiden groups match the documentation's own folders (plan v2 N13).

    python tools/study/group_quality.py <out.json> <set>=<file.kapu>:<rooms-dir> ...

The truth is the source folder of each room in `<rooms-dir>.meta/manifest.json`
(`src`), one and two levels below the docs root (Kubernetes
`content/en/docs/concepts/workloads/...`: `concepts`, `concepts/workloads`).
Scores: Adjusted Rand Index and Normalised Mutual Information (arithmetic
mean) of the file's groups (`#MAP`), against two references with the same
number of groups: the file's group sizes dealt out at random (mean of 20
draws), and k-means (k-means++, seed 42) on room vectors (the mean of the
room's 512-token chunk vectors from `chunk_baseline.py`). The edge-type
ablation reruns Leiden (leidenalg, modularity, seed 42) on the rooms' graph
from `#GRAPH` with only some edge types, weights summed per room pair as the
builder does. Also the size of the largest group in rooms and tokens.
"""

import json
import math
import random
import sys
from collections import Counter
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from chunk_baseline import chunk_index  # noqa: E402

ROOTS = ("content/en/docs/", "files/en-us/", "src/")
SUBSETS = [("link",), ("term",), ("sim",), ("link", "term"), ("link", "sim"), ("term", "sim"), ("link", "term", "sim")]


def sections(path):
    text = Path(path).read_text(encoding="utf-8", errors="replace")
    out, name, lines = {}, None, []
    for line in text.split("\n"):
        if name is None and line in ("#MAP", "#GRAPH"):
            name, lines = line, []
        elif name is not None and line == "#END":
            out[name] = lines
            name = None
        elif name is not None:
            lines.append(line)
        if "#MAP" in out and "#GRAPH" in out:
            break
    return out


def file_groups(map_lines):
    rooms, sizes = {}, {}
    for line in map_lines:
        f = [x.strip() for x in line.split(" | ")]
        if line.startswith("R "):
            rooms[f[0][2:]] = f[3]
        elif line.startswith("G "):
            sizes[f[0][2:]] = (int(f[2]), int(f[3].rstrip("t")))
    return rooms, sizes


def truth(rooms_dir, ids, depth):
    manifest = json.loads((Path(f"{rooms_dir}.meta") / "manifest.json").read_text(encoding="utf-8"))
    out = {}
    for rid in ids:
        src = manifest.get(rid, {}).get("src", "").replace("\\", "/")
        for root in ROOTS:
            if src.startswith(root):
                src = src[len(root):]
                break
        parts = src.split("/")
        # the folders, not the file; an `index.md` page (MDN) or `_index.md`
        # section (Kubernetes) is its folder, which is not its group
        parts = parts[:-2] if parts[-1] in ("index.md", "_index.md") else parts[:-1]
        out[rid] = "/".join(parts[:depth]) or "(root)"
    return out


def comb2(n):
    return n * (n - 1) / 2


def ari(a, b):
    pairs = Counter(zip(a, b))
    sa, sb = Counter(a), Counter(b)
    index = sum(comb2(n) for n in pairs.values())
    ea, eb = sum(comb2(n) for n in sa.values()), sum(comb2(n) for n in sb.values())
    expected = ea * eb / comb2(len(a))
    top = (ea + eb) / 2
    return (index - expected) / (top - expected) if top != expected else 1.0


def nmi(a, b):
    n = len(a)
    pairs = Counter(zip(a, b))
    sa, sb = Counter(a), Counter(b)
    mi = sum(c / n * math.log(c * n / (sa[x] * sb[y])) for (x, y), c in pairs.items())
    h = lambda s: -sum(c / n * math.log(c / n) for c in s.values())
    ha, hb = h(sa), h(sb)
    return 2 * mi / (ha + hb) if ha + hb > 0 else 1.0


def scores(labels, gold):
    return {"ari": round(ari(labels, gold), 4), "nmi": round(nmi(labels, gold), 4)}


def sq_dist(x, c):
    return np.maximum((x ** 2).sum(1)[:, None] + (c ** 2).sum(1)[None] - 2 * x @ c.T, 0.0)


def kmeans(x, k, seed=42, iters=50):
    rng = np.random.default_rng(seed)
    centres = [x[rng.integers(len(x))]]
    d = sq_dist(x, np.array(centres))[:, 0]
    for _ in range(1, k):
        centres.append(x[rng.choice(len(x), p=d / d.sum())])
        d = np.minimum(d, sq_dist(x, centres[-1][None])[:, 0])
    c = np.array(centres)
    for _ in range(iters):
        labels = np.argmin(sq_dist(x, c), axis=1)
        new = np.array([x[labels == j].mean(0) if (labels == j).any() else c[j] for j in range(k)])
        if np.allclose(new, c):
            break
        c = new
    return labels.tolist()


def room_vectors(rooms_dir, ids):
    cs, emb = chunk_index(Path(rooms_dir))
    acc = {}
    for (rid, _), v in zip(cs, emb):
        acc.setdefault(rid, []).append(v)
    dim = emb.shape[1]
    vecs = np.array([np.mean(acc[r], axis=0) if r in acc else np.zeros(dim) for r in ids], dtype=np.float64)
    vecs /= np.linalg.norm(vecs, axis=1, keepdims=True) + 1e-9
    return vecs


def leiden_on(graph_lines, ids, kinds):
    import igraph as ig
    import leidenalg
    index = {r: i for i, r in enumerate(ids)}
    weights = {}
    for line in graph_lines:
        f = line.split("\t")
        if len(f) != 4 or f[2] not in kinds:
            continue
        a, b = index.get(f[0].split("/")[0]), index.get(f[1].split("/")[0])
        if a is None or b is None or a == b:
            continue
        key = (min(a, b), max(a, b))
        weights[key] = weights.get(key, 0.0) + float(f[3])
    g = ig.Graph(n=len(ids), edges=list(weights), directed=False)
    part = leidenalg.find_partition(g, leidenalg.ModularityVertexPartition,
                                    weights=list(weights.values()) or None, seed=42)
    return part.membership


def main():
    out = Path(sys.argv[1])
    result = {}
    for spec in sys.argv[2:]:
        name, rest = spec.split("=", 1)
        kfile, rooms_dir = rest.rsplit(":", 1)
        sec = sections(kfile)
        rooms, sizes = file_groups(sec["#MAP"])
        ids = sorted(rooms)
        labels = [rooms[r] for r in ids]
        k = len(set(labels))
        entry = {"rooms": len(ids), "groups": k,
                 "largest_group_rooms": max(s[0] for s in sizes.values()),
                 "largest_group_tokens": max(s[1] for s in sizes.values()),
                 "group_sizes": sorted((s[0] for s in sizes.values()), reverse=True)[:10]}
        vecs = room_vectors(rooms_dir, ids)
        km = kmeans(vecs, k)
        for depth in (1, 2):
            t = truth(rooms_dir, ids, depth)
            gold = [t[r] for r in ids]
            rand = []
            for seed in range(20):
                shuffled = labels[:]
                random.Random(seed).shuffle(shuffled)
                rand.append(scores(shuffled, gold))
            e = {"truth_groups": len(set(gold)),
                 "file_groups": scores(labels, gold),
                 "random_same_sizes": {m: round(sum(r[m] for r in rand) / len(rand), 4) for m in ("ari", "nmi")},
                 "kmeans_room_vectors": scores(km, gold),
                 "edges": {}}
            for kinds in SUBSETS:
                e["edges"]["+".join(kinds)] = scores(leiden_on(sec["#GRAPH"], ids, kinds), gold)
            entry[f"depth{depth}"] = e
            print(name, depth, json.dumps(e), flush=True)
        result[name] = entry
    out.write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
