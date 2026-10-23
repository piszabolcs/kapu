# KAPU v1 – Megvalósítási terv

Dátum: 2026-10-21. Alap: `SPEC-KAPU.md` v1.0 (2026-10-20). Ez a dokumentum a spec 13. fejezetét (M0–M9) bontja le ellenőrizhető lépésekre, és rögzíti az induláskor hozott döntéseket. A spec az igazságforrás; ha a kettő ütközik, a spec nyer, és ezt a dokumentumot kell javítani.

Jelölés: `[ ]` nyitott, `[x]` kész, `[~]` folyamatban. Minden mérföldkő végén az „Ellenőrzés” parancsai zöldek, és az `ALLAPOT.md` frissül.

---

## 0. Környezet (ellenőrzött tények, 2026-10-21, otthoni gép)

A projekt M0 után az otthoni gépre költözött; az eredeti (céges, nem rendszergazdai) környezet adatait az `ALLAPOT.md` M0 bejegyzése őrzi.

| Elem | Állapot |
|---|---|
| Gép | Windows 11 Pro, a felhasználó rendszergazda (UAC-emeléssel). Repó: `C:\Users\kaposzta\Desktop\Project\kapu_file\kapu_file` |
| Rust | rustup 1.29.2, `1.99.1-x86_64-pc-windows-msvc` a `rust-toolchain.toml` szerint, clippy + rustfmt; nightly 1.101.0 csak fuzzhoz. Hely: `%USERPROFILE%\.cargo` |
| C/C++ toolchain | Visual Studio 2026 Community 18.10, MSVC 14.51 (`VC.Tools.x86.x64`), Windows SDK 10.0.26100 (`D:\Windows Kits\10`) |
| Linkelés | a workspace tiszta buildből fordul, a három bináris fut |
| Python | nincs telepítve (csak a `tools/distill` fejlesztői eszközhöz kell majd, M4/M8 előtt) |
| git / gh | git 2.53, gh 2.102, bejelentkezve (`piszabolcs`); `origin` = `github.com/piszabolcs/kapu` (privát) |
| Hálózat | crates.io, static.rust-lang.org, github.com elérhető |
| Segédeszközök | cargo-deny 0.20.2, cargo-audit 0.22.2, cargo-fuzz 0.13.2 telepítve |

---

## 1. Induló döntések (2026-10-21, a felhasználóval egyeztetve)

| # | Döntés | Következmény |
|---|---|---|
| T1 | A repó gyökere a `kapu_file/` mappa. Git most, GitHub (privát repó + Actions CI) akkor, amikor a `gh` be van jelentkezve. | M0 CI-része addig a `scripts/check.ps1` lokális futtatása; a 3 platformos ellenőrzés a GitHub-lépéstől él. |
| T2 | Kód, kommentek, rustdoc, commit üzenet: **angol**. Csak a spec által szó szerint rögzített magyar szövegek (std-1 útmutató, tartalomvizsgálati minták) maradnak magyarul. A projektdokumentumok (TERV, ALLAPOT, DONTESNAPLO, bench/REPORT) magyarul. | README nyelve a kiadásnál (M9) dől el. |
| T3 | Korpusz: M1-ben kézi 3 szobás minta + hibás fájlok; M3-tól generált 30/300/3000 szobás szintetikus fájl; a valódi korpusz (`Archive/tesztkorpusz_prompt.md`) a repóidból külön munkamenetben, M8 előtt. | A recall/MRR számok csak M8-nál lesznek valósak. |
| T4 | Rust telepítés rustup-pal a felhasználói profilba, MSVC célra. **Megtörtént.** | `rust-toolchain.toml` = `1.99.1`. |
| T5 | Commit szerző kizárólag a felhasználó, AI-attribúció nélkül. Push csak kifejezett kérésre. Titok-szkennelés minden push előtt. | github-guardian konvenció. |
| T6 | Döntésnapló folytatása: a spec D1–D30 után a `DONTESNAPLO.md`-ben D31-től, a spec nem módosul v1.0 alatt. | A spec pontosításai (lent, 2. fejezet) is ide kerülnek. |

### 1.1 Függőségek (ellenőrzött verziók és licencek, crates.io 2026-10-21)

| Cél | Választás | Verzió | Licenc | Megjegyzés |
|---|---|---|---|---|
| `o200k_base` tokenizáló (FR-08) | `tiktoken-rs` | 0.12.1 (2026-09) | MIT | **UNVERIFIED-01 lezárva**: van karbantartott crate. Tartalék: `bpe-openai` 0.3.2 (MIT, GitHub rust-gems), gyorsabb. |
| Ujjlenyomat | `blake3` | 1.8.7 | CC0 / Apache-2.0 | hivatalos |
| Szótövezés (FR-09) | `rust-stemmers` | 1.2.0 (2019) | MIT / BSD-3 | Hungarian + English Snowball benne van; régi, de a Snowball algoritmus nem változik |
| Vektormodell futtatás (FR-10) | `model2vec-rs` **vagy** saját betöltő (`safetensors` 0.8 + `tokenizers` 0.23) | 0.3.0 | MIT (LICENSE fájl ellenőrizve) | `default-features = false, features = ["local-only", ...]` → nincs `hf-hub`/`ureq` (hálózat tiltva, 7.5). Döntés M4-ben, determinizmus-teszt után (lásd 2.11). |
| MCP (FR-15) | `rmcp` | 3.6.0 (2026-10-20) | Apache-2.0 | hivatalos SDK; `default-features = false, features = ["server", "macros", "transport-io"]` → nincs reqwest/oauth |
| CLI | `clap` 4.6 (derive) | 4.6.7 | MIT / Apache-2.0 | |
| Async (csak kapu-mcp) | `tokio` | 1.54 | MIT | csak az rmcp miatt; a core szinkron |
| JSON | `serde`, `serde_json` | 1.x | MIT / Apache-2.0 | |
| Unicode (NFC/NFKD, FR-02, FR-04) | `unicode-normalization` | 0.1 | MIT / Apache-2.0 | |
| YAKE (FR-06) | `yake-rust` 1.0.3 (MIT) **vagy** saját | | | M3-ban eldől: ha nincs magyar stopszólista a crate-ben, saját implementáció (HU+EN stopszavak a repóban). |
| Leiden (FR-12) | **saját implementáció** a `kapu-core`-ban | | | Nincs Rust crate (ellenőrizve: `leiden`, `louvain` nem létezik). Referencia: Traag–Waltman–van Eck 2019; fejlesztői ellenőrzés Python `leidenalg`-gal ugyanazon a gráfon. |
| BM25, RRF, TF-IDF csoportnév | saját | | | kevés függőség, determinizmus |
| Fuzz | `cargo-fuzz` + nightly | | | NFR-03/5 |
| Licenc/sebezhetőség | `cargo-deny`, `cargo-audit` | | | NFR-03/4; `deny.toml` allowlist: MIT, Apache-2.0, BSD-2/3, CC0, ISC, Unicode, Zlib, MPL-2.0 |

**Modell (D28, D29):** `minishlab/potion-multilingual-128M`: MIT, a nyelvlistában szerepel a `hu` (**ASSUMPTION-02 igazolva**), `hidden_dim=256` (egyezik a 6. fejezet 256 × f32 vektorával), tokenizer `BAAI/bge-m3`. Fájlméretek: `model.safetensors` 512 MB (f32), `tokenizer.json` 18.6 MB. i8 kvantálással ≈ 128 MB + 19 MB → **UNVERIFIED-02 nagyságrendileg igaz (≈150 MB)**. Ez a portable csomag (NFR-06) fő súlya; a saját desztillált HU+EN+kód modell (pl. 50 000 tokenes szókincs, i8) 15–20 MB lenne. M8/5 ezt méri.

---

## 2. Spec-ellenőrzés: pontosítandó pontok

Átnéztem a spec minden FR/NFR pontját. Ellentmondást nem találtam, de az alábbiak nincsenek elég pontosan megadva ahhoz, hogy bájtra determinisztikus implementáció készüljön belőlük. Mindegyikhez írtam javasolt alapértelmezést. **A ✋ jelűeknél a te döntésed kell, a többit feltételezésként viszem és a DONTESNAPLO-ba írom (D31+).**

| # | Pont | Mi hiányzik | Javasolt alapértelmezés |
|---|---|---|---|
| 2.1 | FR-04, FR-14 | A szoba tartalmának pontos határa: beleszámít-e a szoba utolsó sora utáni LF? | A tárolt törzs: minden szobára `@@@ <id>\n` + tartalom + `\n` (elválasztó LF). A `#DOCS` hossz az elválasztó LF nélkül értendő, így a kiolvasás bájtra pontos, üres szoba (`@@@ id\n\n`) is kezelhető. Memóriabeli újraépítésnél (FR-15/2) az elválasztó az `@@@` sor előtti LF. |
| 2.2 | FR-03 | `bodyhash` mire számolódik? | A `#BODY\n` sor utáni teljes tárolt (escape-elt) bájtsorozatra. |
| 2.3 | FR-03 | `embhash` melyik fájlra? A modell két fájl (`model.safetensors` + `tokenizer.json`). | BLAKE3 a `model.safetensors` és a `tokenizer.json` bájtjain, ebben a sorrendben összefűzve. Más tokenizáló más vektort ad, ezért kell mindkettő. |
| 2.4 | FR-03 | Mi történik, ha a fejléc nem fér 512 bájtba? | Build-hiba (E002 kiterjesztve: „header overflow”). Számítás szerint ~400 bájt a maximum normál mezőkkel, de a `emb=` név hosszát 64 karakterre korlátozzuk. |
| 2.5 ✋ | FR-02, FR-18, 7.3 | `kapu import` és `kapu add` CRLF-es `.md` fájlnál: hiba (E003) vagy normalizálás? Windows-on a legtöbb `.md` CRLF. | **Javaslat:** import/add CRLF → LF normalizál és figyelmeztet; a `.kapu` fájlban talált CR továbbra is E003. |
| 2.6 | FR-09 | A szoba indexelt tartalma tartalmazza az alajtói szövegét is, így ugyanarra a kifejezésre a szoba és az alajtója is találat lesz. | Így hagyjuk (az AI választhat az olcsó alajtó és a teljes szoba között). A BM25 a rövidebb alajtót rangsorolja előrébb. Mérés M8-ban mutatja, zavar-e. |
| 2.7 | FR-11/2 | A `term` él súlyának „normalizált értéke”: mihez képest? | A fájlban előforduló legnagyobb IDF-összeggel osztva, így a maximum 1.0. |
| 2.8 | FR-12/1 | Leiden „rögzített iterációszám”, felbontás (γ). | γ = 1.0 (standard modularitás), legfeljebb 10 iteráció vagy amíg nincs változás; a véletlen csak a csúcsok bejárási sorrendjében (seed=42, saját, platformfüggetlen PRNG: xoshiro256** vagy hasonló, nem a std `HashMap`). |
| 2.9 | FR-19/5 | „hálózati téma” szólista a cím/tagekhez. | Repóban karbantartott lista (HU+EN): network, hálózat, http, api, curl, download, letöltés, upload, feltöltés, request, fetch, wget, proxy, socket, url. |
| 2.10 ✋ | FR-15, 8. fejezet | **Nincs szobatörlés** sem MCP-ben, sem CLI-ben. Törlés csak Notepaddal + build, vagy export → fájl törlése → import. | **Javaslat:** `kapu rm <fájl> <id>` CLI parancs (kicsi, build-del). MCP-ben nem (az AI ne töröljön tudást). Ha nem kell, kihagyom. |
| 2.11 | NFR-02 | Platformfüggetlen bájtazonosság lebegőpontos műveleteknél (koszinusz, BM25, Leiden). | Szabály a magban: nincs `HashMap`-iteráció a kimenetben (`BTreeMap`/rendezett `Vec`), nincs BLAS/SIMD-kernel (saját skalárszorzat rögzített sorrendben), a modell átlagolása saját kóddal f32-ben. Ezért valószínű a saját Model2Vec-betöltő a `model2vec-rs` `ndarray`-os útja helyett; M4-ben Windows+Linux CI dönti el. |
| 2.12 | FR-10/3, FR-16 | Első keresésnél az olvasó minden ajtó vektorát kiszámolja (a teljes törzset beolvassa, de nem adja vissza). | Elfogadott; a gyorsítótár után gyors. NFR-05 méri 3000 szobán. Gyorsítótár helye: `dirs::cache_dir()/kapu/<embhash>/`. |
| 2.13 ✋ | FR-17 | A std-1 útmutató csak magyar. Angol nyelvű ügynök/prompt esetén is ezt kapja. | **Javaslat:** a `std-1` szövege marad, de az MCP eszközleírás kétnyelvű (magyar + angol bekezdés). Ha ragaszkodsz a tiszta spechez, marad egynyelvű. |
| 2.14 | 8. fejezet `kapu bench` | A `kerdesek.jsonl` `valasz_fajl` + `valasz_szakasz` mezőiből hogyan lesz ajtóazonosító, és mi számít találatnak? | Elvárt ajtó = `slug(valasz_fajl név)/slug(valasz_szakasz)`; ha nincs szakasz, a szoba. Találat, ha az elvárt ajtó **vagy a szobája** szerepel a top-k-ban (a szoba tartalmazza az alajtót). Mindkét számot riportoljuk (szigorú és megengedő). |
| 2.15 | FR-20 `--full` | A build-újraelőállítás ellenőrzése modellt igényel; eltérő `embhash` esetén nem végezhető. | Ilyenkor `--full` figyelmeztetéssel kihagyja ezt a lépést és nem nulla kóddal jelzi (`W001 model mismatch, rebuild check skipped`). |
| 2.16 | ASSUMPTION-01, M8/6 | A Claude Code a `.kapu`-t a `Read` eszközzel is beolvashatja, ha a fájl a munkakönyvtárban van. | M8/6 protokoll: a fájl a munkakönyvtáron kívül van, és a naplóban ellenőrizzük, hogy nincs `Read` hívás a `.kapu`-ra. Ha mégis van, az RISK-01 kiterjesztése, a riportba kerül. |

---

## 3. Repó felépítése

```
kapu_file/                      git gyökér
  SPEC-KAPU.md                  v1.0, nem módosul
  TERV.md  ALLAPOT.md  DONTESNAPLO.md
  Archive/                      v0.1 tervezetek, csak olvasásra
  Cargo.toml                    workspace (resolver = "2"), [workspace.lints] a NFR-03 lintekkel
  rust-toolchain.toml           channel = "1.99.1"
  rustfmt.toml  deny.toml  .gitattributes  .gitignore
  crates/kapu-core/             lib: format, header, body, build, index, graph, groups, search, validate, scan
  crates/kapu-cli/              bin `kapu`
  crates/kapu-mcp/              bin `kapu-mcp` (stdio)
  crates/kapu-bench/            bin: szintetikus korpusz generátor + mérés-futtató segéd
  bench/                        korpuszok (kerdesek.jsonl, md mappák), REPORT.md
  tests/vectors/                M1: minta.kapu, hibás fájlok E001–E011, elvárt kimenetek
  fuzz/                         cargo-fuzz célpontok
  models/                       letöltött/desztillált modell, gitignore
  tools/distill/                Python, venv, csak fejlesztői eszköz
  scripts/check.ps1             lokális CI: fmt, clippy -D warnings, test, deny, audit
  .github/workflows/ci.yml      Windows/Linux/macOS mátrix (GitHub-lépéstől)
```

`kapu-core` modulok (tervezett): `header`, `sections`, `body` (escape, szobahatárok), `doors` (cím/leírás/tag, alajtók), `price` (tiktoken), `index` (tokenizálás, stemmer, BM25), `embed` (modell, gyorsítótár), `graph`, `leiden`, `groups`, `build`, `write` (atomi mentés), `read`, `search` (RRF), `validate`, `scan` (FR-19), `error` (E001–E011 + figyelmeztetések).

---

## 4. Mérföldkövek

Minden mérföldkő: `[ ]` feladatok → Ellenőrzés parancsai zöldek → `ALLAPOT.md` + commit. A következőre csak zöld után lépünk (spec 13).

### M0 – Repó, toolchain, minőségi kapuk  (becslés: S)
- [x] Rust stable 1.99.1 MSVC telepítve, linkelés igazolva
- [ ] `cargo install cargo-deny cargo-audit cargo-fuzz`; `rustup toolchain install nightly` (csak fuzzhoz)
- [ ] `git init`, `.gitattributes` (`*.kapu text eol=lf`, `*.md text eol=lf`), `.gitignore` (`target/`, `models/`, `tools/distill/.venv/`, `*.kapu.tmp`)
- [ ] Workspace, 4 crate váza, `#![forbid(unsafe_code)]` a core-ban, `[workspace.lints]`: `clippy::pedantic`, `clippy::unwrap_used`, `clippy::expect_used`, `clippy::panic`, `clippy::indexing_slicing` = deny (tesztekben `#[allow]`)
- [ ] `deny.toml` licenc-allowlist + tiltott forrás (csak crates.io)
- [ ] `scripts/check.ps1`, `ci.yml` megírva (a CI fájl akkor él, amikor GitHubon van)
- [ ] `ALLAPOT.md`, `DONTESNAPLO.md` (D31–: a 2. fejezet feltételezései)
- Ellenőrzés: `scripts/check.ps1` zöld (fmt, clippy, üres teszt, deny, audit)

### M1 – Tesztvektorok és tesztvázak  (S–M)  — kész 2026-10-21
- [x] `tests/vectors/minta.kapu`: 3 szoba, 4 alajtó, 2 link, `@@@` escape (0 és 1 visszaperjellel), kódblokkban lévő `## `, `|` a címben, a `kapu new` sablon mint `szoba`. A `tools/build_vectors.py` építi (tiktoken o200k_base, Snowball, BLAKE3 referencia), és visszaolvasással ellenőrzi; a bájtszintű döntések D56–D67
- [x] Hibás fájl minden kódhoz: E001…E011 (11 fájl), plusz 2 edge (`edge-stale-same-length`, `edge-stale-shifted`); `expected.json` az elvárt kóddal, a minta ajtóinak adataival és a kiszolgálandó szövegekkel
- [x] Tesztvázak a spec minden Acceptance pontjához (FR-01…FR-20, 1.3, M3/M8 ellenőrzések): `crates/kapu-core/tests/acceptance.rs`, 33 `#[ignore]` váz a mérföldkő nevével, `--ignored` futtatásra elbuknak; 5 valódi vektor-integritási teszt fut
- [x] Generátor (`kapu-bench gen --rooms N --seed 42 --out DIR`): saját xoshiro256** PRNG, HU+EN szöveg, `tags:`, `##` szakaszok, kódblokk, link másik szobára; reprodukálhatósági teszt
- Ellenőrzés: `cargo test` zöld (5 valódi + 33 ignored), `--ignored` mind elbukik, a vektorok a repóban, `check.ps1` zöld

### M2 – Olvasás és validálás  (M)  — kész 2026-10-23 (az 1 órás fuzz-kapu nyitott)
- [x] Fejléc-parser (512 bájt, mezők, határellenőrzés NFR-01, ASSUMPTION-08 korlátok): `header.rs`, D68
- [x] Szakaszolvasó; `#DOCS` → pozíció → tartalom; escape-visszafejtés; ujjlenyomat-ellenőrzés; FR-15 edge 1–2 (újraolvasás, memóriabeli újraépítés `index_stale` jelzéssel): `sections.rs`, `docs.rs`, `reader.rs`, D69–D73, D77
- [x] `validate` (alap + `--full`), E001–E011: `validate.rs`, D74–D76; a `--full` újraépítés-összevetése M3-ban
- [x] `kapu_open` és `kapu_describe` logika a core-ban (keret, szomszédlista): `reader.rs`, `catalog.rs`, D78–D79
- [x] Fuzz célpontok: `header`, `sections`, `docs`, `catalog` (map/desc/graph), `validate`, `reader` (teljes fájl): `fuzz/`, `scripts/fuzz.ps1`, D86
- Ellenőrzés: minden M1 vektor a várt eredményt adja (zöld); fuzz célpontonként 2 perc hiba nélkül (kb. 6,4 millió bemenet). **Nyitott:** célpontonként 1 óra (`scripts/fuzz.ps1 -Seconds 3600`, kb. 6 óra gépidő), a kiadás (M9) előtt kötelező.

### M3 – Build alapok  (L)  — kész 2026-10-23
- [x] Szobahatárok, azonosítóképzés (FR-04/4, NFKD), ütközéskezelés, escape íráskor: `text.rs`, `build.rs` (`parse_body`, D36)
- [x] Feliratok FR-06: cím (D87), leírás (szóhatáron vágás, D9 képlet, kézi leírás 300 tokennél, D88), tagek (kézi / YAKE, D84), rövid tagek
- [x] Alajtók FR-07 (kódblokk-felismerés: ``` és ~~~ kerítés)
- [x] Árak FR-08 (`tiktoken-rs` 0.12 o200k_base, megjegyzések nélkül) + karakterszám
- [x] Index FR-09: tokenizálás, NFC, kettős stemmer (`rust-stemmers` 1.2, D80), 3× cím/tag súly, rendezett sorok; BM25 keresés (`search.rs`, D85)
- [x] `#DOCS`, `#MAP`, `#DESC`, `#GRAPH` (`link`, `term`), fejléc; atomi mentés (`write_atomic`)
- [x] `import`, `export`, `add`, `new`, `rm` a core-ban (`edit.rs`, D90); CRLF-kezelés (D40, W004); a CLI-parancsok M6-ban
- [x] `validate --full` újraépítés-összevetés (D89), leírás-korlát a validátorban (E010)
- Ellenőrzés (zöld): import→export bájtra azonos; build×2 bájtra azonos; `validate --full` hibátlan; **a Rust build a `minta.kapu`-t bájtra reprodukálja**; FR-02/04/05/06/07/08/09/15(4 ajtó)/18 Acceptance tesztek zöldek
- Mérés: `bench/REPORT.md` 1. táblázat (30/300/3000 szoba: 21 ms / 183 ms / 2,1 s build, térkép/törzs 0,26–0,27 a generált korpuszon)

### M4 – Vektor, gráf, csoportok  (L)  — kész 2026-10-23
- [x] Modell letöltve `models/potion-multilingual-128M/` alá (safetensors + tokenizer.json, SHA-256 a Hugging Face LFS-azonosítókkal egyezik), `embhash` (D38)
- [x] Saját betöltő (`embed.rs`): safetensors-fejléc saját kóddal, `tokenizers` 0.22 (`fancy-regex`, hálózat és C-regex nélkül), f64 átlag (D46); a Python `model2vec`-kel 6 szövegen egyezik (D91). i8 kvantálás: a betöltő tudja, az előállítás M8/5-ben
- [x] Vektor-gyorsítótár (`VectorCache`, D91), az olvasó M5-ben használja
- [x] Élek FR-11: `link`, `term` (ritkasági küszöb a közös kifejezések között, D93, jóváhagyásra vár), `sim` (k=5, ≥0,6)
- [x] Leiden FR-12 saját implementációval (`leiden.rs`, D92), csoportnév (osztályszintű TF-IDF), gid-sorrend, izolált szoba
- [x] Fejlesztői ellenőrzés `leidenalg`-gal (`tools/leiden_check.py`): karateklub ±0,01, generált korpusz 30/300/3000 szobán a Rust nem rosszabb 0,01-nél
- Ellenőrzés (zöld): FR-11 és FR-12 Acceptance; build×2 bájtra azonos modellel; a determinizmus-teszt egy commitolt tesztmodellel rögzített BLAKE3-at vár, a CI mindhárom platformon futtatja (D95); `validate --full` a modellel újraépít és összevet
- Mérés: `bench/REPORT.md` 2. táblázat (modularitás 0,25 / 0,35 / 0,41; csoportok 4 / 8 / 9)

### M5 – Hibrid keresés  (M)  — kész 2026-10-23, az FR-16 Acceptance nyitott (A21)
- [x] Lekérdezés-feldolgozás = index-tokenizálás; BM25 top-50; vektor top-50; RRF k=60; döntetlen → azonosító szerint (`search.rs`, D96)
- [x] `max_tokens` szűrés, `k` paraméter, találatforrás (`bm25`/`vec`/`both`)
- [x] Modell-eltérés: `vector: unavailable`, csak BM25 (FR-10/4 Acceptance zöld)
- [x] Vektor-gyorsítótár törzsenként (D97)
- Ellenőrzés: FR-10/4 zöld. **FR-16 Acceptance a `potion-multilingual-128M`-mel nem teljesül** (magyar lekérdezés → CSS-kód, AGGALYOK A21); a teszt indoklással `ignored`, a D28 modellválasztás (M8/5) dönt
- Mérés: `bench/REPORT.md` 3. táblázat (3000 szoba: BM25 138 ms, hibrid meleg 162 ms, hideg 3,3 s)

### M6 – CLI  (M)  — kész 2026-10-23
- [x] 8. fejezet minden parancsa (`new`, `import`, `export`, `add`, `rm`, `build`, `validate [--full]`, `map [--group]`, `search [--k] [--max-tokens]`, `describe`, `open`, `bench`), `--json`, kilépési kódok, hibakód a kimenetben, `--model`/`--no-model` (D98)
- [x] `kapu bench` (D49: recall@5/10, MRR@10, BM25/vektor/hibrid, szigorú és megengedő; `evaluate.rs`)
- [x] Térkép-nézet és std-1 útmutató a core-ban (`view.rs`), az MCP is ezt használja
- Ellenőrzés (zöld): integrációs teszt minden parancsra (`crates/kapu-cli/tests/cli.rs`, `--json` szerkezet rögzítve); keresztellenőrzés a referenciával 300 szobán (D99)

### M7 – MCP szerver  (M)  — kész 2026-10-23 (a kézi Claude Code-próba nyitott, A25)
- [x] `rmcp` 3.7 stdio szerver, 5 eszköz (FR-15), minden hívásnál fájl-újraolvasás, útmutató munkamenetenként az első térképnél (FR-17), kétnyelvű eszközleírások (D48, D100)
- [x] FR-19 tartalomvizsgálat (`scan.rs`, `data/scan_patterns.txt`, D101): első megnyitáskor és `kapu_write`-nál; a CLI `kapu add` is
- [x] Automatikus teszt: egyetlen eszközválasz sem tartalmazza a teljes törzset; négy ajtó egy hívásban
- [x] Claude Code / Claude Desktop konfigurációs példa a README-ben
- Ellenőrzés (zöld): MCP konformancia a hivatalos `rmcp` klienssel (initialize, tools/list, tools/call mind az öt eszközre); FR-15 és FR-19 Acceptance (`crates/kapu-mcp/tests/mcp.rs`). **Nyitott:** kézi próba Claude Code-ból (A25)

### M8 – Mérés és modellválasztás  (L, részben külön munkamenet)  — részben kész 2026-10-23
1. [ ] **Korpusz** a repóidból a `tesztkorpusz_prompt.md` szerint, ≥ 40 kérdés. Nyitott (A26): a te repóidból, külön munkamenetben.
2. [~] `kapu bench` kész (recall@5/10, MRR@10, BM25/vektor/hibrid, szigorú és megengedő, D49); futtatás a korpusz után.
3. [x] Skálázás: 30/300/3000 szoba, térkép/fájl tokenarány, build-, térkép-, kinyitás- és keresési idők, csomagméret (`bench/REPORT.md` 1–4. táblázat)
4. [ ] Gráf élpontosság és csoportnevek kézi mintán. Nyitott (emberi ellenőrzés).
5. [~] D28: i8 kvantálás kész és alapértelmezett (D102); saját desztillált modell és az összevetés a korpuszon nyitott; az FR-16 eset a jelenlegi modellel nem teljesül (A21).
6. [ ] Végponttól végpontig Claude Code-dal. Nyitott (A25).
- Mérés közben: a `leidenalg`-összevetés (2. táblázat) és a Python–Rust keresztellenőrzés (D99) is lefutott.
- 2026-10-23, helyettesítő valódi korpuszon (telepített skillek + frontend-minták, 251 szoba, 118 kérdés): `kapu bench` (REPORT 6.), MCP válaszidők (7.), élő próba Claude-ügynökökkel 10/10 (8.); három hiba javítva (D120-D122), öt formátumjavaslat az AGGALYOK A részében (A32-A36).

### M9 – Kiadás  (M)  — részben kész 2026-10-23
- [x] Portable csomag Windowsra: `scripts/package.ps1` (`kapu.exe` + `kapu-mcp.exe` + `models/potion-multilingual-128M-i8/`, 123,5 MB zip, füstteszttel); Linux/macOS: `.github/workflows/release.yml` címkére, piszkozat kiadással (D103; helyben nem futtatható, A28)
- [x] README (angol, A27), spec, licencek (MIT kód; CC BY 4.0 spec: `LICENSE-SPEC.txt` a csomagban), `THIRD-PARTY.md` (`tools/third_party.py`: 149 crate, plusz a modell és a tokenizáló)
- [~] Titok-szkennelés lefutott, nincs találat; push és GitHub release: a felhasználó döntése
- [~] Fuzz: a CI-ban minden pushnál 30 s/célpont (D104); helyben 10 perc/célpont, kb. 26,7 millió bemenet, nincs találat; az 1 óra/célpont kiadás előtt nyitott
- Ellenőrzés: tiszta gépen, telepítés nélkül az 1.3 forgatókönyv működik — **nyitott** (A25, A28)

### M10 – Asztali ablak (`kapu-gui`)  (M)  — kész 2026-10-23 (a felhasználó kérése a portfólió-véglegesítéshez)
A spec v1 nem ír elő grafikus felületet; ez kiegészítés, a formátumot és a többi eszközt nem érinti.
- **Cél:** a `.kapu` fájl úgy nézhető meg, ahogy egy ügynök látja: térkép a tokenárakkal, kinyitott ajtó a leírással, címkékkel és szomszédokkal, hibrid keresés tokenkerettel, gráf a csoportokkal, gyors és teljes ellenőrzés; alul a szerzői aláírás (`psz // v{verzió}`) az app-info kártyával; ebből készülnek a README képernyőképei.
- **Nem cél (v1):** írás (új szoba, import, build): az a `kapu` és a `kapu-mcp` dolga; több fájl egyszerre; magyar felület.
- **Felépítés (D115):** `crates/kapu-gui`, Tauri 2 a rendszer webview-jával. A felület az `ui/` mappában sima HTML, CSS és JavaScript, build-lépés és Node nélkül, a binárisba ágyazva. Egyetlen parancsréteg (`src/api.rs`, `Session::dispatch`) szolgálja ki az ablakot és a fejlesztői szervert (`examples/serve.rs`, csak 127.0.0.1), így a böngészős képernyőképek és a felület-tesztek ugyanazt a kódot futtatják.
- **Biztonság (D116):** a szobák szövege nem megbízható. Markdown → HTML Rustban (`pulldown-cmark`); a nyers HTML szövegként jelenik meg, a kép csak alt-szövegként, a link csak `http`, `https`, `mailto` vagy ajtó-link lehet. CSP `default-src 'self'`; a JavaScript csak a saját három parancsot hívja (`kapu`, `pick`, `open_external`), a capability csak `core:default`.
- **Design (D117, frontend-design-2026):** regiszter: Precision Linear / Dense Utility (fejlesztői eszköz, sűrű adat). Paletta: obszidián / tinta / egyetlen kiemelés, a kapu-ikon cinóbere (OKLCH, hue 38), sötét és világos téma a rendszer szerint. Betű: Geist + Geist Mono (OFL, változó, latin-ext, a binárisba ágyazva). Elrendezés: felső sáv nézetváltóval, bal oldalt a térkép, középen a nézet, jobbra az ajtó adatai, alul állapotsor az aláírással. Kockázat: minden ajtósorban ármérő, a legdrágább ajtóhoz mérve, mert a KAPU lényege, hogy csak azt nyisd ki, amit megfizetsz.
- **Bemutató fájl (D118):** `examples/kapu-spec.kapu`, a specifikáció KAPU-fájlba csomagolva (`tools/split_spec.py`, 35 szoba, 42 ajtó, 4 csoport, i8 modellel).
- **Ellenőrzés:** `api` és `markdown` egységtesztek (megnyitás, szoba, keresés, gráf, ellenőrzés, hibakódok, nem megbízható jelölés); `contrast.py`: 0 pár 4,5:1 alatt, mindkét témában; `preflight_audit.py`: 0 hiba, 1 indokolt figyelmeztetés (több `h1`, de egyszerre csak egy nézet látszik); kézi végigkattintás a fejlesztői szerveren; a Tauri ablak Windowson elindul.

---

## 5. Mérési terv (összefoglaló)

| Mit | Hogyan | Mikor | Hova |
|---|---|---|---|
| Determinizmus | build×2 bájtösszevetés; CI 3 platformon ugyanaz a `bodyhash` → ugyanaz a fájl (BLAKE3 a teljes fájlra) | M3-tól minden commit | CI, `check.ps1` |
| Tokenár helyessége | `tiktoken-rs` vs Python `tiktoken` o200k_base 20 mintaszövegen (HU, EN, kód, emoji) | M3 | teszt |
| Térkép/fájl tokenarány | `kapu map` tokenje ÷ `#BODY` tokenje, 30/300/3000 szoba | M3, M8 | REPORT 1. táblázat |
| Build idő, válaszidők | `std::time::Instant` a CLI-ben `--json` alatt (`timing_ms`), 5 futás mediánja, hideg/meleg gyorsítótár | M3/M5/M7, M8 | REPORT 2. táblázat |
| recall@5, recall@10, MRR@10 | `kapu bench`, BM25 / vec / hibrid, szigorú és megengedő | M8 | REPORT 3. táblázat |
| Modularitás, csoportméret | build-naplóból (`--json`) | M4, M8 | REPORT 4. táblázat |
| Él-pontosság, csoportnév | kézi minta, 50 él/típus, 10 csoportnév | M8 | REPORT 5. táblázat |
| Modellválasztás (D28) | ugyanaz a bench két modellel + csomagméret + válaszidő | M8/5 | REPORT 6. táblázat + D-bejegyzés |
| Végponttól végpontig (1.3) | Claude Code munkamenet MCP-naplóval, tokenösszeg vs. fájl tokenje | M8/6 | REPORT melléklet |
| Fuzz | 1 óra/célpont hiba nélkül | M2, M9 előtt újra | ALLAPOT |

---

## 6. Minőségi kapuk (minden commit előtt)

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
cargo audit
```
Ezt a `scripts/check.ps1` futtatja. A CI ugyanezt a 3 platformon, plusz M3-tól a determinizmus-összevetést.

---

## 7. Kockázatok

| # | Kockázat | Hatás | Kezelés |
|---|---|---|---|
| R1 | Saját Leiden implementáció hibás vagy lassú 3000 szobán | csoportok rosszak / build lassú | referencia-összevetés `leidenalg`-gal M4-ben; a gráf ritka (k=5 sim + term), O(E) iterációk |
| R2 | Lebegőpontos eltérés platformok között → NFR-02 bukik | CI piros Linux/macOS-en | 2.11 szabályok; ha mégis eltér, az `f32` eredmények kerekítése 6 tizedesre a kiírás és a döntések előtt |
| R3 | Modellcsomag 150 MB | NFR-06 nehézkes | D28 saját desztillált modell M8-ban; i8 kvantálás |
| R4 | `rmcp` API gyorsan változik (3.6.0 tegnapi) | M7 átírás | verzió rögzítve `Cargo.lock`-ban; csak a stabil `server` + `transport-io` felület |
| R5 | `tokenizers` crate C/C++ fordítás (onig, esaxx) CI-ban | build hiba macOS/Linux | `fancy-regex` feature, vagy saját betöltő (2.11) |
| R6 | A valódi korpusz (M8/1) késik vagy céges adatot tartalmaz | mérés csúszik | szintetikus korpusz addig; a korpusz-prompt tiltólistája + titok-szkennelés a `bench/korpusz`-ra is |
| R7 | Claude Code közvetlenül olvassa a `.kapu`-t (RISK-01) | 1.3 nem teljesül | 2.16 protokoll; ha bukik, a riport rögzíti, a formátum nem változik |
| R8 | 512 bájtos fejléc túlcsordul | build hiba | 2.4 korlátok, teszt a maximális értékekkel |

---

## 8. Munkamenet-fegyelem

- Egy mérföldkő egyszerre; az M-ek sorrendje kötött. Mérföldkövön belül feladatonként: teszt előbb (a spec Acceptance pontja), utána implementáció, utána `check.ps1`.
- `ALLAPOT.md`: minden commitnál frissül (mi kész, mi fut, mi a következő, nyitott kérdések).
- `DONTESNAPLO.md`: D31-től minden olyan döntés, ami a spec feltételezéseit pontosítja vagy módszert cserél (D30).
- Commit: a felhasználó a szerző, angol üzenet, AI-attribúció nélkül. Push csak kérésre, titok-szkennelés után.
- Ha a megvalósítás közben a spec és a valóság ütközik: megállás, 3 soros jelentés, két opció, döntés a felhasználótól.

---

## 9. Következő lépés

A 2. fejezet ✋ pontjainak (2.5, 2.10, 2.13) eldöntése, majd M0 indítása.
