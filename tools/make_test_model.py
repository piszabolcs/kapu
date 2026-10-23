"""Tiny deterministic Model2Vec-style model for tests and CI (M4, NFR-02).

Writes tests/model/kapu-test-tiny/: a WordLevel tokenizer.json over a small
Hungarian/English vocabulary, model.safetensors with a 16-dim `embeddings`
tensor (fixed pseudo-random values from a seeded generator) and config.json.
The files are committed; rerunning the script must give the same bytes.

    tools/.venv/Scripts/python tools/make_test_model.py
"""
import json
import pathlib
import struct

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "model" / "kapu-test-tiny"
DIM = 16

WORDS = sorted(set("""
a az és gomb gombok szín színek sötét téma üvegszerű elmosott háttér hiba megoldás függvény szoba ajtó
térkép keresés tartalom minta szabály lásd mindig soha csak például fontos gyors lassú kicsi nagy türkiz
bronz glass button color colors dark theme blurred background error fix function room door map search
content pattern rule see always never only example important fast slow small large and the of to in
electron preload bridge ipc renderer context isolation css html rust python sql cli mcp design frontend
backend server client cache queue stream parser header footer modal toast layout grid token palette
router schema worker build index graph group vector model test data file text word line code block
""".split()))


def lcg(seed: int):
    """64-bit LCG (Knuth MMIX constants); platform independent."""
    state = seed
    while True:
        state = (state * 6364136223846793005 + 1442695040888963407) % 2**64
        yield state


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    vocab = {"[UNK]": 0}
    for w in WORDS:
        vocab[w] = len(vocab)
    tokenizer = {
        "version": "1.0",
        "truncation": None,
        "padding": None,
        "added_tokens": [{"id": 0, "content": "[UNK]", "single_word": False, "lstrip": False, "rstrip": False, "normalized": False, "special": True}],
        "normalizer": {"type": "Lowercase"},
        "pre_tokenizer": {"type": "Whitespace"},
        "post_processor": None,
        "decoder": None,
        "model": {"type": "WordLevel", "vocab": vocab, "unk_token": "[UNK]"},
    }
    (OUT / "tokenizer.json").write_text(json.dumps(tokenizer, ensure_ascii=False, indent=1) + "\n", encoding="utf-8", newline="\n")

    gen = lcg(42)
    values = []
    for _ in range(len(vocab) * DIM):
        # uniform in [-1, 1), exactly representable in f32 (multiples of 2^-16)
        values.append(((next(gen) >> 48) - 32768) / 32768.0)
    data = struct.pack(f"<{len(values)}f", *values)
    header = {"embeddings": {"dtype": "F32", "shape": [len(vocab), DIM], "data_offsets": [0, len(data)]}}
    hbytes = json.dumps(header, separators=(",", ":")).encode()
    hbytes += b" " * ((8 - len(hbytes) % 8) % 8)
    (OUT / "model.safetensors").write_bytes(struct.pack("<Q", len(hbytes)) + hbytes + data)
    (OUT / "config.json").write_text(json.dumps({"model_type": "model2vec", "hidden_dim": DIM, "normalize": True}, indent=1) + "\n", encoding="utf-8", newline="\n")
    print(f"vocab {len(vocab)}, dim {DIM}, {len(data)} bytes of vectors -> {OUT}")


if __name__ == "__main__":
    main()
