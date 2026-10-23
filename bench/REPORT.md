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
