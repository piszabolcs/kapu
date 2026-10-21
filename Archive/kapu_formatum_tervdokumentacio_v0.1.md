# KAPU: navigálható tudáskonténer fájlformátum

Tervdokumentáció és építési terv, v0.1 tervezet
Ötletgazda: Pintér Szabolcs
Munkanév: KAPU, kiterjesztés: `.kapu` (kiadás előtt ellenőrizni kell, hogy foglalt-e)
Dátum: 2026. október

---

## 1. Mi ez egy mondatban

**A KAPU egy tisztán szöveges fájl, ami több Markdown dokumentumot (szobát) hordoz egyetlen fájlban, és mindegyikhez előre elkészített feliratot, tokenárat és kereshető térképet ad, így egy AI csak azt a részt olvassa be, amire ténylegesen szüksége van.**

Hasonlat: olyan, mintha 10 md fájlt bezippelnél, de a zip megnyitásakor minden fájl fölött ott lenne egy felirat, hogy mi van benne és mennyibe kerül elolvasni. Az AI ezek alapján dönt, és csak a kiválasztott ajtón megy be.

---

## 2. A probléma, amit megold

1. **A tudás szétszóródik.** Sok md fájl, skill mappák, végtelen hosszú dokumentumok. A végén a felhasználó sem tudja, mi hol van, és az AI vagy megtalálja, vagy nem.
2. **A keresés tokent éget.** Egy 100 000 soros mintatárban az AI próbálgatva keres (grep, részolvasás, újabb keresés), pedig csak egy 50 soros mintára van szüksége.
3. **Ugyanazok a hibák ismétlődnek.** A kódoló AI minden appnál ugyanazokat a hibákat követi el, és 2-3 javító prompt kell. Ha a jó megoldás egyszer el van téve, és könnyen előkerül, ez megszűnik.

A felhasználó szemszögéből a cél: **nem kell tudnom, mi van a fájlban, mert a fájl feliratai tudják helyettem.** Elég annyit mondani: "használd ezt a fájlt a frontend tervezéshez".

---

## 3. Alapelvek (lezárt döntések)

1. **Egyetlen fájl, tisztán szöveg** (UTF-8, LF sorvég). Notepad++-ban megnyitható és olvasható. Nincs benne bináris rész, kép vagy vektor.
2. **A fájl passzív.** Csak tárol és térképet ad. Nem utasít, nem rendszerez, nem optimalizál, és soha nem futtat semmit. Az ajtók mögött lehet kód, de az csak szövegként jön ki.
3. **Az AI dönt.** A fájl a térkép, a küldetés az AI-é. A térkép megmutatja, hova lehet menni, és ha ott vagy, betölti a környék térképét.
4. **Három kötelező mező:** azonosító, cím, tartalom. Minden más ajánlott vagy szabad.
5. **A feliratok automatikusan kitöltődnek** a szoba tartalmából (Markdown jelölések alapján), ha a felhasználó nem tölti ki őket.
6. **A tartalom szó szerint megmarad.** A formátum nem rövidít és nem foglal össze semmit.
7. **A szoba mérete a felhasználó dolga.** Nincs méretkorlát, nincs automatikus feldarabolás.
8. **A fájl nem tárolja, ki írta bele a tartalmat.** Ember vagy AI, mindegy.
9. **A tokenszámot a mentést végző program számolja ki,** és beírja a fájlba, ahogy a Word is beírja a szószámot.
10. **A gyorsítás (vektoros keresés, gyorsítótár) az olvasó dolga,** nem a fájl része.

---

## 4. Fogalmak

1. **Szoba (room):** egy Markdown dokumentum a fájlon belül. Ez a tartalom.
2. **Ajtó (door):** a szoba felirata a térképen: azonosító, cím, leírás, tagek, ár.
3. **Alajtó:** a szobán belüli `##` címsorokból automatikusan képződő, külön kinyitható rész, saját árral.
4. **Térkép (map):** az összes ajtó és alajtó felirata egy helyen, a fájl elején.
5. **Index:** a kulcsszavak listája, minden szónál azokkal az ajtókkal, ahol előfordul.
6. **Kapcsolat (link):** két ajtó közötti él, a szobák Markdown linkjeiből képződik.
7. **Ár:** a szoba (vagy alajtó) tokenszáma és karakterszáma.

---

## 5. A fájl felépítése

A fájl négy szakaszból áll, ebben a sorrendben:

```
1. FEJLÉC     fix 256 bájt, a szakaszok helyével
2. TÉRKÉP     ajtók, alajtók, kapcsolatok (ezt olvassa az AI először)
3. INDEX      kulcsszó -> ajtóazonosítók
4. TÖRZS      a szobák szó szerinti Markdown tartalma
```

A térkép és az index azért van elöl, hogy Notepadban megnyitva rögtön látszódjon, mi van a fájlban, és hogy az olvasó egyetlen rövid olvasással elérje.

### 5.1 Fejléc

Fix 256 bájt, szóközökkel kitöltve, `\n`-nel lezárva. Így az olvasó egyetlen olvasással megtudja, hová kell ugrania.

```
%KAPU 0.1
eol=LF enc=UTF-8 tok=o200k_base
map=256+4120 index=4376+9800 body=14176+512000
rooms=12 doors=87 tokens=131204
```

1. **Első sor (magic):** `%KAPU 0.1`. Erről ismeri fel minden program, hogy KAPU fájlról van szó, és melyik verzióról.
2. **`eol` és `enc`:** a sorvég és a kódolás. Az olvasó ellenőrzi, és ha CRLF-et talál, hibát jelez (lásd 11. fejezet).
3. **`tok`:** melyik referencia tokenizálóval készült az ár.
4. **`map`, `index`, `body`:** a szakaszok kezdő bájtja és hossza (`kezdet+hossz`).
5. **Összesítők:** szobák, ajtók, tokenek száma.

### 5.2 Térkép

Soronként egy bejegyzés, `|` jellel tagolva. Három sortípus van:

```
#MAP
R frontend-design | Frontend design alapok | 4821t 19234c | @0+19234 | frontend, gomb, kártya, glass | Az én stílusom: sötét téma, glassmorphism, bronz és türkiz.
S frontend-design/szinek | Színek | 310t 1180c | @220+1180
S frontend-design/glass-gomb | Glass gomb | 640t 2600c | @1400+2600
R electron-leckek | Electron leckék | 2210t 8800c | @19234+8800 | electron, preload, ipc | Visszatérő Electron hibák és a helyes megoldásuk.
S electron-leckek/preload | Preload bridge | 480t 1900c | @19400+1900
L frontend-design/glass-gomb -> frontend-design/szinek
L electron-leckek/preload -> frontend-design
#END
```

1. **`R` (room):** szoba. Mezők: azonosító, cím, ár, hely, tagek, leírás.
2. **`S` (sub):** alajtó. Mezők: azonosító (`szoba/alajtó`), cím, ár, hely.
3. **`L` (link):** kapcsolat két ajtó között.
4. **Az ár formája:** `4821t 19234c`, vagyis tokenszám és karakterszám.
5. **A hely formája:** `@kezdet+hossz`, bájtban, **a törzs elejéhez képest** (relatív). Így a térkép és az index bővülése nem rontja el a tartalom címzését.
6. **A felhasználó saját mezői** `kulcs=érték` formában a sor végére kerülhetnek. Az olvasó megőrzi és megmutatja őket, de nem értelmezi.

### 5.3 Index

Ábécérendbe rendezett sorok, soronként egy kulcsszó és az ajtók listája:

```
#INDEX
electron	electron-leckek,electron-leckek/preload
glass	frontend-design,frontend-design/glass-gomb
glassmorphism	frontend-design,frontend-design/glass-gomb
preload	electron-leckek/preload
szín	frontend-design/szinek
#END
```

1. A kulcsszavak a címekből, tagekből, címsorokból és a szövegből jönnek, kisbetűsítve.
2. Az ábécérend miatt a szóeleji (prefix) keresés is működik.
3. Kis és közepes fájlnál az olvasó az egész indexet beolvassa a memóriába. Nagy fájlnál a rendezett sorokban felezéses kereséssel ugrálhat (ez v0.2 utáni optimalizáció).

### 5.4 Törzs

A szobák egymás után, mindegyik egy jelölősorral kezdődik:

```
#BODY
@@@ frontend-design
# Frontend design alapok
> Az én stílusom: sötét téma, glassmorphism, bronz és türkiz.
tags: frontend, gomb, kártya, glass

## Színek
...

## Glass gomb
...
@@@ electron-leckek
# Electron leckék
...
```

1. A `@@@ azonosító` sor csak emberi olvashatóságra és helyreállításra való. Az olvasó a térképben lévő bájtpozíciókat használja.
2. **Escape szabály:** ha a tartalomban egy sor `@@@`-mal kezdődne, az író `\@@@`-ra cseréli mentéskor, és olvasáskor visszaalakítja.
3. A szoba tartalma tetszőleges Markdown, akár kódblokkokkal együtt.

---

## 6. A feliratok automatikus kitöltése

A felhasználónak semmit nem kötelező kitöltenie. Mentéskor az író a szoba tartalmából állítja elő a feliratot:

1. **Cím:** az első `#` sor. Ha nincs, a szoba első nem üres sora.
2. **Leírás:** a `>` sor. Ha nincs, az első bekezdés első kb. 300 karaktere.
3. **Tagek:** a `tags:` sor. Ha nincs, a `##` címsorokból és a leggyakoribb tartalmi szavakból képződnek.
4. **Azonosító:** a címből készül (kisbetű, ékezet nélkül, kötőjellel). Létrehozás után nem változik, akkor sem, ha a cím megváltozik, hogy a hivatkozások ne törjenek el.
5. **Alajtók:** a `##` címsorokból, az azonosító `szoba/címsor` formában.
6. **Kapcsolatok:** a szobák Markdown linkjeiből (`[szöveg](#azonosító)` vagy `[szöveg](kapu:azonosító)`).
7. **Ár:** a referencia tokenizálóval számolt tokenszám és a karakterszám, minden szobára és alajtóra.

### 6.1 Sablon új szobához

Új szoba létrehozásakor ez kerül bele, mint egy kitöltendő űrlap. A `<!-- -->` megjegyzések a Markdown nézőkben nem látszanak, Notepadban súgóként olvashatók, az olvasó pedig átugorja őket.

```
# 
<!-- Ide jön a szoba címe. Ha üres marad, a szövegből lesz. -->

> 
<!-- Rövid leírás: mi van ebben a szobában. Ha üres marad, az első bekezdésből lesz. -->

tags: 
<!-- Keresőszavak vesszővel. Ha üres marad, a ## címsorokból lesznek. -->

<!-- Innen jöhet a tartalom, sima Markdownban. -->
```

Egy új, üres KAPU fájl egy szobát tartalmaz ezzel a sablonnal, mint egy üres Excel egy munkalappal.

---

## 7. Miért nem darálja be az AI az egészet

Ez a rendszer legfontosabb technikai pontja.

1. **A formátum önmagában nem garantálja a spórolást.** Ha a fájlt egy chatfelületre feltöltöd, és a platform a feltöltéskor az egész szöveget kinyeri, akkor minden token elmegy. Ezt a fájl nem tudja megakadályozni, mert passzív.
2. **A spórolást az olvasó adja.** Az AI nem közvetlenül kapja meg a fájlt, hanem egy olvasóprogramon keresztül, ami pozíció szerint olvas (seek):
   1. Beolvassa a 256 bájtos fejlécet.
   2. Odaugrik a térképhez, és csak azt olvassa be (néhány KB).
   3. Az AI kiválaszt egy ajtót.
   4. Az olvasó a térképben lévő bájtpozícióra ugrik, és csak azt a szobát vagy alajtót olvassa ki.
3. **Token csak abból lesz, amit az olvasó átad.** Egy 500 MB-os fájlból is csak a térkép és a kiválasztott néhány KB kerül a modell elé.

```
Felhasználó: "használd a design.kapu-t, kell egy glass gomb"
      |
      v
     AI  --(MCP eszközhívás)-->  KAPU olvasó  --(seek + read)-->  design.kapu
      ^                               |
      |______ csak a kért rész _______|
```

---

## 8. Mit kell köré építeni

A rendszer egy közös magból és ráépülő vékony rétegekből áll. Minden réteg ugyanazt a magot használja, így a formátum logikája egyetlen helyen van.

```
                 kapu-core   (a formátum teljes logikája)
        ___________|_____________________________
       |           |            |                |
   kapu-cli    kapu-mcp    kapu-editor     kapu-skill
  (parancssor) (AI híd)   (szerkesztő)   (használati útmutató az AI-nak)
```

### 8.1 kapu-core (a mag)

Ez maga a formátum működő megvalósítása. Feladatai:

1. **Olvasás:** fejléc, térkép, index, szoba és alajtó olvasása pozíció szerint.
2. **Keresés:** kulcsszavas keresés az indexben, rangsorolással, tokenkeret figyelembevételével.
3. **Írás:** szoba hozzáadása, módosítása, törlése, átnevezése (az azonosító marad).
4. **Építés (build):** mentéskor a feliratok, alajtók, kapcsolatok, index és árak teljes újraszámolása, és a fájl atomi kiírása (ideiglenes fájlba ír, majd átnevezi, így áramszünetnél sem sérül).
5. **Ellenőrzés (validate):** bájtpozíciók, sorvég, kódolás, hivatkozások ellenőrzése.
6. **Import és export:** md fájlok mappájából KAPU fájl, és vissza.

Egy fontos egyszerűsítés: **mentéskor mindig az egész fájl újraépül.** Ez egy szöveges fájlnál, még több tíz MB-nál is, gyors. Így nincs szükség bonyolult részleges frissítésre, és a pozíciók soha nem csúsznak el.

### 8.2 kapu-cli (parancssor)

```
kapu new design.kapu
kapu import ./md-mappa design.kapu
kapu map design.kapu
kapu search design.kapu "glass gomb" --budget 2000
kapu open design.kapu frontend-design/glass-gomb
kapu add design.kapu ujszoba.md
kapu validate design.kapu
kapu export design.kapu ./md-mappa
```

Ez fejlesztéshez, teszteléshez és szkripteléshez kell, és Claude Code is használhatja közvetlenül.

### 8.3 kapu-mcp (az AI híd)

Az MCP egy szabványos csatlakozó az AI és a program között. Nem ez a megoldás lényege, hanem a hídja: ha egyszer megírod, Claude Desktop, Claude Code, Cursor, VS Code agentek és a Vellum is ugyanazon a módon érik el a KAPU fájlokat.

Eszközök:

1. **`kapu_map(file, door?)`:** a teljes térkép, vagy egy ajtó alatti rész térképe.
2. **`kapu_search(file, query, budget?)`:** találatok ár szerint, a keretbe beleférő legjobbak elöl.
3. **`kapu_open(file, door)`:** a szoba vagy alajtó szó szerinti tartalma, mellé a szomszédos ajtók feliratai árral (a környék térképe).
4. **`kapu_write(file, door?, markdown)`:** új szoba vagy meglévő módosítása. Ezzel tudja Claude menet közben bővíteni a tudástárat.

A tartalom mindig csak szövegként jön ki, az MCP szerver semmit nem futtat.

### 8.4 kapu-editor (szerkesztő)

Asztali szövegszerkesztő, ami a `.kapu` mellett a `.md` és `.txt` fájlokat is megnyitja.

1. **`.txt`:** egyszerű szövegszerkesztő.
2. **`.md`:** szerkesztő és Markdown előnézet.
3. **`.kapu`:** bal oldalon a szobák és alajtók fája, jobb oldalon a Markdown szerkesztő, alul állapotsor (karakter, token, alajtók, kapcsolatok száma).
4. **Űrlap élmény:** a sablon súgói szürkén látszanak, és gépeléskor eltűnnek.
5. **Élő tokenszámláló,** mint a Word szószámlálója.
6. **Mentéskor a mag `build` parancsa fut,** így a fájl mindig érvényes marad.

### 8.5 kapu-skill (használati útmutató az AI-nak)

Egy kicsi, hagyományos SKILL.md, ami nem része a KAPU fájlnak, hanem az AI oldalán van. Megtanítja az AI-nak, hogyan használja a KAPU fájlokat:

1. Soha ne kérd le az egész fájlt.
2. Kezdd a térképpel vagy kereséssel.
3. Ismert célnál keress, felfedezésnél nézd a térképet.
4. Csak a kiválasztott ajtókat nyisd ki.
5. Ha a felhasználó spórolást kér, ugyanolyan találatok közül a kisebb árút válaszd.
6. Ha tanulságot találsz (pl. egy hibát több körben javítottatok), javasold, hogy kerüljön be egy szobába.

---

## 9. Milyen nyelven

**Javaslat: TypeScript mindenhol, egyetlen monorepóban.**

Indoklás:

1. **Egy nyelv a teljes rendszerre.** A mag, a CLI, az MCP szerver és a szerkesztő ugyanazt a kódot használja, nem kell két megvalósítást szinkronban tartani.
2. **A meglévő stackedhez illik.** A Vellum és a termeléstervező app is Electron, React és TypeScript, így a mag közvetlenül beköthető a Vellumba.
3. **Az MCP-nek hivatalos TypeScript SDK-ja van,** és a Node.js-es MCP szerverek a legelterjedtebbek.
4. **npm-en egyszerű terjeszteni,** egy `npx kapu` paranccsal bárki kipróbálhatja.

Komponensenként:

| Komponens | Technológia |
|---|---|
| kapu-core | TypeScript, Node.js `fs` (pozíció szerinti olvasás), külső függőség nélkül, kivéve a tokenizálót |
| tokenszámlálás | `js-tiktoken`, `o200k_base` kódolással |
| kapu-cli | TypeScript, a mag fölött |
| kapu-mcp | TypeScript, hivatalos MCP TypeScript SDK, stdio kapcsolat |
| kapu-editor | Electron, React, CodeMirror 6 (könnyebb, mint a Monaco, jó Markdown támogatással) |
| tesztek | Vitest, mintafájlokkal |

**Később:** egy Python olvasó (csak olvasás) a Python alapú AI eszközökhöz és a széles elterjedéshez. Ehhez a formátumspecifikáció legyen olyan pontos, hogy bárki megírhasson egy olvasót a saját nyelvén.

### 9.1 A tokenszámlálásról

1. Minden modell máshogy számol tokent, ezért a fájlban egy **referencia tokenizáló** (`o200k_base`) szerinti ár szerepel. Ez becslés, de egységes és összehasonlítható.
2. A **karakterszám is el van tárolva,** így bármelyik olvasó a saját modelljére át tudja számolni (egyszer megméri a saját tokenizálójával a térképet, és ebből szorzót számol).
3. Magyar szövegnél ugyanannyi karakter általában több token, ezért az ajtón a valódi tokenszám szerepel, nem a karakterszámból becsült érték.

---

## 10. A keresés négy útja

Mind a négy ugyanarra az ajtóazonosítóra mutat, így mindegy, hogyan találja meg az AI a szobát.

1. **Navigálás a térképen:** az AI a feliratok alapján lépked (világ, szoba, alajtó). Nagy modelleknek való.
2. **Kulcsszavas keresés:** az indexből, a mag végzi. Ez a gerinc, mindig elérhető.
3. **Kapcsolatok bejárása:** egy kinyitott ajtó mellett a szomszédos ajtók feliratai is megjelennek, és innen tovább lehet lépni.
4. **Jelentés szerinti (vektoros) keresés:** opcionális, az olvasó számolja ki és tárolja a saját gyorsítótárában. A fájl változatlan és szöveges marad. Akkor hasznos, ha a kérés szavai nem szerepelnek a szövegben, vagy ha más nyelven kérdezel, mint amin a tartalom íródott.

Kis lokális modellnél (pl. Qwen 3B) ne az AI navigáljon. A mag keressen, és a kész találatokat tegye a modell elé, vagy adjon neki egyetlen választást ("ebből az 5 ajtóból melyik 2 kell?").

---

## 11. Ismert technikai csapdák és kezelésük

1. **CRLF sorvégek.** Windowson a Git a `\n`-t `\r\n`-re cserélheti, és ettől minden bájtpozíció elcsúszik. Kezelés: a fejlécben `eol=LF`, egy `.gitattributes` szabály (`*.kapu text eol=lf`), és az olvasó hibát jelez, ha CRLF-et talál. A `kapu validate --fix` újraépíti a fájlt.
2. **Kézi szerkesztés Notepadban.** Ha valaki a szerkesztő nélkül ír bele, a pozíciók elcsúszhatnak. Kezelés: az olvasó minden megnyitáskor ellenőrzi, hogy a térkép szerinti pozíción tényleg a várt `@@@ azonosító` sor áll-e. Ha nem, a `@@@` sorok alapján automatikusan újraépíti a térképet.
3. **UTF-8 és bájtpozíciók.** Az ékezetes betűk több bájtosak, ezért minden pozíció bájtban értendő, nem karakterben.
4. **A `@@@` sor a tartalomban.** Az escape szabály kezeli (lásd 5.4).
5. **Félig kiírt fájl.** Az író atomi mentést használ: ideiglenes fájlba ír, majd átnevezi.
6. **Chatfeltöltés.** Ha a platform az egész fájlt feldolgozza, a spórolás elvész. Ez nem a formátum hibája. A KAPU-t olvasón keresztül kell használni.

---

## 12. Ütemterv

| Fázis | Tartalom | Kész, ha |
|---|---|---|
| 0. Specifikáció | Ez a dokumentum véglegesítve, plusz egy kézzel írt mintafájl 3 szobával | A mintafájl Notepadban olvasható, és minden pozíció kézzel ellenőrizve stimmel |
| 1. Mag (olvasás) | Fejléc, térkép, index, szoba olvasása, validálás | Egy teszt a mintafájlból pontosan a kért alajtót olvassa ki |
| 2. Mag (írás) | Build, automatikus feliratok, alajtók, kapcsolatok, index, árak, import és export | 20 md fájl importálva, exportálva, és a tartalom bájtra azonos marad |
| 3. CLI | A 8.2 parancsai | Minden parancs működik a mintafájlon |
| 4. MCP és skill | A négy eszköz és a SKILL.md | Claude Code egy valódi kérésnél a térkép és a keresés után a jó szobát nyitja ki |
| 5. Mérés | 30 valódi kódos kérés, KAPU kontra mostani skill mappa grep-pel | Kiderül a token, az eszközhívások száma és a találati pontosság közötti különbség |
| 6. Szerkesztő | Electron szerkesztő txt, md, kapu támogatással | Új KAPU fájl létrehozható, szerkeszthető, menthető, élő tokenszámlálóval |
| 7. Vellum | A mag bekötése a Vellumba | A Vellum egy KAPU fájlt tudásforrásként használ |
| 8. Kiadás | GitHub, npm, specifikáció CC BY 4.0, kód MIT | Bárki telepítheti egy paranccsal |

A sorrend azért ilyen, mert a mérés (5. fázis) a szerkesztő előtt jön. Ha kiderül, hogy valamin változtatni kell a formátumban, azt még a szerkesztő megírása előtt jobb megtudni.

---

## 13. A monorepó felépítése

```
kapu/
  spec/
    KAPU-0.1.md            a formátum hivatalos leírása
    examples/minta.kapu    kézzel írt mintafájl
  packages/
    core/                  kapu-core
    cli/                   kapu-cli
    mcp/                   kapu-mcp
    editor/                kapu-editor (Electron)
  skill/
    SKILL.md               kapu-skill
  bench/                   a mérés kérései és szkriptjei
  .gitattributes           *.kapu text eol=lf
  LICENSE
  README.md
```

---

## 14. Mitől lesz ez valódi újdonság

Az alapelv (pozíció szerinti olvasás, tartalomjegyzék bájtpozíciókkal) régi: a GNU Info fájlok már az 1980-as években így működtek. Az egyfájlos AI memória is létezik (pl. Memvid), de bináris.

A KAPU hozzájárulása a kombináció:

1. **Tisztán szöveges, egyfájlos** tudáskonténer, ami Notepadban és Gitben is áttekinthető.
2. **AI-ra szabott térkép,** ahol az ajtókon tokenár is szerepel.
3. **Automatikus feliratozás** sima Markdownból, új nyelv tanulása nélkül.
4. **Szabványos olvasó és MCP híd,** így bármelyik AI eszköz használhatja.
5. **Folyamatosan bővíthető** tudástár, amit az AI munka közben írhat.

Ezt a mérés (5. fázis) dönti el: ha a mostani skill és grep megoldásnál mérhetően kevesebb tokenből és kevesebb lépésből adja vissza a pontos mintát, akkor van egy bizonyítottan hasznos, új formátumod.

---

## 15. Nyitott kérdések a következő körre

1. **Név:** a KAPU munkanév, kiadás előtt ellenőrizni kell a kiterjesztést (fileinfo.com, filext.com), a GitHub és npm nevet, és a domaint.
2. **Kapcsolatok szintaxisa:** `[szöveg](#azonosító)` vagy `[szöveg](kapu:azonosító)`, vagy mindkettő.
3. **Csomagok közötti hivatkozás** (pl. `design.kapu#szinek` egy másik fájlból): v0.1-ben kimarad, később jöhet.
