# KAPU arXiv-tanulmány: kutatási terv (vázlat)

- **Állapot:** vázlat, 2026-10-23. Amíg a tervet jóvá nem hagyod, semmi nem épül belőle.
- **Nyitott kérdések:** a 11. fejezetben vannak, fontossági sorrendben.
- **Olvasási útmutató:** a „(saját döntés)” jelölésű pontokat én választottam, ezek felülírhatók. A „(tőled)” jelölésűeket te mondtad.

## 1. Cél és miért

A cél a KAPU-t, az egyfájlos, ügynökök által bejárható tudáskonténert, nyilvános és megismételhető mérésekkel összevetni a ma szokásos megoldásokkal, az eredményt pedig arXiv-preprintként közzétenni. A te szavaiddal: „ez nagyon hasznos lehet nagyon sok embernek” (tőled).

### Ami eddig van

Mérnöki mérések a `bench/REPORT.md` 1–10. fejezetében és a `bench/RESULTS.md`-ben:

- angol korpusz: 244 szoba, hibrid recall@10 0,90;
- élő próba: 10/10 helyes válasz, mediánban 842 token kérdésenként;
- PixelLab-próba (kb. 207 ezer token tudás): 11 helyes és 1 részben helyes válasz, mediánban 2 624 token kérésenként.

### Miért kevés ez egy tanulmányhoz

1. **Nincs összehasonlítás.** Nem tudjuk, mennyit ér a KAPU a teljes szöveg beolvasásához, a darabolós RAG-hoz vagy egy ügynök fájlkereséséhez képest.
2. **A kérdéseket a tartalmat ismerők írták.** Ez felfelé torzít.
3. **Szűk a mérés:** egy modell, egy futás, konfidenciaintervallum nélkül.
4. **A korpuszok nem nyilvánosak** (harmadik fél tartalma), ezért a mérést más nem tudja megismételni.

## 2. Állítás és hipotézisek

### Fő állítás

Ezt kell igazolni vagy cáfolni. Egy ügynök, amely egy egyfájlos tudáskonténert jár be (térkép tokenárakkal, hibrid keresés kivonatsorokkal, kinyitás azonosító szerint):

- nagyjából olyan pontosan válaszol dokumentációs kérdésekre, mintha a teljes dokumentációt elolvasta volna;
- közben a tokenek kis, a tudás méretétől alig függő hányadát használja;
- azonos pontosság mellett kevesebb tokennel és hívással dolgozik, mint a fájlrendszeres keresés vagy a darabolós RAG.

### Hipotézisek

A fő futás előtt rögzítjük őket (5.5).

| # | Hipotézis | Hogyan mérjük |
|---|---|---|
| H1 | **Skála.** A KAPU token/kérdés értéke a korpusz méretével (S → L, kb. 50-szeres) legfeljebb lassan nő; a teljes kontextusé lineárisan nő. | token/kérdés a korpuszméret függvényében, log-log görbe |
| H2 | **Pontosság.** Ahol a teljes szöveg befér a kontextusba (S), a KAPU pontossága legfeljebb 5 százalékponttal marad el a teljes kontextusétól, és nem rosszabb a darabolós RAG-énál. | non-inferiority próba előre rögzített 5 pontos határral (saját döntés) |
| H3 | **Hatékonyság.** Azonos pontosság mellett a KAPU kevesebb tokent és eszközhívást használ, mint az ügynöki darabolós RAG (C2b), a grep/read alapú fájlkeresés vagy az `llms.txt`. | páros összevetés kérdésenként |
| H4 | **Mi számít.** A kivonatsorok és a térkép tokenárai csökkentik a fogyasztást. A vektoros keresés angol kérdésnél angol szövegen keveset ad hozzá, nyelvközi esetben sokat. | ablációk (5.3) |
| H5 | **Nyelvközi (opcionális).** Ha magyar kérdés angol tartalomra ügynökön át megy (az ügynök maga fordít), közel ugyanolyan pontos, mint az angol kérdés; nyers magyar lekérdezésnél nem. | magyar kérdéssor, két mód (11. kérdés) |

A H4 tokenár-ablációja a KAPU egyik valóban sajátos ötletét méri: azt, hogy az ügynök a kinyitás előtt látja, mennyibe kerül egy ajtó.

## 3. Célok, nem célok, kerülendők

### Célok

- Tisztességes, megismételhető összehasonlítás: ugyanaz az ügynökhurok, ugyanaz a modell és ugyanaz a hívási keret minden rendszernél.
- Nyilvános kód, kérdéssorok, nyers naplók és bírálatok.
- Egyetlen, jól körülhatárolt, kísérletekkel alátámasztott cikk.

### Nem célok

- Új beágyazó modell, vagy a magyar modellválasztás (M8/5) megoldása.
- Csúcseredmény egy általános retrieval-benchmarkon.
- Lektorált konferenciacikk most. Később lehet, de ez a sablont befolyásolja (11. kérdés).

### Kerülendő (ezek kudarcnak számítanának)

- **Túlzó állítás.** Például „10 millió tokenes tudás 200 tokenért”, kontextus nélkül. A keresési válasz ára nem azonos egy megválaszolt kérdés árával.
- **Csak saját kérdések** és alulhangolt összehasonlító rendszerek („szalmabáb”).
- **Áttekintő vagy állásfoglaló cikk.** Az arXiv CS ilyet csak lektorálás után fogad el (4. fejezet).
- **Sűrű, gépiesen írt szöveg ellenőrzés nélkül.** Az arXiv 2026-os szigorításai pont az ilyen cikkek miatt születtek.

## 4. Kötöttségek

### arXiv (2026-10-23-án ellenőrizve)

- **Ajánlás (endorsement).** Az első beküldéshez a választott kategóriában ajánlás kell egy ott publikáló szerzőtől. Az intézményi e-mail cím ma már csak gyorsítja a folyamatot, önmagában nem elég.
- **Áttekintő és állásfoglaló cikkek.** A CS kategória 2025 októbere óta ezeket csak lektorálás után fogadja el. Kísérletes kutatási cikkre ez nem vonatkozik.
- **Beküldési korlát.** 2026. október 1-től beküldőnként havonta legfeljebb 2 beküldés, és egyszerre legfeljebb 3 aktív beküldés lehet.
- **MI-használat.** Kutatási eszközként megengedett, ha jelzik, és a munka megfelel a szabályoknak.
- **Nyelv.** Teljes angol változat kell (ezt másodlagos forrás állítja). A cikk angol lesz.

### Repó, licenc, költség, gép

- **Repó.** Jelenleg privát (A29). A cikkhez nyilvánosnak kell lennie, címkézett verzióval, mert a mérés csak így ismételhető meg.
- **Licenc.** A csomagba csak szabad licencű korpusz kerülhet. A PixelLab-anyag és a skill-korpusz nem.
- **Költség.** API-tokenek, a nagyságrend az 5.7 fejezetben. A keretet te döntöd el.
- **Gép.** Egy Windows-gép; a GPU nincs tisztázva. Ez a nyílt súlyú modell futtatásánál számít.
- **Commit-szabályok** (tőled): noreply szerző, a commitokban nincs MI-attribúció. Hogy a cikkben hogyan jelezzük az MI-használatot, az külön kérdés (11.).

## 5. Kísérleti terv

### 5.1 Korpuszok

Három méretosztály, mindegyik nyilvános, szabad licencű, angol Markdown/RST dokumentáció (saját döntés):

| Osztály | Tudás (token, cél) | Mire kell | Jelöltek (licenc a befagyasztáskor ellenőrizve) |
|---|---|---|---|
| S | 150–250 ezer | befér egy 1M-es kontextusba: itt van a teljes kontextusú összevetés (H2) | The Rust Programming Language (MIT/Apache-2.0); egy újabb, kevéssé ismert projekt dokumentációja |
| M | 1–3 millió | középső pont a skálagörbén | Godot-dokumentáció (CC BY 3.0); Kubernetes-dokumentáció (CC BY 4.0) |
| L | 10 millió felett | egyetlen kontextusba sem fér bele | MDN Web Docs en-US (szövege CC BY-SA 2.5), vagy több M-korpusz együtt |

A méreteket becsültem; a befagyasztáskor mérjük őket. A népszerű dokumentációkat a modellek tanítóadata is tartalmazhatja, ezért kell a zárt könyves baseline (C0). A többi védelem a 9. fejezetben van.

### 5.2 Kérdéssorok

Három forrásból, hogy a szerzőség független legyen (saját döntés):

| Sor | Forrás | Mire jó |
|---|---|---|
| Q-valós | Nyilvános fórumkérdések (Stack Overflow, GitHub Discussions, a projekt fóruma), ahol az elfogadott válasz a dokumentáció egy oldalára hivatkozik. Az az oldal vagy szakasz lesz az arany ajtó. | Független szerzők, valódi és gyakran kezdő megfogalmazás. A licencet (CC BY-SA) és az adatletöltés feltételeit ellenőrizni kell. |
| Q-gen | Egy nem Claude modellcsalád generál kérdést véletlenszerűen kiválasztott szakaszokból, a szakasz szavait kerülve. Szűrés után egy mintát ember ellenőriz. | Skálázható, és lefedi a dokumentációt. |
| Q-eset | Kis, kézzel írt forgatókönyv amatőr szerepben (mint a Pixel Paws). | Minőségi esettanulmány a cikkben. |

Méret (saját döntés, a pilot után újraszámolva):

- S: 200 kérdés, mert itt van a fő H2-összevetés;
- M és L: 100–100 kérdés.

Ahol lehet, rövid, ellenőrizhető választ kérünk (az mcp-builder formátuma szerint); a forgatókönyvnél szabad szöveget.

**Rétegek.** A kérdések kb. 80%-a egyetlen szakaszra kérdez rá. Kb. 20% több oldalt vagy áttekintést igényel, például „melyik eszközzel kezdjem” vagy „hogyan függ össze A és B”. Erre a rétegre azért van szükség, mert a térkép és a szomszédlista ablációja (5.3) csak ilyen kérdéseken mutathat hatást. Ezeket a kérdéseket főleg a Q-gen és a Q-eset adja (saját döntés).

**Fejlesztői halmaz és teszthalmaz** (saját döntés):

- A pilot és minden hangolás egy külön fejlesztői halmazon fut: 50 kérdés S-en és 20 M-en, ugyanabból a három forrásból.
- A teszthalmazt a pilot nem látja. Az F1-ben befagyasztjuk, és csak az F3-ban nyúlunk hozzá.
- A fő futás alatt nincs spec- vagy kódváltozás.

### 5.3 Rendszerek és ablációk

| Jel | Rendszer | Leírás |
|---|---|---|
| C0 | zárt könyv | Eszköz nélkül. Megmutatja, mennyit tud a modell fejből (kontamináció). |
| C1 | teljes kontextus | A teljes korpusz a promptban, S-en; M-en csak 1M-es kontextusú modellel, opcionálisan. A gyorsítótárazás hatását külön jelentjük. |
| C2a | darabolós RAG, egylépéses | 512 tokenes darabok, 64 átfedés, top-k = 8, egyszeri beillesztés, erős dense beágyazóval. Ez csak pontossági viszonyítás: a tokenköltsége szerkezetéből adódóan állandó, és egyetlen hívásból áll. |
| C2b | darabolós RAG, ügynöki | Ugyanazok a darabok egy keresőeszközként, ugyanabban a hurokban, mint a C3–C5. Ez választja szét a formátum és a felület hatását. Erős beágyazóval minden modellen, a statikus `potion` modellel csak egy modellen (a beágyazó hatása). |
| C3 | ügynöki fájlkeresés | list/grep/read ugyanazon a Markdown-mappán, amelyből a `.kapu` épül (a Claude Code-hoz hasonló munkamód). |
| C4 | `llms.txt` | Generált index linkekkel, és egy read eszköz a lapokhoz. |
| C5 | KAPU MCP | map/search/open/describe, spec befagyasztva. |

Ablációk C5-ön, egy modellel:

1. kivonatsorok nélkül;
2. csak BM25, vektor nélkül;
3. térkép nélkül;
4. tokenárak nélkül (a futtató törli őket a kimenetből);
5. szomszédlista nélkül.

Minden ügynöki rendszer (C2b, C3, C4, C5) közös keretet kap: ugyanaz a rendszerprompt-váz, legfeljebb 12 eszközhívás kérdésenként, ugyanaz a modell és ugyanaz a mintavételi beállítás. Mindegyik rendszer ugyanannyi, dokumentált hangolást kap a pilot alapján.

### 5.4 Modellek és ismétlés

Három modell (saját döntés):

- egy erős: Claude Sonnet 5.5;
- egy kicsi: Claude Haiku 5.5;
- egy nyílt súlyú modell OpenAI-kompatibilis végponton. Azt, hogy melyik, és hol fut, a 11. kérdés dönti el.

A nyílt súlyú modell azért van benne, hogy a mérést más is olcsón megismételhesse, és látsszon, hogy az eredmény nem egy szolgáltatóhoz kötött. Ha szűkíteni kell, ezt hagyjuk el először. A C1 csak a Claude-modelleken fut, mert egy nyílt súlyú modell kontextusa jellemzően 128 ezer token vagy kevesebb, és ebbe az S korpusz nem fér bele (saját döntés).

A Q-gen kérdésgenerálója és a két bíráló nem Claude modellcsaládokból jön. Ez egy vagy két további szolgáltatói fiókot jelent (11. fejezet, 1. kérdés).

Minden kombináció 3 ismétléssel fut. A modellazonosítót, a dátumot és a mintavételi beállítást minden futásnál naplózzuk.

### 5.5 Mérőszámok és statisztika

| Szint | Mérőszám |
|---|---|
| Elsődleges | **Pontosság.** Két bíráló két különböző, nem Claude modellcsaládból. Emellett egy rögzített, 200 elemes emberi bírálati minta, rendszerek szerint rétegezve. Az egyezést Cohen-féle kappával mérjük, a két gépi bíráló között és az emberhez képest is. |
| Elsődleges | **Számlázott token/kérdés** (főszám). Az összes bemeneti és kimeneti token, ahogy a szolgáltató számlázza, a gyorsítótár szerint bontva. Ezt fizeti a felhasználó. |
| Másodlagos | **Olvasott token/kérdés.** Csak az eszközök által visszaadott tokenek; ez a formátumnak tulajdonítható rész (a README 842 tokenje ilyen). A rögzített többletet (eszközleírások, útmutató, újraküldött előzmények) külön jelentjük. |
| Másodlagos | eszközhívások, falióra-idő, megalapozottság (forrással alátámasztott állítások aránya, mint a PixelLab-bírálatban), költség a futáskori árlistával |
| Komponens | recall@k és MRR a keresőre, BEIR-stílusban, ügynök nélkül |

A térkép tokenárai `o200k_base` tokenizálóval számolódnak, a számlázás viszont minden modell saját tokenizálójával. A cikkben ezt kimondjuk, és a modellenkénti eltérést egy mintán megmérjük.

Statisztika:

- kérdésszintű páros összevetés;
- 95%-os bootstrap-konfidenciaintervallum;
- McNemar-próba a helyes/helytelen válaszokra;
- H2-re a non-inferiority határ előre rögzítve.

Fő ábrák:

- pontosság a token függvényében (Pareto-front);
- token/kérdés a korpuszméret függvényében (log-log);
- ablációs táblázat.

Előzetes rögzítés: a hipotéziseket, a határt, a kérdéssorokat és a verziókat a fő futás előtt commitoljuk. Ez a preregisztráció egyszerű változata.

### 5.6 Futtató

A futtató egy közös Python-program a `tools/study/` alatt (saját döntés):

- **Közös hurok.** Egyetlen ügynökhurok, cserélhető eszközkészlettel.
  - A KAPU egy MCP-munkamenetet kap (a mai `kapu_mcp_call.py` hívásonként új szervert indít, ami felfelé torzítja a tokenszámot).
  - A fájlkeresés homokozóba zárt fájlműveleteket kap.
  - A RAG egylépéses.
- **Naplózás.** Minden hívás JSONL-naplóba kerül, ahogy most is.
- **Végpontok.** Anthropic API és egy OpenAI-kompatibilis végpont.
- **Nem `claude -p`.** A fő futás nem arra épül, mert ott a modellverzió és a beállítás nem rögzíthető, és a bejelentkezés lejárt (A25).

### 5.7 Volumen-becslés (nagyságrend, nem ár)

Feltételek:

- 400 kérdés (200 + 100 + 100), 3 modell, 3 ismétlés;
- egy ügynöki kérdés kb. 25 ezer bemeneti tokent fogyaszt összesen, mert minden kör újraküldi az addigi beszélgetést;
- a teljes kontextus S-en kb. 210 ezer token kérdésenként.

| Tétel | Bemeneti token (kb.) |
|---|---:|
| C3, C4, C5 (3 rendszer × 3 600 futás × 25 ezer) | 270 millió |
| Ablációk (5 × 1 200 futás × 25 ezer, egy modell) | 150 millió |
| C1 teljes kontextus S-en (2 Claude-modell × 200 kérdés × 3 ismétlés = 1 200 futás × 210 ezer) | 250 millió (gyorsítótárral jóval olcsóbb) |
| C2b ügynöki RAG (3 600 futás erős beágyazóval + 1 200 a `potion` modellel, × 25 ezer) | 120 millió |
| C2a egylépéses RAG és C0 | 25 millió |
| **Összesen** | **kb. 0,8 milliárd** |

A pilot (1 modell, 1 futás, a 50 kérdéses fejlesztői halmaz, S) ennek kb. 2%-a. A bírálók tokenjei ezen felül jönnek, egy másik szolgáltatónál. A forintösszeget a futás napján érvényes árlistából számoljuk; a keretet te döntöd el (11. kérdés).

**Ütemezés.** Az API percenkénti tokenkorlátja a fiók szintjétől függ. Alacsony szinten ekkora volumen napokig vagy hetekig tart.

Ha szűkebb terv kell, ezek a karok:

- 2 modell;
- 2 ismétlés;
- ablációk csak S-en;
- C1 csak a kisebb modellel.

## 6. Szakaszok és kapuk

| Szakasz | Mit ad | Kapu a továbblépéshez |
|---|---|---|
| F0 | Ez a terv, jóváhagyva | A 11. fejezet döntései megvannak |
| F0.5 | Megvalósíthatósági próba az L méretre. Kb. 10 millió token tudásból (több M-korpusz együtt) építünk egy fájlt, és mérjük: build-idő, fájlméret, memória, térképhívás tokenje és ideje, keresés és kinyitás ideje. Ma a 0,57 millió tokenes korpuszból 33 MB-os fájl lesz, 90%-ban index, és a szerver minden hívásnál a teljes fájlt olvassa; L-en ez kb. 0,5 GB. | Ha a hívásonkénti idő vagy a térkép mérete a tudással arányosan nő, a 11. kérdés 3. pontja dönt: mérnöki munka, L elhagyása, vagy korlátként jelentjük. |
| F1 | A korpuszok és a kérdéssorok befagyasztva (licenc, méret, arany ajtók) | Korpuszonként megvan a kérdésszám; a Q-gen emberi mintája legalább 90%-ban rendben van |
| F2 | Futtató, baseline-ok, pilot a fejlesztői halmazon | A pilotban minden rendszer fut, a naplók teljesek. Ha a KAPU egyértelműen rosszabb a C1-nél, a C2b-nél vagy a C3-nál, előbb kiderítjük, miért, és szükség esetén átfogalmazzuk az állítást; ennek az eredménynek is meg kell jelennie a cikkben. A KAPU a pilot alapján legfeljebb két körben változhat, csak a fejlesztői halmaz alapján, és minden változás D-szám alatt a döntésnaplóba kerül. A cikk a befagyasztott verziót jelenti; a v1.0.3-as kiinduló eredményt összevetésként a függelék közli. |
| F3 | Fő futások | A kód és a spec verziója címkézve, a hipotézisek commitolva |
| F4 | Elemzés, ábrák, a cikk szövege (LaTeX, 8–12 oldal és függelék) | Minden szám a nyers naplóból újragenerálható egy paranccsal |
| F5 | Kiadás | A repó nyilvános, van verziócímke, az adat- és naplócsomag kész, megvan az ajánlás, megtörtént a beküldés (cs.IR, keresztlistázva: cs.CL, cs.AI). Ha nincs ajánló: ugyanaz a csomag technikai riportként jelenik meg a Zenodón (DOI-val) és a GitHubon, az arXiv-beküldés pedig akkor jön, amikor megvan az ajánlás (saját döntés). |

## 7. A cikk váza

- **Munkacím:** *KAPU: a single-file knowledge container that agents navigate at a size-independent cost*. A végleges címet az eredmények döntik el.
- **Hossz:** 8–12 oldal, angolul.

Fejezetek:

1. Introduction: a probléma (a tudás nem fér a kontextusba, a RAG darabol, az ügynöki fájlkeresés drága) és a hozzájárulások.
2. Related work (13. fejezet).
3. The KAPU format:
   - tervezési célok: egy fájl, determinisztikus build, ujjlenyomatok;
   - a térkép tokenárakkal;
   - a tartalom adatként kezelése (biztonsági keret).
4. Implementation: Rust, statikus beágyazások, BM25 + RRF, Leiden-csoportok, MCP-szerver.
5. Experimental setup (5. fejezet).
6. Results: Pareto, skála, ablációk, hibaelemzés.
7. Case study: amatőr forgatókönyv (11. kérdés: PixelLab vagy nyílt korpusz). Az előtanulmány a mostani, 244 szobás skill-korpuszon (REPORT 9. fejezet) motivációként szerepel, előzetes mérésként jelölve, mert a korpusza nem osztható meg (saját döntés).
8. Limitations and threats to validity (9. fejezet).
9. Conclusion; adat- és kódelérhetőség; MI-használat nyilatkozat.

## 8. Reprodukálhatóság és kiadás

- **Kód.** A repó nyilvános, a futtató és az elemzés egy címkézett verzióban.
- **Korpuszok.**
  - Ahol a licenc engedi, a befagyasztott forrás a csomagban van.
  - Ahol nem, egy letöltő szkript rögzített verzióra vagy commitra mutat, ellenőrző összeggel.
- **Kérdéssorok, nyers naplók, bírálatok és az emberi bírálat** a csomagban vannak.
- **Archiválás.** Opcionálisan Zenodo-archívum DOI-val, hogy a csomag hivatkozható legyen.

## 9. Kockázatok és az érvényességet fenyegető tényezők

| Kockázat | Kezelés |
|---|---|
| Kontamináció: a népszerű dokumentáció benne van a modellek tanítóadatában | Négy védelem (saját döntés). (1) C0 zárt könyv. (2) Konkrét, verziófüggő részletekre kérdezünk. (3) Robusztussági próbaként az S korpusz egy tudatosan módosított másolata is fut: más verziószámok, alapértékek és kapcsolók, így a fejből tudott válasz hibás lesz. Itt az arany válasz a módosított szöveg, és ezt a cikk kimondja. (4) Ha találunk friss, szabad licencű korpuszt, azt is bevonjuk. A fő H2-próba a Q-valós kérdéseken fut. |
| Bírálói torzítás (azonos modellcsalád) | Más családbeli bíráló, emberi minta, kappa |
| Szalmabáb-baseline | Mindegyik rendszer dokumentált, egyenlő hangolást kap; a beállítások a cikkben |
| A KAPU túlhangolása a teszthalmazra | Befagyasztott verzió és kérdéssor, holdout |
| A futtató torzít (hívásonként új szerver) | Munkamenet-alapú futtató (5.6) |
| Az eredmények gyorsan avulnak a modellváltások miatt | Dátum és modellazonosító minden számnál |
| Az ajánlás elakad | Korán ajánlót keresni (11. kérdés) |
| Költségtúllépés | A pilot után újratervezés, a karok az 5.7-ben |

## 10. Ami eldőlt

### Tőled

- arXiv-irány, tesztekkel és összehasonlításokkal.
- A publikált eredmények angolul.
- A magyar szál párhuzamos, a saját ügyed.
- A repó egyelőre privát; push csak kérésre.

### Saját döntés (felülírható)

- Elsődleges kategória: cs.IR.
- Három méretosztály, a C0–C5 rendszerek (a RAG egylépéses és ügynöki változatban) és az öt abláció.
- 5 százalékpontos non-inferiority határ.
- 3 modell × 3 ismétlés; a nyílt súlyú modellt hagyjuk el először; a C1 csak Claude-on fut.
- Python-futtató; háromforrású kérdéssor kb. 20% több oldalt igénylő kérdéssel.
- Külön fejlesztői halmaz a pilotnak; a KAPU legfeljebb két körben változhat, a befagyasztott verziót jelentjük.
- Főszám a számlázott token/kérdés, az olvasott token másodlagos.
- Két gépi bíráló két nem Claude családból, és egy 200 elemes emberi minta.
- Kontamináció ellen: C0 és egy módosított S-másolat.
- Ha nincs ajánló, technikai riport a Zenodón.
- Az előtanulmány motivációként szerepel.
- A dokumentum helye: `docs/research/`.

### A végrehajtóra hagyva

- a darabméret és a top-k finomhangolása a pilot alapján;
- a promptok pontos szövege;
- a statisztikai csomag;
- az ábrák kinézete;
- a LaTeX-sablon, amíg a célhely nincs eldöntve.

## 11. Nyitott kérdések (a te döntésed)

Fontossági sorrendben. Az első négy a közeli munkát határozza meg (F0.5–F2); a többi az írásig ráér.

1. **Költségkeret és hozzáférés.**
   - Mekkora keret jut az API-tokenekre? A teljes terv kb. 0,8 milliárd bemeneti token, és ezen felül jönnek a bírálók.
   - Van-e (vagy lesz-e) saját Anthropic API-kulcsod, és milyen szintű a fiók? A szint szabja meg a percenkénti tokenkorlátot, ettől függ, hány napig tart a futás.
   - A bírálókhoz és a Q-gen generátorhoz egy vagy két további, nem Claude szolgáltató kell. Melyik legyen?
   - A nyílt súlyú modell bérelt végponton fusson, vagy van GPU a gépben?
   - Ezek döntik el, hogy a teljes terv vagy egy szűkített változat fut-e (5.7).
2. **Emberi munka.** A tervben négy kézi feladat van:
   - a Q-valós fórumkérdések összegyűjtése és az arany ajtók ellenőrzése;
   - a Q-gen minta átnézése;
   - 200 válasz emberi bírálata;
   - a forgatókönyv megírása.

   Egyedül csinálod, vagy van segítség? Hány óra jut rá hetente? Melyik dokumentációs területen (Rust, Godot, Kubernetes, web) tudod magabiztosan megítélni, hogy egy válasz helyes-e? Ha kevés az idő, kevesebb lesz a Q-valós kérdés, több a Q-gen, és kisebb a kérdéssor.
3. **Mi legyen, ha az L méret nem skálázódik** (F0.5)? Fontos tudni: a korábbi „10 millió token” a teljes fájl volt, a tudás 0,57 millió token. Az L tehát kb. 17-szerese a legnagyobb eddig mért tudásnak. Három út van:
   - (a) mérnöki munka a befagyasztás előtt: memóriába leképezett olvasás, többszintű térkép; ez heteket vehet igénybe;
   - (b) az L elmarad, az M lesz a legnagyobb, és a cím állítása szűkebb;
   - (c) az L-t korlátként jelentjük.

   A javaslatom: előbb a próba, utána döntünk; de előre jó tudni, belefér-e az (a).
4. **Időkeret.** Mikorra szeretnéd a beküldést? Ettől függ, mennyire szűkítjük a tervet, és belefér-e a 3(a).
5. **Szerzőség és ajánló.** Egyedül írod, vagy van társszerző vagy egyetemi kapcsolat, aki ajánlani tud a cs.IR kategóriában? Ajánlót keresni hetekig tarthat, ezért érdemes korán elkezdeni. Ha nincs ajánló, a technikai riport a tartalék (6. fejezet, F5).
6. **A repó nyilvánossá tétele.** A cikk feltétele (A29). Mehet nyilvánosra az F5-ben, vagy előbb kell egy kiadás (A28)?
7. **A magyar nyelvközi rész (H5).** Bekerüljön-e a cikkbe? Ez egy magyar kérdéssort jelent angol korpuszra, két móddal (nyers lekérdezés, illetve ügynöki fordítás), és plusz kérdésírással jár.
8. **A PixelLab-esettanulmány.** Szerepeljen-e a cikkben?
   - Ehhez engedélyt kellene kérni a PixelLabtől.
   - A másik út, hogy csak összesített számok szerepelnek, a tartalom nem.
   - A harmadik lehetőség egy nyílt korpuszon írt saját forgatókönyv.
9. **MI-használat a cikkben.** A javaslatom: a módszertanban és a köszönetnyilvánításban leírjuk, hogy a szoftver és a mérések MI-segítséggel készültek, és a szerző minden állítást ellenőrzött. Ez megfelel az arXiv szabályának; a commitokra vonatkozó szabályodat nem érinti.
10. **Célhely később.** Csak arXiv, vagy később egy konferencia is (pl. SIGIR resource track vagy ECIR)? Ez a sablont és az anonimitási szabályokat befolyásolja.

## 12. A pilot eredménye (2026-10-23)

Lefutott a terv API-keret nélkül futtatható része (REPORT 11. fejezet, RESULTS 8. fejezet). Ami ebből a tervet érinti:

- **F0.5: az L a kiadási eszközzel nem épül meg** (A42). A térkép és a válaszidő a mérettel nő (A43, A44). A 11. fejezet 3. kérdése (mérnöki munka vagy az L elhagyása) ezzel valós döntés lett, nem feltételezés.
- **Kontamináció.** A népszerű Stack Overflow-kérdéseken a zárt könyv 30/30. A fő pontossági összevetéshez nem ismert tartalom kell: a 9. fejezet módosított korpusza vagy egy friss, kevéssé ismert dokumentáció.
- **A KAPU előnye ma ismeretlen tartalomnál az olvasott tokeneken látszik, nem a pontosságon.** A PixelLab-próbán a KAPU kb. harmadannyit olvasott, mint az egy `.md` fájl kereséssel vagy a PDF, azonos pontosság mellett. Kis, ismert korpuszon a grep olvasott a legkevesebbet.
- **Bírálat.** A két Claude-bíráló 94,7%-ban egyezett, egy-egy válasznál pedig két bíráló eltérő osztályzatot adott. A nem Claude bíráló és az emberi minta (5.5) továbbra is kell.

## 13. Kiinduló irodalom (az írás közben ellenőrizendő)

**Ügynöki és visszakereséssel kiegészített hozzáférés**

- BrowseComp-Plus (Chen et al., 2025, arXiv:2508.06600): rögzített korpuszos értékelés deep-research ügynököknek. Módszertani minta a mi tervünkhöz.
- GraphRAG (Edge et al., 2024): Leiden-közösségek és összefoglalók. A KAPU-csoportok legközelebbi rokona, hivatkozni kell rá.
- RAPTOR (Sarthi et al., 2024): fa szerkezetű összefoglaló-index.
- Contextual Retrieval (Anthropic, 2024): darabolós RAG kontextussal.
- Agent Skills (Anthropic, 2025): fokozatos feltárás, előbb a metaadat. Rokon ötlet a térképpel.
- Context7 és más dokumentációs MCP-szerverek; az `llms.txt`-javaslat (Howard, 2024).

**Hosszú kontextus kontra RAG**

- Lost in the Middle (Liu et al., 2024).
- RULER (Hsieh et al., 2024).
- Li et al. (2024): „Retrieval Augmented Generation or Long-Context LLMs?”.

**Visszakeresés**

- BEIR (Thakur et al., 2021).
- BM25 (Robertson és Zaragoza, 2009).
- RRF (Cormack et al., 2009).
- Model2Vec (Minish, 2024).
- Leiden (Traag et al., 2019).
- YAKE (Campos et al., 2020).

## Források

**Az arXiv szabályai (2026-10-23-án ellenőrizve):**

- [Endorsement](https://info.arxiv.org/help/endorsement.html)
- [Updated Practice for Review Articles and Position Papers in arXiv CS](https://blog.arxiv.org/?p=1740)
- [Updated rate limit policy (2026-10-01)](https://blog.arxiv.org/2026/10/01/updated-rate-limit-policy/)
- [Az angol nyelvi és az ajánlási változások összefoglalója (másodlagos forrás)](https://casrai.org/news/arxiv-2026-endorsement-english-ai-policy-changes)

**Módszertani minta:** [BrowseComp-Plus](https://arxiv.org/abs/2508.06600)
