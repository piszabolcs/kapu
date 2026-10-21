# Feladat: KAPU tesztkorpusz építése a meglévő appjaimból

Készíts egy újrafelhasználható frontend és fejlesztési tudástárat Markdown fájlokban a megadott repóimból. Ez lesz a KAPU formátum tesztkorpusza, ezért a pontosság és a következetesség fontosabb a mennyiségnél.

## Bemenet
A repók: <IDE ÍRD A REPÓK ÚTVONALAIT>

## Mit gyűjts ki
1. **Komponensek:** önállóan újrafelhasználható UI elemek (gombok, kártyák, modalok, formok, layoutok, navigáció). Egy minta = egy kis, működő egység a szükséges CSS-sel vagy Tailwind osztályokkal együtt.
2. **Alapok:** design tokenek, színek, közös változók, témák, amelyekre a komponensek épülnek.
3. **Leckék:** a git történetből a visszatérő hibák. Nézd meg a javító commitokat (fix, bug, hiba szavak, illetve az egymás utáni javítások ugyanabban a fájlban). Minden leckéhez: mi volt a hiba, mi okozta, mi a helyes megoldás, rövid kódpéldával.
4. **Receptek:** nagyobb, több komponensből álló minták (pl. beállítások ablak jelszóval, adatlista szűréssel).

## Formátum, minden fájlnál kötelező
```
# <rövid, egyértelmű cím>
> <egy mondat: mi ez és mikor hasznos>
tags: <5-10 keresőszó vesszővel, angolul és magyarul is>

## <alrész címe>
<magyarázat 1-3 mondatban>
<kódblokk nyelvjelöléssel>
```
1. Egy fájl egy témakör (pl. `gombok.md`, `electron-leckek.md`), benne `##` szakaszonként egy minta.
2. Ha egy minta egy másikra épül, linkeld: `[Színek](#frontend-alapok/szinek)`.
3. A kód legyen működő és teljes, ne rövidítsd `...`-tal.
4. Ugyanazt a mintát ne vedd fel kétszer. Ha két változat van, egy szakaszban hasonlítsd össze őket.

## Biztonság és adatvédelem
1. Céges adat, belső szervernév, IP-cím, felhasználónév, jelszó, kapcsolati sztring, API kulcs, tenant vagy kliens azonosító NEM kerülhet a korpuszba. Ha egy minta ilyet tartalmaz, cseréld helyőrzőre (pl. `<SERVER>`).
2. Céges termék- és rendszerneveket általános nevekre cseréld.

## Kimenet
1. `korpusz/` mappa a md fájlokkal.
2. `korpusz/kerdesek.jsonl`: legalább 40 tesztkérés, soronként:
   `{"kerdes": "...", "valasz_fajl": "gombok.md", "valasz_szakasz": "Glass gomb", "nyelv": "hu"}`
   A kérések fele magyar, fele angol legyen. Legyenek köztük olyanok is, ahol a kérés szavai nem szerepelnek a kódban (pl. "üvegszerű, homályos gomb", miközben a kódban `backdrop-filter` van).
3. `korpusz/LEIRAS.md`: rövid összesítő, hány fájl, hány szakasz, és milyen forrásokból.

Dolgozz repónként, és minden repó után foglald össze, mit vettél fel. A végén ellenőrizd, hogy minden fájl megfelel-e a formátumnak, és nincs-e benne érzékeny adat.
