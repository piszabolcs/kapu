"""PDF access like a PDF reader (study pilot), logged like tools/kapu_mcp_call.py.

    python tools/study/pdf_call.py <file.pdf> info [--tag t]
    python tools/study/pdf_call.py <file.pdf> pages <from> <to> [--tag t]
    python tools/study/pdf_call.py <file.pdf> find "<regex>" [--tag t]

`pages` returns the extracted text of at most 20 pages (the limit of a PDF
read in Claude Code); `find` is a reader's search: at most 30 hits as
`p.<page>: snippet`. Page texts are cached next to the PDF.
"""

import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from calllog import pop_tag, run  # noqa: E402

MAX_PAGES, MAX_HITS = 20, 30


def page_texts(pdf: Path):
    cache = pdf.with_suffix(".pages.json")
    if cache.is_file() and cache.stat().st_mtime >= pdf.stat().st_mtime:
        return json.loads(cache.read_text(encoding="utf-8"))
    import pypdf
    texts = [p.extract_text() or "" for p in pypdf.PdfReader(str(pdf)).pages]
    cache.write_text(json.dumps(texts, ensure_ascii=False), encoding="utf-8")
    return texts


def info(pdf: Path) -> str:
    return f"{pdf.name}: {len(page_texts(pdf))} pages"


def pages(pdf: Path, a: int, b: int) -> str:
    texts = page_texts(pdf)
    a, b = max(1, a), min(len(texts), b, a + MAX_PAGES - 1)
    return "\n\n".join(f"--- page {n} ---\n{texts[n - 1]}" for n in range(a, b + 1))


def find(pdf: Path, pattern: str) -> str:
    rx, hits, total = re.compile(pattern, re.I), [], 0
    for n, text in enumerate(page_texts(pdf), 1):
        for m in rx.finditer(text):
            total += 1
            if len(hits) < MAX_HITS:
                s = " ".join(text[max(0, m.start() - 60):m.end() + 60].split())
                hits.append(f"p.{n}: ...{s}...")
    tail = f"\n... {total - MAX_HITS} more hits" if total > MAX_HITS else ""
    return (f"{total} hits\n" + "\n".join(hits) + tail) if total else "0 hits"


def main(argv):
    tag, argv = pop_tag(argv)
    if len(argv) < 2:
        print(__doc__)
        return 2
    pdf, cmd = Path(argv[0]), argv[1]
    if cmd == "info":
        return run("info", {}, tag, lambda: info(pdf))
    if cmd == "pages" and len(argv) >= 4:
        a, b = int(argv[2]), int(argv[3])
        return run("pages", {"from": a, "to": b}, tag, lambda: pages(pdf, a, b))
    if cmd == "find" and len(argv) >= 3:
        return run("find", {"pattern": argv[2]}, tag, lambda: find(pdf, argv[2]))
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
