# Developer-only tools (not shipped)

- `build_vectors.py`: builds `tests/vectors/` (the M1 sample file, one invalid
  file per error code, two reader edge cases and `expected.json`). It is the
  executable reference for every byte-level rule the spec leaves open
  (DONTESNAPLO.md D56-D66); the Rust builder must reproduce `minta.kapu` byte
  for byte (M3 check).
- `blake3_ref.py`: pure-Python BLAKE3 reference used by the script.
- `distill/`: Python distillation of the vector model (M8/5), arrives later.

Setup (Windows, from the repository root):

```
py -3.13 -m venv tools/.venv
tools/.venv/Scripts/python -m pip install -r tools/requirements.txt
tools/.venv/Scripts/python tools/build_vectors.py
```

`tiktoken` downloads the `o200k_base` vocabulary on first use.
