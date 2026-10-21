# Réteges, AI-ra szabott tudásfájl: összefoglaló a beszélgetésről

Szerző és ötletgazda: Pintér Szabolcs
Állapot: koncepció, az első specifikáció (v0.1) előtt
Dátum: 2026. október
Cél: a téma átadása más modelleknek is, véleményezésre és továbbgondolásra

---

## 1. A probléma, amit meg akarok oldani

1. **A hosszú anyagok nem adhatók oda egyben az AI-nak.** Egy 300 oldalas szkenner manuált nem tudtam odaadni Claude Opusnak, mert túl nagy volt. A jelenlegi megoldás az, hogy az ember feldarabolja, és részenként adogatja, ez lassú és kényelmetlen.
2. **A teljes anyag beolvasása pazarlás.** Egy kérdés megválaszolásához általában az anyag 1-2%-a kell, mégis az egész elmegy tokenben. Ez pénz, idő és energia.
3. **A kis, lokális modellek (pl. Qwen 3B) kontextusa kicsi.** Náluk nem kényelmi kérdés a spórolás: ezen múlik, hogy egyáltalán használhatók-e tudásbázissal.
4. **A tudás ma szét van szórva.** Sok kis txt fájl, vektoradatbázis, külön keresőmotor, darabolószkriptek. Nincs egy hordozható, egyfájlos, AI-ra szabott formátum.

A két fő cél, amelynek minden más alá van rendelve:

1. **Tokenspórolás:** az AI csak azt olvassa be, ami a kérdéshez kell.
2. **Nagyon gyors keresés:** az olvasó egyenesen a jó helyre ugorjon, végigolvasás nélkül.

---

## 2. Az eredeti ötletem, ahogy kitaláltam (a későbbi pontosítások nélkül)

Ezek az én eredeti elképzeléseim, akkor is ideírom őket, ha a beszélgetés során kiderült, hogy valamelyik így nem működik. Más modellek számára érdekes lehet, hátha ők máshogy látják.

1. **Excel-szerű, de egyszerű, szöveg alapú fájl.** Több "fül" vagy "tábla", mint egy adatbázis, de txt alapon, szöveges tartalomra.
2. **Vektorosan címkézett táblák.** Az AI a címkék vagy RAG alapján tudja, melyik táblába kell mennie, és csak onnan olvas be pl. egy 1000 karakteres részt.
3. **Konverter.** Könyvek, hosszú PDF manuálok egy kattintással átalakíthatók erre a formátumra.
4. **Lokális LLM memória.** Ugyanez a fájl lokális modelleknél memóriaként vagy tudásbázisként is használható lenne.
5. **Notepad++-ban is nyíljon meg,** ne úgy, mint egy mai bináris adatbázisfájl.
6. **Az "ajtós" modell.** A fájlban ajtók vannak. Aki megnyitja, először csak az ajtókig jut el, és minden ajtón ott a felirat (kulcsszavak, sűrített tartalom), hogy mi van mögötte. Ez a spórolás.
7. **A fájl saját képessége.** Eredetileg úgy képzeltem, hogy maga a fájl akadályozza meg a végigolvasást, és a fájlon kell "csinálni valamit", hogy egy ajtó kinyíljon. A belépés egy léptetett lépéssorozat lenne.
8. **Megfordítva:** nem a fájl nyitja az ajtót, hanem az, aki be akar menni.
9. **Excel irány.** Ha elengedem a Notepad-olvashatóságot, legyen inkább olyan, mint az Excel.
10. **Szöveges és bináris egyben.** Egy fájlban keverni a szöveges és a bináris részt.
11. **Lvl2 skill rendszer.** Claude Code-hoz egy fejlettebb skill rendszer, ami több ezer sornyi kódot, design mintát tartalmaz (pl. frontend fejlesztéshez).
12. **Vellum.** A saját lokális LLM futtató appomba (Vellum) ezzel minőségi memóriát és tudásbázist lehetne építeni.
13. **Kis modell okosítása.** Egy régebbi, butább Qwen 3B is sokkal okosabb lehetne vele.
14. **Nagy modell tudásának "átöntése".** Open weight modellek (Qwen, Mistral) tudását kinyerni, és ebbe a formátumba betölteni, hogy egy pici modell is nagyobb tudással rendelkezzen.
15. **Gráfelmélet.** A gráfelméletet bevinni egy szöveges (nem bináris) új fájlformátumba.

---

## 3. Mit tisztáztunk közben (alapok)

1. **A fájl bájtok sorozata.** Egy bájt 0 és 255 közötti szám. A bájtok nem tudják magukról, mit jelentenek, a jelentést a formátum szabálykönyve adja, amit az olvasó program alkalmaz. (Hasonlat: atomokból a fizika szabályai szerint lesz tárgy, bájtokból a formátum szabályai szerint lesz szöveg vagy kép.)
2. **A szöveges fájl** az, ahol minden bájt (vagy bájtcsoport) egy karakterkódolás szerint egy karaktert jelent. Ma ez jellemzően UTF-8, ahol az ékezetes betűk két bájtot foglalnak (pl. "á" = `C3 A1`).
3. **A Notepad mindent megnyit,** csak mindenre a szövegszabályt alkalmazza. Bináris fájlnál ezért lesz krix-krax.
4. **A kiterjesztés csak címke.** A valódi fájltípus a specifikáció. Sok formátum az első bájtjaival azonosítja magát (magic number, pl. PNG: `89 50 4E 47`, ZIP/xlsx: `50 4B` = "PK").
5. **Új fájltípust bárki létrehozhat:** kell egy specifikáció és egy program, ami ismeri. Attól lesz "valódi", hogy mások is használják.
6. **A fájl passzív.** Magától semmit nem csinál, még egy exe sem: azt is az operációs rendszer tölti be. Ezért ajtót mindig az olvasó nyit, nem a fájl.
7. **Pozíció szerinti olvasás (seek).** Egy program egy fájl tetszőleges bájtjához ugorhat, és csak annyit olvashat, amennyit akar. Erre épül a ZIP, az SQLite, a PDF, és erre épülne az ajtós elv is.
8. **Token csak abból lesz, amit egy program kiolvas és átad a modellnek.** Nem a fájl mérete számít, hanem hogy mi kerül a kontextusba.
9. **Szöveg és bináris keverése létezik:** a PDF eleje és xref táblája olvasható, közte bináris streamek vannak. A safetensors elején egy szöveges JSON fejléc mondja meg, melyik bináris blokk hol van. Fordított irány: base64, ahol bináris adat szövegként van tárolva (kb. 33%-kal nagyobb lesz).

---

## 4. Mi működik és mi nem (jó és rossz)

### 4.1 Ami működik

1. **Az ajtós elv agent környezetben** (Claude Code, MCP szerver, kódfuttatásra képes AI) pontosan úgy működik, ahogy elképzeltem: az AI beolvassa a térképet (néhány száz token), kiválasztja az ajtót, és csak azt a részt olvassa be.
2. **A réteges felépítés** a tokenspórolás kulcsa: térkép, ajtókivonatok, alajtók, nyers szöveg. Becslés: egy kb. 150 000 tokenes manuálnál egy kérdés 2000-3000 tokenből megválaszolható.
3. **A kétféle index** a gyors keresés kulcsa. A kulcsszóindex pontos dolgokra (hibakód, paraméternév) a leggyorsabb, a vektorindex jelentés szerinti keresésre jó.
4. **A gráf** a továbblépést adja: kereszthivatkozások, hibakódok és alkatrészek között.
5. **A kis modellnek tudást ad,** ha a program elé teszi a releváns részt. Kevesebbet kell "emlékeznie", inkább csak olvasnia kell.
6. **A nagy modell a konverterben** tud igazán sokat adni: egyszer dolgozik, megírja az összefoglalókat, kulcsszavakat, kérdéseket, és ezt a kis modell minden kérdésnél használja.

### 4.2 Ami nem működik, vagy csak korlátokkal

1. **A fájl nem tud saját képességet,** csak futtatható kóddal tudna, azt viszont (mint az Excel makrót) biztonsági okból az AI platformok és a céges IT nem futtatják.
2. **Sima chatfeltöltésnél** a platform még az AI előtt szöveggé alakítja az egész fájlt (a "portás mindent kihord"), ott a formátum magában nem segít. Egy ismeretlen ZIP konténert a platform jó eséllyel érintetlenül hagy, ezt platformonként ki kell próbálni.
3. **A vektor nem olvasható felirat.** A vektor számsor, az AI nem tudja elolvasni, csak egy keresőprogram tudja használni. Ezért az ajtókra kulcsszó és összefoglaló kell, a vektor külön szakaszba vagy fájlba.
4. **A kis modell nem lesz okosabb,** csak többet fog tudni. A gondolkodás (több rész összevetése, következtetés) egy 3B modellnél gyenge marad. A kis modellt ne engedjük navigálni: a program keressen, és a kész darabokat tegye elé.
5. **Egy modell tudása nem olvasható ki a súlyokból.** A tény milliónyi számban, szétkenve tárolódik. Kerülőutak: desztilláció (a nagy modell válaszaival tanítjuk a kicsit) vagy a nagy modell által generált szöveg betöltése a formátumba. Ennek buktatója, hogy a nagy modell hallucinációi is bekerülnek. Jobb az eredeti forrást konvertálni.
6. **A vegyes (szöveg és bináris) fájl Notepadban nézhető, de nem menthető,** mert a mentés tönkreteheti a bináris részt és a bájtpozíciókat.
7. **A gráf minősége mindent eldönt.** A rossz él félrevezet, a túl sűrű gráf haszontalan.

### 4.3 Open weight és open source

1. **Open weight:** a súlyok letölthetők, futtathatók, finomhangolhatók.
2. **Open source szigorúan:** a tanítóadat és a tanítási kód is nyilvános. Ilyen modell kevés van.
3. **A licenc dönti el, mit szabad,** pl. hogy egy modell kimenetével másikat taníts. Modellenként külön ellenőrizni kell.

---

## 5. Amit a piacon találtam (hasonló megoldások)

1. **Memvid (.mv2).** Egyfájlos AI memória, Apache-2.0. Fejléc magic numberrel, adatblokkok, BM25 kulcsszóindex és HNSW vektorindex, a végén bájtpozíciós tartalomjegyzék. Ez áll a legközelebb, de bináris, és nincsenek benne rétegzett ajtók.
2. **PageIndex.** MIT. Vektor nélküli, "érvelés alapú" RAG: a dokumentumból fejezetfát épít összefoglalókkal, és az LLM ezen navigál. Ez maga az ajtós navigálás, de nincs saját hordozható fájlformátuma. A gyártó 98,7%-os FinanceBench pontosságot állít (saját adat).
3. **RAPTOR (Stanford, kutatás).** Alulról felfelé, rekurzív összefoglalással épít többszintű fát. Összetett kérdéseknél jelentős pontosságjavulást mértek. Ez a réteges ötlet tudományos alátámasztása.
4. **ZIM / Kiwix.** Nyílt, tömörített, egyfájlos formátum beépített teljes szöveges keresőindexszel (pl. teljes Wikipédia offline). A kiwix-rag projekt helyi LLM-mel használja, embedding nélkül, csak a kulcsszóindexszel.
5. **Satchel.** MIT. Hordozható RAG "pendrive-on", MCP-n keresztül, hibrid kereséssel.
6. **llms.txt.** Markdown térkép weboldalakhoz az AI számára (v2, 2026. augusztus). Ez a 0. réteg, a térkép mintája.
7. **Claude skillek.** Ugyanez az elv mappákban: először csak a leírás töltődik be, a részletek akkor, amikor kellenek (progressive disclosure).

### 5.1 A rés, ahová ez az ötlet illik

Egyik meglévő megoldás sem rakja össze egy fájlban ezeket: réteges ajtótérkép, kétféle index, gráf, olvasható szöveges felépítés, konverter, MCP olvasó és skill. Az alapelv nem új, a kombináció viszont nem létezik.

---

## 6. A jelenlegi irány (döntések)

1. **Saját formátumot építek.** A meglévőket ihletként használom, kódot csak licenc szerint (MIT, Apache-2.0: licencszöveg és szerzők megtartásával), a RAPTOR-ra hivatkozással.
2. **A felépítés három pillére:** a fa adja a rétegeket, az indexek a belépési pontot, a gráf a továbblépést.
3. **A fájl szöveges** (UTF-8), csak az opcionális vektorindex lehet bináris, a fájl legvégén.

### 6.1 Mitől lehet valóban új

1. **Az ajtókon ár van.** Minden csúcs mellett szerepel a mögötte lévő tokenszám és a teljes részfa tokenszáma. Az AI így egy keretből tud tervezni ("4000 tokenem van, ez az ajtó 1200-ba kerül").
2. **Kérdéshorgonyok.** A konverter minden darabhoz 2-3 előre megírt kérdést generál, amire az a darab válaszol, és ezek bekerülnek a kulcsszóindexbe. Vektor nélkül is működik, kis modellnél is.
3. **Olvasható, szabványos protokoll.** Szöveges fájl, Gitben diffelhető, rögzített olvasási protokoll néhány MCP paranccsal.

### 6.2 A fájl felépítése (v0.1 tervezet, munkanév)

```
#KBT 0.1
#FEJLÉC    forrás, nyelv, létrehozás, konverter, tokenizáló, szakaszok bájtpozíciói
#TÉRKÉP    0. réteg: ajtók címe, egymondatos leírás, tokenár (mindig beolvassák)
#CSÚCSOK   id | cím | szint | szülő | bájt eleje-vége | token | részfa token | összefoglaló
#ÉLEK      forrás > cél | típus | megjegyzés
#TERMEK    kulcsszóindex: szó | csúcsazonosítók
#HORGONYOK kérdés | csúcsazonosító
#TARTALOM  === A3.1 === ...nyers szöveg...
#VEK       (opcionális, bináris) embeddingek csúcsazonosító szerint
```

Éltípusok a gráfban:

1. **Tartalmazza:** a fa, vagyis a rétegek.
2. **Következő:** olvasási sorrend.
3. **Hivatkozik:** "lásd 5. fejezet" típusú kereszthivatkozások.
4. **Említi:** entitások (hibakód, paraméter, alkatrészszám) saját csúcsként.

Gráf példa szövegesen:

```
#CSOMÓPONTOK
A3      | Hibakódok           | szint 0
A3.1    | E040-E049 olvasási  | szint 1 | bájt 18200-24050
A5.2    | Ablak tisztítása    | szint 1 | bájt 41000-43800
@E047   | hibakód             | entitás
#ÉLEK
A3      > A3.1   | tartalmazza
A3.1    ~ A5.2   | hivatkozik | "lásd tisztítás"
A3.1    @ @E047  | említi
```

### 6.3 Tervezési szabályok

1. **Fix hosszú fejléc** (pl. 512 bájt), benne a szakaszok bájtpozíciói, így egy olvasással kiderül, hová kell ugrani.
2. **Az élek azonosítókra mutatnak,** bájtpozíció csak a csúcstáblában van.
3. **A térkép mérete korlátozott** (pl. legfeljebb 600 token), ha nem fér bele, a konverter összevon.
4. **Darabméret 300-1200 token,** fejezethatáron vágva.
5. **A kulcsszóindex rendezett szöveg,** nagy fájlnál felezéses kereséssel olvasható.
6. **Kézzel nem szerkesztjük:** kicsomagolás Markdownba, szerkesztés, majd újraépítés (`build`).

### 6.4 Alkatrészek

1. **Mag (Python könyvtár):** olvasás, ugrás, keresés, gráfbejárás, építés, validálás.
2. **Konverter:** PDF-ből fa, darabok, indexek, élek, összefoglalók és kérdéshorgonyok. Céges anyagnál lokális LLM-mel (Ollama).
3. **MCP olvasó:** `map` (térkép), `search` (keresés tokenkerettel), `open` (ajtó kinyitása szomszédokkal és árakkal), `neighbors` (gráfbejárás).
4. **Claude skill:** az olvasási protokoll leírása.
5. **Vellum integráció:** importáló, a már meglévő chunker, `bge-m3` és hibrid keresés felhasználásával.
6. **Szerkesztő:** ráér a végére.

### 6.5 Ütemterv

1. **v0.1:** specifikáció, kézzel írt minta fájl, olvasó, validátor.
2. **v0.2:** konverter, első éles teszt a szkenner manuálon.
3. **v0.3:** MCP olvasó és skill.
4. **v0.4:** mérés. 30 kérdés ismert válaszokkal, négy változat (teljes szöveg, csak kulcsszó, navigálás, keresés), token, idő, pontosság, Qwen 3B-vel és Claude-dal is.
5. **v0.5:** gráf és vektor, ha a mérés szerint kell.

---

## 7. A nyitott kérdések és a válaszaim

### 7.1 Melyik tokenizálóval számoljuk az ajtók árát?

**Kérdésem:** nem ismerem ezeket, és a magyar nyelv csavart vihet bele.

**Javaslat:**

1. **Referencia tokenizáló: `o200k_base` (OpenAI tiktoken).** MIT licencű, Pythonból két sorral használható, gyors, és offline is működik, ha a kódolási fájlt egyszer letöltjük. Az újabb OpenAI modellek ezt használják, és a régebbi kódolásnál jobb a nem angol szövegekre.
2. **A fejlécben rögzítjük:** `tokenizer: o200k_base`. Az ár így becslés más modelleknél (Claude, Qwen saját tokenizálóval számol), de egységes és összehasonlítható.
3. **A karakterszámot is eltároljuk** minden csúcsnál. Így egy olvasó a saját modelljére át tud számolni: egyszer megméri a saját tokenizálójával a térképet, kiszámol egy szorzót, és azzal korrigálja az árakat.
4. **A magyar csavar:** a ragozó nyelvek egy szóra jellemzően több tokent kapnak, mint az angol, ezért magyar szövegnél ugyanannyi karakter drágább. Ezért nem a karakterszám, hanem a valódi tokenszám kerül az ajtóra, és a fejlécben ott a nyelv is. A Qwennél a pontos számhoz a Hugging Face `tokenizers` könyvtárral a Qwen saját tokenizálója is használható.

### 7.2 Legyen-e bináris farok a vektoroknak?

**Kérdésem:** legyen? Megéri?

**Válasz: nálam valószínűleg megéri, de opcionálisan, és csak a mérés után.**

1. **A konkrét ok:** a manuáljaim jellemzően angolok, én viszont magyarul kérdezek. A kulcsszóindex nyelvek között nem talál egymásra, a többnyelvű vektormodell (pl. `bge-m3`, ami a Vellumban már megvan) viszont igen.
2. **Olcsóbb alternatíva, amit előbb érdemes kipróbálni:** a kérdéshorgonyokat a konverter magyarul és angolul is megírja. Ez vektor nélkül is áthidalhatja a nyelvi különbséget.
3. **Döntés:** a formátum v0.1-ben már tartalmazza a `#VEK` szakasz helyét és szabályát (hogy ne kelljen később átírni), de a konverter csak kérésre tölti ki. A v0.4 mérés dönti el, hogy alapértelmezett legyen-e.

### 7.3 Licenc

**Döntésem:** nyitott licenc. A specifikáció nyílt (pl. CC BY 4.0), a kód MIT vagy Apache-2.0. Ha sokan használnák, arra elképesztően büszke lennék.

### 7.4 Név

**Ötletem:** a monogramom, `.psz`, vagy kitalálni egy nevet, és abból monogramot képezni.

1. **A `.psz` foglalt.** Tömörített PostScript fájlok és az Agisoft PhotoScan/Metashape projektfájljai is használják. Ütközne, ezért kiterjesztésnek nem ajánlott.
2. **A monogram máshová is mehet:** a magic numberbe vagy a specifikáció szerzői részébe, így a "kézjegyem" minden fájlban ott van a kiterjesztés ütközése nélkül.
3. **Ötletek (még ellenőrizetlenek, foglaltságot nézni kell):**
   1. **Kapu (`.kapu`):** az ajtós elvből, magyar gyökérrel, négybetűs kiterjesztés is lehetséges.
   2. **Tárló:** a kiállított, feliratozott tárgyak üvegszekrénye, pont olyan, mint egy felcímkézett ajtó.
   3. **Doorway / Lintel:** angol, nemzetközi elterjedéshez.
   4. **Atlas:** térkép, rétegek, tájékozódás.
4. **Ellenőrzési lista a választás előtt:** fileinfo.com és filext.com (kiterjesztés), GitHub és PyPI (projektnév), domain szabad-e.

---

## 8. Kérdések más modelleknek

1. Létezik olyan megoldás, ami a 2. és 6. fejezetben leírt kombinációt már megvalósítja, és amit nem találtam meg?
2. A tokenárral ellátott ajtók és a kérdéshorgonyok valóban újdonságok, vagy van rájuk ismert módszer?
3. Szöveges formátumban milyen kulcsszóindex-szerkezet a leggyorsabb nagy fájlnál?
4. Hogyan lehetne elérni, hogy a chatfelületek feltöltéskor ne alakítsák szöveggé az egész fájlt, hanem a modell maga nyissa meg?
5. Mi a véleményed a kis modelleknél a "keresés először, navigálás csak nagy modellnek" felosztásról?
6. Mit gondolsz az eredeti, elrugaszkodottabb ötleteimről (2. fejezet), van köztük olyan, amit érdemes mégis újragondolni?

---

## 9. Források

1. Memvid dokumentáció: https://docs.memvid.com/
2. Memvid GitHub: https://github.com/memvid/memvid
3. MV2 formátum specifikáció (fork): https://github.com/dus4w/ai-memvid-memory-knowledge-base/blob/main/MV2_SPEC.md
4. PageIndex GitHub: https://github.com/VectifyAI/PageIndex
5. PageIndex weboldal: https://pageindex.ai/
6. RAPTOR cikk (arXiv 2401.18059): https://arxiv.org/abs/2401.18059
7. ZIM formátum (Wikipedia): https://en.wikipedia.org/wiki/ZIM_%28file_format%29
8. Kiwix és ZIM útmutató: https://www.bigiron.cc/guides/kiwix-and-zim-offline-mirrors-of-wikipedia-and-stack-exchange
9. kiwix-rag: https://github.com/amih/kiwix-rag
10. Satchel: https://github.com/virgilvox/satchel
11. llms.txt v2: https://llmstxt.org/
12. tiktoken (OpenAI): https://github.com/openai/tiktoken
13. Tokenszámlálás tiktokennel (OpenAI Cookbook): https://developers.openai.com/cookbook/examples/how_to_count_tokens_with_tiktoken
14. o200k_base tokenizáló profil: https://tokenreference.com/tokenizers/o200k_base/
15. tiktoken offline használata (Stack Overflow): https://stackoverflow.com/questions/76106366/how-to-use-tiktoken-in-offline-mode-computer
16. .psz kiterjesztés (FileInfo): https://fileinfo.com/extension/psz
17. .psz kiterjesztés (file.org): https://file.org/extension/psz
