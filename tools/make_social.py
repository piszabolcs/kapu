"""The repository's social preview image (1280 x 640, GitHub Settings > Social preview).

    py -3.12 tools/make_social.py   # needs Pillow

Writes docs/assets/social-preview.png: the app icon, the name, one line of
what it is, three measured facts (bench/RESULTS.md chapters 5, 11, 12) and the
graph view of the desktop window. The text uses Geist, the window's own
SIL OFL font (crates/kapu-gui/ui/fonts).
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
FONTS = ROOT / "crates" / "kapu-gui" / "ui" / "fonts"
W, H = 1280, 640
BG = (13, 17, 23)          # GitHub dark canvas
FG = (239, 242, 245)       # the icon's room colour
MUTED = (154, 164, 175)
ACCENT = (240, 118, 79)    # the icon's gate colour
CHIP = (24, 30, 38)
LINE = (48, 56, 66)


def font(name, size, weight):
    f = ImageFont.truetype(str(FONTS / name), size)
    f.set_variation_by_axes([weight])
    return f


def main():
    img = Image.new("RGB", (W, H), BG)
    d = ImageDraw.Draw(img)
    sans = lambda size, weight=400: font("geist-latin-wght-normal.woff2", size, weight)
    mono = lambda size, weight=400: font("geist-mono-latin-wght-normal.woff2", size, weight)

    # the graph view, right, faded into the background on its left edge
    shot = Image.open(ROOT / "docs" / "screenshots" / "03-graph.png").convert("RGB")
    shot = shot.resize((560, round(560 * shot.height / shot.width)))
    x0, y0 = W - shot.width + 30, (H - shot.height) // 2 + 30
    mask = Image.new("L", shot.size, 255)
    md = ImageDraw.Draw(mask)
    for i in range(220):
        md.line([(i, 0), (i, shot.height)], fill=round(255 * i / 220))
    img.paste(shot, (x0, y0), mask)

    logo = Image.open(ROOT / "docs" / "assets" / "logo.png").convert("RGBA").resize((96, 96))
    img.paste(logo, (64, 64), logo)
    d.text((180, 66), "KAPU", font=sans(76, 700), fill=FG)
    d.text((184, 150), "one indexed knowledge file for AI agents", font=mono(19, 400), fill=ACCENT)

    lines = ["Pack Markdown docs and agent skills into one", "text file. Claude Code, Codex or a local model",
             "searches it over MCP and reads only what it", "needs, within a token budget."]
    for i, line in enumerate(lines):
        d.text((64, 214 + i * 38), line, font=sans(28, 400), fill=FG)

    facts = [("92% vs 28%", "correct on manuals the model never saw"),
             ("23 of 24", "said \"not in the docs\" instead of guessing"),
             ("$0.099 vs $0.236", "per task, finding 91 agent skills")]
    y = 400
    for big, small in facts:
        d.rounded_rectangle([64, y, 660, y + 54], radius=12, fill=CHIP, outline=LINE)
        d.text((84, y + 12), big, font=sans(24, 700), fill=ACCENT)
        bw = d.textlength(big, font=sans(24, 700))
        d.text((84 + bw + 16, y + 16), small, font=sans(19, 400), fill=MUTED)
        y += 66

    d.text((64, H - 44), "github.com/piszabolcs/kapu   ·   Rust · MCP · offline · MIT", font=mono(17, 400), fill=MUTED)
    out = ROOT / "docs" / "assets" / "social-preview.png"
    img.save(out, optimize=True)
    print("written", out, out.stat().st_size, "bytes")


if __name__ == "__main__":
    main()
