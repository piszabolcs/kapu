"""Rebuild the two example files from this repository's own documents.

    python tools/make_examples.py [path/to/kapu]

examples/kapu-docs.kapu: the English documentation, one room per document:
the README (without its Hungarian part and the HTML image blocks),
bench/RESULTS.md, CHANGELOG.md and the READMEs of tests/vectors, tools and fuzz.
examples/kapu-spec.kapu: SPEC-KAPU.md split by tools/split_spec.py.

Both are built with the vector model in models/multilingual-e5-small-q8 and
checked with `kapu validate --full`. Run it after changing any of these files.
"""

import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MODEL = ROOT / "models" / "multilingual-e5-small-q8"

DOCS = [
    ("overview.md", "README.md"),
    ("results.md", "bench/RESULTS.md"),
    ("changelog.md", "CHANGELOG.md"),
    ("test-vectors.md", "tests/vectors/README.md"),
    ("python-tools.md", "tools/README.md"),
    ("fuzzing.md", "fuzz/README.md"),
]


def overview(readme: str) -> str:
    """The README as a room: its English part, without the HTML image blocks."""
    english = readme.split("\n---\n\n## Magyar", 1)[0]
    # from the end of the demo block: the opening paragraphs and everything after
    body = english.split("</p>\n", 1)[1] if "</p>\n" in english else english[english.index("> "):]
    body = re.sub(r"<p align=\"center\">.*?</p>\n*", "", body, flags=re.S)
    body = re.sub(r"<table>.*?</table>\n*", "", body, flags=re.S)
    return "# KAPU overview\n\n" + body.strip() + "\n"


def run(*cmd: str) -> None:
    print("$", " ".join(cmd))
    subprocess.run(cmd, check=True)


def main(kapu: str) -> None:
    rooms = ROOT / "target" / "docs-rooms"
    shutil.rmtree(rooms, ignore_errors=True)
    rooms.mkdir(parents=True)
    for name, source in DOCS:
        text = (ROOT / source).read_text(encoding="utf-8")
        (rooms / name).write_text(overview(text) if source == "README.md" else text, encoding="utf-8", newline="\n")
    docs = ROOT / "examples" / "kapu-docs.kapu"
    run(kapu, "import", "--model", str(MODEL), str(rooms), str(docs))
    run(kapu, "validate", "--full", "--model", str(MODEL), str(docs))

    spec_rooms = ROOT / "target" / "spec-rooms"
    shutil.rmtree(spec_rooms, ignore_errors=True)
    run(sys.executable, str(ROOT / "tools" / "split_spec.py"), str(spec_rooms))
    spec = ROOT / "examples" / "kapu-spec.kapu"
    run(kapu, "import", "--model", str(MODEL), str(spec_rooms), str(spec))
    run(kapu, "validate", "--full", "--model", str(MODEL), str(spec))


if __name__ == "__main__":
    main(str(Path(sys.argv[1]).resolve()) if len(sys.argv) > 1 else str(ROOT / "target" / "release" / "kapu"))
