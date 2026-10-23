# KAPU – Állapotnapló

Minden commitnál frissül (TERV.md 8. fejezet). Legfrissebb bejegyzés felül.

---

## 2026-10-23 késő este – Spec v1.0.2 (A32-A36) és a teljesen angol mérés

**A felhasználó döntései:** az A32-A36 javaslatok a javaslat szerint készülnek el; a mérési eredmények kerüljenek ki a GitHubra; a publikált mérés legyen teljesen angol (D128); a magyar kérdés angol szövegre közös ötletelés tárgya, párhuzamos szálként.

**Kész**
- Spec v1.0.2 (15. fejezet): D123 fájlon belüli horgony, D124 keresési kivonat, D125 szobás csoportlista és `room=`, D126 legfeljebb 8 szomszéd, D127 összesítő sor; Rust és Python referencia, a minta és a tesztvektorok változatlanok, 300 szobán bájtazonos keresztellenőrzés.
- Angol mérés (244 szoba, 77 kérdés, élő próba 10/10, mediánban 842 token kérdésenként): `bench/RESULTS.md` (angol), `bench/results/2026-10-23-english-corpus.json`, README „Measurements”; a vegyes korpusz eredményei: `bench/results/2026-10-23-real-corpus.json`, REPORT 5-9.
- A bemutató fájl újraépítve a v1.0.2 specből (36 szoba, 43 ajtó).

**Ellenőrzés (lefuttatva):** clippy tiszta, `cargo test --workspace` zöld (új tesztek: horgonylink, összesítő és szobanézet, kivonat, MCP v1.0.2 kimenetek).

**Nyitott:** A38 angol címkék a kimenetben, A39 jobb kivonat-sorválasztás, A40 CLI broken pipe; A21 közös ötletelés.

---

## 2026-10-23 este – Teszt- és mérési kör valódi korpuszon, MCP-ellenőrzés

**A felhasználó kérése:** nagy próbaadatbázis a telepített skillekből és modern frontend-mintakódokból, KAPU-fájlba építve, élesben kipróbálva; az MCP szerver ellenőrzése az `mcp-builder` skill szerint; mérések és egy összefoglaló artifact.

**Kész**
- Korpusz (`bench/korpusz/`, nincs gitben): 217 skill-szoba (`tools/collect_skills.py`) és 34 frontend-szoba (háttér-ügynök írta, webes forrásokkal), 251 szoba, 1960 ajtó, 33,4 MB; a build 4,8 s, a teljes ellenőrzés hibátlan, a második build bájtra azonos.
- Keresés 118 kérdésen (REPORT 6.): hibrid r@10 0,65 összesen, angolul 0,86, magyarul 0,45.
- MCP (REPORT 7.): keresés medián 138 ms, kinyitás 71 ms, térkép 81 ms; az eszközök annotáltak, a hibák tippet adnak (D120). Élő próba 5 Claude-ügynökkel valódi MCP-hívásokon át (REPORT 8.): 10/10 helyes, mediánban 2 hívás és kb. 2,2 ezer token kérdésenként.
- Javított hibák: angol stopszavak a csoportnevekben (D121), téves S5 tartalomvizsgálati találatok részszóra (D122).
- Új eszközök: `tools/collect_skills.py`, `tools/mcp_probe.py`, `tools/kapu_mcp_call.py`, `tools/mcp_eval.py`.

**Ellenőrzés (lefuttatva):** clippy tiszta, `cargo test --workspace` zöld, tesztvektorok és determinizmus-ujjlenyomat változatlan, 300 szobás Python–Rust keresztellenőrzés bájtazonos.

**Nyitott:** A32-A36 formátumjavaslatok döntésre; A25 `claude -p` futtatás a CLI újbóli bejelentkezése után; A26 a te repóidból.

---

## 2026-10-23 – Portfólió-véglegesítés: M10 asztali ablak, aláírás, README

**A felhasználó kérése:** a repót a `github-portfolio-publisher` és a `signature` skillel kell véglegesíteni, és a KAPU kapjon grafikus felületet (`frontend-design-2026` skill). Döntései: a commitok szerzője a GitHub noreply cím lesz, a történet átíródik (D112, felváltja a D111-et); az aláírás parancssori formája jóváhagyva (D113); a GUI-t saját javaslat alapján kellett megépíteni.

**Kész**
- CI: a cargo-audit job `checks: write` jogot kapott (D114); az előző futás minden más jobja zöld volt (három platform, deny, fuzz).
- Aláírás: `kapu --help` utolsó sora `psz // v0.1.0`, a `kapu --version` és a `kapu-mcp --version` az app-info kártya, az MCP szerverinfó címe és repó-linkje; egyetlen forrás: `kapu_core::about` (D113).
- M10: `crates/kapu-gui` Tauri 2 ablak (D115–D119): térkép ármérőkkel, szoba olvasó, hibrid keresés tokenkerettel, gráf, gyors és teljes ellenőrzés, aláírás és info-kártya, fájl megnyitása párbeszédablakkal, ledobással és parancssorból. Fejlesztői szerver (`examples/serve.rs`) a képernyőképekhez.
- Bemutató fájl: `examples/kapu-spec.kapu` (a specifikáció 35 szobában, `tools/split_spec.py`); ikon és logó (`tools/make_icons.py`).
- README a portfólió-sablon szerint, angolul és magyarul, képernyőképekkel; `CHANGELOG.md`; a csomag és a kiadási workflow a `kapu-gui`-t és a bemutató fájlt is viszi.

**Ellenőrzés (2026-10-23, lefuttatva)**
- `cargo clippy --workspace --all-targets -D warnings` tiszta, `cargo test --workspace` zöld (a `kapu-gui` 6 új teszttel), `cargo deny check` zöld a kivétellel (D119).
- `contrast.py`: minden szövegpár ≥ 4,5:1 mindkét témában; `preflight_audit.py`: 0 hiba, 1 indokolt figyelmeztetés.
- A natív ablak Windowson elindul, megnyitja a bemutató fájlt, a címe a fájl neve; a felület minden nézete kézzel kipróbálva a fejlesztői szerveren.

**Nyitott:** A31 (az ablak Linuxon és macOS-en nincs kipróbálva), A21, A25, A26, A28, A6.

---

## 2026-10-23 – Közös áttekintés: döntések átvezetve, spec v1.0.1

**A felhasználó döntései (AGGALYOK.md A rész):** A21 (FR-16) az M8 modellmérés dönt; D93 (`term` küszöb) és D97 (törzsenkénti vektor-gyorsítótár) jóváhagyva; A2, A4, A5, A18 átvezetve; A20 szigorúbb Leiden-kritérium; A27 kétnyelvű README.

**Kész**
- `SPEC-KAPU.md` v1.0.1: FR-05 (megjegyzés-szabály), FR-10/3 (gyorsítótár-kulcs), FR-11/2 és ASSUMPTION-06 (`term` küszöb), FR-13 (`\\` escape), FR-15/2 (keret határolóval), FR-20 (E012), 14. fejezet: változáslista. A fájlok első sora marad `%KAPU 1.0`.
- D106 keret `boundary`-vel; D107 mező-escape; D108 új megjegyzés-szabály (Python és Rust); D109 E012 a környezeti hibákra; D110 Leiden 8 indítással, konvergenciáig (3000 szobán 0,4153 a `leidenalg` 0,3949-ével szemben; `kapu-bench leiden` alparancs).
- A minta és a tesztvektorok bájtra változatlanok; a tesztmodelles ujjlenyomat sem változott.
- README kétnyelvű (angol, majd magyar).
- Új aggály: A30 (a commitok szerzői e-mailje gmail, a portfólió-szokás a GitHub noreply cím).

**Ellenőrzés (2026-10-23, lefuttatva)**
- `scripts/check.ps1` zöld; minden munkaterület-teszt zöld (az FR-16 teszt indoklással `ignored`).

---

## 2026-10-23 – M8 részben, M9 részben: i8 modell, mérések, csomag

**Kész**
- i8 kvantálás (`tools/quantize.py`), a betöltő int8-at tart a memóriában; az i8 lett az alapmodell (D102). 124 MB, a vektorok ≥ 0,99996 koszinusszal egyeznek, a sebesség azonos.
- Mérések: `bench/REPORT.md` 1–4. táblázat (build, fájlméret, térkép/törzs arány, gráf és csoportok, modularitás a `leidenalg`-gal összevetve, térkép-, kinyitás- és keresési idők, f32 kontra i8).
- Portable Windows-csomag (`scripts/package.ps1`, 123,5 MB zip, füstteszttel), kiadási workflow Linuxra és macOS-re címkére (piszkozat), `THIRD-PARTY.md`, README frissítve (D103).
- CI: Linuxos fuzz-füstteszt minden pushnál (D104).
- Fuzz helyben, célpontonként 10 perc: header 13,3 M, sections 1,8 M, docs 4,4 M, catalog 6,4 M, validate 0,5 M, reader 0,3 M bemenet, összesen kb. 26,7 millió, **nincs találat** (napló: `fuzz/fuzz-run.log`, nincs gitben).

**Nyitott (a felhasználóval):** valódi korpusz és rajta a `kapu bench` (M8/1–2), kézi ellenőrzések (M8/4), saját desztillált modell (M8/5), Claude Code végponttól végpontig (M8/6, A25), tiszta gépes próba (A28), 1 órás fuzz-kapu, push és GitHub release. Részletek: `AGGALYOK.md`.

**Ellenőrzés (2026-10-23, lefuttatva)**
- `scripts/check.ps1` zöld; minden munkaterület-teszt zöld (egy FR-16 teszt indoklással `ignored`, A21).

---

## 2026-10-23 – M7 kész: MCP szerver

**Kész**
- `kapu-mcp`: `rmcp` 3.7 stdio szerver öt eszközzel (`kapu_map`, `kapu_search`, `kapu_describe`, `kapu_open`, `kapu_write`), std-1 útmutató, kétnyelvű leírások, tartalomvizsgálat (D100).
- `scan.rs` (FR-19, S1–S6, D101); a CLI `kapu add` is vizsgál.
- README: MCP-beállítás Claude Code-hoz és Claude Desktophoz, CLI-példák.

**Ellenőrzés (2026-10-23, lefuttatva)**
- `scripts/check.ps1` zöld. MCP konformancia a hivatalos klienssel: 2 teszt (teljes folyamat a mintán, FR-19 írás).
- Nem futott: kézi próba Claude Code-ból (A25).

**Következő: M8 – mérés és modellválasztás**

---

## 2026-10-23 – M6 kész: CLI

**Kész**
- `kapu` CLI mind a 12 paranccsal, `--json`, FR-20 kódokkal és kilépési kódokkal, modellválasztással (D98).
- `view.rs` (térkép-nézet, std-1 útmutató), `evaluate.rs` (`kapu bench`), `search::rankings`.
- `tools/crosscheck.py`: a Python referencia és a Rust 300 szobán szakaszonként bájtra azonos (D99); A14 lezárva, helyette A23.

**Ellenőrzés (2026-10-23, lefuttatva)**
- `scripts/check.ps1` zöld; CLI-integrációs tesztek: 4 (minden parancs).

**Következő: M7 – MCP szerver**

---

## 2026-10-23 – M5 kész: hibrid keresés (FR-16 Acceptance nyitott)

**Kész**
- `search.rs`: hibrid keresés (BM25 + vektor, RRF k=60, `max_tokens`, `k`, forrás), modell-eltérésnél csak BM25 és `vector: unavailable` (D96).
- Vektor-gyorsítótár törzsenként egy fájl (D97): 3000 szobánál a meleg keresés 1,2 s helyett 0,16 s.
- `kapu-bench search`: válaszidő-mérés, `bench/REPORT.md` 3. táblázat.

**Nyitott, kiemelt:** az FR-16 Acceptance („üvegszerű, homályos gomb” → `backdrop-filter: blur`) a jelenlegi modellel nem teljesül; a Python `model2vec` is ugyanezt mutatja (AGGALYOK A21). M8/5 modellválasztás dönt.

**Ellenőrzés (2026-10-23, lefuttatva)**
- `scripts/check.ps1` zöld; a keresés tesztjei: FR-10/4, hibrid rangsor, gyorsítótár, `max_tokens`/`k`.

**Következő: M6 – CLI**

---

## 2026-10-23 – M4 kész: vektormodell, gráf, Leiden

**Kész**
- `embed.rs`: Model2Vec-betöltő (safetensors f32/i8, `tokenizers` 0.22 hálózat és C nélkül), kódolás a `model2vec` 0.10 szerint, koszinusz, modellkeresés (`KAPU_MODEL_DIR` vagy a futtatható mellett `models/<név>`), vektor-gyorsítótár. A Python `model2vec`-kel egyezik (D91).
- `leiden.rs`: saját Leiden (lokális mozgatás, finomítás, aggregálás, 2 külső kör, D92); a `leidenalg`-gal összevetve nem rosszabb 0,01-nél.
- `build.rs`: `build_with`/`rebuild_with` modellel: `sim` élek, Leiden-csoportok, `emb`/`embhash` a fejlécben. `validate_with`: a teljes ellenőrzés a modellel is újraépít.
- **Spec-eltérés (D93, jóváhagyásra vár):** a `term` élek ritkasági küszöbe a közös kifejezések között számolódik, mert a spec szó szerinti szabálya soha nem ad élt. A `minta.kapu` ezért változott (új `term` él), a Rust továbbra is bájtra reprodukálja.
- Tesztmodell a CI-hoz (`tests/model/kapu-test-tiny`), determinizmus-teszt rögzített BLAKE3-mal (D95).
- `paste` advisory kivétel indoklással (D94). Új aggályok: A16–A20.
- A modell (`models/`, 530 MB) letöltve, nincs gitben.

**Ellenőrzés (2026-10-23, lefuttatva)**
- `scripts/check.ps1` zöld: 55 egységteszt, 31+12+2+3 integrációs teszt (4 ignored: M5, M7, M8), deny, audit.
- Mérés: `bench/REPORT.md` 2. táblázat.

**Következő: M5 – hibrid keresés**

---

## 2026-10-23 – M3 kész: build

**Kész**
- `kapu-core` új modulok: `text.rs` (slug, escape, kódblokk, bekezdés, cím/leírás/tag, linkek), `terms.rs` (index-tokenizálás, kettős Snowball-tövezés, o200k_base ár), `yake.rs` (automatikus tagek, D84), `build.rs` (szobák, alajtók, feliratok, index, `link`/`term` gráf, csoportok, szakaszok, fejléc, atomi mentés), `search.rs` (BM25, D85), `edit.rs` (`new`, `import`, `export`, `add`, `rm`).
- Új függőségek: `tiktoken-rs` 0.12, `rust-stemmers` 1.2, `unicode-normalization` 0.1 (a `cargo-deny` elfogadta).
- **A Rust build a `tests/vectors/minta.kapu`-t bájtra reprodukálja.**
- A validátor teljes szintje újraépít és bájtra összevet (D89); a leírás 300 tokenes korlátja E010 (D88).
- Python referencia javítva két spec-eltérésre (D87, D88); a vektorok változatlanok.
- `kapu-bench measure`: első mérés, `bench/REPORT.md` 1. táblázat. A `term` élek invertált számítása a 3000 szobás buildet 64 s-ról 2,1 s-ra vitte le.
- `AGGALYOK.md`: A1 és A3 lezárva, új: A10–A15.

**Ellenőrzés (2026-10-23, lefuttatva)**
- `scripts/check.ps1` zöld: 50 egységteszt, 29+12 integrációs teszt (6 ignored: M4, M5, M7, M8), clippy pedantic `-D warnings`, deny, audit.

**Következő: M4 – vektor, gráf, csoportok**

---

## 2026-10-23 – M2 kész: olvasás, validálás, fuzz

**Kész**
- `kapu-core`: `error.rs` (E001–E011), `header.rs` (512 bájtos fejléc), `sections.rs` (szakaszhatárok, CR/vezérlő/bidi/láthatatlan karakterek, parserkorlátok, szigorú és toleráns törzshossz), `docs.rs` (`#DOCS`, ajtók kivágása, BLAKE3, escape, megjegyzések), `catalog.rs` (`#MAP`/`#DESC`/`#GRAPH` sorok), `validate.rs` (alap és teljes szint, rögzített sorrend), `reader.rs` (kinyitás, leírás, keret, szomszédok, FR-15 edge 1–2 újraépítéssel).
- Első külső függőség: `blake3` 1.8 (D73).
- `fuzz/`: hat cargo-fuzz célpont, `scripts/fuzz.ps1`. Windowson a VS AddressSanitizer komponens kell hozzá (telepítve, D86).
- Python referencia: `snowballstemmer` 2.2.0-ra rögzítve, mert a 3.x angol tövezője eltér a Rust-tól (D80); összegzési sorrend és NFC javítva (D81, D82). A vektorok bájtra változatlanok.
- `AGGALYOK.md`: a végső áttekintésre gyűjtött kérdések (A1–A9).
- A tanulómódot a felhasználó 2026-10-23-án kikapcsolta; a munka folyamatosan halad a terv szerint.

**Ellenőrzés (2026-10-23, lefuttatva)**
- `scripts/check.ps1` zöld: 30 egységteszt, 29 elfogadási teszt (17 ignored, M3–M8), deny, audit.
- Mind a 11 hibakód-vektor és mindkét edge-vektor a várt eredményt adja.
- Fuzz: célpontonként 2 perc, kb. 6,4 millió bemenet, nincs találat. Az 1 órás kapu még hátravan.

**Következő: M3 – build**

---

## 2026-10-21 – M1 kész: tesztvektorok, tesztvázak, generátor

**Kész**
- `tools/build_vectors.py` (Python 3.13 venv: tiktoken, snowballstemmer; BLAKE3 a `tools/blake3_ref.py` referenciából): megépíti a `tests/vectors/minta.kapu`-t és belőle a 11 hibás fájlt (E001–E011) és a 2 olvasói edge-fájlt, kiírja az `expected.json`-t, és a kész mintát olvasóként visszaellenőrzi (fejléc, szakaszhatárok, bodyhash, ajtó-ujjlenyomatok, `@@@` határok, escape-körút).
- A minta: 3 szoba (`frontend-design-alapok`, `electron-leckek`, `szoba` = a kitöltetlen `kapu new` sablon), 4 alajtó, 2 `link` él, 2 csoport; 17 772 bájt, a fejléc 329 bájt hasznos. Modell nélkül épült (`emb=none`, D57).
- `crates/kapu-core/tests/acceptance.rs`: 5 futó vektor-integritási teszt + 33 `#[ignore]` váz (FR-01…FR-19 Acceptance, E001–E011, FR-15 edge 1–2, M3 bájtazonosság és import/export, 1.3 sikerkritérium), mindegyik a mérföldkő nevével; `cargo test -- --ignored` mind a 33-at elbuktatja.
- `kapu-bench gen --rooms N --seed S --out DIR`: determinisztikus korpuszgenerátor (saját xoshiro256**, D43), 3 egységteszt. Új függőség: `clap` 4 (derive); `serde_json` csak dev-dependency a core tesztjeihez.
- `DONTESNAPLO.md`: D56–D67 (a vektorok által rögzített bájtszintű szabályok: modell nélküli build, üres slug, megjegyzés-eltávolítás, leírásvágás, index-tokenizálás és `dl`, linkforrás, élsúly-formátum, csoportnév, alajtó-határ).
- `.gitattributes`: `tests/vectors/*.kapu binary` (a hibás vektorok szándékosan tartalmaznak CR-t és vezérlőbájtot).
- `tests/vectors/README.md`, `tools/README.md` frissítve.

**Ellenőrzés (2026-10-21, lefuttatva)**
- `scripts/check.ps1` zöld: fmt, clippy `-D warnings`, test (5 + 2 + 3 teszt, 33 ignored), deny, audit.
- A szkript kétszer futtatva bájtra azonos vektorokat ad (`git status` tiszta a második futás után).
- CI az előző pushnál: Ubuntu és Windows zöld, deny és audit zöld; a macOS futó 15 perc sor után elindulás nélkül törlődött (runner nem állt rendelkezésre, nem kódhiba). Újra próbálva ennél a pushnál.

**Következő: M2 – olvasás és validálás**
- Fejléc-parser (512 bájt, mezők, NFR-01 határellenőrzés), szakaszolvasó, `#DOCS` → pozíció → tartalom, escape-visszafejtés, ujjlenyomat-ellenőrzés, FR-15 edge 1–2.
- `validate` E001–E011, a 11 vektoron; `kapu_open`/`kapu_describe` logika a core-ban.
- Fuzz célpontok (`cargo fuzz`), 1 óra/célpont.
- Új függőség M2-ben: `blake3`.

**Nyitott kérdések**
- Ha a macOS CI-futó tartósan nem indul, dönteni kell: marad kötelező (NFR-04) és kivárjuk, vagy a macOS ellenőrzés külön, kézi indítású workflow-ba kerül.

---

## 2026-10-21 – Költözés az otthoni gépre, GitHub bekötve

**Kész**
- A projekt átkerült az otthoni gépre: `C:\Users\kaposzta\Desktop\Project\kapu_file\kapu_file` (git gyökér). Windows 11 Pro, a felhasználó rendszergazda (UAC-emeléssel).
- Rust újratelepítve: rustup 1.29.2, a `rust-toolchain.toml` szerinti 1.99.1 MSVC toolchain, nightly 1.101.0 (csak fuzzhoz). cargo-deny 0.20.2, cargo-audit 0.22.2, cargo-fuzz 0.13.2.
- C/C++ toolchain: Visual Studio 2026 Community (18.10), MSVC 14.51. A gépen hiányzott a Windows SDK és az asztali CRT (`lib\x64`), ezért a `link.exe` nem linkelt; a VS Installerrel pótolva: `Windows11SDK.26100` (a D:\Windows Kits\10 alá települt) és `VC.Tools.x86.x64`.
- GitHub: a `gh` be van jelentkezve (piszabolcs). A privát `piszabolcs/kapu` repó `origin`-ként bekötve; a helyi két commit rebase-elve a repó kezdő commitja (MIT `LICENSE`) fölé, a történet lineáris.
- A TERV.md 0. fejezete frissítve az új gép tényeire.

**Ellenőrzés (2026-10-21, lefuttatva az új gépen)**
- `cargo build --workspace` tiszta buildből zöld, a három bináris fut és kiírja a verziót.
- `scripts/check.ps1` zöld: fmt, clippy `-D warnings`, 2 teszt, deny (advisories, bans, licenses, sources ok), audit (1348 advisory, 4 crate).

**Következő: M1 – tesztvektorok** (lásd lent, változatlan). Előtte push az `origin main`-re, ezzel a CI mátrix is élesedik.

---

## 2026-10-21 – M0 kész: repó, toolchain, minőségi kapuk

**Kész**
- Rust stable 1.99.1 (MSVC) telepítve rustup-pal a felhasználói profilba; `rust-toolchain.toml` rögzíti.
- cargo-deny 0.20.2, cargo-audit 0.22.2, cargo-fuzz 0.13.2 és egy minimális nightly (1.101.0, csak fuzzhoz) telepítve.
- Git repó a `kapu_file/` gyökérben, `main` ág, helyi szerző: Pintér Szabolcs. `.gitattributes`: minden szöveges fájl LF.
- Cargo workspace: `crates/kapu-core` (lib), `crates/kapu-cli` (`kapu`), `crates/kapu-mcp` (`kapu-mcp`), `crates/kapu-bench` (`kapu-bench`). Mind a négy fordul, a binárisok futnak és kiírják a verziót.
- NFR-03 lintek a workspace szintjén: `unsafe_code = forbid`, clippy pedantic, `unwrap_used`/`expect_used`/`panic`/`unimplemented`/`indexing_slicing` = deny (tesztmodulokban engedélyezve).
- `deny.toml`: licenc-allowlist (MIT, Apache-2.0, BSD, CC0, ISC, Unicode, Zlib, BSL-1.0, MPL-2.0), csak crates.io forrás, yanked tiltva. A workspace-en belüli path-függőségek verzió nélkül engedélyezve (`allow-wildcard-paths`), mert a cargo-deny 0.20 a `workspace = true` alakot nem tudja feloldani; ezért a crate-ek közvetlen `path = "../kapu-core"` hivatkozást használnak.
- `scripts/check.ps1` és `scripts/check.sh`: fmt, clippy `-D warnings`, test, deny, audit.
- `.github/workflows/ci.yml`: Windows/Linux/macOS mátrix + deny + audit. Akkor fut, amikor a repó GitHubon lesz.
- `DONTESNAPLO.md`: D31–D46 (a TERV.md 2. fejezetének pontosításai, a 2.5/2.10/2.13 pontok a felhasználó jóváhagyásával).
- `TERV.md`: a teljes M0–M9 terv.

**Ellenőrzés (2026-10-21, lefuttatva)**
- `scripts/check.ps1` zöld: fmt ok, clippy `-D warnings` ok, 2 teszt ok, deny ok (csak „license not encountered” figyelmeztetések, mert még nincs külső függőség), audit ok (1348 advisory, 4 crate).
- A három bináris fut: `kapu 0.1.0 (KAPU format 1.0)` stb.

**Következő: M1 – tesztvektorok**
- `tests/vectors/minta.kapu` kézzel (3 szoba, alajtók, linkek, escape), Python segédszkript a `tools/` alá.
- Hibás fájl minden kódhoz (E001–E011) + 2 edge (ujjlenyomat-eltérés, memóriabeli újraépítés).
- Tesztvázak minden Acceptance ponthoz, `#[ignore]`-ral.
- `kapu-bench gen` generátor váza.

**Nyitott kérdések**
- GitHub: `gh auth login` a felhasználónál, utána privát repó + CI. *(Megoldva az otthoni gépen, lásd a fenti bejegyzést.)*
- README nyelve M9-ben dől el.
