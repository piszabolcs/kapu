# KAPU mérési riport

A TERV.md 5. fejezete szerinti mérések. Minden táblázatnál: dátum, gép, parancs. A számok a szintetikus korpuszra vonatkoznak, amíg a valódi korpusz (M8/1) el nem készül.

## 1. Build, fájlméret, térkép/törzs tokenarány (M3, modell nélkül)

- Dátum: 2026-10-23. Gép: otthoni Windows 11 Pro, release build (`lto`, `codegen-units = 1`).
- Parancs: `kapu-bench measure --rooms 30,300,3000 --runs 3` (seed 42, a `kapu-bench gen` korpusza memóriában).
- A build modell nélkül fut (`emb=none`, D57): nincs vektorszámítás, nincs `sim` él, a csoportok összefüggő komponensek (D66). Az M4 után a táblázat újramérendő.

| Szobák | Ajtók | Fájl (KiB) | Build (ms, medián) | Törzs (token) | Térkép (token) | Térkép/törzs |
|---:|---:|---:|---:|---:|---:|---:|
| 30 | 122 | 234.9 | 21 | 9660 | 2491 | 0.258 |
| 300 | 1192 | 2307.6 | 183 | 92357 | 23863 | 0.258 |
| 3000 | 12024 | 24031.4 | 2064 | 925495 | 250180 | 0.270 |

Megjegyzések:

1. Az első mérésen a 3000 szobás build 64 s volt, mert a `term` élek minden szobapárt összevetettek (négyzetes). Az invertált változat (ritka kifejezésenként a szobapárok) ugyanazt az eredményt adja 2,1 s alatt; a skálázás most közel lineáris.
2. A térkép/törzs arány a generált korpuszon 26–27%, mert a generált szobák kicsik (átlag kb. 80 token, alajtókkal együtt kb. 4 ajtó szobánként). Valódi skill-szobákon ez várhatóan jóval kisebb; lásd AGGALYOK A10.
3. A fájl kb. 80%-a az `#INDEX` szakasz (kettős tövezés, szoba- és alajtó-posztingok); lásd AGGALYOK A11.

## 2. Gráf és csoportok a vektormodellel (M4, frissítve D110 után)

- Dátum: 2026-10-23. Gép és build mint az 1. táblázatnál. Modell: `potion-multilingual-128M-i8` (az alapmodell, D102).
- Parancs: `kapu-bench measure --rooms 30,300,3000 --runs 3 --model models/potion-multilingual-128M-i8 --dump-graph <mappa>`; Leiden-ellenőrzés: `tools/leiden_check.py <mappa>/graph-<n>.tsv`.
- Leiden: 8 indítás (mag 42–49), indításonként a konvergenciáig ismételt optimalizálás, a legnagyobb modularitás nyer (D110).

| Szobák | Build (ms, medián) | Fájl (KiB) | link él | term él | sim él | Csoportok | Legnagyobb csoport | Medián csoport | Modularitás (Rust) | leidenalg, mag 42 | leidenalg, legjobb a 10 magból |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 30 | 30 | 240.8 | 30 | 25 | 113 | 4 | 10 | 9 | 0.2664 | 0.2330 | 0.2330 |
| 300 | 323 | 2375.2 | 300 | 260 | 1298 | 7 | 61 | 48 | 0.3601 | 0.3421 | 0.3584 |
| 3000 | 5594 | 24696.8 | 3000 | 1308 | 13306 | 8 | 773 | 486 | 0.4153 | 0.3949 | 0.4171 |

Megjegyzések:

1. A D110 kritérium (a Rust nem lehet 0,005-nél rosszabb a `leidenalg` 42-es magjánál) mindhárom méreten bőven teljesül; a `leidenalg` 10 magból vett legjobbjától is legfeljebb 0,002-vel marad el. Az első változat (egy optimalizálási kör, egy mag) 300 szobánál −0,011-gyel, a második (két kör, egy mag) 3000 szobánál −0,007-tel maradt el.
2. Az eredmény a csúcsok sorrendjétől is függ: ugyanazon a gráfon, a térkép sorrendjében indexelve a Rust 3000 szobánál 0,4114-et ad (a `leidenalg` 42-es magja 0,3949). A build mindig a törzs sorrendjét használja, így a fájl determinisztikus.
3. A `sim` élek száma kb. 4,4 szobánként (k = 5, küszöb 0,6): a generált korpusz szókincse kicsi és ismétlődő.
4. A 3000 szobás build modellel 5,6 s, nagy része a `sim` élek páronkénti koszinusza (négyzetes); a Leiden kb. 0,4 s.
5. A karateklub-gráfon (34 csúcs) a Rust a `leidenalg` 0,4198-ja − 0,005 felett van (egységteszt).

## 3. Keresési válaszidő (M5)

- Dátum: 2026-10-23. Gép mint fent. Parancs: `kapu-bench search --rooms 30,300,3000 --model models/potion-multilingual-128M` (lekérdezés: „üvegszerű homályos gomb”, k=10, három futás mediánja; a modell betöltése nincs benne).
- „Hideg”: üres vektor-gyorsítótár, minden ajtó vektorát ki kell számolni és el kell menteni. „Meleg”: a fájl törzséhez tartozó gyorsítótárfájl megvan.

| Szobák | Ajtók | BM25 (ms) | Hibrid, hideg (ms) | Hibrid, meleg (ms) |
|---:|---:|---:|---:|---:|
| 30 | 122 | 1 | 29 | 1 |
| 300 | 1192 | 11 | 249 | 12 |
| 3000 | 12024 | 138 | 3313 | 162 |

Megjegyzések:

1. Az első változat ajtónként külön gyorsítótárfájlt írt: 3000 szobánál hidegen 12,5 s, melegen 1,2 s (12 ezer apró fájl). A törzsenkénti egyetlen gyorsítótárfájllal (D97) ez 3,3 s és 0,16 s.
2. A hideg idő nagy része a 12 ezer ajtó tokenizálása és átlagolása. Az MCP szerver ezt a fájl első megnyitásakor egyszer fizeti meg.
3. A modell betöltése (512 MB f32, kb. 0,5–1 s) a CLI minden modellt használó hívásánál hozzáadódik; lásd AGGALYOK A17.

## 4. Modell: f32 kontra i8, térkép és kinyitás válaszideje (M8/3, M8/5 első rész)

- Dátum: 2026-10-23. Parancs: `kapu-bench search --rooms 30,300,3000 --model models/<modell>`; az i8 modell a `tools/quantize.py` kimenete. Mérés közben egy fuzz-futás is ment a háttérben, a számok ezért legfeljebb ±20%-kal zajosak.
- Vektorhűség: soronként a koszinusz az f32-höz min. 0,99976, átlag 0,99996; a 6 referenciaszövegen a kódolt vektorok koszinusza ≥ 0,99996 (`tests/model.rs`).

| Modell | Fájl | Memória (mátrix) | Betöltés + embhash (ms) |
|---|---:|---:|---:|
| potion-multilingual-128M (f32) | 512 MB + 18,6 MB tokenizáló | kb. 512 MB | 1422 |
| potion-multilingual-128M-i8 | 124 MB + 18,6 MB tokenizáló | kb. 128 MB | 1168 |

| Szobák | Ajtók | Térkép (ms) | 4 ajtó nyitása (ms) | BM25 (ms) | Hibrid hideg, f32 / i8 (ms) | Hibrid meleg, f32 / i8 (ms) |
|---:|---:|---:|---:|---:|---:|---:|
| 30 | 122 | 0,9 | 0,6 | 1 | 27 / 27 | 2 / 1 |
| 300 | 1192 | 8,4 | 5,9 | 12 | 240 / 258 | 12 / 19 |
| 3000 | 12024 | 127,5 | 81,0 | 145 | 3352 / 3205 | 213 / 141 |

Következtetés: az i8 modell negyedakkora, a vektorai gyakorlatilag azonosak, a sebesség nem változik; a csomagolt alapmodell az i8 lett (D102). A térkép és a négyajtós kinyitás 3000 szobán is 150 ms alatt van; ennek nagy része a 24 MB-os fájl beolvasása és szakaszellenőrzése (AGGALYOK A8).

## 5. Valódi korpusz: telepített skillek és frontend-mintakódok (M8, 2026-10-23)

- Gép és build mint fent, release. Modell: `potion-multilingual-128M-i8`.
- Korpusz (helyben, `bench/korpusz/`, nincs gitben, mert harmadik féltől származó szöveg): a gépen telepített Claude-skillek (`tools/collect_skills.py`: anthropic-skills 52, superpowers 56, figma 101, claude-md-management 4, github-portfolio-publisher 3, frontend-design 1 szoba; pluginonként a legfrissebb verzió, érzékeny kifejezés szűrve, 0 kizárva) és 34 saját írású frontend-mintakód-szoba (28 angol, 6 magyar, források MDN, web.dev, caniuse).
- Parancs: `kapu import bench/korpusz/rooms bench/korpusz/kapu-corpus.kapu --model models/potion-multilingual-128M-i8 --json`.

| Mérőszám | Érték |
|---|---:|
| Szobák / ajtók / csoportok | 251 / 1960 / 10 |
| Törzs (token, o200k_base) | 579 794 |
| Fájl (bájt / token) | 33 392 880 / 9 961 477 |
| Build modellel | 4,8 s |
| `validate --full` (újraépítés-összevetéssel) | 6,1 s, hibátlan |
| Második build | bájtra azonos (NFR-02) |
| Első térképhívás (útmutató, csoportlista, tartalomvizsgálat) | 762 token (D122 előtt 2971) |
| Későbbi térképhívás | 205 token |
| Egy csoport ajtólistája | 2 609 - 15 756 token, összesen 61 317 (a törzs 10,6%-a) |
| W003 (link hiányzó ajtóra) | 274, mind fájlon belüli horgony (AGGALYOK A32) |

Csoportnevek a D121 előtt és után (ugyanaz a beosztás): „the, and, css” → „css, rem, element”; „the, and, color” → „color, figma, css”; „motion, the, node” → „motion, node, keyframe”; „the, and, superpowers” → „superpowers, plan, task”; „the, skill, and” → „skill, claude, skills”; „the, and, server” → „server, mcp, modelcontextprotocol”.

## 6. Keresési minőség a valódi korpuszon (`kapu bench`, M8/2)

- Kérdések: 118 (`bench/korpusz/all-questions.jsonl`): 50 a skillekre (én írtam, a tartalom ismeretében), 68 a frontend-szobákra (a szobákat író ügynök írta, a cím és az API neve nélkül); 60 magyar, 58 angol; 32 egy alajtóra, 86 egy szobára mutat. Torzítás: a kérdések szerzői ismerték a tartalmat; a te repóidból készülő korpusz (A26) ezt nem váltja ki.
- Szigorú: a várt ajtó a top k-ban; megengedő: a várt ajtó vagy a szobája (D49).

| Kérdések | n | BM25 r@10 | vektor r@10 | hibrid r@5 | hibrid r@10 | hibrid MRR@10 | hibrid megengedő r@10 |
|---|---:|---:|---:|---:|---:|---:|---:|
| összes | 118 | 0,58 | 0,55 | 0,58 | 0,65 | 0,42 | 0,68 |
| angol | 58 | 0,88 | 0,64 | 0,81 | 0,86 | 0,62 | 0,88 |
| magyar | 60 | 0,28 | 0,47 | 0,35 | 0,45 | 0,23 | 0,48 |
| skillek | 50 | 0,62 | 0,60 | 0,60 | 0,72 | 0,43 | 0,74 |
| frontend | 68 | 0,54 | 0,51 | 0,56 | 0,60 | 0,41 | 0,63 |
| szobára mutat | 86 | 0,62 | 0,59 | 0,63 | 0,70 | 0,47 | 0,70 |
| alajtóra mutat | 32 | 0,47 | 0,44 | 0,44 | 0,53 | 0,27 | 0,62 |

Megjegyzések:

1. A hibrid rangsor összességében mindkét önálló rangsornál jobb. Angol kérdésen a BM25 egymagában is erős (r@10 0,88), a hibrid ezt tartja (0,86).
2. A magyar kérdés angol szövegre a gyenge pont: a BM25 a nyelvhatáron nem lát át (0,28), a statikus többnyelvű modell részben igen (0,47), a hibrid ebből 0,45-öt tart meg. Ez az A21 és az M8/5 (modellválasztás, desztillált modell) mérhető célja.
3. Csak a skillekkel (217 szoba, 50 kérdés) a hibrid r@10 0,80 volt; a 34 frontend-szoba új zavaró jelöltként 0,72-re vitte le.

## 7. MCP szerver: válaszidő és válaszméret (`tools/mcp_probe.py`)

- Hivatalos Python MCP kliens (`mcp` 2.3) stdio-n, egy munkamenet, a 118 kérdés keresésként, az első 25 találat kinyitva és leírva. Tokenek o200k_base szerint.

| Eszköz | Medián idő | p90 idő | Válasz (token, medián) | Megjegyzés |
|---|---:|---:|---:|---|
| initialize | 140 ms | | | |
| `kapu_map` | 81 ms | 85 ms | 205 | csoportlista; csoporttal 71 ms és 5 740 token (g1) |
| `kapu_search` (k=5) | 138 ms | 141 ms | 178 | az első hívás 1,29 s (modellbetöltés) |
| `kapu_open` (1 ajtó) | 71 ms | 75 ms | 1 739 | p90 2 877, max 6 209 token |
| `kapu_describe` (1 ajtó) | 70 ms | 72 ms | 53 | |

Az eszközleírások tokenje: `kapu_map` 297 (benne az std-1 útmutató), `kapu_search` 128, `kapu_open` 98, `kapu_write` 100, `kapu_describe` 37. Minden eszköz annotált (D120).

## 8. Élő próba: Claude-ügynökök csak a KAPU MCP eszközeivel (mcp-builder értékelés)

- 10 kérdés az mcp-builder formátumában (`bench/korpusz/mcp-evaluation.xml`), egyértelmű, ellenőrizhető válasszal. 5 Claude Sonnet ügynök, ügynökönként 2 kérdés; csak `tools/kapu_mcp_call.py`-on át érhették el a szervert (valódi MCP-hívás, naplózva), a fájlt közvetlenül nem olvashatták, `kapu_write` tiltva.
- A `claude -p` alapú, teljesen elszigetelt futtatás (`tools/mcp_eval.py`) kész, de a gép önálló Claude CLI-bejelentkezése lejárt (401); újra kell futtatni a bejelentkezés után (A25).
- A burkoló minden hívásnál új szervert indít, ezért az útmutató és a tartalomvizsgálat minden térképhívásban megjelent; valódi munkamenetben egyszer.

| # | Téma | Helyes | Hívások | Eszközök | Válaszok tokenje |
|---:|---|:---:|---:|---|---:|
| 1 | terrakotta helyett gombszín (design-referencia) | igen | 5 | search, search, map, search, open | 2 142 |
| 2 | hibakeresés: melyik fázis előzi meg a javítást | igen | 2 | search, open | 3 033 |
| 3 | táblázat-képletek ellenőrző szkriptje | igen | 2 | search, open | 712 |
| 4 | MCP lapozás alapértelmezett mérete | igen | 2 | search, open | 2 358 |
| 5 | README jelvények maximuma | igen | 2 | search, open | 2 335 |
| 6 | az egyetlen magyar skill azonosítója | igen | 3 | map, map, describe | 8 788 |
| 7 | hány szoba van összesen | igen | 2 | map, map | 5 061 |
| 8 | anchor positioning: melyik böngészőverzió | igen | 2 | search, open | 686 |
| 9 | `scheduler.yield()`: melyik böngésző hiányzik | igen | 2 | search, open | 493 |
| 10 | magyar kérdés: AbortController szoba | igen | 1 | search | 221 |

Eredmény: 10/10 helyes, mediánban 2 hívás és kb. 2,2 ezer token kérdésenként, miközben a törzs 579 794, a fájl 9,96 millió token. Az ügynökök visszajelzései (AGGALYOK A33-A36): a keresési találatok mellé rövid kivonat kellene; egy csoport ajtólistája túl hosszú; a kinyitáskor kapott szomszédlista zajos; a térképről hiányzik az összes szoba száma.

## 9. Spec v1.0.2 és a teljesen angol mérés (2026-10-23)

A felhasználó kérésére a publikált mérés teljesen angol: angol korpusz (a 7 magyar szoba nélkül: 216 skill + 28 frontend-szoba, 244 szoba) és csak angol kérdések (77). A teljes angol riport: [RESULTS.md](RESULTS.md), nyers számok: `results/2026-10-23-english-corpus.json`. A magyar kérdés angol szövegre külön szál (A21, 6. fejezet).

| Mérőszám | Érték |
|---|---:|
| Törzs (karakter / token) | 2 350 156 / 573 733 |
| Teljes fájl (karakter / token) | 32 976 901 / 9 809 442 (90% index) |
| Build / teljes ellenőrzés | 4,6 s / 3,3 s, bájtazonos második build, 0 figyelmeztetés |
| Hibrid r@5 / r@10 / MRR@10 (77 angol kérdés) | 0,84 / 0,90 / 0,67 (BM25: 0,83 / 0,90 / 0,68; vektor: 0,64 / 0,73 / 0,47) |
| Térkép (első / később) | 777 / 220 token, 80 ms |
| Csoportlista (v1.0.2, csak szobák) | 399 - 2 005 token, a 10 együtt 9 698 |
| Keresés kivonatokkal (k=5) | 333 token, 280 ms medián |
| Kinyitás egy ajtó | 1 655 token medián, 71 ms |
| Élő próba (5 Sonnet-ügynök, 10 angol kérdés) | 10/10 helyes, mediánban 842 token és 1,5 hívás kérdésenként; 5 válasz a kivonatból, kinyitás nélkül |

A v1.0.2 hatása a vegyes korpuszon mérve (a 7. és 8. fejezethez képest): a legnagyobb csoportlista 15 756 → 2 005 token (D125), a W003 274 → 0 (D123, 113 új `link` él), az első térképhívás 2 971 → 777 token (D122), az élő próba mediánja 2 238 → 842 token (D124; más korpusz, a 10 kérdésből 8 azonos). A keresés minősége a v1.0.2-től nem változott (a gráf nem vesz részt a rangsorban).

## 10. Élő próba egy valódi tudásfájlon: PixelLab, amatőr indie-fejlesztő szerepben (2026-10-23)

- **Tudásfájl:** a felhasználó saját `pixellab.kapu` fájlja: a PixelLab teljes dokumentációja (súgóoldalak, REST API v2 referencia, MCP-eszközök, Python SDK) és a hivatkozott videók átirata. 154 szoba, 7 csoport, kb. 206 900 token tudás, 7,1 MB. Harmadik fél tartalma, nincs a repóban.
- **Forgatókönyv:** "Pixel Paws", felülnézetes indie játék egy kutya főszereplővel; a csapat teljesen kezdő. 12 kérés: 9 magyarul, 3 angolul, kezdő szóhasználattal (pl. "minden irányból ugyanúgy néz ki", "hogyan szedjük le a hátteret").
- **Futtatás:** 4 Claude Sonnet ügynök, ügynökönként 3 kérés, csak a `tools/kapu_mcp_call.py` burkolón át (valódi MCP, `kapu_write` tiltva, a fájl közvetlenül nem olvasható). Spec v1.0.3.
- **Bírálat:** egy független Sonnet-ügynök minden hivatkozott ajtót kinyitott (és keresett is), a válaszokat állításokra bontotta, és mindegyiket a forrásszöveghez mérte.

| # | Kérés (rövidítve) | Nyelv | Hívások | Token | Helyes | Alátámasztott állítás | Hasznosság (1-5) |
|---:|---|:---:|---:|---:|:---:|---:|---:|
| 1 | hogyan használható egyáltalán (web, Aseprite, más) | HU | 3 | 2 167 | igen | 7/7 | 4 |
| 2 | 8 irányú kutya felülnézetes játékhoz | HU | 4 | 3 323 | igen | 12/12 | 4 |
| 3 | séta animáció, hány képkocka | HU | 4 | 5 355 | igen | 10/10 | 4 |
| 4 | összeillő fű- és földút-csempék | EN | 5 | 3 984 | igen | 14/15 | 3 |
| 5 | egységes stílus minden képen | HU | 3 | 2 580 | igen | 10/10 | 5 |
| 6 | Aseprite-bővítmény telepítése | HU | 2 | 1 092 | igen | 10/10 | 5 |
| 7 | Claude Code + Godot + PixelLab összekötése | EN | 3 | 2 667 | igen | 11/12 | 4 |
| 8 | ütés animáció a kutyának | HU | 5 | 6 662 | igen | 9/9 | 3 |
| 9 | saját kutyafotóból pixel art | HU | 2 | 1 557 | igen | 10/10 | 4 |
| 10 | átlátszó háttér a sprite-okon | HU | 2 | 1 120 | igen | 8/8 | 4 |
| 11 | UI-gombok és életerő-csík | EN | 2 | 1 721 | igen | 14/14 | 4 |
| 12 | 50 féle tárgy programból: API, hitelesítés | HU | 4 | 2 725 | részben | 12/13 | 3 |

Eredmény: 11 helyes és 1 részben helyes válasz; a 130 állításból 127 alátámasztott, kitalált csomagnév, szám vagy felületelem nem volt. Kérésenként mediánban 3 hívás és 2 624 token (átlag 2 913, legtöbb 6 662), a 12 kérés együtt 34 953 token, a tudás 206 900 tokenjéhez képest kérésenként kb. 1/80. Hívások: 17 keresés (medián 415 token), 18 kinyitás (medián 1 291 token), 4 térkép (293 token, útmutatóval). A keresés 1,3 s mediánja a burkoló miatt magas: minden hívás új szervert indít, és újratölti a vektormodellt (valódi munkamenetben 0,3 s körül).

Megfigyelések:

- **Nyelv:** mind a 17 keresési lekérdezés angol volt, a 9 magyar kérésnél is: az ügynök maga fordított a tartalom nyelvére. A 6. fejezet 0,45-ös magyar r@10 értéke nyers magyar lekérdezésekre vonatkozik; ügynökön át ez a helyzet ezen a próbán nem állt elő (A21).
- **A részben helyes válasz (12) olvasási hiba, nem keresési:** a kinyitott API-ajtó leírja, hogy a `size` szabja meg, hány tárgy jön egy hívásból (≤42 px → 64, ≤85 → 16, ≤170 → 4), és hogy `item_descriptions`-szel tárgyanként külön leírás adható; az ügynök ezt nem vette észre, és hívásonként egy tárgyból számolt költséget.
- **"Ezt a dokumentáció nem mondja meg":** két válasz (3: a kutya sétájának képkockaszáma, 8: az `attack` sablon négylábúra) ezt jelezte, és a bíráló kereséssel igazolta, hogy tényleg nincs benne.
- **Hiányok:** a 4. válasz csak az MCP-utat írta le (a webes "Create tileset" eszközt nem), a 7. nem említette, hogy az MCP-hez előfizetés kell, a 8. csak MCP-szintaxist adott a Characters oldal egykattintásos sablonja helyett. Az amatőr szemszögből ezek a leggyengébb pontok (hasznosság 3).
- **Csoportnevek:** az API-referencia csoportja "image, optional, str", egy másiké "pixel, pixellab, let"; a YAKE a paramétertáblák szavait és a "let" szót is kulcsszónak veszi (A41).

Nyers számok: `results/2026-10-23-pixellab-live.json`.
