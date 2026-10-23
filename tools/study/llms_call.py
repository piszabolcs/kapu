"""llms.txt-style access to a room folder (study pilot, system C4), logged like
tools/kapu_mcp_call.py.

    python tools/study/llms_call.py <dir> index [--tag t]
    python tools/study/llms_call.py <dir> read <file> [--tag t]

`index` is a generated llms.txt: one `- [title](file): first sentence` line per
page. `read` returns a whole page, as an llms.txt link would.
"""

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from calllog import pop_tag, run  # noqa: E402


def first_sentence(text: str) -> str:
    for block in re.split(r"\n\s*\n", text):
        b = block.strip()
        if not b or b.startswith(("#", "```", "|", "<", "-", "*", ">")):
            continue
        s = re.split(r"(?<=[.!?])\s", " ".join(b.split()), maxsplit=1)[0]
        return s[:150]
    return ""


def index(d: Path) -> str:
    lines = [f"# {d.name}", ""]
    for f in sorted(d.glob("*.md")):
        text = f.read_text(encoding="utf-8")
        m = re.search(r"^#\s+(.+)$", text, re.M)
        title = m.group(1).strip() if m else f.stem
        lines.append(f"- [{title}]({f.name}): {first_sentence(text)}")
    return "\n".join(lines)


def read(d: Path, name: str) -> str:
    f = (d / name).resolve()
    if f.parent != d.resolve() or not f.is_file():
        raise ValueError(f"no such file: {name}")
    return f.read_text(encoding="utf-8")


def main(argv):
    tag, argv = pop_tag(argv)
    if len(argv) >= 2 and argv[1] == "index":
        return run("index", {}, tag, lambda: index(Path(argv[0])))
    if len(argv) >= 3 and argv[1] == "read":
        return run("read", {"file": argv[2]}, tag, lambda: read(Path(argv[0]), argv[2]))
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
