# Developer-only tools (not shipped)

- `build_vectors.py`: builds `tests/vectors/` (the M1 sample file, one invalid
  file per error code, two reader edge cases and `expected.json`). It is the
  executable reference for every byte-level rule the spec leaves open
  (decisions D56-D66); the Rust builder must reproduce `minta.kapu` byte
  for byte (M3 check).
- `blake3_ref.py`: pure-Python BLAKE3 reference used by the script.
- `crosscheck.py`: builds a Markdown folder with the reference and compares it,
  section by section, with the Rust build.
- `quantize.py`: makes the int8 copy of the vector model the tools look for.
- `make_examples.py`: rebuilds `examples/kapu-docs.kapu` (this documentation)
  and `examples/kapu-spec.kapu` (the specification, via `split_spec.py`).
- `make_demo_svg.py`: writes `docs/assets/demo.svg`, the animated terminal in
  the README, from real `kapu map`, `search` and `open` output.
- `study/`: the measurement scripts of `bench/RESULTS.md` chapters 8 and 9;
  `study/results_page.py` writes `docs/results/index.html` from the results JSON.
- `mcp_probe.py`, `mcp_eval.py`, `kapu_mcp_call.py`: MCP latency, isolated
  agent runs and a logged one-call client for agents.

Setup (Windows, from the repository root):

```
py -3.13 -m venv tools/.venv
tools/.venv/Scripts/python -m pip install -r tools/requirements.txt
tools/.venv/Scripts/python tools/build_vectors.py
```

`tiktoken` downloads the `o200k_base` vocabulary on first use.
