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
- `make_social.py`: writes `docs/assets/social-preview.png`, the repository's
  link preview image (Pillow, `py -3.12` on the build machine).
- `make_demo_svg.py`: writes `docs/assets/demo.svg`, the animated terminal in
  the README, from real `kapu map`, `search` and `open` output.
- `study/`: the measurement scripts of `bench/RESULTS.md` chapters 8 to 11;
  `study/skill_pool.py` and `study/skill_eval.py` (with the tasks in `study/skill_tasks.json`) measure skill finding;
  `study/results_page.py` writes `docs/results/index.html` from the results JSON files (the pilot, format 1.2, skill finding).
  `study/results_pdf.py` prints that page to `docs/results/kapu-measurements-{en,hu}.pdf` with a headless Edge or Chrome.
  Plan v2 (bench/RESULTS.md chapter 12): `study/split_questions.py`, `study/ablation.py`, `study/ask_eval.py`,
  `study/title_weight.py`, `study/neighbour_follow.py`, `study/group_quality.py`, `study/oracle_eval.py`,
  `study/collect_v2.py` and `study/v2_tables.py`.
- `mcp_probe.py`, `mcp_eval.py`, `kapu_mcp_call.py`: MCP latency, isolated
  agent runs and a logged one-call client for agents.

Setup (Windows, from the repository root):

```
py -3.13 -m venv tools/.venv
tools/.venv/Scripts/python -m pip install -r tools/requirements.txt
tools/.venv/Scripts/python tools/build_vectors.py
```

`tiktoken` downloads the `o200k_base` vocabulary on first use.
