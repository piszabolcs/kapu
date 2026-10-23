"""Reference vectors from Python model2vec for the Rust model loader (M4).

Developer tool. Writes tests/model/potion_reference.json: for a few texts the
token ids and the 256-dim vector that `model2vec` 0.10 computes. The Rust test
`crates/kapu-core/tests/model.rs` compares against it when the model is present.

    tools/.venv/Scripts/python tools/model_reference.py
"""
import json
import pathlib

from model2vec import StaticModel

ROOT = pathlib.Path(__file__).resolve().parent.parent
MODEL = ROOT / "models" / "potion-multilingual-128M"
TEXTS = [
    "Az üvegszerű gomb elmosott hátteret kap, a színe türkiz.",
    "The glass button gets a blurred background: backdrop-filter: blur(12px).",
    "contextBridge.exposeInMainWorld('api', { ping: () => ipcRenderer.invoke('ping') });",
    "",
    "Hosszú szöveg a tokenkorláthoz. " * 300,
    "\u2603 \u0f00 \U0001F600 ◆◆◆",
]

model = StaticModel.from_pretrained(str(MODEL))
out = []
for t in TEXTS:
    ids = model.tokenize([t[: 512 * model.median_token_length]])[0][:512]
    vec = model.encode(t)
    out.append({"text": t, "ids": [int(i) for i in ids], "vector": [float(x) for x in vec]})
(ROOT / "tests" / "model").mkdir(parents=True, exist_ok=True)
(ROOT / "tests" / "model" / "potion_reference.json").write_text(
    json.dumps({"median_token_length": model.median_token_length, "cases": out}, ensure_ascii=False), encoding="utf-8"
)
print("median token length", model.median_token_length, "cases", len(out), [len(c["ids"]) for c in out])
