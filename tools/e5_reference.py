"""Reference vectors for the Rust BERT encoder (plan v3 V2, D171).

Developer tool. For a few texts it writes `tests/model/e5_reference.json`: the
token ids, the vector ONNX Runtime computes from the source `model.onnx`
(batch of one), and the vector of a NumPy re-implementation that reads the
converted `model.safetensors` (`tools/onnx_to_kapu.py`) and follows the ONNX
graph step by step: `DynamicQuantizeLinear` per tensor (round half to even),
`MatMulInteger` in exact integers, layer norm, erf GELU, softmax, mean pooling
over every token, L2 normalisation. The Rust test
`crates/kapu-core/tests/bert.rs` compares against both when the model is there.

    tools/.venv/Scripts/python -I tools/e5_reference.py models/multilingual-e5-small-qint8 models/multilingual-e5-small-q8
"""
import json
import math
import pathlib
import struct
import sys

import numpy as np

ROOT = pathlib.Path(__file__).resolve().parent.parent
TEXTS = [
    "query: how do I center a div horizontally",
    "passage: The glass button gets a blurred background: backdrop-filter: blur(12px).",
    "query: Mikor kell beadni a társasági adó bevallását?",
    "passage: contextBridge.exposeInMainWorld('api', { ping: () => ipcRenderer.invoke('ping') });",
    "passage: " + "Hosszú szöveg a tokenkorláthoz, sok szóval és írásjellel. " * 60,
    "query: ",
    "passage: ☃ ༀ \U0001F600 ◆◆◆",
]


def load(path: pathlib.Path) -> dict[str, np.ndarray]:
    raw = path.read_bytes()
    n = struct.unpack("<Q", raw[:8])[0]
    header = json.loads(raw[8 : 8 + n])
    kinds = {"F32": "<f4", "U8": "u1", "I8": "i1"}
    out = {}
    for name, t in header.items():
        if name == "__metadata__":
            continue
        a, b = t["data_offsets"]
        out[name] = np.frombuffer(raw[8 + n + a : 8 + n + b], dtype=kinds[t["dtype"]]).reshape(t["shape"])
    return out


F = np.float32


def dql(x: np.ndarray):
    """ONNX DynamicQuantizeLinear: u8 per tensor, the range always holds 0."""
    lo, hi = F(min(F(0), x.min())), F(max(F(0), x.max()))
    scale = F((hi - lo) / F(255))
    if scale == 0:
        return np.zeros(x.shape, np.int32), F(0)
    zero = np.rint(np.clip(F(0) - lo / scale, 0, 255))
    q = np.clip(np.rint(x / scale) + zero, 0, 255)
    return (q - zero).astype(np.int32), scale


def linear(x, w, p):
    q, s = dql(x)
    wq = w[f"{p}.w"].astype(np.int32) - w[f"{p}.zero"].astype(np.int32)[:, None]
    acc = q @ wq.T
    return acc.astype(F) * (s * w[f"{p}.scale"]).astype(F) + w[f"{p}.bias"]


def layer_norm(x, weight, bias, eps):
    mean = x.mean(axis=-1, keepdims=True, dtype=F)
    d = x - mean
    var = (d * d).mean(axis=-1, keepdims=True, dtype=F)
    return d / np.sqrt(var + F(eps)) * weight + bias


def dequant(w, kind, ids):
    q = w[f"{kind}.q"][ids].astype(np.int32) - int(w[f"{kind}.zero"][0])
    return q.astype(F) * w[f"{kind}.scale"][0]


ERF = np.vectorize(lambda v: F(math.erf(float(v))), otypes=[F])


def encode(w, cfg, ids):
    n = len(ids)
    hidden, heads = cfg["hidden"], cfg["heads"]
    dh = hidden // heads
    h = dequant(w, "word", ids) + dequant(w, "type", np.zeros(n, int))
    h = h + dequant(w, "position", np.arange(n))
    h = layer_norm(h, w["ln.weight"], w["ln.bias"], cfg["layer_norm_eps"])
    for layer in range(cfg["layers"]):
        p = f"layer.{layer}"
        q = linear(h, w, f"{p}.q").reshape(n, heads, dh).transpose(1, 0, 2)
        k = linear(h, w, f"{p}.k").reshape(n, heads, dh).transpose(1, 2, 0)
        v = linear(h, w, f"{p}.v").reshape(n, heads, dh).transpose(1, 0, 2)
        qq, qs = dql(q)
        kq, ks = dql(k)
        scores = (qq @ kq).astype(F) * F(qs * ks) / F(math.sqrt(dh))
        scores = np.exp(scores - scores.max(axis=-1, keepdims=True))
        probs = scores / scores.sum(axis=-1, keepdims=True, dtype=F)
        pq, ps = dql(probs)
        vq, vs = dql(v)
        ctx = (pq @ vq).astype(F) * F(ps * vs)
        ctx = ctx.transpose(1, 0, 2).reshape(n, hidden)
        h = layer_norm(linear(ctx, w, f"{p}.o") + h, w[f"{p}.ln1.weight"], w[f"{p}.ln1.bias"], cfg["layer_norm_eps"])
        up = linear(h, w, f"{p}.up")
        g = up * (ERF(up / F(1.4142135381698608)) + F(1)) * F(0.5)
        h = layer_norm(linear(g, w, f"{p}.down") + h, w[f"{p}.ln2.weight"], w[f"{p}.ln2.bias"], cfg["layer_norm_eps"])
    mean = h.astype(np.float64).mean(axis=0)
    return mean / np.linalg.norm(mean)


def main() -> None:
    src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
    import onnxruntime as ort
    from tokenizers import Tokenizer

    tok = Tokenizer.from_file(str(dst / "tokenizer.json"))
    tok.enable_truncation(512)
    cfg = json.loads((dst / "config.json").read_text(encoding="utf-8"))
    w = load(dst / "model.safetensors")
    opts = ort.SessionOptions()
    opts.intra_op_num_threads = 1
    sess = ort.InferenceSession(str(src / "model.onnx"), opts, providers=["CPUExecutionProvider"])
    cases = []
    for text in TEXTS:
        ids = tok.encode(text).ids
        a = np.array([ids], dtype=np.int64)
        out = sess.run(None, {"input_ids": a, "attention_mask": np.ones_like(a), "token_type_ids": np.zeros_like(a)})[0][0]
        ref = out.astype(np.float64).mean(axis=0)
        ref /= np.linalg.norm(ref)
        emu = encode(w, cfg, np.array(ids))
        print(f"{len(ids):4d} tokens  cos(numpy, onnxruntime) = {float(ref @ emu):.6f}")
        cases.append({"text": text, "ids": ids, "onnxruntime": [float(x) for x in ref], "numpy": [float(x) for x in emu]})
    out_path = ROOT / "tests" / "model" / "e5_reference.json"
    out_path.write_text(json.dumps({"model": dst.name, "cases": cases}, ensure_ascii=False), encoding="utf-8")
    print(out_path)


if __name__ == "__main__":
    main()
