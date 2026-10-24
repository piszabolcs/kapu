# Fuzz targets (NFR-03/5)

`cargo-fuzz` (libFuzzer) targets for every parser of `kapu-core`. The goal is
that no input, however broken, makes the library panic, overflow or read out
of bounds: every input must end in `Ok` or an `Err` with an FR-20 code.

| Target | What it feeds |
|---|---|
| `header` | any bytes into `Header::parse` |
| `sections` | whole file, strict and tolerant, tight limits, door reads |
| `docs` | `#DOCS` lines, unescape, comment stripping, served text |
| `catalog` | `#MAP`, `#DESC`, `#GRAPH` line parsers |
| `validate` | whole file, basic and full validation |
| `reader` | whole file, open every door plus unknown ids, describe, frame |

Run (nightly is needed only here; the seed corpus is the M1 test vectors):

```
mkdir fuzz/corpus/<target>; cp tests/vectors/*.kapu fuzz/corpus/<target>/
cargo +nightly fuzz run <target> fuzz/corpus/<target> -- -max_total_time=3600
```

Release gate: one hour per target without a finding.
`fuzz/corpus` and `fuzz/artifacts` are git-ignored.
