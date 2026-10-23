"""Chunk-RAG search tool over a room folder (study pilot, system C2b), logged
like tools/kapu_mcp_call.py.

    python tools/study/rag_call.py <dir> search "<query>" [--k 8] [--tag t]

Returns the top-k 512-token chunks by potion-multilingual-128M cosine
(chunk_baseline.py, embeddings cached), each with its file name.
"""

import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from calllog import pop_tag, run  # noqa: E402
from chunk_baseline import chunk_index, embedder  # noqa: E402


def search(d: Path, query: str, k: int) -> str:
    cs, emb = chunk_index(d)
    q = embedder().encode([query]).astype(np.float32)[0]
    q /= np.linalg.norm(q) + 1e-9
    order = np.argsort(-(emb @ q))[:k]
    return "\n\n".join(f"[{n}] {cs[i][0]}.md\n{cs[i][1].strip()}" for n, i in enumerate(order, 1))


def main(argv):
    tag, argv = pop_tag(argv)
    if len(argv) >= 3 and argv[1] == "search":
        k = int(argv[argv.index("--k") + 1]) if "--k" in argv else 8
        return run("search", {"query": argv[2], "k": k}, tag, lambda: search(Path(argv[0]), argv[2], k))
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
