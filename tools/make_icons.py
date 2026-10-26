"""Draw the KAPU app icon (an arched gate in a framed tile, with the map lines inside) and
write every size the desktop window needs, plus the SVG used by the README and the interface.
The geometry follows the icon artwork of 2026-10-26 (`kapuicon.jpg`, 2048 px, measured
and halved to the 1024 grid).

    python tools/make_icons.py            (needs Pillow)

Outputs: crates/kapu-gui/icons/*, crates/kapu-gui/ui/icon.svg, docs/assets/logo.{svg,png}.
Colours are the interface tokens (crates/kapu-gui/ui/styles/tokens.css), converted
from OKLCH here so the icon and the interface stay the same colour.
"""

import math
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
ICONS = ROOT / "crates" / "kapu-gui" / "icons"
UI = ROOT / "crates" / "kapu-gui" / "ui"
DOCS = ROOT / "docs" / "assets"

# Geometry on a 1024 grid: a dark tile, a light frame inside it, an arched gate
# (centre line of the stroke) and three centred map lines of different length
# under the arch, the doors with their prices.
TILE_RADIUS = 224
FRAME = (129, 128, 895, 895)  # centre line of the frame's stroke
FRAME_RADIUS, FRAME_W = 131, 15
ARCH_CX, ARCH_CY, ARCH_R, ARCH_BOTTOM, ARCH_W = 512, 465, 189, 748, 39
BARS = ((440, 410, 584, 450), (394, 492, 630, 532), (451, 574, 573, 614))


def oklch_to_hex(l, c, h):
    a = c * math.cos(math.radians(h))
    b = c * math.sin(math.radians(h))
    l_ = (l + 0.3963377774 * a + 0.2158037573 * b) ** 3
    m_ = (l - 0.1055613458 * a - 0.0638541728 * b) ** 3
    s_ = (l - 0.0894841775 * a - 1.2914855480 * b) ** 3
    rgb = (
        4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_,
        -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_,
        -0.0041960863 * l_ - 0.7034186147 * m_ + 1.7076147010 * s_,
    )

    def encode(x):
        x = min(1.0, max(0.0, x))
        x = 12.92 * x if x <= 0.0031308 else 1.055 * x ** (1 / 2.4) - 0.055
        return round(x * 255)

    return "#{:02x}{:02x}{:02x}".format(*(encode(x) for x in rgb))


TILE = oklch_to_hex(0.19, 0.014, 255)   # --card (dark)
GATE = oklch_to_hex(0.70, 0.16, 38)     # --primary (dark)
ROOMC = oklch_to_hex(0.96, 0.005, 255)  # --foreground (dark)


def draw(size):
    scale = 4  # supersample, then shrink for smooth edges
    big = size * scale
    k = big / 1024
    img = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    def box(r):
        return tuple(round(v * k) for v in r)

    d.rounded_rectangle((0, 0, big - 1, big - 1), radius=round(TILE_RADIUS * k), fill=TILE)
    fh = FRAME_W / 2
    frame = (FRAME[0] - fh, FRAME[1] - fh, FRAME[2] + fh, FRAME[3] + fh)
    d.rounded_rectangle(box(frame), radius=round((FRAME_RADIUS + fh) * k), outline=ROOMC, width=round(FRAME_W * k))
    half = ARCH_W / 2
    outer = (ARCH_CX - ARCH_R - half, ARCH_CY - ARCH_R - half, ARCH_CX + ARCH_R + half, ARCH_CY + ARCH_R + half)
    d.arc(box(outer), start=180, end=360, fill=GATE, width=round(ARCH_W * k))
    for x in (ARCH_CX - ARCH_R, ARCH_CX + ARCH_R):
        d.rectangle(box((x - half, ARCH_CY, x + half, ARCH_BOTTOM)), fill=GATE)
        d.ellipse(box((x - half, ARCH_BOTTOM - half, x + half, ARCH_BOTTOM + half)), fill=GATE)
    for bar in BARS:
        d.rounded_rectangle(box(bar), radius=round((bar[3] - bar[1]) / 2 * k), fill=ROOMC)
    return img.resize((size, size), Image.LANCZOS)


def svg():
    left, right = ARCH_CX - ARCH_R, ARCH_CX + ARCH_R
    arch = (
        f'<path d="M{left} {ARCH_BOTTOM} V{ARCH_CY} A{ARCH_R} {ARCH_R} 0 0 1 {right} {ARCH_CY} V{ARCH_BOTTOM}" '
        f'fill="none" stroke="{GATE}" stroke-width="{ARCH_W}" stroke-linecap="round"/>'
    )
    bars = [
        f'<rect x="{x0}" y="{y0}" width="{x1 - x0}" height="{y1 - y0}" rx="{(y1 - y0) // 2}" fill="{ROOMC}"/>'
        for x0, y0, x1, y1 in BARS
    ]
    x0, y0, x1, y1 = FRAME
    frame = (
        f'<rect x="{x0}" y="{y0}" width="{x1 - x0}" height="{y1 - y0}" rx="{FRAME_RADIUS}" '
        f'fill="none" stroke="{ROOMC}" stroke-width="{FRAME_W}"/>'
    )
    parts = [f'<rect width="1024" height="1024" rx="{TILE_RADIUS}" fill="{TILE}"/>', frame, arch, *bars]
    body = "\n  ".join(parts)
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" role="img" aria-label="KAPU">\n'
        f"  {body}\n</svg>\n"
    )


def main():
    ICONS.mkdir(parents=True, exist_ok=True)
    DOCS.mkdir(parents=True, exist_ok=True)
    draw(32).save(ICONS / "32x32.png")
    draw(128).save(ICONS / "128x128.png")
    draw(256).save(ICONS / "128x128@2x.png")
    draw(512).save(ICONS / "icon.png")
    draw(256).save(ICONS / "icon.ico", sizes=[(s, s) for s in (16, 24, 32, 48, 64, 128, 256)])
    draw(1024).save(ICONS / "icon.icns")
    draw(512).save(DOCS / "logo.png")
    for target in (UI / "icon.svg", DOCS / "logo.svg"):
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(svg(), encoding="utf-8", newline="\n")
    print("tile", TILE, "gate", GATE, "room", ROOMC)


if __name__ == "__main__":
    main()
