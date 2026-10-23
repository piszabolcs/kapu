"""Turn a public documentation tree into a flat KAPU room folder (study pilot).

    python tools/study/prepare_corpus.py rust-book <src> <out_dir>
    python tools/study/prepare_corpus.py k8s <src> <out_dir> [--redirects <file>]
    python tools/study/prepare_corpus.py mdn <src> <out_dir> [--only web,glossary] [--exclude web/api]

Writes `<id>.md` rooms (first line `# <title>`, front matter, HTML comments and
template macros removed), `manifest.json` ({id: {url, src, title}}),
`urlmap.json` ({canonical url key: id}, redirects and aliases included) and
`corpus.txt` (every room in id order, for the full-context run). Ids are
ASCII slugs of at most 64 bytes, so `kapu import` keeps them unchanged.
"""

import json
import re
import sys
import unicodedata
from pathlib import Path

import tiktoken

ENC = tiktoken.get_encoding("o200k_base")
MAX_ID = 64


def slug(text: str) -> str:
    """The FR-04/4 slug of `kapu import` (text.rs `slug`) for one file stem."""
    out, dash = [], False
    for c in unicodedata.normalize("NFKD", text.lower()):
        if unicodedata.combining(c):
            continue
        if ("a" <= c <= "z") or ("0" <= c <= "9"):
            out.append(c)
            dash = False
        elif not dash:
            out.append("-")
            dash = True
    s = "".join(out).strip("-")[:MAX_ID].strip("-")
    return s or "szoba"


def unique(base: str, used: set) -> str:
    rid, n = base, 2
    while rid in used:
        suffix = f"-{n}"
        rid = base[: MAX_ID - len(suffix)].rstrip("-") + suffix
        n += 1
    used.add(rid)
    return rid


def split_front_matter(text: str):
    """Returns (fields, body); fields holds `title`, `slug` and the `aliases` list."""
    if not text.startswith("---\n"):
        return {}, text
    end = text.find("\n---\n", 4)
    if end < 0:
        return {}, text
    fields, aliases, in_aliases = {}, [], False
    for line in text[4:end].splitlines():
        m = re.match(r"^(title|slug|short-title):\s*(.*)$", line)
        if m:
            fields[m.group(1)] = m.group(2).strip().strip("\"'")
            in_aliases = False
            continue
        if re.match(r"^aliases:\s*$", line):
            in_aliases = True
            continue
        m = re.match(r"^aliases:\s*\[(.*)\]\s*$", line)
        if m:
            aliases += [a.strip().strip("\"'") for a in m.group(1).split(",") if a.strip()]
            continue
        if in_aliases and re.match(r"^\s*-\s+", line):
            aliases.append(re.sub(r"^\s*-\s+", "", line).strip().strip("\"'"))
            continue
        if not line.startswith(" "):
            in_aliases = False
    fields["aliases"] = aliases
    return fields, text[end + 5:]


COMMENT = re.compile(r"<!--.*?-->", re.S)


def rust_includes(body: str, here: Path) -> str:
    """Resolves mdBook `{{#include}}` / `{{#rustdoc_include}}` (anchors and line ranges)."""

    def repl(m):
        spec = m.group(2).strip()
        path, _, sel = spec.partition(":")
        f = (here / path).resolve()
        if not f.is_file():
            return ""
        lines = f.read_text(encoding="utf-8", errors="replace").splitlines()
        if sel and not re.match(r"^[\d:]+$", sel):
            start = next((i for i, l in enumerate(lines) if re.search(rf"ANCHOR:\s*{re.escape(sel)}\b", l)), None)
            stop = next((i for i, l in enumerate(lines) if re.search(rf"ANCHOR_END:\s*{re.escape(sel)}\b", l)), None)
            if start is not None and stop is not None:
                lines = lines[start + 1:stop]
        elif sel:
            nums = [int(x) if x else None for x in sel.split(":")]
            a = (nums[0] or 1) - 1
            b = nums[1] if len(nums) > 1 and nums[1] else len(lines)
            lines = lines[a:b]
        return "\n".join(l for l in lines if "ANCHOR" not in l)

    return re.sub(r"\{\{#(include|rustdoc_include)\s+([^}]+)\}\}", repl, body)


def hugo_shortcodes(body: str) -> str:
    body = re.sub(r'\{\{<\s*glossary_tooltip[^>]*?text="([^"]*)"[^>]*>\}\}', r"\1", body)
    body = re.sub(r'\{\{<\s*ref\s+"([^"]*)"\s*>\}\}', r"\1", body)
    body = re.sub(r'\{\{%\s*heading\s+"([^"]*)"\s*%\}\}', lambda m: m.group(1).replace("whatsnext", "What's next").replace("prerequisites", "Before you begin").capitalize(), body)
    body = re.sub(r"\{\{<\s*(note|caution|warning)\s*>\}\}", lambda m: f"**{m.group(1).capitalize()}:**", body)
    body = re.sub(r"\{\{[<%].*?[>%]\}\}", "", body, flags=re.S)
    return body


def mdn_macros(body: str) -> str:
    def repl(m):
        inner = m.group(1).strip()
        arg = re.match(r"^\w+\s*\(\s*[\"']([^\"']*)[\"']", inner)
        return arg.group(1) if arg else ""

    return re.sub(r"\{\{([^{}]*)\}\}", repl, body)


def clean(body: str) -> str:
    body = COMMENT.sub("", body)
    return re.sub(r"\n{3,}", "\n\n", body).strip() + "\n"


def first_h1(body: str):
    m = re.search(r"^#\s+(.+)$", body, re.M)
    return m.group(1).strip() if m else None


def url_key(kind: str, url: str):
    """Canonical lookup key of a docs URL, or None when it is not a page of the corpus."""
    url = url.strip().split("#")[0].split("?")[0]
    if kind == "rust-book":
        m = re.search(r"doc\.rust-lang\.org/(?:(?:stable|beta|nightly|[\d.]+)/)?book/(?:(?:second|first|2018)-edition/)?([\w-]+)\.html", url)
        return m.group(1).lower() if m else None
    if kind == "k8s":
        m = re.search(r"(?:kubernetes\.io|k8s\.io)(/docs/[^\s)\"'>]*)", url)
        if not m:
            return None
        path = re.sub(r"(index)?\.html?$", "", m.group(1).lower())
        return path if path.endswith("/") else path + "/"
    if kind == "mdn":
        m = re.search(r"developer\.mozilla\.org/(?:[a-z]{2}(?:-[a-z]{2})?/)?docs/([^\s)\"'>]+)", url, re.I)
        return ("/en-us/docs/" + m.group(1).rstrip("/")).lower() if m else None
    raise ValueError(kind)


def read_redirects(kind: str, path):
    """{old key: new key} from a Netlify (k8s) or MDN `_redirects.txt` file."""
    if not path or not Path(path).is_file():
        return {}
    out = {}
    for line in Path(path).read_text(encoding="utf-8", errors="replace").splitlines():
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        parts = line.split()
        if len(parts) < 2 or "*" in parts[0] or ":" in parts[0]:
            continue
        old, new = parts[0], parts[1]
        if kind == "k8s":
            old_k = url_key("k8s", "https://kubernetes.io" + (old if old.startswith("/docs") else "/docs" + old))
            new_k = url_key("k8s", "https://kubernetes.io" + new) if new.startswith("/docs") else None
        else:
            old_k = url_key("mdn", "https://developer.mozilla.org" + old)
            new_k = url_key("mdn", "https://developer.mozilla.org" + new)
        if old_k and new_k:
            out[old_k] = new_k
    return out


def collect(kind: str, src: Path, only, exclude=()):
    """Yields (id_base, url, url_key, title, body, src_rel, extra_keys)."""
    if kind == "rust-book":
        # sub-chapter pages start at `##`; their titles live in the table of contents
        summary = (src / "src" / "SUMMARY.md").read_text(encoding="utf-8")
        titles = {m.group(2): m.group(1) for m in re.finditer(r"\[([^\]]+)\]\(([\w-]+\.md)\)", summary)}
        for f in sorted((src / "src").glob("*.md")):
            if f.name == "SUMMARY.md":
                continue
            body = rust_includes(f.read_text(encoding="utf-8"), f.parent)
            key = f.stem.lower()
            yield f.stem, f"https://doc.rust-lang.org/book/{f.stem}.html", key, first_h1(body) or titles.get(f.name, f.stem), body, f"src/{f.name}", []
    elif kind == "k8s":
        root = src / "content" / "en" / "docs"
        for f in sorted(root.rglob("*.md")):
            rel = f.relative_to(root).with_suffix("")
            parts = list(rel.parts)
            if parts[-1] == "_index":
                parts = parts[:-1]
            if not parts:
                parts = ["home"]
            fields, body = split_front_matter(f.read_text(encoding="utf-8", errors="replace"))
            path = "/docs/" + "/".join(parts) + "/"
            aliases = [a for a in fields.get("aliases", []) if a.startswith("/")]
            extra = [url_key("k8s", "https://kubernetes.io" + (a if a.startswith("/docs") else "/docs" + a)) for a in aliases]
            yield "-".join(parts), "https://kubernetes.io" + path, path.lower(), fields.get("title") or parts[-1], hugo_shortcodes(body), str(f.relative_to(src)), [k for k in extra if k]
    elif kind == "mdn":
        root = src / "files" / "en-us"
        for f in sorted(root.rglob("index.md")):
            rel = f.parent.relative_to(root)
            if only and rel.parts and rel.parts[0] not in only:
                continue
            if any(rel.as_posix() == x or rel.as_posix().startswith(x + "/") for x in exclude):
                continue
            fields, body = split_front_matter(f.read_text(encoding="utf-8", errors="replace"))
            page = fields.get("slug") or "/".join(rel.parts)
            url = "https://developer.mozilla.org/en-US/docs/" + page
            yield "-".join(rel.parts), url, url_key("mdn", url), fields.get("title") or rel.parts[-1], mdn_macros(body), str(f.relative_to(src)), []
    else:
        raise ValueError(kind)


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return 2
    kind, src, out = argv[0], Path(argv[1]), Path(argv[2])
    redirects_file = argv[argv.index("--redirects") + 1] if "--redirects" in argv else None
    only = set(argv[argv.index("--only") + 1].split(",")) if "--only" in argv else None
    exclude = tuple(argv[argv.index("--exclude") + 1].split(",")) if "--exclude" in argv else ()
    if kind == "mdn" and not redirects_file:
        redirects_file = src / "files" / "en-us" / "_redirects.txt"
    out.mkdir(parents=True, exist_ok=True)
    for old in out.glob("*.md"):
        old.unlink()
    used, manifest, urlmap, total = set(), {}, {}, 0
    for base, url, key, title, body, rel, extra in collect(kind, src, only, exclude):
        rid = unique(slug(base), used)
        body = clean(body)
        if first_h1(body) is None or not body.lstrip().startswith("# "):
            body = f"# {title}\n\n{body}"
        (out / f"{rid}.md").write_text(body, encoding="utf-8", newline="\n")
        manifest[rid] = {"url": url, "src": rel, "title": title}
        for k in [key, *extra]:
            urlmap.setdefault(k, rid)
        total += len(ENC.encode(body, disallowed_special=()))
    redirects = read_redirects(kind, redirects_file)
    for old, new in redirects.items():
        seen, target = 0, new
        while target not in urlmap and target in redirects and seen < 5:
            target, seen = redirects[target], seen + 1
        if target in urlmap:
            urlmap.setdefault(old, urlmap[target])
    meta = out.parent / f"{out.name}.meta"
    meta.mkdir(exist_ok=True)
    (meta / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=0), encoding="utf-8")
    (meta / "urlmap.json").write_text(json.dumps(urlmap, ensure_ascii=False, indent=0), encoding="utf-8")
    with (meta / "corpus.txt").open("w", encoding="utf-8", newline="\n") as f:
        for rid in sorted(manifest):
            f.write(f"\n\n===== {rid} =====\n\n" + (out / f"{rid}.md").read_text(encoding="utf-8"))
    print(json.dumps({"kind": kind, "rooms": len(manifest), "tokens": total, "url_keys": len(urlmap)}))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
