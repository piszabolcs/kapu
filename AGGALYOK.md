# KAPU – Aggálylista

Fejlesztés közben talált kérdések, kockázatok és spec-rések a végső áttekintéshez. Három rész: **A) döntést igényel tőled**, **B) nyitott munka** (veled közösen vagy kiadás előtt), **C) elfogadott, megfigyelendő**. Minden pont: hol, mi a gond, mi a mostani állapot, mi a javaslat. A lezárt pontok a döntésnaplóba kerülnek, innen kikerülnek. Utolsó frissítés: 2026-10-23, a közös áttekintés után (a lezárt pontok: A2, A4, A5, A16, A18, A20, A22, A27 → D106–D110, spec v1.0.1).

## A) Döntést igényel

Jelenleg nincs nyitott döntés (a 2026-10-23-i áttekintés döntései: D93, D97, D106–D111).

## B) Nyitott munka

| # | Hol | Mi van hátra | Mostani állapot | Javaslat |
|---|---|---|---|---|
| A21 | **FR-16 Acceptance** (döntés: M8 dönt) | A „üvegszerű, homályos gomb” magyar lekérdezésre a `potion-multilingual-128M` a `backdrop-filter: blur` CSS-szobát nem hozza az első 10-be: a magyar lekérdezés és a kódblokk koszinusza 0,149, egy általános magyar szobáé 0,193 (a Python `model2vec` is ugyanezt adja). Angol lekérdezéssel működik (0,454 kontra 0,028). A statikus modell a magyar→kód átjárásban gyenge. | A teszt `ignored` indoklással (`crates/kapu-core/tests/search.rs`). | M8/5 (D28): saját desztillált HU+EN+kód modell vagy más modell mérése ezen az eseten; ha egyik sem teljesíti, az FR-16 Acceptance módosítása vagy elfogadott korlát. |
| A26 | M8 | Valódi korpusz a te repóidból (M8/1, céges adat szabályai), `kapu bench` rajta (M8/2), kézi élpontosság és csoportnév-ellenőrzés (M8/4), saját desztillált modell (M8/5, D28; a BAAI/bge-m3 tanár kb. 2,3 GB és torch kell). | A mérőeszközök készen állnak (`kapu bench`, `kapu-bench measure/search`, `tools/leiden_check.py`, `tools/crosscheck.py`, `tools/quantize.py`). | Közös munkamenet a korpuszra; utána a mérések egy nap alatt lefuthatnak. |
| A25 | M7, M8/6 | Kézi próba Claude Code-ból és az 1.3 forgatókönyv MCP-naplóval: ehhez a te Claude Code-konfigurációd kell. | A README tartalmazza a beállítást. | `claude mcp add kapu -- <út>\kapu-mcp.exe`, majd az 1.3 prompt egy 30 szobás `skill.kapu`-n. |
| A28 | M9 | A `release.yml` csak `v*` címkére fut, még nem futott (most a `kapu-gui` is benne van, Linuxon WebKitGTK-val); a csomag füsttesztje a repón belül futott; a „tiszta gépen, telepítés nélkül” próba nem történt meg. A CI fuzz-jobja 2026-10-23-án zöld volt. | Megírva, a YAML szintaxis ellenőrizve. | Az első címke után a piszkozat kiadást nézni; a zipet egy másik gépen vagy profilban kipróbálni. |
| A31 | M10 ablak | A `kapu-gui` ablakot csak Windowson futtattam (WebView2); Linuxon és macOS-en a CI csak fordítja és teszteli a parancsréteget. A felületnek nincs automatikus böngészős tesztje a CI-ban; a kattintásos ellenőrzés kézzel, a fejlesztői szerveren történt. | A parancsréteg egységtesztjei zöldek; a képernyőképek a fejlesztői szerverről, a natív ablak Windowson kipróbálva. | Linuxos és macOS-es kézi próba egy gépen; később Playwright-füstteszt a CI-ban a `serve` példán. |
| A6 | NFR-03/5 fuzz | Az 1 órás kapu célpontonként (kiadás előtt) még nem futott; most 10 perc/célpont futott helyben (kb. 26,7 millió bemenet, nincs találat), a CI-ban minden pushnál 30 s/célpont lesz (D104). | Windowson VS AddressSanitizer kell (D86, telepítve). | Kiadás előtt éjszaka: `scripts/fuzz.ps1 -Seconds 3600`. |
| A29 | Push és GitHub | A repó privát; a portfólió-publikálás (D112) a teljes történetet a noreply szerzőre írja át, és `--force-with-lease`-szel pusholja. Nyilvánossá tenni és kiadást címkézni a felhasználó dönt. | Titok-szkennelés tiszta (HIGH = 0). | Nyilvánossá tétel: `gh repo edit piszabolcs/kapu --visibility public --accept-visibility-change-consequences`; a release a `v*` címkével indul (piszkozat). |

## C) Elfogadott, megfigyelendő

| # | Hol | Aggály | Mostani állapot | Javaslat |
|---|---|---|---|---|
| A10 | D21 térkép ára | A generált korpuszon a térkép a törzs tokenjeinek kb. 26%-a, mert a generált szobák kicsik (kb. 80 token). Valódi, 1000+ tokenes szobáknál várhatóan 1–2%. | `bench/REPORT.md` 1. táblázat. | M8-ban a valódi korpuszon mérni. |
| A11 | Fájlméret | A fájl kb. 80%-a az `#INDEX` (kettős tövezés, szoba- és alajtó-posztingok, D41). | Működik. | v1.1: tömörebb posztinglista, spec-változással. |
| A8 | D71 teljes fájl memóriában | Az olvasó minden hívásnál a teljes fájlt beolvassa. | Mérve: 3000 szobánál (24 MB) a térkép 128 ms, négy ajtó nyitása 81 ms. | Elfogadható; ha kell, `bodyhash`-gyorsítótár. |
| A9 | D77 alajtó-újraépítés | Ha egy kézzel szerkesztett szobában a `##` címsorok száma változott, az alajtói a következő buildig nem nyithatók ki. | E007 azonosítónként, a szoba kinyitható. | Elfogadható. |
| A12 | YAKE (D84) | A Python referencia nem implementálja; a Rust egyszerűsített YAKE. | Determinisztikus, tesztelt; a minőség nincs mérve. | M8/4 kézi ellenőrzés. |
| A13 | `\w` Unicode | A Python `\w` és a Rust `is_alphanumeric` ritka írásrendszereknél eltér; magyar és angol szövegnél nem. | Elfogadva. | Csak nem latin tartalomnál kell foglalkozni vele. |
| A15 | BM25 lekérdezés (D85) | Egy szó és a töve külön tagként adódik össze. | FR-09 Acceptance zöld. | M8 recall/MRR alapján dönteni. |
| A17 | Modell betöltése | Az i8 alapmodell (D102) 124 MB, kb. 128 MB memória; a betöltés és az `embhash` kb. 1,2 s CLI-hívásonként (az MCP egyszer). | Elfogadható. | `embhash`-gyorsítótár; kisebb tokenizáló a desztillált modellel. |
| A19 | `paste` (D94) | Karbantartatlan proc-macro a `tokenizers`-en keresztül. | Kivétel a `deny.toml`-ban. | A `tokenizers` frissítéseit figyelni. |
| A23 | Referencia-lefedettség | A Python és a Rust 300 szobán bájtra egyezik (D99); a YAKE-nek és a Leidennek nincs független bájtszintű referenciája. | Saját tesztek, `leidenalg`-összevetés. | M8/4. |
| A24 | MCP szerver futása | A szinkron core az aszinkron kezelőkben fut; egy nagy fájl első hibrid keresése másodpercekig foglalja a szervert. | Egy kliens, stdio: elfogadható. | Több kliensnél `spawn_blocking`. |
| A7 | CI macOS | Az első pushnál a macOS futó elindulás nélkül törlődött, másodjára lefutott. | Kötelező job. | Ha ismétlődik: `timeout-minutes` és újrapróbálás. |
