# KAPU

**English** · [Magyar](#magyar)

Navigable single-file knowledge container for AI agents.

A `.kapu` file is one UTF-8 text file: a fixed 512-byte header, a map of "doors" with their token prices, long descriptions, a graph, a BM25 index, byte positions with BLAKE3 fingerprints, and the Markdown body itself. The body is the only source of truth; every other section is derived from it by the builder, byte for byte. A reader (the `kapu` CLI or the `kapu-mcp` MCP server) looks at the map, searches, and opens only the rooms a task needs, so an agent never has to read the whole file.

**Status:** v1 release candidate. Format, builder, reader, validator, hybrid search, CLI and MCP server are done (milestones M0–M7) and tested on Windows (the CI runs Linux and macOS on every push); the measurement milestone M8 is partly done and the real-corpus evaluation is open. See `ALLAPOT.md` for the current state and `AGGALYOK.md` for open questions.

Hungarian documents: the specification (`SPEC-KAPU.md` v1.0.1, the source of truth), the milestone plan (`TERV.md`), the status log (`ALLAPOT.md`), the decision log (`DONTESNAPLO.md`) and the open questions (`AGGALYOK.md`). The code, its comments and commit messages are in English.

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
scripts/fuzz.ps1         # cargo-fuzz, every target (Windows needs the VS C++ AddressSanitizer)
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
(`.github/workflows/release.yml`, as a draft release).

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
      "env": { "KAPU_MODEL_DIR": "C:\\path\\to\\models\\potion-multilingual-128M-i8" }
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

---

## Magyar

[English](#kapu) · **Magyar**

Navigálható, egyfájlos tudáskonténer AI ügynököknek.

Egy `.kapu` fájl egyetlen UTF-8 szövegfájl: egy rögzített, 512 bájtos fejléc, az „ajtók” térképe a tokenárukkal, hosszú leírások, egy gráf, egy BM25 index, bájtpozíciók BLAKE3 ujjlenyomattal, és maga a Markdown törzs. A törzs az egyetlen igazságforrás; minden más szakaszt a build állít elő belőle, bájtra azonosan. Az olvasó (a `kapu` parancssori eszköz vagy a `kapu-mcp` MCP szerver) megnézi a térképet, keres, és csak azokat a szobákat nyitja ki, amelyekre a feladathoz szükség van, így az ügynöknek soha nem kell a teljes fájlt beolvasnia.

**Állapot:** v1 kiadásjelölt. A formátum, a build, az olvasó, a validátor, a hibrid keresés, a parancssori eszköz és az MCP szerver kész (M0–M7 mérföldkő), Windowson tesztelve (a CI minden pushnál Linuxon és macOS-en is fut); a mérési mérföldkő (M8) részben kész, a valódi korpuszon végzett kiértékelés nyitott. A mostani állapot: `ALLAPOT.md`, a nyitott kérdések: `AGGALYOK.md`.

Magyar nyelvű dokumentumok: a specifikáció (`SPEC-KAPU.md` v1.0.1, az igazságforrás), a mérföldkő-terv (`TERV.md`), az állapotnapló (`ALLAPOT.md`), a döntésnapló (`DONTESNAPLO.md`) és a nyitott kérdések (`AGGALYOK.md`). A kód, a kommentek és a commit-üzenetek angolok.

### Felépítés

```
crates/kapu-core    könyvtár: olvasás, validálás, build, index, gráf, csoportok, keresés
crates/kapu-cli     `kapu` parancssori eszköz
crates/kapu-mcp     `kapu-mcp` MCP szerver (stdio)
crates/kapu-bench   szintetikus korpuszgenerátor és mérőeszközök
tests/vectors       mintafájl, hibakódonként egy hibás fájl, olvasói határesetek (M1)
tests/model         apró, commitolt tesztmodell és model2vec referenciavektorok
bench/              REPORT.md a mérésekkel (M3–M8)
fuzz/               cargo-fuzz célpontok (M2)
tools/              csak fejlesztői Python-eszközök: bájtszintű referencia-build,
                    kvantáló, keresztellenőrzések, harmadik féltől származó komponensek listája
scripts/            ellenőrző, fuzz- és csomagoló szkriptek
models/             vektormodellek (nincsenek gitben; lásd „Vektormodell”)
Archive/            v0.1 tervezetek, referenciának
```

### Fordítás és ellenőrzés

Rust kell hozzá; a `rustup` a `rust-toolchain.toml`-ban rögzített verziót magától telepíti.

```
cargo build --workspace
scripts/check.ps1        # Windows: fmt, clippy -D warnings, test, deny, audit
scripts/check.sh         # Linux / macOS
scripts/fuzz.ps1         # cargo-fuzz minden célpontra (Windowson a VS C++ AddressSanitizer kell)
```

Az ellenőrző szkriptekhez kell a `cargo-deny` és a `cargo-audit` (`cargo install --locked cargo-deny cargo-audit`).

### Vektormodell

A jelentés szerinti kereséshez és a gráf `sim` éleihez egy statikus beágyazómodell kell a binárisok mellett, a `models/<név>/` mappában (vagy a `KAPU_MODEL_DIR`-ben megadott helyen). A csomagolt modell a [`minishlab/potion-multilingual-128M`](https://huggingface.co/minishlab/potion-multilingual-128M) int8 változata (MIT licenc, kb. 124 MB):

```
# letöltés a models/potion-multilingual-128M mappába (config.json, README.md, tokenizer.json, model.safetensors)
tools/.venv/Scripts/python tools/quantize.py models/potion-multilingual-128M
```

Modell nélkül minden működik, kivéve a vektoros keresést (`vector: unavailable`) és a `sim` éleket; a fájl fejlécében ilyenkor `emb=none` áll.

### Hordozható csomag

A `scripts/package.ps1` elkészíti a `dist/kapu-<verzió>-windows-x64.zip` csomagot: `kapu.exe`, `kapu-mcp.exe`, a modellmappa, a licencek (`LICENSE`, `LICENSE-SPEC.txt`, `THIRD-PARTY.md`), ez a README és a specifikáció. Telepíteni nem kell; kibontás után futtatható. Verziócímkére (`v*`) a CI ugyanezt Linuxra és macOS-re is elkészíti (`.github/workflows/release.yml`, piszkozat kiadásként).

### Használat Claude Code-ból vagy Claude Desktopból (MCP)

A `kapu-mcp` egy stdio MCP szerver öt eszközzel: `kapu_map`, `kapu_search`, `kapu_describe`, `kapu_open`, `kapu_write`. Fordítsd le a `cargo build --release` paranccsal, a vektormodell legyen mellette (`models/potion-multilingual-128M-i8/`, vagy állítsd be a `KAPU_MODEL_DIR`-t a modellmappára), aztán regisztráld.

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
      "env": { "KAPU_MODEL_DIR": "C:\\path\\to\\models\\potion-multilingual-128M-i8" }
    }
  }
}
```

Ezután elég egy ilyen kérés: „nyisd meg a skill.kapu-t, és ehhez a weboldalhoz használd az ott lévő frontend és design skilleket”. Az ügynök elolvassa a térképet, keres, és csak a szükséges szobákat nyitja ki. Ha biztosra akarsz menni, hogy az ügynök az eszközöket használja és nem közvetlenül olvassa a fájlt, tartsd a `.kapu` fájlt a munkakönyvtáron kívül (D51).

### Parancssor

```
kapu new skill.kapu
kapu import ./skills skill.kapu        # minden .md fájl egy szoba
kapu map skill.kapu
kapu search skill.kapu "üvegszerű gomb" --k 5
kapu open skill.kapu frontend/glass-gomb
kapu validate skill.kapu --full
```

Minden parancs elfogadja a `--json` kapcsolót. Részletek: `kapu --help`.

### Licenc

Kód: MIT. Specifikáció: CC BY 4.0. Szerző és ötletgazda: Pintér Szabolcs.
