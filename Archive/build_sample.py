# KAPU 0.1 mintafajl epito PROTOTIPUS (nem a vegleges Rust implementacio).
# BLAKE3 ujjlenyomat + kotelezo Ed25519 alairas.
import re, base64, unicodedata
from blake3_ref import blake3
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey, Ed25519PublicKey
from cryptography.hazmat.primitives import serialization as ser

ROOMS = [
"""# Frontend design alapok
> Sotet tema, glassmorphism, bronz es turkiz szinek: az alap stilus minden apphoz.
tags: frontend, design, glass, szinek

## Szinek
```css
:root {
  --bg: #12161c;
  --bronz: #c08a4a;
  --turkiz: #3fb6a8;
  --glass: rgba(255, 255, 255, 0.08);
}
```

## Glass gomb
A gomb a szineket hasznalja, lasd [Szinek](#frontend-design-alapok/szinek).
```css
.btn-glass {
  background: var(--glass);
  backdrop-filter: blur(12px);
  border: 1px solid rgba(255, 255, 255, 0.18);
  border-radius: 12px;
  color: var(--turkiz);
  transition: transform .15s ease;
}
.btn-glass:hover { transform: translateY(-1px); }
```
""",
"""# Electron leckek
> Visszatero Electron hibak es a helyes megoldasuk.
tags: electron, preload, ipc, hiba

## Preload bridge
Soha ne kapcsold ki a contextIsolation-t. A renderer csak a preloadban
kiengedett fuggvenyeket lassa, lasd [Frontend](#frontend-design-alapok).
```ts
contextBridge.exposeInMainWorld('api', { ping: () => ipcRenderer.invoke('ping') });
```
""",
"""# Jegyzet
Ez egy hanyagul megirt szoba, nincs leirasa es tagje sem.
Az epito a szovegbol tolti ki a feliratot.
""",
]
HEADER = 512
DOMAIN = b"KAPU-SIG-v1\n"

def slug(s):
    s = unicodedata.normalize("NFKD", s.lower())
    s = "".join(c for c in s if not unicodedata.combining(c))
    return re.sub(r"[^a-z0-9]+", "-", s).strip("-")[:64] or "szoba"
def toks(t): return max(1, round(len(t.encode()) / 4))   # becsles; a vegleges: o200k_base
def desc_budget(room_tokens): return max(15, min(300, round(room_tokens * 0.05)))
def esc(c): return "\n".join(("\\" + l) if re.match(r"^\\*@@@", l) else l for l in c.split("\n"))

body = b""; maplines = []; hashes = []; index = {}; links = []
for raw in ROOMS:
    cb = esc(raw).encode(); lines = raw.split("\n")
    title = next((l[2:].strip() for l in lines if l.startswith("# ")), lines[0].strip())
    rid = slug(title); rt = toks(raw); budget = desc_budget(rt)
    desc = next((l[2:].strip() for l in lines if l.startswith("> ")), "")
    if not desc:  # automatikus leiras: aranyos hossz, szohataron vagva
        desc = " ".join(l for l in lines if l.strip() and not l.startswith(("#", "tags:")))
        if len(desc) > budget * 4: desc = desc[: budget * 4].rsplit(" ", 1)[0] + "..."
    desc = desc[:1200]                              # kemeny felso korlat (kb. 300 token)
    tagl = next((l[5:] for l in lines if l.startswith("tags:")), "")
    tags = [t.strip() for t in tagl.split(",") if t.strip()] or [slug(l[3:]) for l in lines if l.startswith("## ")]
    marker = f"@@@ {rid}\n".encode(); start = len(body) + len(marker); body += marker + cb
    maplines.append(f"R {rid} | {title} | {rt}t {len(raw)}c | @{start}+{len(cb)} | {', '.join(tags)} | {desc}")
    hashes.append(f"{rid}\tb3:{blake3(cb)}")
    for w in set(re.findall(r"[a-z0-9]{3,}", (title + ' ' + ' '.join(tags)).lower())): index.setdefault(w, set()).add(rid)
    pos = 0; heads = []
    for l in cb.decode().split("\n"):
        if l.startswith("## "): heads.append((pos, l[3:].strip()))
        pos += len(l.encode()) + 1
    for i, (p, t) in enumerate(heads):
        end = heads[i + 1][0] if i + 1 < len(heads) else len(cb)
        seg = cb[p:end]; sid = f"{rid}/{slug(t)}"
        maplines.append(f"S {sid} | {t} | {toks(seg.decode())}t {len(seg.decode())}c | @{start+p}+{len(seg)}")
        hashes.append(f"{sid}\tb3:{blake3(seg)}")
        for w in set(re.findall(r"[a-z0-9]{3,}", t.lower())): index.setdefault(w, set()).add(sid)
        for tgt in re.findall(r"\]\(#([a-z0-9/-]+)\)", seg.decode()): links.append(f"L {sid} -> {tgt}")

sec_map = ("#MAP\n" + "\n".join(maplines + links) + "\n#END\n").encode()
sec_idx = ("#INDEX\n" + "\n".join(f"{k}\t{','.join(sorted(v))}" for k, v in sorted(index.items())) + "\n#END\n").encode()
sec_hash = ("#HASH\n" + "\n".join(sorted(hashes)) + "\n#END\n").encode()
sec_bodyhdr = b"#BODY\n"
o_map = HEADER; o_idx = o_map + len(sec_map); o_hash = o_idx + len(sec_idx)
o_body = o_hash + len(sec_hash) + len(sec_bodyhdr)

# DEMO kulcs: csak a mintafajlhoz, a titkos kulcs nem kerul ki sehova.
sk = Ed25519PrivateKey.generate()
pk = sk.public_key().public_bytes(ser.Encoding.Raw, ser.PublicFormat.Raw)
fields = [
 "%KAPU 0.1", "eol=LF", "enc=UTF-8", "tok=o200k_base", "guide=std-0.1",
 f"map={o_map}+{len(sec_map)}", f"index={o_idx}+{len(sec_idx)}", f"hash={o_hash}+{len(sec_hash)}",
 f"body={o_body}+{len(body)}", f"rooms={len(ROOMS)}", f"doors={len(maplines)}",
 f"bodyhash=b3:{blake3(body)}", f"key=ed25519:{base64.b64encode(pk).decode()}",
]
pre = ("\n".join(fields) + "\n").encode()
signed_msg = DOMAIN + pre + sec_map + sec_idx + sec_hash
sig = sk.sign(signed_msg)
hdr = pre + f"sig=ed25519:{base64.b64encode(sig).decode()}\n".encode()
assert len(hdr) < HEADER, len(hdr)
hdr += b" " * (HEADER - 1 - len(hdr)) + b"\n"
data = hdr + sec_map + sec_idx + sec_hash + sec_bodyhdr + body
open("minta.kapu", "wb").write(data)

# ---- ellenorzes, ahogy egy olvaso csinalna ----
f = open("minta.kapu", "rb").read()
h = f[:HEADER].decode(); kv = dict(l.split("=", 1) for l in h.split("\n")[1:] if "=" in l)
sig_line_start = f.index(b"\nsig=") + 1
def rng(k): a, b = map(int, kv[k].split("+")); return f[a:a + b]
msg = DOMAIN + f[:sig_line_start] + rng("map") + rng("index") + rng("hash")
Ed25519PublicKey.from_public_bytes(base64.b64decode(kv["key"].split(":")[1])).verify(
    base64.b64decode(kv["sig"].strip().split(":")[1]), msg)
hmap = dict(l.split("\t") for l in rng("hash").decode().split("\n")[1:-2])
b0 = int(kv["body"].split("+")[0])
for m in maplines:
    p = m.split(" | "); s, n = map(int, p[3][1:].split("+"))
    assert "b3:" + blake3(f[b0 + s:b0 + s + n]) == hmap[p[0][2:]], m
assert "b3:" + blake3(f[b0:]) == kv["bodyhash"]
# hamisitasi proba: egy bajt atirasa a torzsben
bad = bytearray(f); bad[b0 + 400] ^= 1
seg_ok = all("b3:" + blake3(bytes(bad[b0 + int(m.split(' | ')[3][1:].split('+')[0]): b0 + sum(map(int, m.split(' | ')[3][1:].split('+')))])) == hmap[m.split(' | ')[0][2:]] for m in maplines)
print("alairas OK, ajtok OK, torzs OK; hamisitott fajlnal minden ajto ep?", seg_ok, "| meret:", len(f), "| fejlec:", len(hdr.rstrip()), "bajt hasznos")
