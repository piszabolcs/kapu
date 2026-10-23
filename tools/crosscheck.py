"""Cross-check the Python reference against the Rust builder on a corpus (A14).

    kapu-bench gen --rooms 300 --out <dir>
    kapu --no-model import <dir> <file.kapu>
    tools/.venv/Scripts/python tools/crosscheck.py <dir> <file.kapu>

Builds the same Markdown folder with tools/build_vectors.py (rooms in file-name
byte order, as `kapu import` does) and compares, section by section, with the
Rust-built file: #DESC, #GRAPH, #INDEX, #DOCS and #BODY byte for byte, and the
R/S lines of #MAP. Group assignment is left out: the reference uses connected
components, the Rust builder Leiden (D66, D92).
"""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).parent))
import build_vectors as bv  # noqa: E402


def sections(data: bytes) -> dict[str, bytes]:
    header = data[: bv.HEADER_LEN].decode()
    kv = dict(l.split("=", 1) for l in header.rstrip(" \n").split("\n")[1:])
    out = {}
    for name in ("map", "desc", "graph", "index", "docs", "body"):
        start, length = map(int, kv[name].split("+"))
        out[name] = data[start : start + length]
    return out


def door_lines(map_section: bytes) -> list[str]:
    lines = map_section.decode().split("\n")
    # drop the group id column of R lines: groups differ by design
    out = []
    for l in lines:
        if l.startswith("R "):
            f = l.split(" | ")
            f[3] = "g?"
            out.append(" | ".join(f))
        elif l.startswith("S "):
            out.append(l)
    return out


def main() -> None:
    folder, kapu_file = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
    files = sorted(folder.glob("*.md"), key=lambda p: p.name.encode())
    rooms = [p.read_text(encoding="utf-8") for p in files]
    ids = {i: bv.slug(p.stem) for i, p in enumerate(files)}
    doors, body = bv.build_doors(rooms, id_override=ids)
    ref = sections(bv.assemble(bv.derive(doors), body))
    rust = sections(kapu_file.read_bytes())
    failures = 0
    for name in ("desc", "graph", "index", "docs", "body"):
        same = ref[name] == rust[name]
        failures += not same
        print(f"{name:6} {'identical' if same else 'DIFFERENT'} ({len(ref[name])} / {len(rust[name])} bytes)")
        if not same:
            a, b = ref[name].decode().split("\n"), rust[name].decode().split("\n")
            for i, (x, y) in enumerate(zip(a, b)):
                if x != y:
                    print(f"   first difference at line {i + 1}:\n   ref:  {x[:160]!r}\n   rust: {y[:160]!r}")
                    break
    # the map is ordered by group, and the groups differ by design: compare as sets
    same = sorted(door_lines(ref["map"])) == sorted(door_lines(rust["map"]))
    failures += not same
    print(f"map R/S lines {'identical' if same else 'DIFFERENT'}")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
