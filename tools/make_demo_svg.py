"""Write docs/assets/demo.svg: an animated terminal showing map, search and open.

    python tools/make_demo_svg.py [path/to/kapu]

The commands run for real on examples/kapu-docs.kapu; their output, cut to the
first lines, is typed and shown one scene after the other in a 15-second loop.
The animation is plain CSS inside the SVG, so GitHub plays it in the README.
"""

import subprocess
import sys
from html import escape
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FILE = "examples/kapu-docs.kapu"
SCENES = [
    (["map", FILE], 13, lambda lines: [l for l in lines if l[:2] in ("G ", "R ", "S ")]),
    (["search", FILE, "how do I connect it to Claude Code", "--k", "3"], 6, lambda lines: lines),
    (["open", FILE, "overview/use-it-from-claude-code"], 13, lambda lines: lines),
]
WIDTH, LINE, TOP, LEFT, CHARS = 900, 19, 58, 20, 104
SCENE_S, TYPE_S, SHOW_S = 5.0, 1.3, 1.5
BG, FG, MUTED, ACCENT, ID = "#1b1b19", "#e8e6df", "#9a988f", "#ee7a52", "#7fb0ee"


def cut(line: str) -> str:
    return line if len(line) <= CHARS else line[: CHARS - 1] + "…"


def output_line(line: str, fields: bool) -> str:
    """The first field of a map or search line in colour, the rest plain."""
    line = cut(line)
    if fields and " | " in line:
        head, rest = line.split(" | ", 1)
        return f'<tspan fill="{ID}">{escape(head)}</tspan><tspan fill="{MUTED}"> | </tspan>{escape(rest)}'
    if line.startswith("<kapu-room") or line.startswith("</kapu-room"):
        return f'<tspan fill="{MUTED}">{escape(line)}</tspan>'
    return escape(line)


def main(kapu: str) -> None:
    rows = max(n for _, n, _ in SCENES) + 2
    height = TOP + rows * LINE + 18
    total = SCENE_S * len(SCENES)
    pct = lambda s: f"{100 * s / total:.2f}%"
    css, groups = [], []
    for i, (args, keep, pick) in enumerate(SCENES):
        out = subprocess.run([kapu, *args], cwd=ROOT, capture_output=True, text=True, encoding="utf-8", check=True).stdout
        lines = [l for l in pick(out.splitlines()) if l.strip()][:keep]
        start = i * SCENE_S
        command = "kapu " + " ".join(f'"{a}"' if " " in a else a for a in args)
        typed = len(command) * 7.9
        css.append(
            f"@keyframes s{i}{{0%,{pct(start)}{{opacity:0}}{pct(start + 0.01)},{pct(start + SCENE_S - 0.25)}{{opacity:1}}"
            f"{pct(start + SCENE_S)},100%{{opacity:0}}}}"
            f".s{i}{{animation:s{i} {total}s linear infinite}}"
            f"@keyframes c{i}{{0%,{pct(start)}{{transform:translateX(0)}}{pct(start + TYPE_S)},100%{{transform:translateX({typed + 12:.0f}px)}}}}"
            f".c{i}{{animation:c{i} {total}s steps({len(command)}) infinite}}"
            f"@keyframes o{i}{{0%,{pct(start + SHOW_S)}{{opacity:0}}{pct(start + SHOW_S + 0.15)},100%{{opacity:1}}}}"
            f".o{i}{{animation:o{i} {total}s linear infinite}}"
        )
        y = TOP
        body = [
            f'<text x="{LEFT}" y="{y}"><tspan fill="{ACCENT}">$ </tspan>{escape(command)}</text>',
            f'<rect class="c{i}" x="{LEFT + 16}" y="{y - 14}" width="{typed + 12:.0f}" height="19" fill="{BG}"/>',
        ]
        text = "".join(
            f'<text x="{LEFT}" y="{y + (n + 1) * LINE}">{output_line(l, args[0] != "open")}</text>' for n, l in enumerate(lines)
        )
        body.append(f'<g class="o{i}">{text}</g>')
        groups.append(f'<g class="s{i}">{"".join(body)}</g>')

    svg = f"""<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{height}" viewBox="0 0 {WIDTH} {height}" role="img" aria-label="kapu map, kapu search and kapu open on the example file">
<style>text{{font-family:ui-monospace,"Cascadia Mono",Consolas,"DejaVu Sans Mono",monospace;font-size:13px;fill:{FG};white-space:pre}}{"".join(css)}</style>
<rect width="{WIDTH}" height="{height}" rx="10" fill="{BG}"/>
<circle cx="22" cy="20" r="6" fill="#ff5f57"/><circle cx="42" cy="20" r="6" fill="#febc2e"/><circle cx="62" cy="20" r="6" fill="#28c840"/>
<text x="{WIDTH / 2}" y="24" text-anchor="middle" style="fill:{MUTED}">kapu — {FILE}</text>
{"".join(groups)}
</svg>
"""
    out = ROOT / "docs" / "assets" / "demo.svg"
    out.write_text(svg, encoding="utf-8", newline="\n")
    print("written", out, len(svg.encode("utf-8")), "bytes")


if __name__ == "__main__":
    main(str(Path(sys.argv[1]).resolve()) if len(sys.argv) > 1 else str(ROOT / "target" / "release" / "kapu"))
