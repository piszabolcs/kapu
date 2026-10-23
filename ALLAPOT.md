# KAPU – Állapotnapló

Minden commitnál frissül (TERV.md 8. fejezet). Legfrissebb bejegyzés felül.

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
