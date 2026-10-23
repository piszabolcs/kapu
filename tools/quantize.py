"""i8 quantization of a Model2Vec model for the portable package (D28, D53, A17).

    tools/.venv/Scripts/python tools/quantize.py models/potion-multilingual-128M

Writes `<model>-i8/`: `model.safetensors` with `embeddings` as I8 (`vocab × dim`)
and `scales` as F32 (`vocab`), row value = i8 × scale, scale = max |row| / 127;
`tokenizer.json`, `config.json` and the README (licence) are copied unchanged.
The Rust loader (`crates/kapu-core/src/embed.rs`) reads both forms. The model
name, and so `emb=` in built files, is the folder name.
"""
import json
import pathlib
import shutil
import struct
import sys

import numpy as np


def read_embeddings(path: pathlib.Path) -> np.ndarray:
    raw = path.read_bytes()
    n = struct.unpack("<Q", raw[:8])[0]
    header = json.loads(raw[8 : 8 + n])
    t = header["embeddings"]
    assert t["dtype"] == "F32", t["dtype"]
    start, end = t["data_offsets"]
    rows, dim = t["shape"]
    return np.frombuffer(raw[8 + n + start : 8 + n + end], dtype="<f4").reshape(rows, dim)


def main() -> None:
    src = pathlib.Path(sys.argv[1])
    dst = src.with_name(src.name + "-i8")
    dst.mkdir(parents=True, exist_ok=True)
    emb = read_embeddings(src / "model.safetensors").astype(np.float64)
    scales = np.abs(emb).max(axis=1) / 127.0
    scales[scales == 0] = 1.0
    q = np.clip(np.rint(emb / scales[:, None]), -127, 127).astype(np.int8)
    scales32 = scales.astype("<f4")
    q_bytes, s_bytes = q.tobytes(), scales32.tobytes()
    header = {
        "embeddings": {"dtype": "I8", "shape": list(q.shape), "data_offsets": [0, len(q_bytes)]},
        "scales": {"dtype": "F32", "shape": [len(scales32)], "data_offsets": [len(q_bytes), len(q_bytes) + len(s_bytes)]},
        "__metadata__": {"quantization": "per-row symmetric int8, scale = max|row| / 127", "source": src.name},
    }
    h = json.dumps(header, separators=(",", ":")).encode()
    h += b" " * ((8 - len(h) % 8) % 8)
    (dst / "model.safetensors").write_bytes(struct.pack("<Q", len(h)) + h + q_bytes + s_bytes)
    for name in ("tokenizer.json", "config.json", "README.md"):
        if (src / name).exists():
            shutil.copyfile(src / name, dst / name)
    # quality: cosine of every row with its dequantized version
    deq = q.astype(np.float64) * scales[:, None]
    norms = np.linalg.norm(emb, axis=1) * np.linalg.norm(deq, axis=1)
    ok = norms > 0
    cos = (emb[ok] * deq[ok]).sum(axis=1) / norms[ok]
    size = (dst / "model.safetensors").stat().st_size
    print(f"{dst}: {size / 2**20:.1f} MiB; row cosine min {cos.min():.6f}, mean {cos.mean():.6f}")


if __name__ == "__main__":
    main()
