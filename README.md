# KAPU

Navigable single-file knowledge container for AI agents.

A `.kapu` file is one UTF-8 text file: a fixed 512-byte header, a map of "doors" with their token prices, long descriptions, a graph, a BM25 index, byte positions with BLAKE3 fingerprints, and the Markdown body itself. The body is the only source of truth; every other section is derived from it by the builder, byte for byte. A reader (the `kapu` CLI or the `kapu-mcp` MCP server) looks at the map, searches, and opens only the rooms a task needs, so an agent never has to read the whole file.

**Status:** v1 release candidate. Format, builder, reader, validator, hybrid search, CLI and MCP server are done (milestones M0–M7) and tested on Windows (the CI runs Linux and macOS on every push); the measurement milestone M8 is partly done and the real-corpus evaluation is open. See `ALLAPOT.md` for the current state and `AGGALYOK.md` for open questions.

Hungarian documents: the specification (`SPEC-KAPU.md`, the source of truth), the milestone plan (`TERV.md`), the status log (`ALLAPOT.md`) and the decision log (`DONTESNAPLO.md`). The code, its comments and commit messages are in English.

## Layout

```
crates/kapu-core    library: read, validate, build, index, graph, groups, search
crates/kapu-cli     `kapu` command-line tool
crates/kapu-mcp     `kapu-mcp` MCP server (stdio)
crates/kapu-bench   synthetic corpus generator and measurement helpers
tests/vectors       sample file, one invalid file per error code, reader edge cases (M1)
tests/model         tiny committed test model and model2vec reference vectors
bench/              REPORT.md with the measurements (M3-M8)
fuzz/               cargo-fuzz targets (M2)
tools/              developer-only Python tools: byte-level reference builder,
                    quantizer, cross-checks, third-party list
scripts/            check, fuzz and package scripts
models/             vector models (not in git; see "Vector model")
Archive/            v0.1 drafts, kept for reference
```

## Build and check

Requires Rust; `rustup` installs the pinned toolchain from `rust-toolchain.toml` automatically.

```
cargo build --workspace
scripts/check.ps1        # Windows: fmt, clippy -D warnings, test, deny, audit
scripts/check.sh         # Linux / macOS
```

The check scripts need `cargo-deny` and `cargo-audit` (`cargo install --locked cargo-deny cargo-audit`).

## Vector model

The search by meaning and the `sim` edges of the graph need a static
embedding model next to the binaries, in `models/<name>/` (or `KAPU_MODEL_DIR`).
The packaged model is the int8 form of
[`minishlab/potion-multilingual-128M`](https://huggingface.co/minishlab/potion-multilingual-128M)
(MIT, about 124 MB):

```
# download into models/potion-multilingual-128M (config.json, README.md, tokenizer.json, model.safetensors)
tools/.venv/Scripts/python tools/quantize.py models/potion-multilingual-128M
```

Without a model everything works except vector search (`vector: unavailable`)
and `sim` edges; files then say `emb=none`.

## Portable package

`scripts/package.ps1` builds `dist/kapu-<version>-windows-x64.zip`: `kapu.exe`,
`kapu-mcp.exe`, the model folder, licences (`LICENSE`, `LICENSE-SPEC.txt`,
`THIRD-PARTY.md`), this README and the specification. Nothing to install; unzip
and run. A version tag (`v*`) builds the same for Linux and macOS in CI
(`.github/workflows/release.yml`).

## Use from Claude Code or Claude Desktop (MCP)

`kapu-mcp` is a stdio MCP server with five tools: `kapu_map`, `kapu_search`, `kapu_describe`, `kapu_open`, `kapu_write`. Build it with `cargo build --release`, keep the vector model next to it (`models/potion-multilingual-128M-i8/`, or point `KAPU_MODEL_DIR` at the model folder), then register it.

Claude Code:

```
claude mcp add kapu -- /path/to/kapu-mcp
```

Claude Desktop (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "kapu": {
      "command": "C:\\path\\to\\kapu-mcp.exe",
      "env": { "KAPU_MODEL_DIR": "C:\\path\\to\\models\\potion-multilingual-128M" }
    }
  }
}
```

Then a prompt like "open skill.kapu and use its frontend and design skills for this page" is enough: the agent reads the map, searches and opens only the rooms it needs. Keep the `.kapu` file outside the working directory if you want to be sure the agent uses the tools instead of reading the file directly (D51).

## Command line

```
kapu new skill.kapu
kapu import ./skills skill.kapu        # one room per .md file
kapu map skill.kapu
kapu search skill.kapu "üvegszerű gomb" --k 5
kapu open skill.kapu frontend/glass-gomb
kapu validate skill.kapu --full
```

Every command accepts `--json`. See `kapu --help`.

## Licence

Code: MIT. Specification: CC BY 4.0. Author and originator: Pintér Szabolcs.
