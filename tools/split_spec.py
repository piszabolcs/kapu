"""Split SPEC-KAPU.md into one Markdown file per room, for the demo file
examples/kapu-spec.kapu: the specification packed as a KAPU file.

    python tools/split_spec.py target/spec-rooms
    target/release/kapu import target/spec-rooms examples/kapu-spec.kapu \
        --model models/potion-multilingual-128M-i8

Rooms: the preface (c00), every chapter (c01..c14), and every functional
requirement (fr-01..fr-20) as a room of its own. A chapter's ### sections become
its ## sub-doors.
"""

import re
import sys
import unicodedata
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def slug(text):
    text = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode()
    return re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")


def split_fenced(lines):
    """Yield (line, in_code) so headings inside code blocks are left alone."""
    fence = None
    for line in lines:
        m = re.match(r"^(```+|~~~+)", line)
        if m:
            if fence is None:
                fence = m.group(1)[:3]
            elif line.startswith(fence):
                yield line, True
                fence = None
                continue
        yield line, fence is not None


def main():
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "target/spec-rooms")
    out.mkdir(parents=True, exist_ok=True)
    for old in out.glob("*.md"):
        old.unlink()
    lines = (ROOT / "SPEC-KAPU.md").read_text(encoding="utf-8").splitlines()

    rooms = []  # (file stem, title, body lines)
    current = None
    for line, in_code in split_fenced(lines):
        chapter = None if in_code else re.match(r"^## (\d+)\. (.+)$", line)
        fr = None if in_code else re.match(r"^### (FR-(\d\d)) (.+)$", line)
        if current is None and line.startswith("# ") and not in_code:
            current = ("c00-specification", line[2:].strip(), [])
            rooms.append(current)
            continue
        if chapter:
            n, title = int(chapter.group(1)), chapter.group(2).strip()
            current = (f"c{n:02d}-{slug(title)}", f"{n}. {title}", [])
            rooms.append(current)
            continue
        if fr:
            current = (f"fr-{fr.group(2)}-{slug(fr.group(3))}", f"{fr.group(1)} {fr.group(3).strip()}", [])
            rooms.append(current)
            continue
        if current is None:
            continue
        if not in_code:
            if line.startswith("#### "):
                line = "## " + line[5:]
            elif line.startswith("### "):
                line = "## " + line[4:]
        current[2].append(line)

    for stem, title, body in rooms:
        text = "\n".join(body).strip("\n")
        (out / f"{stem}.md").write_text(f"# {title}\n\n{text}\n", encoding="utf-8", newline="\n")
    print(f"{len(rooms)} rooms in {out}")


if __name__ == "__main__":
    main()
