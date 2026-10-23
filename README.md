<div align="center">

<img src="docs/assets/logo.png" alt="KAPU logo" width="120" />

# KAPU

**A knowledge file AI agents open door by door, paying tokens only for what they read**

![Rust](https://img.shields.io/badge/Rust-1.99-B7410E?style=flat-square&logo=rust&logoColor=white)
![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?style=flat-square&logo=tauri&logoColor=white)
![MCP](https://img.shields.io/badge/MCP-stdio%20server-1F2328?style=flat-square)
![Status](https://img.shields.io/badge/status-active-2EA44F?style=flat-square)
![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)

[Features](#features) · [Screenshots](#screenshots) · [Architecture](#architecture) · [Installation](#installation) · [Magyar](#magyar)

</div>

<p align="center">
  <img src="docs/screenshots/01-main.png" alt="KAPU window: the map with token prices on the left, an open room in the middle, its tags, neighbours and fingerprint on the right" width="900" />
</p>

## The problem

AI agents get their working knowledge as Markdown: a library of skills, a specification, project notes. Handing over the whole folder costs tokens on every task and pushes the useful part out of the context window. Splitting it into many small files loses the overview, and the agent has to guess which file to read.

## The solution

KAPU packs the Markdown into one text file with a map of "doors" (rooms and their sections), each with its token price, plus a search index, a graph of related doors and fingerprints. Through the MCP server an agent looks at the map, searches, and opens only the rooms the task needs. The Markdown body stays the single source of truth; every other section is rebuilt from it byte for byte. On the demo file (the KAPU specification itself, 12,956 tokens of Markdown) the first map call costs 251 tokens and one group's door list 901, while the whole `.kapu` file, which an agent never needs to read, is 120,451 tokens.

## Features

- **Map with prices.** Every room and sub-section shows what it costs to open, grouped by topic (Leiden clusters, named by their keywords).
- **Hybrid search.** BM25 with Hungarian and English stemming plus a multilingual static embedding model, merged with reciprocal rank fusion; a token budget keeps only what fits.
- **MCP server.** Five tools (`kapu_map`, `kapu_search`, `kapu_describe`, `kapu_open`, `kapu_write`) for Claude Code, Claude Desktop or any MCP client, over stdio.
- **Desktop window.** Read a file the way an agent does: map, rooms, search, graph and checks, in a native window.
- **Same bytes everywhere.** The build is deterministic on Windows, Linux and macOS; a full check rebuilds the file and compares.
- **Integrity checks.** BLAKE3 fingerprints for every door and for the body, 12 error codes, a fuzzed reader.
- **Content scan.** Writing a room warns about instruction-like text and secrets before it lands in the file.
- **Offline and portable.** One folder with the binaries and the 124 MB model; nothing to install, no network.

## Screenshots

<table>
  <tr>
    <td align="center" width="50%"><img src="docs/screenshots/02-search.png" alt="Search view: a Hungarian question, ten doors ranked by words and meaning, with token prices and a running total" /><br/><sub>Hybrid search: words and meaning, with a running token total</sub></td>
    <td align="center" width="50%"><img src="docs/screenshots/03-graph.png" alt="Graph view: rooms as dots coloured by group, edges for shared terms and similar meaning" /><br/><sub>Graph of the doors, coloured by group</sub></td>
  </tr>
  <tr>
    <td align="center" width="50%"><img src="docs/screenshots/04-check.png" alt="Check view: a full check that rebuilt the file and found the same bytes" /><br/><sub>Full check: fingerprints and a byte-for-byte rebuild</sub></td>
    <td align="center" width="50%"><img src="docs/screenshots/05-about.png" alt="App info card opened from the psz signature in the status bar" /><br/><sub>App info card behind the signature</sub></td>
  </tr>
  <tr>
    <td align="center" width="50%"><img src="docs/screenshots/06-light.png" alt="The room view in the light theme" /><br/><sub>Light theme, following the system</sub></td>
    <td align="center" width="50%"><img src="docs/screenshots/07-cli.png" alt="Terminal: kapu map, search and open on the demo file" /><br/><sub>The same file from the command line</sub></td>
  </tr>
</table>

## Architecture

```mermaid
flowchart LR
  subgraph Input
    MD["Markdown files"]
    MOD["potion-multilingual-128M (int8)"]
  end
  subgraph Core["kapu-core"]
    B["Builder: index, prices, vectors, graph, groups"]
    F[".kapu file"]
    R["Reader and validator"]
    S["Hybrid search (BM25 + vectors, RRF)"]
  end
  subgraph Tools
    CLI["kapu (CLI)"]
    MCP["kapu-mcp (stdio)"]
    GUI["kapu-gui (Tauri)"]
  end
  AG["AI agent"]
  MD --> B --> F --> R --> S
  MOD --> B
  MOD --> S
  R --> CLI & MCP & GUI
  S --> CLI & MCP & GUI
  MCP --> AG
```

`kapu-core` is the whole format: it builds a `.kapu` file from Markdown and reads, checks and searches it. The three tools are thin layers over it: the command line for people and scripts, the MCP server for agents, the window for reading and checking a file by eye. The vector model is optional; without it everything works except search by meaning and the `sim` edges.

## Tech stack

| Layer | Technology |
|---|---|
| Language | Rust 1.99 (edition 2024), no `unsafe` |
| File format | UTF-8 text, 512-byte header, BLAKE3 fingerprints, token prices with `tiktoken-rs` (o200k_base) |
| Search | BM25 with Snowball stemming (`rust-stemmers`), Model2Vec static embeddings (`tokenizers`, safetensors), RRF |
| Graph and groups | `link`, `term` and `sim` edges, own Leiden implementation (checked against `leidenalg`), YAKE group names |
| Agent interface | MCP over stdio with `rmcp` 3.7 |
| Command line | `clap` 4 |
| Desktop window | Tauri 2, plain HTML, CSS and JavaScript, Geist fonts, `pulldown-cmark` |
| Quality | clippy pedantic, cargo-deny, cargo-audit, cargo-fuzz, CI on Windows, Linux and macOS, byte-level Python reference builder |

## Installation

1. Install Rust with [rustup](https://rustup.rs); it picks up the pinned toolchain from `rust-toolchain.toml`. On Linux the window also needs WebKitGTK: `sudo apt install libwebkit2gtk-4.1-dev libxdo-dev libssl-dev librsvg2-dev`.
2. Build everything:
   ```bash
   git clone https://github.com/piszabolcs/kapu.git
   cd kapu
   cargo build --release
   ```
3. Optional, for search by meaning: download [`minishlab/potion-multilingual-128M`](https://huggingface.co/minishlab/potion-multilingual-128M) (`config.json`, `README.md`, `tokenizer.json`, `model.safetensors`) into `models/potion-multilingual-128M`, then make the int8 copy the tools look for:
   ```bash
   python tools/quantize.py models/potion-multilingual-128M
   ```
4. Windows portable package (binaries, model, demo file, licences): `powershell -ExecutionPolicy Bypass -File scripts/package.ps1` writes `dist/kapu-0.1.0-windows-x64.zip`.

## Usage

Open the demo file in the window:

```bash
target/release/kapu-gui examples/kapu-spec.kapu
```

The same from the command line:

```bash
kapu map examples/kapu-spec.kapu
kapu search examples/kapu-spec.kapu "how does an agent open a room" --k 5
kapu open examples/kapu-spec.kapu fr-15-olvasas-es-mcp-eszkozok-core-flow
kapu validate examples/kapu-spec.kapu --full
```

Make your own from a folder of Markdown files (one room per file, `##` headings become sub-doors):

```bash
kapu import ./skills skill.kapu
```

Give it to Claude Code or Claude Desktop through MCP:

```bash
claude mcp add kapu -- /path/to/kapu-mcp
```

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

Then ask, for example, "open skill.kapu and use its frontend skills for this page". Keep the `.kapu` file outside the working directory if you want the agent to use the tools instead of reading the file directly. Every CLI command accepts `--json`; see `kapu --help`.

## Project structure

```text
kapu/
├── crates/
│   ├── kapu-core/    format, builder, reader, validator, search, graph, groups
│   ├── kapu-cli/     `kapu` command line tool
│   ├── kapu-mcp/     `kapu-mcp` MCP server (stdio)
│   ├── kapu-gui/     `kapu-gui` desktop window (Tauri, ui/ is the interface)
│   └── kapu-bench/   corpus generator and measurements
├── examples/         kapu-spec.kapu, the specification as a KAPU file
├── tests/            test vectors (one file per error code) and the test model
├── bench/            REPORT.md with the measurements
├── fuzz/             cargo-fuzz targets
├── tools/            Python reference builder, quantizer, icon and demo scripts
├── scripts/          check, fuzz and package scripts
└── docs/             logo and screenshots
```

The planning documents are in Hungarian: `SPEC-KAPU.md` (specification, the source of truth), `TERV.md` (milestones), `ALLAPOT.md` (status log), `DONTESNAPLO.md` (decision log), `AGGALYOK.md` (open questions).

## Roadmap

- [ ] Evaluate retrieval on a real corpus and choose the final model (recall@5, MRR@10, a distilled model)
- [ ] End-to-end run with Claude Code on a clean machine
- [ ] One-hour fuzz run per target before the first tagged release
- [ ] Editing in the window (new room, import) and a Hungarian interface
- [ ] Release packages for Linux and macOS from the tag workflow

## License

Code: MIT, see [LICENSE](LICENSE). Specification (`SPEC-KAPU.md`): CC BY 4.0. Fonts in `crates/kapu-gui/ui/fonts`: SIL Open Font License 1.1. Third-party crates and the model: [THIRD-PARTY.md](THIRD-PARTY.md).

---

## Magyar

[English](#kapu) · **Magyar**

**Tudásfájl, amelyet az AI ügynök ajtónként nyit ki, és csak azért fizet tokenben, amit elolvas**

### A probléma

Az AI ügynökök a munkához szükséges tudást Markdownban kapják: skill-gyűjteményt, specifikációt, projektjegyzeteket. Ha az egész mappát odaadjuk, minden feladatnál fizetünk érte tokenben, és a hasznos rész kiszorul a kontextusablakból. Ha sok kis fájlra bontjuk, elvész az áttekintés, és az ügynöknek találgatnia kell, melyiket olvassa el.

### A megoldás

A KAPU a Markdownt egyetlen szövegfájlba csomagolja. Ebben van az „ajtók” (szobák és alfejezeteik) térképe a tokenárukkal, egy keresőindex, a rokon ajtók gráfja és ujjlenyomatok. Az ügynök az MCP szerveren át megnézi a térképet, keres, és csak azokat a szobákat nyitja ki, amelyek a feladathoz kellenek. A Markdown törzs az egyetlen igazságforrás, minden más szakasz bájtra azonosan újraépül belőle. A bemutatófájlon (maga a KAPU specifikáció, 12 956 token Markdown) az első térképhívás 251 tokenbe kerül, egy csoport ajtólistája 901-be, a teljes `.kapu` fájl pedig, amelyet az ügynöknek soha nem kell elolvasnia, 120 451 token.

### Funkciók

- **Térkép árakkal.** Minden szoba és alfejezet mutatja, mennyibe kerül kinyitni, témák szerint csoportosítva (Leiden-klaszterek, a kulcsszavaikkal elnevezve).
- **Hibrid keresés.** BM25 magyar és angol szótövezéssel, plusz többnyelvű statikus beágyazómodell, a két rangsor RRF-fel egyesítve; a tokenkeret csak azt tartja meg, ami belefér.
- **MCP szerver.** Öt eszköz (`kapu_map`, `kapu_search`, `kapu_describe`, `kapu_open`, `kapu_write`) Claude Code-hoz, Claude Desktophoz vagy bármely MCP kliensnek, stdio-n.
- **Asztali ablak.** A fájl úgy olvasható, ahogy az ügynök látja: térkép, szobák, keresés, gráf és ellenőrzés, natív ablakban.
- **Mindenhol ugyanazok a bájtok.** A build determinisztikus Windowson, Linuxon és macOS-en; a teljes ellenőrzés újraépíti a fájlt és összeveti.
- **Épségellenőrzés.** BLAKE3 ujjlenyomat minden ajtóra és a törzsre, 12 hibakód, fuzzolt olvasó.
- **Tartalomvizsgálat.** Szoba írásakor figyelmeztet az utasításszerű szövegre és a titkokra, mielőtt bekerülnének a fájlba.
- **Offline és hordozható.** Egy mappa a binárisokkal és a 124 MB-os modellel; nincs telepítés, nincs hálózat.

### Képernyőképek

A képek a fenti angol részben vannak: [Screenshots](#screenshots). Sorrendben: a szoba nézet, a hibrid keresés, a gráf, a teljes ellenőrzés, az aláírás mögötti app-info kártya, a világos téma és a parancssor.

### Architektúra

A diagram a fenti angol részben van: [Architecture](#architecture). A `kapu-core` maga a formátum: Markdownból `.kapu` fájlt épít, és olvassa, ellenőrzi, keresi. A három eszköz vékony réteg fölötte: a parancssor embereknek és szkripteknek, az MCP szerver ügynököknek, az ablak a fájl szemmel való átnézéséhez. A vektormodell nem kötelező; nélküle minden működik, kivéve a jelentés szerinti keresést és a `sim` éleket.

### Technológiák

| Réteg | Technológia |
|---|---|
| Nyelv | Rust 1.99 (2024-es kiadás), `unsafe` nélkül |
| Fájlformátum | UTF-8 szöveg, 512 bájtos fejléc, BLAKE3 ujjlenyomatok, tokenárak `tiktoken-rs`-sel (o200k_base) |
| Keresés | BM25 Snowball szótövezéssel (`rust-stemmers`), Model2Vec statikus beágyazás (`tokenizers`, safetensors), RRF |
| Gráf és csoportok | `link`, `term` és `sim` élek, saját Leiden-megvalósítás (a `leidenalg`-gal összevetve), YAKE csoportnevek |
| Ügynök-felület | MCP stdio-n, `rmcp` 3.7 |
| Parancssor | `clap` 4 |
| Asztali ablak | Tauri 2, sima HTML, CSS és JavaScript, Geist betűk, `pulldown-cmark` |
| Minőség | clippy pedantic, cargo-deny, cargo-audit, cargo-fuzz, CI Windowson, Linuxon és macOS-en, bájtszintű Python referencia-build |

### Telepítés

1. Telepítsd a Rustot a [rustup](https://rustup.rs) segítségével; a `rust-toolchain.toml`-ban rögzített verziót magától hozza. Linuxon az ablakhoz WebKitGTK is kell: `sudo apt install libwebkit2gtk-4.1-dev libxdo-dev libssl-dev librsvg2-dev`.
2. Fordítás: `git clone https://github.com/piszabolcs/kapu.git`, `cd kapu`, `cargo build --release`.
3. Nem kötelező, a jelentés szerinti kereséshez: töltsd le a [`minishlab/potion-multilingual-128M`](https://huggingface.co/minishlab/potion-multilingual-128M) modellt (`config.json`, `README.md`, `tokenizer.json`, `model.safetensors`) a `models/potion-multilingual-128M` mappába, majd készítsd el az int8 másolatot: `python tools/quantize.py models/potion-multilingual-128M`.
4. Hordozható Windows-csomag (binárisok, modell, bemutatófájl, licencek): `powershell -ExecutionPolicy Bypass -File scripts/package.ps1`, az eredmény `dist/kapu-0.1.0-windows-x64.zip`.

### Használat

- Ablak a bemutatófájllal: `target/release/kapu-gui examples/kapu-spec.kapu`.
- Parancssor: `kapu map`, `kapu search`, `kapu open`, `kapu validate --full`; minden parancs elfogadja a `--json` kapcsolót, részletek: `kapu --help`.
- Saját fájl egy Markdown-mappából (minden fájl egy szoba, a `##` fejezetek alajtók): `kapu import ./skills skill.kapu`.
- Claude Code-hoz: `claude mcp add kapu -- /path/to/kapu-mcp`; Claude Desktophoz a fenti `claude_desktop_config.json` részlet.
- Ezután elég egy ilyen kérés: „nyisd meg a skill.kapu-t, és ehhez a weboldalhoz használd az ott lévő frontend skilleket”. Ha biztosra akarsz menni, hogy az ügynök az eszközöket használja, tartsd a `.kapu` fájlt a munkakönyvtáron kívül.

### Mappaszerkezet

A fa a fenti angol részben van: [Project structure](#project-structure). A tervezési dokumentumok magyarul: `SPEC-KAPU.md` (specifikáció, az igazságforrás), `TERV.md` (mérföldkövek), `ALLAPOT.md` (állapotnapló), `DONTESNAPLO.md` (döntésnapló), `AGGALYOK.md` (nyitott kérdések).

### Tervek

- [ ] Visszakeresési mérés valódi korpuszon és a végleges modell kiválasztása (recall@5, MRR@10, desztillált modell)
- [ ] Végponttól végpontig próba Claude Code-dal, tiszta gépen
- [ ] Egyórás fuzz-futás célpontonként az első címkézett kiadás előtt
- [ ] Szerkesztés az ablakban (új szoba, import) és magyar felület
- [ ] Linuxos és macOS-es kiadási csomagok a címkés workflow-ból

### Licenc

Kód: MIT, lásd [LICENSE](LICENSE). Specifikáció (`SPEC-KAPU.md`): CC BY 4.0. A `crates/kapu-gui/ui/fonts` betűi: SIL Open Font License 1.1. Harmadik féltől származó crate-ek és a modell: [THIRD-PARTY.md](THIRD-PARTY.md).

---

<div align="center">
<sub>Built by Szabolcs Pintér · <a href="https://github.com/piszabolcs">GitHub</a> · <a href="https://www.linkedin.com/in/piszabolcs">LinkedIn</a></sub>
</div>
