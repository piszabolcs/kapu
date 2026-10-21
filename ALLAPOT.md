# KAPU – Állapotnapló

Minden commitnál frissül (TERV.md 8. fejezet). Legfrissebb bejegyzés felül.

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
