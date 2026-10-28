"""Converts the int8 ONNX form of a BERT sentence encoder into a KAPU model folder
(plan v3 V2, D171).

    tools/.venv/Scripts/python -I tools/onnx_to_kapu.py models/multilingual-e5-small-qint8 models/multilingual-e5-small-q8

The source folder holds `model.onnx` (the dynamically quantized export of
`intfloat/multilingual-e5-small`, `onnx/model_qint8_avx512_vnni.onnx`: opset
11, `DynamicQuantizeLinear` + `MatMulInteger`), `tokenizer.json` and the
Hugging Face `config.json`. The ONNX file is read with the small protobuf
reader below, so no `onnx` package is needed.

Writes `model.safetensors` with these tensors (H hidden, V vocabulary):

- `word.q` U8 [V, H], `word.scale` F32 [1], `word.zero` U8 [1], and the same
  for `position` (512 rows) and `type` (2 rows): value = (q - zero) * scale
- `ln.weight`, `ln.bias` F32 [H]: the embedding layer norm
- per layer `l` and linear map `m` in q, k, v, o (attention output), up
  (intermediate), down (output): `layer.l.m.w` I8 [N, K] (one row per output),
  `layer.l.m.scale` F32 [N], `layer.l.m.zero` I8 [N], `layer.l.m.bias` F32 [N]
- per layer `layer.l.ln1.*` (after attention) and `layer.l.ln2.*` (after the
  feed-forward part)

and a `config.json` that the Rust loader (`crates/kapu-core/src/bert.rs`)
reads: `"kind": "bert"`, sizes, the layer-norm epsilon, the query and passage
prefixes, mean pooling. `tokenizer.json` is copied unchanged. The model name,
and so `emb=` in built files, is the folder name.
"""
import json
import pathlib
import shutil
import struct
import sys

import numpy as np

# --- protobuf wire format, just what an ONNX model needs -------------------


def _varint(b: bytes, i: int) -> tuple[int, int]:
    r = s = 0
    while True:
        c = b[i]
        i += 1
        r |= (c & 0x7F) << s
        s += 7
        if c < 0x80:
            return r, i


def _fields(b: bytes):
    i, n = 0, len(b)
    while i < n:
        key, i = _varint(b, i)
        f, w = key >> 3, key & 7
        if w == 0:
            v, i = _varint(b, i)
        elif w == 1:
            v, i = b[i : i + 8], i + 8
        elif w == 2:
            ln, i = _varint(b, i)
            v, i = b[i : i + ln], i + ln
        elif w == 5:
            v, i = b[i : i + 4], i + 4
        else:
            raise ValueError(f"wire type {w}")
        yield f, w, v


def _ints(w: int, v) -> list[int]:
    if w != 2:
        return [v - (1 << 64) if v >= 1 << 63 else v]
    out, i = [], 0
    while i < len(v):
        x, i = _varint(v, i)
        out.append(x - (1 << 64) if x >= 1 << 63 else x)
    return out


DTYPES = {1: "<f4", 2: "u1", 3: "i1", 6: "<i4", 7: "<i8"}


def _tensor(b: bytes) -> tuple[str, np.ndarray]:
    dims, dtype, name, raw = [], None, "", None
    floats: list[float] = []
    ints: list[int] = []
    for f, w, v in _fields(b):
        if f == 1:
            dims += _ints(w, v)
        elif f == 2:
            dtype = DTYPES[v]
        elif f == 8:
            name = v.decode()
        elif f == 9:
            raw = v
        elif f == 4:
            floats += list(struct.unpack(f"<{len(v) // 4}f", v)) if w == 2 else [struct.unpack("<f", v)[0]]
        elif f in (5, 7):
            # int32_data (also int8 and uint8 values) and int64_data
            ints += _ints(w, v)
    if raw is not None:
        return name, np.frombuffer(raw, dtype=dtype).reshape(dims)
    values = floats if dtype == "<f4" else ints
    return name, np.array(values).astype(dtype).reshape(dims)


def read_onnx(path: pathlib.Path) -> tuple[list[dict], dict[str, np.ndarray]]:
    """(nodes in graph order, initializers by name)."""
    nodes, init = [], {}
    for f, _, v in _fields(path.read_bytes()):
        if f != 7:
            continue
        for gf, _, gv in _fields(v):
            if gf == 1:
                node = {"in": [], "out": [], "op": ""}
                for nf, _, nv in _fields(gv):
                    if nf == 1:
                        node["in"].append(nv.decode())
                    elif nf == 2:
                        node["out"].append(nv.decode())
                    elif nf == 4:
                        node["op"] = nv.decode()
                nodes.append(node)
            elif gf == 5:
                name, arr = _tensor(gv)
                init[name] = arr
    return nodes, init


# --- mapping ----------------------------------------------------------------

LINEAR = {
    "q": "attention/self/query",
    "k": "attention/self/key",
    "v": "attention/self/value",
    "o": "attention/output/dense",
    "up": "intermediate/dense",
    "down": "output/dense",
}
BIAS = {
    "q": "attention.self.query",
    "k": "attention.self.key",
    "v": "attention.self.value",
    "o": "attention.output.dense",
    "up": "intermediate.dense",
    "down": "output.dense",
}


def convert(src: pathlib.Path, dst: pathlib.Path) -> None:
    hf = json.loads((src / "config.json").read_text(encoding="utf-8"))
    assert hf["model_type"] == "bert" and hf["hidden_act"] == "gelu", hf
    assert hf.get("position_embedding_type", "absolute") == "absolute", hf
    hidden, layers = hf["hidden_size"], hf["num_hidden_layers"]
    nodes, init = read_onnx(src / "model.onnx")
    ops = {n["op"] for n in nodes}
    assert {"MatMulInteger", "DynamicQuantizeLinear", "Erf", "Softmax"} <= ops, ops
    assert not {"Attention", "QAttention", "SkipLayerNormalization", "FastGelu"} & ops, ops
    tensors: dict[str, np.ndarray] = {}

    def emb(kind: str, onnx_name: str) -> None:
        q = init[f"embeddings.{onnx_name}.weight_quantized"]
        assert q.dtype == np.uint8 and q.shape[1] == hidden, (kind, q.dtype, q.shape)
        tensors[f"{kind}.q"] = q
        tensors[f"{kind}.scale"] = init[f"embeddings.{onnx_name}.weight_scale"].astype("<f4").reshape(1)
        tensors[f"{kind}.zero"] = init[f"embeddings.{onnx_name}.weight_zero_point"].astype("u1").reshape(1)

    emb("word", "word_embeddings")
    emb("position", "position_embeddings")
    emb("type", "token_type_embeddings")
    tensors["ln.weight"] = init["embeddings.LayerNorm.weight"]
    tensors["ln.bias"] = init["embeddings.LayerNorm.bias"]

    # the weight of each linear map: the MatMulInteger whose output names it
    weight_of: dict[str, str] = {}
    for n in nodes:
        if n["op"] == "MatMulInteger" and n["in"][1] in init:
            weight_of[n["out"][0]] = n["in"][1]
    for layer in range(layers):
        for m, path in LINEAR.items():
            out = f"/encoder/layer.{layer}/{path}/MatMul_output_0_output_quantized"
            w_name = weight_of[out]
            base = w_name.removesuffix("_quantized")
            w = init[w_name]
            assert w.dtype == np.int8 and w.ndim == 2, (w_name, w.dtype, w.shape)
            scale, zero = init[f"{base}_scale"], init[f"{base}_zero_point"]
            assert scale.shape == (w.shape[1],) and zero.shape == (w.shape[1],), (w_name, scale.shape)
            p = f"layer.{layer}.{m}"
            tensors[f"{p}.w"] = np.ascontiguousarray(w.T)
            tensors[f"{p}.scale"] = scale.astype("<f4")
            tensors[f"{p}.zero"] = zero.astype("i1")
            tensors[f"{p}.bias"] = init[f"encoder.layer.{layer}.{BIAS[m]}.bias"].astype("<f4")
        for ln, name in (("ln1", "attention.output.LayerNorm"), ("ln2", "output.LayerNorm")):
            tensors[f"layer.{layer}.{ln}.weight"] = init[f"encoder.layer.{layer}.{name}.weight"]
            tensors[f"layer.{layer}.{ln}.bias"] = init[f"encoder.layer.{layer}.{name}.bias"]
    assert len(weight_of) == 6 * layers, len(weight_of)

    dst.mkdir(parents=True, exist_ok=True)
    header, blobs, offset = {}, [], 0
    names = {"float32": "F32", "uint8": "U8", "int8": "I8"}
    for name in sorted(tensors):
        arr = np.ascontiguousarray(tensors[name])
        data = arr.tobytes()
        header[name] = {"dtype": names[arr.dtype.name], "shape": list(arr.shape),
                        "data_offsets": [offset, offset + len(data)]}
        blobs.append(data)
        offset += len(data)
    header["__metadata__"] = {"source": f"{hf.get('_name_or_path', src.name)} {src.name}/model.onnx",
                              "quantization": "ONNX Runtime dynamic int8: u8 activations per tensor, i8 weights per output"}
    h = json.dumps(header, separators=(",", ":")).encode()
    h += b" " * ((8 - len(h) % 8) % 8)
    with open(dst / "model.safetensors", "wb") as out:
        out.write(struct.pack("<Q", len(h)))
        out.write(h)
        for b in blobs:
            out.write(b)
    config = {
        "kind": "bert",
        "source": hf.get("_name_or_path", src.name),
        "hidden": hidden,
        "layers": layers,
        "heads": hf["num_attention_heads"],
        "intermediate": hf["intermediate_size"],
        "max_tokens": hf["max_position_embeddings"],
        "layer_norm_eps": hf["layer_norm_eps"],
        "query_prefix": "query: ",
        "passage_prefix": "passage: ",
        "pooling": "mean",
        "normalize": True,
    }
    (dst / "config.json").write_text(json.dumps(config, indent=2) + "\n", encoding="utf-8", newline="\n")
    shutil.copyfile(src / "tokenizer.json", dst / "tokenizer.json")
    size = (dst / "model.safetensors").stat().st_size
    print(f"{dst}: {len(tensors)} tensors, {size / 2**20:.1f} MiB")


if __name__ == "__main__":
    convert(pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]))
