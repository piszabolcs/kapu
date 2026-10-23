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
