# KAPU – Aggálylista

Fejlesztés közben talált kérdések, kockázatok és spec-rések a végső áttekintéshez. Három rész: **A) döntést igényel tőled**, **B) nyitott munka** (veled közösen vagy kiadás előtt), **C) elfogadott, megfigyelendő**. Minden pont: hol, mi a gond, mi a mostani állapot, mi a javaslat. A lezárt pontok a döntésnaplóba kerülnek, innen kikerülnek. Utolsó frissítés: 2026-10-23.

## A) Döntést igényel

| # | Hol | Aggály | Mostani állapot | Javaslat |
|---|---|---|---|---|
| A21 | **FR-16 Acceptance** | A „üvegszerű, homályos gomb” magyar lekérdezésre a `potion-multilingual-128M` a `backdrop-filter: blur` CSS-szobát nem hozza az első 10-be: a magyar lekérdezés és a kódblokk koszinusza 0,149, egy általános magyar szobáé 0,193 (a Python `model2vec` is ugyanezt adja). Angol lekérdezéssel működik (0,454 kontra 0,028). A statikus modell a magyar→kód átjárásban gyenge. | A teszt `ignored` indoklással (`crates/kapu-core/tests/search.rs`). | M8/5 (D28): saját desztillált HU+EN+kód modell vagy más modell mérése ezen az eseten; ha egyik sem teljesíti, az FR-16 Acceptance módosítása vagy elfogadott korlát. |
| A16 | D93 `term` élek – **spec-eltérés** | A spec ritkasági szabálya (felső 10% IDF az összes kifejezés között) soha nem ad élt, mert a legritkább kifejezések egyetlen szobában vannak; a spec saját Acceptance-e sem teljesülhet vele. A küszöböt a legalább két szobában előforduló kifejezések között számolom. | Átvezetve a Pythonba és a Rustba; a `minta.kapu` ezért változott. | Jóváhagyás, utána az ASSUMPTION-06 szövegének pontosítása a specben. |
| A22 | D97 vektor-gyorsítótár – **spec-eltérés** | A gyorsítótár kulcsa a teljes törzs, nem az ajtó: 3000 szobánál a meleg keresés 0,16 s az ajtónkénti 1,2 s helyett, de bármely változás után minden ajtóvektor újraszámolódik (kb. 3 s az első keresés). | Mérve, működik. | Jóváhagyás; ha zavaró, kétszintű gyorsítótár. |
| A2 | FR-15/2 keret (D78) | Ha egy szoba tartalmában szerepel a `</kapu-room>` szöveg, az AI felé a keret „bezárhatónak” látszik (prompt injection felület). | A tartalomvizsgálat S6 szabálya jelzi (D101), a keret nem változott. | Spec-döntés: egyedi záró jelölő (pl. véletlen határoló), vagy marad a figyelmeztetés. |
| A4 | FR-13 mező-escape | A `#MAP` mezőiben csak a `\|` escape létezik; egy címben szereplő szó szerinti `\|` kétértelmű. | A Python és a Rust ugyanígy viselkedik. | Spec-pontosítás: a `\` is escape-elődjön (`\\`), vagy a cím nem tartalmazhat `\|`-t. |
| A5 | D59 megjegyzés-eltávolítás | Többsoros `<!-- -->` megjegyzésnél a szabály csak a whitespace-sorokat dobja el, a sortörések száma nem intuitív. | A Python és a Rust bájtra egyezik. | Egyszerűbb szabály a spec v1.1-be. |
| A18 | FR-20 hibakódok | A környezeti hibák (I/O, hiányzó vagy hibás modellfájl) E005-öt kapnak („szakasz ... hiányzik”), ami félrevezető. | E005, részletező szöveggel. | Spec v1.1: külön kód (pl. E012). |
| A20 | Leiden ellenőrzés (D92) | A D43 „modularitás ±0,01” kritériumát úgy értelmezem, hogy a Rust nem lehet 0,01-nél rosszabb a `leidenalg`-nál (a generált korpuszon 30 és 300 szobánál a Rust jobb, 3000-nél 0,007-tel rosszabb). | Mérve. | Elfogadás vagy szigorúbb kritérium. |
| A27 | README nyelve (D32) | A README angol; a portfólió-szokásod kétnyelvű (EN/HU). | Angol. | Angol marad, vagy kétnyelvű. |

## B) Nyitott munka

| # | Hol | Mi van hátra | Mostani állapot | Javaslat |
|---|---|---|---|---|
| A26 | M8 | Valódi korpusz a te repóidból (M8/1, céges adat szabályai), `kapu bench` rajta (M8/2), kézi élpontosság és csoportnév-ellenőrzés (M8/4), saját desztillált modell (M8/5, D28; a BAAI/bge-m3 tanár kb. 2,3 GB és torch kell). | A mérőeszközök készen állnak (`kapu bench`, `kapu-bench measure/search`, `tools/leiden_check.py`, `tools/crosscheck.py`, `tools/quantize.py`). | Közös munkamenet a korpuszra; utána a mérések egy nap alatt lefuthatnak. |
| A25 | M7, M8/6 | Kézi próba Claude Code-ból és az 1.3 forgatókönyv MCP-naplóval: ehhez a te Claude Code-konfigurációd kell. | A README tartalmazza a beállítást. | `claude mcp add kapu -- <út>\kapu-mcp.exe`, majd az 1.3 prompt egy 30 szobás `skill.kapu`-n. |
| A28 | M9 | A `release.yml` és a CI új fuzz-jobja csak pushkor fut; a csomag füsttesztje a repón belül futott; a „tiszta gépen, telepítés nélkül” próba nem történt meg. | Megírva, a YAML szintaxis ellenőrizve. | Push után a CI-t nézni; a zipet egy másik gépen vagy profilban kipróbálni. |
| A6 | NFR-03/5 fuzz | Az 1 órás kapu célpontonként (kiadás előtt) még nem futott; most 10 perc/célpont futott helyben, a CI-ban minden pushnál 30 s/célpont lesz (D104). | Windowson VS AddressSanitizer kell (D86, telepítve). | Kiadás előtt éjszaka: `scripts/fuzz.ps1 -Seconds 3600`. |
| A29 | Push és GitHub | A helyi `main` sok committal jár a GitHub előtt; push és kiadás csak kérésre (D35). | Titok-szkennelés tiszta. | A végső áttekintés után push; a release a `v*` címkével indul (piszkozat). |

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
