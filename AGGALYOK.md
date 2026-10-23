# KAPU – Aggálylista

Fejlesztés közben talált kérdések, kockázatok és spec-rések, amelyek a végső áttekintésen döntést igényelnek. Minden pont: hol, mi a gond, mi a mostani állapot, mi a javaslat. A lezárt pontok a döntésnaplóba (D-bejegyzés) kerülnek, innen pedig kikerülnek.

| # | Hol | Aggály | Mostani állapot | Javaslat |
|---|---|---|---|---|
| A1 | FR-06/2, validátor (D75) | A leírás 300 tokenes kemény korlátját a validátor nem nézi, mert tokenizáló kell hozzá. | M3-ban a tokenizálóval együtt bekerül. | M3-ban pótolni. |
| A2 | FR-15/2 keret (D78) | Ha egy szoba tartalmában szerepel a `</kapu-room>` szöveg, az AI felé a keret „bezárhatónak” látszik, és a szoba utáni szöveg keretén kívülinek tűnhet (prompt injection felület). | Nincs kezelve. | M7-ben az FR-19 tartalomvizsgálat jelezze, és a keret kapjon egyedi záró jelölőt (pl. `</kapu-room id="…">`), vagy a tartalomban escape-elni kell. Spec-döntés kell. |
| A3 | FR-02/6 | A validátor nem ellenőrzi, hogy a származtatott szakaszok NFC-normalizáltak-e (D69). | A `--full` újraépítés-összevetés (M3) közvetve megfogja. | Elég az újraépítés-összevetés; külön ellenőrzés nem kell. |
| A4 | FR-13 mező-escape | A `#MAP` mezőiben csak a `\|` escape létezik. Egy címben szereplő szó szerinti `\|` kétértelmű: íráskor `\\|` lesz belőle, olvasáskor visszafejtve `\|`, de a szabály nem írja le a `\` escape-elését. | A Python referencia és a Rust ugyanígy viselkedik. | Spec-pontosítás: vagy a `\` is escape-elődik (`\\`), vagy kimondjuk, hogy a cím nem tartalmazhat `\|`-t. |
| A5 | D59 megjegyzés-eltávolítás | Többsoros `<!-- -->` megjegyzésnél a szabály „csak a whitespace-sorokat dobja el”, ami a sortörések számát kiszámíthatatlanná teszi. Bájtazonos, de nem intuitív. | A Python és a Rust egyezik. | Egyszerűbb szabály a spec v1.1-be: a megjegyzés helye törlődik, és ha a sor üressé válik, a sor is. |
| A6 | NFR-03/5 fuzz Windowson | A `cargo fuzz` Windowson MSVC AddressSanitizert igényel (VS komponens). | ASan telepítés folyamatban. | A CI-ba Linuxos fuzz-job (nightly, rövid futás minden pushnál). |
| A7 | NFR-04 CI macOS | Az első pushnál a macOS futó 15 perc várakozás után elindulás nélkül törlődött; másodjára lefutott. | Kötelező job maradt. | Megfigyelni; ha ismétlődik, `timeout-minutes` és újrapróbálás. |
| A8 | D71 teljes fájl memóriában | Az olvasó minden hívásnál a teljes fájlt beolvassa és a `bodyhash`-t kiszámolja (D77). 3000 szobánál ez hívásonként több MB olvasás és hash. | Működik, nincs mérve. | M8 NFR-05 mérés dönt; ha lassú, `bodyhash` gyorsítótár fájlméret+módosítási idő kulccsal. |
| A9 | D77 alajtó-újraépítés | Ha egy szobában a `##` címsorok száma megváltozott, az alajtói a következő buildig nem nyithatók ki. | E007 azonosítónként, a szoba maga kinyitható. | Elfogadható; a hibaüzenet mondja meg, hogy buildelni kell. |
