"""Vector-model candidates on the same units (plan v3 V1).

    python tools/study/model_candidates.py run <rooms_dir> <questions.jsonl> \
        <kapu rankings.jsonl> <out.json> [--configs base,passage-prefix-3-en] [--models a,b]
    python tools/study/model_candidates.py report <splits-dir> <out.json> <set>=<run.json> ...

`run` embeds the 512-token chunks of `chunk_baseline.py` with every model of
MODELS and keeps, per model and question, the rank of the expected room among
distinct rooms (1-based, None beyond 100) for three rankings: the chunk vectors
alone, the chunk hybrid (chunk BM25 + chunk vectors, RRF k 60 of the first 50
chunks, as `chunk_baseline.py`), and KAPU's BM25 door ranking of each
`--configs` configuration (from `ablation.py run`) fused with the chunk
vectors by RRF (k 60) of the first 50 distinct rooms of each, which is what a
model swap in KAPU could give. KAPU's own BM25 and hybrid ranks of the same
configurations are kept for reference. Embeddings are cached next to the room
folder (`<rooms>.meta/emb-<model>/`), the transformer in shards of SHARD
chunks, so an interrupted run goes on where it stopped; a batch that fails is
encoded text by text, and a text that still fails gets a zero vector and is
counted (`failed`).

`report` scores them on the fixed dev/test split (`bench/splits`) and gives,
on the test part, the difference of each model to the first one (the packaged
model) with a paired bootstrap 95% interval, per set and pooled.
"""

import json
import os
import sys
import time
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ablation import DRAWS, SEED, bootstrap, hits, metrics  # noqa: E402
from chunk_baseline import BM25, chunks, dedup  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
MODELS = {
    # name: (kind, directory, query prefix, passage prefix)
    "potion-multilingual-128M": ("static", "potion-multilingual-128M", "", ""),
    "potion-retrieval-32M": ("static", "potion-retrieval-32M", "", ""),
    "multilingual-e5-small-qint8": ("onnx", "multilingual-e5-small-qint8", "query: ", "passage: "),
}
SHARD = 1024
BATCH = 16
MAX_TOKENS = 512
FUSE = 50
RRF_K = 60


class Static:
    def __init__(self, path: Path):
        from model2vec import StaticModel
        self.model = StaticModel.from_pretrained(str(path))
        self.failed = 0
        self.truncated = 0

    def encode(self, texts):
        return self.model.encode(texts, batch_size=512).astype(np.float32)


class Onnx:
    """A BERT-like encoder in ONNX: mean pooling over the attention mask."""

    def __init__(self, path: Path):
        import onnxruntime as ort
        from tokenizers import Tokenizer
        self.tok = Tokenizer.from_file(str(path / "tokenizer.json"))
        self.tok.enable_truncation(MAX_TOKENS)
        self.tok.no_padding()
        opts = ort.SessionOptions()
        opts.intra_op_num_threads = os.cpu_count() or 1
        self.sess = ort.InferenceSession(str(path / "model.onnx"), opts, providers=["CPUExecutionProvider"])
        self.inputs = [i.name for i in self.sess.get_inputs()]
        pad = self.tok.token_to_id("<pad>")
        self.pad = 1 if pad is None else pad
        self.failed = 0
        self.truncated = 0

    def _run(self, encs):
        width = max(len(e.ids) for e in encs)
        ids = np.full((len(encs), width), self.pad, dtype=np.int64)
        mask = np.zeros((len(encs), width), dtype=np.int64)
        for i, e in enumerate(encs):
            ids[i, :len(e.ids)] = e.ids
            mask[i, :len(e.ids)] = 1
        feed = {"input_ids": ids, "attention_mask": mask}
        if "token_type_ids" in self.inputs:
            feed["token_type_ids"] = np.zeros_like(ids)
        out = self.sess.run(None, feed)[0]
        m = mask[:, :, None].astype(np.float32)
        return (out * m).sum(axis=1) / np.maximum(m.sum(axis=1), 1.0)

    def encode(self, texts):
        encs = self.tok.encode_batch(texts)
        self.truncated += sum(1 for e in encs if e.overflowing)
        order = sorted(range(len(texts)), key=lambda i: len(encs[i].ids))
        rows = [None] * len(texts)
        for start in range(0, len(order), BATCH):
            part = order[start:start + BATCH]
            try:
                vecs = self._run([encs[i] for i in part])
            except Exception:  # noqa: BLE001 - one bad batch must not stop the run
                vecs = []
                for i in part:
                    try:
                        vecs.append(self._run([encs[i]])[0])
                    except Exception as e:  # noqa: BLE001
                        print(f"  text {i} failed: {e}", file=sys.stderr)
                        self.failed += 1
                        vecs.append(None)
            for i, v in zip(part, vecs):
                rows[i] = v
        dim = next((len(v) for v in rows if v is not None), 384)
        return np.stack([v if v is not None else np.zeros(dim, dtype=np.float32) for v in rows]).astype(np.float32)


def load(name):
    kind, directory, _, _ = MODELS[name]
    path = ROOT / "models" / directory
    return Static(path) if kind == "static" else Onnx(path)


def normalised(x):
    return x / (np.linalg.norm(x, axis=1, keepdims=True) + 1e-9)


def chunk_embeddings(rooms: Path, texts, name, model):
    """The chunk matrix of model `name`, cached per model; the transformer in shards."""
    _, _, _, prefix = MODELS[name]
    meta = rooms.parent / f"{rooms.name}.meta"
    if name == "potion-multilingual-128M":
        # the cache of chunk_baseline.py
        cache = meta / "chunks.npz"
        if cache.is_file():
            emb = np.load(cache)["emb"]
            if len(emb) == len(texts):
                return emb
    folder = meta / f"emb-{name}"
    folder.mkdir(parents=True, exist_ok=True)
    manifest = folder / "manifest.json"
    want = {"chunks": len(texts), "shard": SHARD}
    if manifest.is_file() and json.loads(manifest.read_text(encoding="utf-8")) != want:
        for f in folder.glob("*.npy"):
            f.unlink()
    manifest.write_text(json.dumps(want), encoding="utf-8")
    parts = []
    started = time.time()
    shards = range(0, len(texts), SHARD)
    for n, start in enumerate(shards):
        f = folder / f"{start // SHARD:05d}.npy"
        if f.is_file():
            parts.append(np.load(f))
            continue
        emb = normalised(model.encode([prefix + t for t in texts[start:start + SHARD]]))
        np.save(f, emb)
        parts.append(emb)
        done = min(start + SHARD, len(texts))
        rate = done / max(time.time() - started, 1e-9)
        print(f"  {name}: {done}/{len(texts)} chunks, {rate:.1f}/s", file=sys.stderr, flush=True)
    return np.concatenate(parts) if parts else np.zeros((0, 1), dtype=np.float32)


def room(door):
    return door.split("/", 1)[0]


def rank(ranked_rooms, gold):
    top = ranked_rooms[:100]
    return top.index(gold) + 1 if gold in top else None


def rrf(*lists):
    fused = {}
    for lst in lists:
        for r, x in enumerate(lst[:FUSE]):
            fused[x] = fused.get(x, 0.0) + 1 / (RRF_K + r + 1)
    return [x for x, _ in sorted(fused.items(), key=lambda kv: (-kv[1], kv[0]))]


def kapu_rows(path: Path, configs, n):
    """Per configuration the rows of `ablation.py run` in question order."""
    out = {c: [] for c in configs}
    with open(path, encoding="utf-8") as f:
        for line in f:
            row = json.loads(line)
            if row["config"] in out:
                out[row["config"]].append(row)
    for c, rows in out.items():
        assert len(rows) == n, (c, len(rows), n)
    return out


def run(argv):
    configs, names = ["base"], list(MODELS)
    for flag in ("--configs", "--models"):
        if flag in argv:
            i = argv.index(flag)
            value = argv[i + 1].split(",")
            del argv[i:i + 2]
            if flag == "--configs":
                configs = value
            else:
                names = value
    rooms, questions, rankings, out = Path(argv[0]), Path(argv[1]), Path(argv[2]), Path(argv[3])
    qs = [json.loads(line) for line in questions.read_text(encoding="utf-8").splitlines() if line.strip()]
    golds = [Path(q["valasz_fajl"]).stem for q in qs]
    cs = chunks(rooms)
    room_of = [r for r, _ in cs]
    texts = [t for _, t in cs]
    kapu = kapu_rows(rankings, configs, len(qs))
    kapu_bm25 = {c: [dedup([room(d) for d in row["bm25"]]) for row in rows] for c, rows in kapu.items()}
    result = {"chunks": len(cs), "questions": len(qs), "configs": configs, "kapu": {}, "models": {}}
    for c, rows in kapu.items():
        result["kapu"][c] = {
            "bm25": [rank(lst, g) for lst, g in zip(kapu_bm25[c], golds)],
            "hybrid": [rank(dedup([room(d) for d in row["hybrid"]]), g) for row, g in zip(rows, golds)],
        }
    bm = BM25(texts)
    bm_orders = [np.argsort(-bm.scores(q["kerdes"]))[:FUSE].tolist() for q in qs]
    result["chunk_bm25"] = [rank(dedup([room_of[j] for j in o]), g) for o, g in zip(bm_orders, golds)]
    for name in names:
        model = load(name)
        started = time.time()
        emb = chunk_embeddings(rooms, texts, name, model)
        build_s = time.time() - started
        _, _, qprefix, _ = MODELS[name]
        started = time.time()
        qemb = normalised(model.encode([qprefix + q["kerdes"] for q in qs]))
        query_ms = (time.time() - started) * 1000 / max(len(qs), 1)
        entry = {"vector": [], "chunk_hybrid": [], "dim": int(emb.shape[1]), "embed_s": round(build_s, 1),
                 "query_ms": round(query_ms, 2), "failed": model.failed, "truncated": model.truncated}
        fused = {c: [] for c in configs}
        for i, g in enumerate(golds):
            order = np.argsort(-(emb @ qemb[i]))[:400].tolist()
            vec_rooms = dedup([room_of[j] for j in order])
            entry["vector"].append(rank(vec_rooms, g))
            hyb = rrf(bm_orders[i], order)
            entry["chunk_hybrid"].append(rank(dedup([room_of[j] for j in hyb]), g))
            for c in configs:
                fused[c].append(rank(rrf(kapu_bm25[c][i], vec_rooms), g))
        for c in configs:
            entry[f"kapu_bm25+vec:{c}"] = fused[c]
        result["models"][name] = entry
        print(f"{name}: dim {entry['dim']}, embed {build_s:.0f} s, query {query_ms:.1f} ms, "
              f"failed {model.failed}, truncated {model.truncated}", file=sys.stderr, flush=True)
    out.write_text(json.dumps(result, separators=(",", ":")) + "\n", encoding="utf-8")


def report(argv):
    splits, out = Path(argv[0]), Path(argv[1])
    rng = np.random.default_rng(SEED)
    result = {"sets": {}, "pooled": {}, "meta": {"draws": DRAWS, "seed": SEED, "fuse": FUSE, "rrf_k": RRF_K}}
    pooled = {}
    for spec in argv[2:]:
        name, path = spec.split("=", 1)
        doc = json.loads(Path(path).read_text(encoding="utf-8"))
        parts = [p for _, p in json.loads((splits / f"{name}.json").read_text(encoding="utf-8"))["lines"]]
        assert len(parts) == doc["questions"], name
        rankings = {"chunk_bm25": doc["chunk_bm25"]}
        for c, r in doc["kapu"].items():
            rankings[f"kapu:{c}:bm25"] = r["bm25"]
            rankings[f"kapu:{c}:hybrid"] = r["hybrid"]
        models = list(doc["models"])
        for m, entry in doc["models"].items():
            for method, ranks in entry.items():
                if isinstance(ranks, list):
                    rankings[f"{m}:{method}"] = ranks
        entry = {"rankings": {}, "models": {m: {k: v for k, v in e.items() if not isinstance(v, list)}
                                            for m, e in doc["models"].items()}}
        for key, ranks in rankings.items():
            entry["rankings"][key] = {part: metrics([r for r, p in zip(ranks, parts) if part == "all" or p == part])
                                      for part in ("dev", "test", "all")}
        base = models[0]
        test = [i for i, p in enumerate(parts) if p == "test"]
        entry["vs_first_model"] = {}
        for m in models[1:]:
            for method, ranks in doc["models"][m].items():
                if not isinstance(ranks, list):
                    continue
                base_ranks = doc["models"][base][method]
                key = f"{m}:{method}"
                entry["vs_first_model"][key] = {}
                for metric in ("r@10", "mrr@10"):
                    a = hits([ranks[i] for i in test], metric)
                    b = hits([base_ranks[i] for i in test], metric)
                    entry["vs_first_model"][key][metric] = bootstrap(a, b, rng)
                    acc = pooled.setdefault(key, {}).setdefault(metric, ([], []))
                    acc[0].append(a)
                    acc[1].append(b)
        result["sets"][name] = entry
    for key, by_metric in pooled.items():
        result["pooled"][key] = {metric: bootstrap(np.concatenate(a), np.concatenate(b), rng)
                                 for metric, (a, b) in by_metric.items()}
    out.write_text(json.dumps(result, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    for name, entry in result["sets"].items():
        print(f"== {name} (test r@10 / mrr@10)")
        for key, parts in entry["rankings"].items():
            t = parts["test"]
            print(f"  {key:60s} {t['r@10']:.3f} {t['mrr@10']:.3f}")
    print("== pooled test, difference to the first model [95% interval]")
    for key, by_metric in result["pooled"].items():
        cells = " ".join(f"{metric} {v['diff']:+.3f} [{v['lo']:+.3f}, {v['hi']:+.3f}]" for metric, v in by_metric.items())
        print(f"  {key:60s} {cells}")


if __name__ == "__main__":
    {"run": run, "report": report}[sys.argv[1]](sys.argv[2:])
