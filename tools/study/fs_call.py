"""File-search tools over a room folder (study pilot, system C3), logged like
tools/kapu_mcp_call.py.

    python tools/study/fs_call.py <dir> ls [--tag t]
    python tools/study/fs_call.py <dir> grep "<regex>" [--tag t]
    python tools/study/fs_call.py <dir> read <file> [--start N] [--lines M] [--tag t]

`grep` is case-insensitive and returns at most 50 `file:line: text` lines;
`read` returns at most 400 numbered lines.
"""

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from calllog import pop_tag, run  # noqa: E402

MAX_GREP, MAX_LINES, MAX_LS = 50, 400, 500


def ls(d: Path) -> str:
    names = sorted(p.name for p in d.glob("*.md"))
    more = f"\n... {len(names) - MAX_LS} more" if len(names) > MAX_LS else ""
    return f"{len(names)} files\n" + "\n".join(names[:MAX_LS]) + more


def grep(d: Path, pattern: str) -> str:
    rx = re.compile(pattern, re.I)
    out, total = [], 0
    for f in sorted(d.glob("*.md")):
        for n, line in enumerate(f.read_text(encoding="utf-8").splitlines(), 1):
            if rx.search(line):
                total += 1
                if len(out) < MAX_GREP:
                    out.append(f"{f.name}:{n}: {line.strip()[:200]}")
    tail = f"\n... {total - MAX_GREP} more matches" if total > MAX_GREP else ""
    return (f"{total} matches\n" + "\n".join(out) + tail) if total else "0 matches"


def read(d: Path, name: str, start: int, count: int) -> str:
    f = (d / name).resolve()
    if f.parent != d.resolve() or not f.is_file():
        raise ValueError(f"no such file: {name}")
    lines = f.read_text(encoding="utf-8").splitlines()
    count = min(count, MAX_LINES)
    part = lines[start - 1:start - 1 + count]
    rest = len(lines) - (start - 1 + len(part))
    body = "\n".join(f"{start + i}\t{l}" for i, l in enumerate(part))
    return body + (f"\n... {rest} more lines (use --start {start + len(part)})" if rest > 0 else "")


def opt(argv, name, default):
    return int(argv[argv.index(name) + 1]) if name in argv else default


def main(argv):
    tag, argv = pop_tag(argv)
    if len(argv) < 2:
        print(__doc__)
        return 2
    d, cmd = Path(argv[0]), argv[1]
    if cmd == "ls":
        return run("ls", {}, tag, lambda: ls(d))
    if cmd == "grep" and len(argv) >= 3:
        return run("grep", {"pattern": argv[2]}, tag, lambda: grep(d, argv[2]))
    if cmd == "read" and len(argv) >= 3:
        start, count = opt(argv, "--start", 1), opt(argv, "--lines", MAX_LINES)
        return run("read", {"file": argv[2], "start": start, "lines": count}, tag, lambda: read(d, argv[2], start, count))
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
