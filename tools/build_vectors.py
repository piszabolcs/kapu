#!/usr/bin/env python3
"""Build the M1 test vectors: tests/vectors/minta.kapu plus one invalid file per
error code (E001-E011) and two reader edge cases (FR-15).

This script is the executable reference for SPEC-KAPU.md v1.0. The Rust builder
(M3) must reproduce minta.kapu byte for byte, so every rule that the spec leaves
open is pinned here and recorded in DONTESNAPLO.md (D56-D66).

Developer tool only. Run from the repository root:

    tools/.venv/Scripts/python tools/build_vectors.py

Dependencies: tiktoken (o200k_base prices), snowballstemmer (FR-09 stemming).
BLAKE3 comes from the pure-Python reference next to this file.
"""

from __future__ import annotations

import json
import math
import re
import sys
import unicodedata
from dataclasses import dataclass, field
from pathlib import Path

import snowballstemmer
import tiktoken

sys.path.insert(0, str(Path(__file__).parent))
from blake3_ref import blake3  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "vectors"

HEADER_LEN = 512
FORMAT_VERSION = "1.0"
# D121: group names leave out the stop words of the YAKE list (same file as Rust).
# D131: words of API parameter tables, never a group's topic (build.rs GROUP_NAME_SKIP)
GROUP_NAME_SKIP = {"bool", "default", "false", "float", "int", "let", "none", "null", "optional", "required", "str", "true"}

STOPWORDS = {
    line.strip()
    for line in (ROOT / "crates" / "kapu-core" / "data" / "stopwords.txt").read_text(encoding="utf-8").splitlines()
    if line.strip() and not line.startswith("#")
}
# D57: the sample is built without a vector model. `emb=none` and the BLAKE3 of
# the empty input mark this; every reader's packaged model differs, so FR-10/4
# (`vector: unavailable`) is exercised by the sample itself.
EMB_NAME = "none"
EMB_HASH = blake3(b"")
TITLE_MAX = 200
TAG_MAX = 64
TAGS_MAX = 32
SHORT_TAGS = 5

ENC = tiktoken.get_encoding("o200k_base")
STEM_HU = snowballstemmer.stemmer("hungarian")
STEM_EN = snowballstemmer.stemmer("english")

# FR-05 template, exactly what `kapu new` writes (ends with LF).
TEMPLATE = (
    "# \n"
    "<!-- Ide jön a szoba címe. Ha üres marad, a szövegből lesz. -->\n"
    "\n"
    "> \n"
    "<!-- Rövid leírás: mi van ebben a szobában. Ha üres marad, a szövegből lesz. -->\n"
    "\n"
    "tags: \n"
    "<!-- Keresőszavak vesszővel. Ha üres marad, a szövegből lesznek. -->\n"
    "\n"
    "<!-- Innen jöhet a tartalom, sima Markdownban. -->\n"
)

# --------------------------------------------------------------------------
# Sample body: 3 rooms, 4 sub-doors, 2 links, `@@@` escapes, one `kapu new`
# template room. Room 1 has no trailing LF, room 2 has one (both are legal).
# --------------------------------------------------------------------------
ROOMS: list[str] = [
    """# Frontend design alapok
> Sötét téma, glassmorphism, bronz és türkiz színek: az alap stílus minden apphoz.
tags: frontend, design, glass, színek, css, gomb

## Színek
A paletta három alapszínre épül, a `--turkiz` változó a kiemelés színe.
```css
:root {
  --bg: #12161c;
  --bronz: #c08a4a;
  --turkiz: #3fb6a8;
  --glass: rgba(255, 255, 255, 0.08);
}
```

## Glass gomb
Az üvegszerű gomb elmosott hátteret kap, lásd még az [Electron preload](#electron-leckek/preload-bridge) alajtót.
```css
.btn-glass {
  background: var(--glass);
  backdrop-filter: blur(12px);
  border: 1px solid rgba(255, 255, 255, 0.18);
  border-radius: 12px;
  color: var(--turkiz);
}
```""",
    """# Electron leckék
> Visszatérő Electron hibák és a helyes megoldásuk.
tags: electron, preload, ipc, hiba

## Preload bridge
Soha ne kapcsold ki a contextIsolation-t. A renderer csak a preloadban kiengedett függvényeket látja, a stílushoz lásd a [Frontend design alapok](#frontend-design-alapok) szobát.
```ts
contextBridge.exposeInMainWorld('api', { ping: () => ipcRenderer.invoke('ping') });
## ez a sor kódblokkban van, nem alajtó
```

## Szobajelölő | escape
A `@@@` sor csak a sor elején jelöl szobát. Egy tartalomsor így is kezdődhet:
```
@@@ hamis
\\@@@ ez sem szoba
```
Íráskor mindkét sor még egy visszaperjelet kap, olvasáskor lekerül.
""",
    TEMPLATE,
]


# --------------------------------------------------------------------------
# Text rules
# --------------------------------------------------------------------------
def nfc(s: str) -> str:
    return unicodedata.normalize("NFC", s)


def slug(title: str) -> str:
    """FR-04/4. D58: an empty slug becomes `szoba`."""
    s = unicodedata.normalize("NFKD", title.lower())
    s = "".join(c for c in s if not unicodedata.combining(c))
    s = re.sub(r"[^a-z0-9]+", "-", s).strip("-")[:64].strip("-")
    return s or "szoba"


def escape_body(content: str) -> str:
    """FR-04/2: one extra backslash before lines that are `\\*@@@...`."""
    return "\n".join(
        ("\\" + line) if re.match(r"^\\*@@@", line) else line
        for line in content.split("\n")
    )




def strip_comments(text: str) -> str:
    """D59/D108: remove every `<!-- ... -->` span (also across lines); a line of
    the result that had a comment removed and is now blank is dropped with its
    LF; every other line stays. An unclosed `<!--` is kept as it is."""
    out: list[str] = []
    touched: set[int] = set()
    line = 0
    i, n = 0, len(text)
    while i < n:
        if text.startswith("<!--", i):
            close = text.find("-->", i + 4)
            if close == -1:
                rest = text[i:]
                out.append(rest)
                break
            touched.add(line)
            i = close + 3
            continue
        c = text[i]
        out.append(c)
        if c == "\n":
            line += 1
        i += 1
    lines = "".join(out).split("\n")
    return "\n".join(l for k, l in enumerate(lines) if not (k in touched and not l.strip()))


def tokens(text: str) -> int:
    return len(ENC.encode(text, disallowed_special=()))


def code_fence_mask(lines: list[str]) -> list[bool]:
    """True for lines inside a ``` or ~~~ fenced block (fence lines included)."""
    mask, fence = [], None
    for line in lines:
        s = line.lstrip()
        if fence is None:
            m = re.match(r"^(`{3,}|~{3,})", s)
            if m:
                fence = m.group(1)[0] * 3
                mask.append(True)
                continue
            mask.append(False)
        else:
            mask.append(True)
            if s.startswith(fence):
                fence = None
    return mask


def paragraphs(text: str, first_only: bool) -> str:
    """FR-06/2 source text: paragraphs outside headings, comments, code and `tags:`.
    Line breaks become spaces."""
    lines = strip_comments(text).split("\n")
    mask = code_fence_mask(lines)
    paras, cur = [], []
    for line, in_code in zip(lines, mask):
        skip = (
            in_code
            or line.startswith("#")
            or line.startswith("> ")
            or line.strip() == ">"
            or line.startswith("tags:")
        )
        if not line.strip() or skip:
            if cur:
                paras.append(" ".join(cur))
                cur = []
            continue
        cur.append(line.strip())
    if cur:
        paras.append(" ".join(cur))
    if not paras:
        return ""
    return paras[0] if first_only else " ".join(paras)


def cut_to_tokens(text: str, target: int) -> str:
    """D60: longest word-boundary prefix with at most `target` tokens, no suffix."""
    if tokens(text) <= target:
        return text
    words = text.split(" ")
    out: list[str] = []
    for w in words:
        cand = " ".join(out + [w])
        if tokens(cand) > target:
            break
        out.append(w)
    return " ".join(out)


def auto_description(text: str, room_tokens: int, first_only: bool) -> str:
    target = max(15, min(300, round(0.05 * room_tokens)))
    return cut_to_tokens(paragraphs(text, first_only), target)


# --------------------------------------------------------------------------
# FR-09 tokenizer and stemming
# --------------------------------------------------------------------------
_IDENT = re.compile(r"[^\W]+(?:[-_][^\W]+)*", re.UNICODE)


def split_subtokens(ident: str) -> list[str]:
    parts = re.split(r"[^0-9A-Za-z\u00C0-\u024F\u0370-\u03FF\u0400-\u04FF]+", ident)
    out: list[str] = []
    for p in parts:
        if not p:
            continue
        # camelCase and letter/digit boundaries
        p = re.sub(r"([a-z\u00DF-\u00FF])([A-Z\u00C0-\u00DE])", r"\1 \2", p)
        p = re.sub(r"([A-Za-z\u00C0-\u024F])([0-9])", r"\1 \2", p)
        p = re.sub(r"([0-9])([A-Za-z\u00C0-\u024F])", r"\1 \2", p)
        out.extend(p.split())
    return out


def index_tokens(text: str) -> list[str]:
    """D61: lower-cased NFC sub-tokens plus the whole identifier when it differs."""
    out: list[str] = []
    for m in _IDENT.finditer(text):
        ident = m.group(0).strip("-_")
        if not ident:
            continue
        subs = [nfc(s.lower()) for s in split_subtokens(ident)]
        whole = nfc(ident.lower())
        out.extend(subs)
        if len(subs) != 1 or subs[0] != whole:
            out.append(whole)
    return out


def stems(term: str) -> list[str]:
    """D61: the term itself and the Hungarian and English Snowball stems, deduplicated."""
    seen: list[str] = []
    for t in (term, STEM_HU.stemWord(term), STEM_EN.stemWord(term)):
        if t and t not in seen:
            seen.append(t)
    return seen


# --------------------------------------------------------------------------
# Doors
# --------------------------------------------------------------------------
@dataclass
class Door:
    id: str
    title: str
    description: str
    tags: list[str]
    stored: bytes  # escaped bytes as they sit in #BODY
    served: str  # unescaped, comments removed
    offset: int  # relative to body start
    is_room: bool
    tokens: int = 0
    chars: int = 0
    dl: int = 0
    terms: dict[str, int] = field(default_factory=dict)  # stemmed entry -> tf
    raw_terms: dict[str, int] = field(default_factory=dict)  # unstemmed -> tf
    gid: str = ""
    links: list[str] = field(default_factory=list)


def heading_title(text: str) -> str:
    """FR-06/1: the first `# ` line; if it is empty or missing, the first
    non-empty line that is not a heading marker, `>` or `tags:` line (D87)."""
    lines = strip_comments(text).split("\n")
    first = next((line[2:].strip() for line in lines if line.startswith("# ")), "")
    if first:
        return first
    for line in lines:
        s = line.strip()
        if s and s not in ("#", ">") and not s.startswith(("# ", "> ", "tags:")):
            return s
    return ""


def manual_description(text: str) -> str:
    for line in strip_comments(text).split("\n"):
        if line.startswith("> ") and line[2:].strip():
            return line[2:].strip()
    return ""


def manual_tags(text: str) -> list[str]:
    for line in strip_comments(text).split("\n"):
        if line.startswith("tags:"):
            return [t.strip()[:TAG_MAX] for t in line[5:].split(",") if t.strip()][:TAGS_MAX]
    return []


def unescape(stored: bytes) -> str:
    return "\n".join(
        line[1:] if re.match(r"^\\+@@@", line) else line
        for line in stored.decode("utf-8").split("\n")
    )


def indexed_text(text: str) -> str:
    """D62: content for the index is the served text without the `tags:` line."""
    return "\n".join(l for l in text.split("\n") if not l.startswith("tags:"))


def count_terms(door: Door) -> None:
    raw: dict[str, int] = {}
    stemmed: dict[str, int] = {}
    dl = 0

    def add(text: str, weight: int) -> None:
        nonlocal dl
        for t in index_tokens(text):
            dl += weight
            raw[t] = raw.get(t, 0) + weight
            for s in stems(t):
                stemmed[s] = stemmed.get(s, 0) + weight

    add(door.title, 3)
    add(" ".join(door.tags), 3)
    add(door.description, 1)
    add(indexed_text(door.served), 1)
    door.terms, door.raw_terms, door.dl = stemmed, raw, dl


def find_links(text: str) -> list[str]:
    lines = text.split("\n")
    mask = code_fence_mask(lines)
    out: list[str] = []
    for line, in_code in zip(lines, mask):
        if in_code:
            continue
        out.extend(re.findall(r"\]\(#([a-z0-9][a-z0-9/-]*)\)", line))
    return out


def build_doors(rooms: list[str], id_override: dict[int, str] | None = None) -> tuple[list[Door], bytes]:
    doors: list[Door] = []
    body = b""
    used: set[str] = set()
    for i, raw in enumerate(rooms):
        title = heading_title(raw)
        rid = slug(title)
        if id_override and i in id_override:
            rid = id_override[i]
        base, n = rid, 2
        while rid in used:
            rid = f"{base}-{n}"
            n += 1
        used.add(rid)
        stored = escape_body(raw).encode("utf-8")
        marker = f"@@@ {rid}\n".encode()
        offset = len(body) + len(marker)
        body += marker + stored + b"\n"  # D36 separator LF
        served = strip_comments(unescape(stored))
        room = Door(rid, title[:TITLE_MAX], "", [], stored, served, offset, True)
        room.tokens, room.chars = tokens(served), len(served)
        manual = manual_description(raw)
        # D88: a manual description is cut only at the 300-token hard limit
        room.description = cut_to_tokens(manual, 300) if manual else auto_description(raw, room.tokens, False)
        room.tags = manual_tags(raw)  # YAKE fallback is not exercised by the sample (D63)
        doors.append(room)

        # FR-07 sub-doors over the stored bytes
        lines = stored.decode("utf-8").split("\n")
        mask = code_fence_mask(lines)
        pos, heads = 0, []
        for line, in_code in zip(lines, mask):
            if line.startswith("## ") and not in_code:
                heads.append((pos, line[3:].strip()))
            pos += len(line.encode("utf-8")) + 1
        sub_used: set[str] = set()
        subs: list[Door] = []
        for k, (p, t) in enumerate(heads):
            end = heads[k + 1][0] if k + 1 < len(heads) else len(stored)
            seg = stored[p:end]
            sid_base = f"{rid}/{slug(t)}"
            sid, n = sid_base, 2
            while sid in sub_used:
                sid = f"{sid_base}-{n}"
                n += 1
            sub_used.add(sid)
            served_seg = strip_comments(unescape(seg))
            sub = Door(sid, t[:TITLE_MAX], "", [], seg, served_seg, offset + p, False)
            sub.tokens, sub.chars = tokens(served_seg), len(served_seg)
            sub.description = auto_description(unescape(seg), sub.tokens, True)
            subs.append(sub)
        # D64: a link belongs to the most specific door that contains it
        room_links = find_links(room.served)
        for sub in subs:
            sub.links = find_links(sub.served)
            for l in sub.links:
                if l in room_links:
                    room_links.remove(l)
        room.links = room_links
        doors.extend(subs)
    for d in doors:
        count_terms(d)
    return doors, body


# --------------------------------------------------------------------------
# Derived sections
# --------------------------------------------------------------------------
def fmt_weight(w: float) -> str:
    """D65: at most 4 decimals, trailing zeros removed, at least one decimal."""
    s = f"{w:.4f}".rstrip("0")
    return s + "0" if s.endswith(".") else s


def build_graph(doors: list[Door]) -> list[tuple[str, str, str, float]]:
    ids = {d.id for d in doors}
    edges: list[tuple[str, str, str, float]] = []
    for d in doors:
        room = d.id.split("/")[0]
        for target in d.links:
            # D123: `#x` also names a section of the same room; an unknown target
            # without `/` is an in-page anchor (no edge, no warning).
            if target not in ids and "/" not in target and f"{room}/{target}" in ids:
                target = f"{room}/{target}"
            if target in ids and target != d.id:
                edges.append((d.id, target, "link", 1.0))
    # `term` edges (FR-11/2) between rooms over room-level documents.
    rooms = [d for d in doors if d.is_room]
    n = len(rooms)
    df: dict[str, int] = {}
    for r in rooms:
        for t in r.terms:
            df[t] = df.get(t, 0) + 1
    idf = {t: math.log((n - c + 0.5) / (c + 0.5) + 1) for t, c in df.items()}
    # D93: the top-10% IDF threshold is taken among the terms that two or more
    # rooms share; a term of a single room can never be a shared term
    shared_vals = sorted((v for t, v in idf.items() if df[t] >= 2), reverse=True)
    if shared_vals:
        k = max(1, math.ceil(0.1 * len(shared_vals)))
        threshold = shared_vals[k - 1]
        rare = {t for t, v in idf.items() if df[t] >= 2 and v >= threshold}
        sums: dict[tuple[str, str], float] = {}
        for i, a in enumerate(rooms):
            for b in rooms[i + 1 :]:
                shared = (set(a.terms) & set(b.terms)) & rare
                if shared:
                    x, y = sorted((a.id, b.id))
                    # D81: fixed summation order (UTF-8 byte order), never set order
                    sums[(x, y)] = sum(idf[t] for t in sorted(shared, key=lambda t: t.encode()))
        if sums:
            mx = max(sums.values())
            for (x, y), s in sums.items():
                edges.append((x, y, "term", s / mx))
    # `sim` edges need the model (D57): none in the sample.
    edges.sort(key=lambda e: (e[0], e[1], e[2]))
    return edges


def build_groups(doors: list[Door], edges) -> list[tuple[str, str, list[Door]]]:
    """D66: on the sample's graph Leiden (gamma=1, seed 42) equals the connected
    components; the script uses components and M4 must confirm the result."""
    rooms = [d for d in doors if d.is_room]
    room_of = {d.id: d.id.split("/")[0] for d in doors}
    parent = {r.id: r.id for r in rooms}

    def find(x: str) -> str:
        while parent[x] != x:
            x = parent[x]
        return x

    for a, b, _, _ in edges:
        ra, rb = find(room_of[a]), find(room_of[b])
        if ra != rb:
            parent[rb] = ra
    comps: dict[str, list[Door]] = {}
    for r in rooms:
        comps.setdefault(find(r.id), []).append(r)
    ordered = sorted(comps.values(), key=lambda rs: min(r.id for r in rs))
    g = len(ordered)
    # class-level TF-IDF group names on unstemmed terms: tf * (1 + ln(G/df));
    # candidates are at least 3 characters long, not purely numeric (D66) and
    # not stop words (D121) and not parameter-table words (D131).
    def candidate(t: str) -> bool:
        return len(t) >= 3 and not t.isdigit() and t not in STOPWORDS and t not in GROUP_NAME_SKIP

    gdf: dict[str, int] = {}
    for rs in ordered:
        for t in {t for r in rs for t in r.raw_terms if candidate(t)}:
            gdf[t] = gdf.get(t, 0) + 1
    out = []
    for i, rs in enumerate(ordered, 1):
        tf: dict[str, int] = {}
        for r in rs:
            for t, c in r.raw_terms.items():
                if candidate(t):
                    tf[t] = tf.get(t, 0) + c
        # D105: scores equal to 1e-9 tie and are broken by byte order
        # (floor(x + 0.5) is Rust's round() for the positive scores here)
        def tie_key(kv):
            score = kv[1] * (1 + math.log(g / gdf[kv[0]]))
            return (-math.floor(score * 1e9 + 0.5), kv[0].encode())

        scored = sorted(tf.items(), key=tie_key)
        name = ", ".join(t for t, _ in scored[:3])
        gid = f"g{i}"
        for r in rs:
            r.gid = gid
        out.append((gid, name, rs))
    for d in doors:
        if not d.is_room:
            d.gid = next(r.gid for r in rooms if r.id == d.id.split("/")[0])
    return out


INVISIBLE = {"\u200b", "\u200c", "\u200d", "\u2060", "\ufeff"}


def derived(s: str) -> str:
    """D135: NFC text without the invisible characters FR-02/5 forbids in a
    derived section (the body may hold them)."""
    return "".join(c for c in nfc(s) if c not in INVISIBLE)


def esc_field(s: str) -> str:
    """FR-13 field escape on NFC text (FR-02/6, D82): `\\` -> `\\\\` first,
    then `|` -> `\\|` (D107), TAB -> space; invisible characters out (D135)."""
    return derived(s).replace("\\", "\\\\").replace("|", "\\|").replace("\t", " ")


def section(name: str, lines: list[str]) -> bytes:
    return (f"#{name}\n" + "".join(l + "\n" for l in lines) + "#END\n").encode("utf-8")


def derive(doors: list[Door]) -> dict[str, bytes]:
    edges = build_graph(doors)
    groups = build_groups(doors, edges)
    rooms_in_order = [d for d in doors if d.is_room]

    map_lines: list[str] = []
    for gid, name, rs in groups:
        map_lines.append(f"G {gid} | {esc_field(name)} | {len(rs)} | {sum(r.tokens for r in rs)}t")
        for r in rooms_in_order:
            if r.gid != gid:
                continue
            map_lines.append(
                f"R {r.id} | {esc_field(r.title)} | {r.tokens}t | {gid} | {esc_field(', '.join(r.tags[:SHORT_TAGS]))}"
            )
            for s in doors:
                if not s.is_room and s.id.split("/")[0] == r.id:
                    map_lines.append(f"S {s.id} | {esc_field(s.title)} | {s.tokens}t")

    desc_lines = [
        f"{d.id}\t{derived(d.description).replace(chr(9), ' ')}\t{derived(', '.join(d.tags)).replace(chr(9), ' ')}" for d in doors
    ]
    graph_lines = [f"{a}\t{b}\t{t}\t{fmt_weight(w)}" for a, b, t, w in edges]

    postings: dict[str, dict[str, int]] = {}
    for d in doors:
        for t, tf in d.terms.items():
            postings.setdefault(t, {})[d.id] = tf
    index_lines = [
        f"{t}\t{len(p)}\t" + ",".join(f"{i}:{tf}" for i, tf in sorted(p.items(), key=lambda kv: kv[0].encode()))
        for t, p in sorted(postings.items(), key=lambda kv: kv[0].encode())
    ]
    docs_lines = [
        f"{d.id}\t@{d.offset}+{len(d.stored)}\tb3:{blake3(d.stored)}\tdl={d.dl}\tc={d.chars}"
        for d in sorted(doors, key=lambda d: d.id.encode())
    ]
    return {
        "MAP": section("MAP", map_lines),
        "DESC": section("DESC", desc_lines),
        "GRAPH": section("GRAPH", graph_lines),
        "INDEX": section("INDEX", index_lines),
        "DOCS": section("DOCS", docs_lines),
        "_counts": json.dumps(
            {"rooms": len(rooms_in_order), "doors": len(doors), "groups": len(groups)}
        ).encode(),
    }


def assemble(sections: dict[str, bytes], body: bytes, overrides: dict[str, str] | None = None) -> bytes:
    counts = json.loads(sections["_counts"])
    order = ["MAP", "DESC", "GRAPH", "INDEX", "DOCS"]
    pos = HEADER_LEN
    spans: dict[str, str] = {}
    for name in order:
        spans[name.lower()] = f"{pos}+{len(sections[name])}"
        pos += len(sections[name])
    body_start = pos + len(b"#BODY\n")
    fields = {
        "eol": "LF",
        "enc": "UTF-8",
        "tok": "o200k_base",
        "emb": EMB_NAME,
        "embhash": f"b3:{EMB_HASH}",
        **spans,
        "body": f"{body_start}+{len(body)}",
        "rooms": str(counts["rooms"]),
        "doors": str(counts["doors"]),
        "groups": str(counts["groups"]),
        "bodyhash": f"b3:{blake3(body)}",
        "guide": "std-1",
    }
    if overrides:
        fields.update(overrides)
    header = (f"%KAPU {FORMAT_VERSION}\n" + "".join(f"{k}={v}\n" for k, v in fields.items())).encode()
    if len(header) > HEADER_LEN - 1:
        raise SystemExit(f"header overflow: {len(header)} bytes (D39)")
    header += b" " * (HEADER_LEN - 1 - len(header)) + b"\n"
    return header + b"".join(sections[n] for n in order) + b"#BODY\n" + body


# --------------------------------------------------------------------------
# Independent read-back check, the way a reader would do it (FR-01, FR-03, FR-14)
# --------------------------------------------------------------------------
def verify(data: bytes, doors: list[Door]) -> None:
    assert len(data) >= HEADER_LEN and data[HEADER_LEN - 1:HEADER_LEN] == b"\n", "E002"
    assert b"\r" not in data, "E003"
    header = data[:HEADER_LEN].decode("utf-8")
    lines = header.rstrip(" \n").split("\n")
    assert lines[0] == f"%KAPU {FORMAT_VERSION}", "E001"
    kv = dict(l.split("=", 1) for l in lines[1:])
    pos = HEADER_LEN
    for name in ("map", "desc", "graph", "index", "docs"):
        start, length = map(int, kv[name].split("+"))
        assert start == pos, (name, start, pos)
        sec = data[start : start + length]
        assert sec.startswith(f"#{name.upper()}\n".encode()) and sec.endswith(b"#END\n"), name
        pos = start + length
    body_start, body_len = map(int, kv["body"].split("+"))
    assert data[pos:body_start] == b"#BODY\n" and body_start + body_len == len(data), "E005"
    body = data[body_start:]
    assert kv["bodyhash"] == f"b3:{blake3(body)}", "E006"
    docs = data[int(kv["docs"].split("+")[0]) :].split(b"#END\n", 1)[0].decode("utf-8").split("\n")[1:-1]
    assert len(docs) == int(kv["doors"]) == len(doors)
    ids = set()
    for line in docs:
        did, span, h, dl, c = line.split("\t")
        assert re.fullmatch(r"[a-z0-9][a-z0-9-]{0,63}(/[a-z0-9][a-z0-9-]{0,63})?", did), ("E008", did)
        assert did not in ids, ("E008 duplicate", did)
        ids.add(did)
        start, length = map(int, span[1:].split("+"))
        seg = body[start : start + length]
        assert h == f"b3:{blake3(seg)}", ("E007", did)
        door = next(d for d in doors if d.id == did)
        assert seg == door.stored and unescape(seg) == (ROOMS[[d.id for d in doors if d.is_room].index(did)] if door.is_room else unescape(seg))
    # room boundaries from the @@@ lines must agree with #DOCS (FR-15 edge 2 rebuild)
    marks = [m.start() for m in re.finditer(rb"(?m)^@@@ ", body)]
    assert marks == [d.offset - len(f"@@@ {d.id}\n") for d in doors if d.is_room]
    # the escape round-trips: the stored room 2 differs from the source, the served text does not
    r2 = next(d for d in doors if d.id == "electron-leckek")
    assert b"\\\\@@@" in r2.stored and "\n@@@ hamis\n" in unescape(r2.stored)
    print("read-back check ok")


# --------------------------------------------------------------------------
# Vectors
# --------------------------------------------------------------------------
def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    doors, body = build_doors(ROOMS)
    sections = derive(doors)
    sample = assemble(sections, body)
    expected: dict[str, object] = {}

    def write(name: str, data: bytes, expect: str, note: str, **extra: object) -> None:
        (OUT / name).write_bytes(data)
        expected[name] = {"expect": expect, "note": note, **extra}

    write(
        "minta.kapu",
        sample,
        "ok",
        "valid sample: 3 rooms, 4 sub-doors, 2 link edges, @@@ escapes, kapu-new template room; built without a model",
        doors=[
            {
                "id": d.id,
                "title": d.title,
                "tokens": d.tokens,
                "chars": d.chars,
                "offset": d.offset,
                "length": len(d.stored),
                "hash": blake3(d.stored),
                "group": d.gid,
                "served": d.served,
            }
            for d in doors
        ],
        header_body=f"{sample.index(b'#BODY' + b'\n') + 6}+{len(body)}",
    )

    def mutate_section(name: str, old: bytes, new: bytes) -> dict[str, bytes]:
        s = dict(sections)
        assert old in s[name], (name, old)
        s[name] = s[name].replace(old, new, 1)
        return s

    # E001 bad magic
    write("E001-magic.kapu", sample.replace(b"%KAPU 1.0", b"%KAPX 1.0", 1), "E001", "magic is %KAPX")
    # E002 header does not end with LF at byte 511
    e002 = bytearray(sample)
    e002[HEADER_LEN - 1] = ord(" ")
    write("E002-header.kapu", bytes(e002), "E002", "byte 511 is a space, not LF")
    # E003 CR inside a derived section (header recomputed, so only the CR is wrong)
    s = mutate_section("MAP", "Electron leckék |".encode(), "Electron leckék\r |".encode())
    write("E003-cr.kapu", assemble(s, body), "E003", "CR inside #MAP")
    # E004 forbidden control character in a derived section
    s = mutate_section("DESC", "Visszatérő".encode(), "Vissza\x1btérő".encode())
    write("E004-control.kapu", assemble(s, body), "E004", "ESC (0x1B) inside #DESC")
    # E005 section span mismatch: desc length overstated by 7 bytes
    desc_span = f"{HEADER_LEN + len(sections['MAP'])}+{len(sections['DESC']) + 7}"
    write("E005-span.kapu", assemble(sections, body, {"desc": desc_span}), "E005", "desc= length overstated, overlaps #GRAPH")
    # E006 bodyhash mismatch (body intact)
    wrong = ("0" * 63 + "1")
    write("E006-bodyhash.kapu", assemble(sections, body, {"bodyhash": f"b3:{wrong}"}), "E006", "bodyhash field is wrong, body intact")
    # E007 one door fingerprint wrong in #DOCS
    good = blake3(doors[0].stored)
    s = mutate_section("DOCS", f"b3:{good}".encode(), f"b3:{good[:-1] + ('0' if good[-1] != '0' else '1')}".encode())
    write("E007-doorhash.kapu", assemble(s, body), "E007", f"fingerprint of {doors[0].id} altered in #DOCS", door=doors[0].id)
    # E008 invalid identifier (uppercase) used consistently everywhere
    bad_doors, bad_body = build_doors(ROOMS, id_override={2: "Szoba"})
    write("E008-id.kapu", assemble(derive(bad_doors), bad_body), "E008", "room id `Szoba` violates the id grammar")
    # E009 index posting points to a door that does not exist
    s = dict(sections)
    s["INDEX"] = s["INDEX"].replace(b"#END\n", b"zzz\t1\tnincs-ilyen:1\n#END\n")
    write("E009-dangling.kapu", assemble(s, body), "E009", "#INDEX posting references `nincs-ilyen`")
    # E010 caption above the hard limit: 201-character title in #MAP
    long_title = "a" * (TITLE_MAX + 1)
    s = mutate_section("MAP", "| Electron leckék |".encode(), f"| {long_title} |".encode())
    write("E010-caption.kapu", assemble(s, body), "E010", "201-character title in #MAP")
    # E011 unknown hash prefix
    write("E011-hashprefix.kapu", assemble(sections, body, {"bodyhash": f"sha256:{blake3(body)}"}), "E011", "bodyhash uses sha256: prefix")

    # FR-15 edge 1: a door changed in place (same length) -> fingerprint mismatch,
    # retry still fails, in-memory rebuild serves the new bytes with `index: stale`.
    body_start = sample.index(b"#BODY\n") + 6
    room1 = doors[0]
    old = "Sötét téma".encode()
    new = "Sötét témo".encode()
    assert len(old) == len(new)
    e1 = bytearray(sample)
    seg = e1[body_start + room1.offset : body_start + room1.offset + len(room1.stored)]
    assert old in seg
    seg = bytes(seg).replace(old, new, 1)
    e1[body_start + room1.offset : body_start + room1.offset + len(room1.stored)] = seg
    write(
        "edge-stale-same-length.kapu",
        bytes(e1),
        "stale",
        "room 1 edited in place with equal length; open must serve the edited text with index: stale",
        door=room1.id,
        served=strip_comments(unescape(seg)),
    )
    # FR-15 edge 2: a room grew, so every later position is shifted.
    insert = "\nÚj sor, amit kézzel írtak a fájlba.".encode()
    e2 = bytearray(sample)
    at = body_start + room1.offset + len(room1.stored)
    e2[at:at] = insert
    write(
        "edge-stale-shifted.kapu",
        bytes(e2),
        "stale",
        "room 1 grew by one line; positions of later doors are off, rebuild from @@@ lines",
        door=doors[0].id,
        served=strip_comments(unescape(room1.stored + insert)),
        second_door="electron-leckek",
        second_served=next(d.served for d in doors if d.id == "electron-leckek"),
    )

    (OUT / "expected.json").write_text(json.dumps(expected, ensure_ascii=False, indent=1) + "\n", encoding="utf-8", newline="\n")
    verify(sample, doors)
    print(f"wrote {len(expected)} vectors to {OUT}")
    print(f"minta.kapu: {len(sample)} bytes, header text {len(sample[:HEADER_LEN].rstrip())} bytes")
    for d in doors:
        print(f"  {d.id:45} {d.tokens:4}t {d.chars:5}c @{d.offset}+{len(d.stored)} {d.gid}")


if __name__ == "__main__":
    main()
