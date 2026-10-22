# KAPU tanulási napló

Lépésenként egy bejegyzés: mi készült, milyen döntéssel, milyen új fogalmakkal. A fogalmak egyszer szerepelnek a Fogalomtárban; a lépésekben csak hivatkozás van rájuk.

## 2026-10-22 | M2 1. lépés: hibakódok és fejléc-parser
Készült: `kapu-core/src/error.rs` (E001-E011 egy hibatípusban) és `header.rs` (az 512 bájtos fejléc beolvasása és ellenőrzése), 9 egységteszt, 3 elfogadási teszt élesítve.
Döntés: egyetlen `Error { code, detail }` típus, külső crate nélkül; a parser csak az első 512 bájtot nézi, és minden számot túlcsordulás-ellenőrzéssel alakít.
Fogalmak: Result és a ? operátor, enum hibakód, kanonikus decimális szám, checked_add, BTreeMap, egységteszt és integrációs teszt

## Fogalomtár

- **Result és a `?` operátor:** a Rust hibakezelése kivétel nélkül: a függvény `Ok(érték)`-et vagy `Err(hiba)`-t ad vissza. A `?` hibánál azonnal visszatér a hívóhoz, sikernél kicsomagolja az értéket.
- **Enum hibakód:** zárt értékkészletű típus (`ErrorCode::E001` ... `E011`); a fordító ellenőrzi, hogy minden esetet kezelünk, elgépelt kód nem fordul le.
- **Kanonikus decimális szám:** egy értéknek csak egy írásmódja megengedett (`7`, nem `07` vagy `+7`). Determinisztikus formátumnál ez kell, különben két különböző bájtsor jelentené ugyanazt.
- **`checked_add`:** összeadás, ami túlcsordulásnál `None`-t ad pánik vagy körbefordulás helyett. Egy támadó által írt `start+len` így nem tud „a fájl elejére visszafordulni”.
- **`BTreeMap`:** kulcs szerint rendezett térkép. A `HashMap`-pel szemben a bejárási sorrendje mindig ugyanaz, ezért a kimenet determinisztikus (D46).
- **Egységteszt és integrációs teszt:** az egységteszt (`#[cfg(test)] mod tests`) a modul belsejét próbálja ki kézzel gyártott bemenettel; az integrációs teszt (`tests/*.rs`) kívülről, a publikus API-n át, a valódi tesztvektor-fájlokon.
