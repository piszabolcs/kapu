"""Chunk-RAG retrieval baselines at room level (study pilot, Task 4).

    python tools/study/chunk_baseline.py <rooms_dir> <questions.jsonl> <out.json>

Rooms are cut into 512-token chunks (o200k_base, 64 overlap) and ranked by
potion-multilingual-128M (f32, model2vec) cosine and by BM25 (k1 1.2, b 0.75,
lower-case word tokens, no stemming). A question hits when a chunk of its gold
room is among the top k units, the same rule as `kapu bench` lenient (a door
or its room); `dedup` also reports k distinct rooms. Embeddings are cached
next to the room folder (`<rooms>.meta/chunks.npz`).
"""

import json
import math
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

import numpy as np
import tiktoken

ROOT = Path(__file__).resolve().parent.parent.parent
MODEL = ROOT / "models" / "potion-multilingual-128M"
ENC = tiktoken.get_encoding("o200k_base")
SIZE, OVERLAP = 512, 64
WORD = re.compile(r"\w+")


def chunks(rooms: Path):
    """[(room id, chunk text)] in room-id order."""
    out = []
    for f in sorted(rooms.glob("*.md")):
        toks = ENC.encode(f.read_text(encoding="utf-8"), disallowed_special=())
        for start in range(0, max(1, len(toks)), SIZE - OVERLAP):
            out.append((f.stem, ENC.decode(toks[start:start + SIZE])))
            if start + SIZE >= len(toks):
                break
    return out


def embedder():
    from model2vec import StaticModel
    return StaticModel.from_pretrained(str(MODEL))


def chunk_index(rooms: Path):
    """(chunk list, normalised embedding matrix), cached in `<rooms>.meta/chunks.npz`."""
    cs = chunks(rooms)
    cache = rooms.parent / f"{rooms.name}.meta" / "chunks.npz"
    if cache.is_file():
        emb = np.load(cache)["emb"]
        if len(emb) == len(cs):
            return cs, emb
    emb = embedder().encode([t for _, t in cs], batch_size=512).astype(np.float32)
    emb /= np.linalg.norm(emb, axis=1, keepdims=True) + 1e-9
    np.savez(cache, emb=emb)
    return cs, emb


class BM25:
    def __init__(self, texts, k1=1.2, b=0.75):
        self.k1, self.b = k1, b
        self.docs = [Counter(WORD.findall(t.lower())) for t in texts]
        self.lens = np.array([sum(d.values()) for d in self.docs], dtype=np.float32)
        self.avg = float(self.lens.mean())
        self.post = defaultdict(list)
        for i, d in enumerate(self.docs):
            for w, tf in d.items():
                self.post[w].append((i, tf))
        n = len(self.docs)
        self.idf = {w: math.log(1 + (n - len(p) + 0.5) / (len(p) + 0.5)) for w, p in self.post.items()}

    def scores(self, query):
        s = np.zeros(len(self.docs), dtype=np.float32)
        for w in set(WORD.findall(query.lower())):
            for i, tf in self.post.get(w, ()):
                s[i] += self.idf[w] * tf * (self.k1 + 1) / (tf + self.k1 * (1 - self.b + self.b * self.lens[i] / self.avg))
        return s


def metrics(ranked_rooms_per_q, golds):
    """recall@5, recall@10, MRR@10 over rank lists of room ids (one unit per rank)."""
    r5 = r10 = mrr = 0.0
    for ranked, gold in zip(ranked_rooms_per_q, golds):
        top = ranked[:10]
        if gold in top[:5]:
            r5 += 1
        if gold in top:
            r10 += 1
            mrr += 1 / (top.index(gold) + 1)
    n = len(golds)
    return {"recall@5": round(r5 / n, 3), "recall@10": round(r10 / n, 3), "mrr@10": round(mrr / n, 3), "n": n}


def dedup(seq):
    seen, out = set(), []
    for x in seq:
        if x not in seen:
            seen.add(x)
            out.append(x)
    return out


def main(rooms: Path, questions: Path, out: Path):
    qs = [json.loads(l) for l in questions.read_text(encoding="utf-8").splitlines() if l.strip()]
    golds = [Path(q["valasz_fajl"]).stem for q in qs]
    cs, emb = chunk_index(rooms)
    room_of = [r for r, _ in cs]
    model = embedder()
    qemb = model.encode([q["kerdes"] for q in qs]).astype(np.float32)
    qemb /= np.linalg.norm(qemb, axis=1, keepdims=True) + 1e-9
    bm = BM25([t for _, t in cs])
    report = {"chunks": len(cs), "chunk_tokens": SIZE, "overlap": OVERLAP}
    for name, score_fn in (("vector", lambda i: emb @ qemb[i]), ("bm25", lambda i: bm.scores(qs[i]["kerdes"]))):
        units, distinct = [], []
        for i in range(len(qs)):
            order = np.argsort(-score_fn(i))[:200]
            ranked = [room_of[j] for j in order]
            units.append(ranked)
            distinct.append(dedup(ranked))
        report[name] = {"units": metrics(units, golds), "dedup": metrics(distinct, golds)}
    out.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main(Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3]))
