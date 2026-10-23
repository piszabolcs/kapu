"""Compare the Rust Leiden with Python leidenalg on a dumped room graph (D43).

    kapu-bench measure --model models/potion-multilingual-128M --dump-graph <dir>
    tools/.venv/Scripts/python tools/leiden_check.py <dir>/graph-300.tsv

Prints the leidenalg modularity (seed 42) and the number of communities. The
Rust value is the `Modularitás` column of the measure table.
"""
import sys

import igraph as ig
import leidenalg as la

for path in sys.argv[1:]:
    edges, weights, n = [], [], 0
    for line in open(path, encoding="utf-8"):
        a, b, w = line.split()
        a, b = int(a), int(b)
        edges.append((a, b))
        weights.append(float(w))
        n = max(n, a + 1, b + 1)
    g = ig.Graph(n=n, edges=edges)
    g.es["weight"] = weights
    best = max(
        (la.find_partition(g, la.ModularityVertexPartition, weights="weight", seed=s) for s in range(10)),
        key=lambda p: p.modularity,
    )
    p42 = la.find_partition(g, la.ModularityVertexPartition, weights="weight", seed=42)
    print(f"{path}: leidenalg seed42 Q={p42.modularity:.4f} ({len(p42)} groups), best of 10 seeds Q={best.modularity:.4f}")
