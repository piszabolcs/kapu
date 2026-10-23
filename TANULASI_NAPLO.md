# KAPU tanulási napló

Lépésenként egy bejegyzés: mi készült, milyen döntéssel, milyen új fogalmakkal. A fogalmak egyszer szerepelnek a Fogalomtárban; a lépésekben csak hivatkozás van rájuk.

## 2026-10-22 | M2 1. lépés: hibakódok és fejléc-parser
Készült: `kapu-core/src/error.rs` (E001-E011 egy hibatípusban) és `header.rs` (az 512 bájtos fejléc beolvasása és ellenőrzése), 9 egységteszt, 3 elfogadási teszt élesítve.
Döntés: egyetlen `Error { code, detail }` típus, külső crate nélkül; a parser csak az első 512 bájtot nézi, és minden számot túlcsordulás-ellenőrzéssel alakít.
Fogalmak: Result és a ? operátor, enum hibakód, kanonikus decimális szám, checked_add, BTreeMap, egységteszt és integrációs teszt

## 2026-10-22 | M2 2. lépés: szakaszolvasó és karakterellenőrzés
Készült: `kapu-core/src/sections.rs`: a fejléc pozíciói alapján kijelöli az öt származtatott szakaszt és a törzset, ellenőrzi a kereteket és a hézagmentességet (E005), a teljes fájlon a CR-t (E003) és a tiltott karaktereket (E004); 7 egységteszt, 4 elfogadási teszt élesítve.
Döntés: a szakaszok kölcsönzött szeletek (`&str`, `&[u8]`), nem másolatok; két mód a törzshosszra (szigorú a validátornak, toleráns az olvasónak, D70); a korlátok túllépése E005 (D69).
Fogalmak: szelet és kölcsönzés (lifetime), bájtszintű és karakterszintű ellenőrzés, bidi vezérlőkarakterek, parserkorlátok

## 2026-10-23 | M2 3. lépés: ajtók olvasása, escape, BLAKE3
Készült: `kapu-core/src/docs.rs`: a `#DOCS` sorainak beolvasása, egy ajtó tárolt bájtjainak kivágása a törzsből, BLAKE3 ellenőrzés (E006, E007), az escape visszafejtése és a megjegyzések eltávolítása; 5 egységteszt, 3 elfogadási teszt, köztük a minta mind a 7 ajtójának kiszolgált szövege a Python referencia ellen.
Döntés: az első külső függőség a hivatalos `blake3` crate (D73); a hibás `#DOCS` sor E005 (D72); a hash-parser a fejléccel közös, csak a hibakódot kapja paraméterként.
Fogalmak: kriptográfiai ujjlenyomat (BLAKE3), referencia-implementáció elleni teszt (differential testing), let-else

## 2026-10-23 | M2 4. lépés: validátor
Készült: `kapu-core/src/validate.rs`: a `validate(fájl, Level)` összefogja a fejléc-, szakasz- és ajtóellenőrzést, és hozzáteszi az azonosító-nyelvtant (E008), a kereszthivatkozásokat (E009) és a feliratkorlátokat (E010); alap és teljes szint. Mind a 11 hibakód éles a vektorokon.
Döntés: rögzített ellenőrzési sorrend, hogy minden vektor a saját hibakódját adja (D74); a leírás tokenkorlátja M3-ra marad (D75); a túl hosszú függvény a clippy jelzésére öt kis függvényre bomlott.
Fogalmak: invariáns és kereszthivatkozás-ellenőrzés, ellenőrzési sorrend mint specifikáció, slice pattern

## 2026-10-23 | M2 5. lépés: olvasó, keret, stale újraépítés
Készült: `catalog.rs` (a `#MAP`, `#DESC`, `#GRAPH` közös sorparsere) és `reader.rs`: `open_bytes`/`open_path`, `describe_bytes`, `frame`; gyors út, egyszeri újraolvasás, majd a pozíciók újraépítése a `@@@` és `## ` sorokból `index_stale` jelzéssel. Mindkét FR-15 edge-teszt és az FR-03 egy-seek teszt éles.
Döntés: a gyors út feltétele a `bodyhash` is, mert a szoba végére írt sor ajtószinten nem látszik (D77); az alajtót sorszám alapján találjuk meg újra (D77); a parserek közös modulba kerültek (D79).
Fogalmak: fast path és fallback, graceful degradation, refaktor tesztháló mellett

## Fogalomtár

- **Result és a `?` operátor:** a Rust hibakezelése kivétel nélkül: a függvény `Ok(érték)`-et vagy `Err(hiba)`-t ad vissza. A `?` hibánál azonnal visszatér a hívóhoz, sikernél kicsomagolja az értéket.
- **Enum hibakód:** zárt értékkészletű típus (`ErrorCode::E001` ... `E011`); a fordító ellenőrzi, hogy minden esetet kezelünk, elgépelt kód nem fordul le.
- **Kanonikus decimális szám:** egy értéknek csak egy írásmódja megengedett (`7`, nem `07` vagy `+7`). Determinisztikus formátumnál ez kell, különben két különböző bájtsor jelentené ugyanazt.
- **`checked_add`:** összeadás, ami túlcsordulásnál `None`-t ad pánik vagy körbefordulás helyett. Egy támadó által írt `start+len` így nem tud „a fájl elejére visszafordulni”.
- **`BTreeMap`:** kulcs szerint rendezett térkép. A `HashMap`-pel szemben a bejárási sorrendje mindig ugyanaz, ezért a kimenet determinisztikus (D46).
- **Egységteszt és integrációs teszt:** az egységteszt (`#[cfg(test)] mod tests`) a modul belsejét próbálja ki kézzel gyártott bemenettel; az integrációs teszt (`tests/*.rs`) kívülről, a publikus API-n át, a valódi tesztvektor-fájlokon.
- **Szelet és kölcsönzés (lifetime):** a `&'a [u8]` és `&'a str` a fájl memóriájába mutató nézet, másolás nélkül. A `'a` életidő mondja meg a fordítónak, hogy a `Sections` csak addig élhet, amíg a beolvasott fájl bájtjai; így nem lehet felszabadított memóriára mutatni.
- **Bájtszintű és karakterszintű ellenőrzés:** a CR, NUL és a C0 vezérlők egyetlen bájtból állnak, ezért dekódolás nélkül, bájtonként kereshetők. A bidi és a láthatatlan karakterek többbájtos UTF-8 sorozatok, ezekhez előbb `str`-re kell dekódolni, és `char`-onként vizsgálni.
- **Bidi vezérlőkarakterek:** U+202A..U+202E és U+2066..U+2069 megfordítják a szöveg megjelenített irányát. Kódban ezzel el lehet rejteni, hogy egy sor valójában mit csinál („Trojan Source”), ezért a formátum tiltja őket.
- **Parserkorlátok (NFR-01):** felső határ a szakaszméretre és a sorhosszra, amit a parser a memóriafoglalás és a beolvasás előtt ellenőriz. Egy hamis fejléc így nem kényszerítheti a programot gigabájtos foglalásra.
- **Kriptográfiai ujjlenyomat (BLAKE3):** egy bájtsorból képzett 256 bites érték; egyetlen bit változása is teljesen más kivonatot ad, és gyakorlatilag lehetetlen két különböző bemenetet találni ugyanazzal a kivonattal. A KAPU sérülésészlelésre használja (D16), nem aláírásra.
- **Referencia-implementáció elleni teszt (differential testing):** ugyanazt a bemenetet két független implementáció dolgozza fel (itt a Python `build_vectors.py` és a Rust olvasó), és a kimeneteknek bájtra egyezniük kell. Olyan hibákat fog meg, amelyekre külön-külön senki nem írt volna tesztet.
- **let-else:** `let Some(x) = kifejezés else { return ...; };` alak: ha a minta illeszkedik, a változók élnek tovább, ha nem, az `else` ág kötelezően kilép. Sok egymásba ágyazott `match` helyett lapos hibakezelést ad.
- **Invariáns és kereszthivatkozás-ellenőrzés:** az invariáns olyan állítás, aminek egy érvényes fájlra mindig igaznak kell lennie (pl. „minden `#INDEX` posztingban szereplő azonosító létezik a `#DOCS`-ban”). A kereszthivatkozás-ellenőrzés két szakasz közötti invariánst néz, ezért csak azután futhat, hogy mindkét szakaszt külön beolvastuk.
- **Ellenőrzési sorrend mint specifikáció:** ha egy hibás bemenet több szabályt is sért, az első ellenőrzés nyer. A sorrend így a viselkedés része, nem mellékes részlet: rögzíteni kell (D74), és teszttel védeni.
- **Slice pattern:** `let [a, b, c] = f.as_slice() else { ... }` egyszerre ellenőrzi, hogy pontosan három elem van, és nevet ad nekik. Indexelés (`f[0]`) nélkül, így pánik sem lehet belőle.
- **Fast path és fallback:** a gyakori, olcsó eset (a fájl friss, a pozíciók igazak) külön ágon fut; csak ha egy ellenőrzés elbukik, akkor jön a drágább, de mindig működő út (újraépítés). A helyesség a fallbackből jön, a sebesség a fast pathből.
- **Graceful degradation:** hiba esetén a rendszer csökkentett, de használható módban működik tovább, és ezt jelzi is (`index: stale`), ahelyett hogy elutasítaná a kérést. Az AI így kézzel szerkesztett fájlból is olvashat, a következő build pedig rendbe teszi.
- **Refaktor tesztháló mellett:** a kód szerkezetének átalakítása viselkedésváltozás nélkül (itt: a parserek áthelyezése). A meglévő tesztek garantálják, hogy semmi nem romlott el; ezért kellett előbb a teszt, utána az áthelyezés.
