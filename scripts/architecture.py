#!/usr/bin/env python3
"""Draw the architecture as a 16 bit illustration, in the launcher's own font.

    scripts/architecture.py [OUT.png]

The picture is composed at 640x400 with an 8x8 bitmap font — the very font the
launcher draws with, read straight out of `shell/src/font8x8.rs` — and then
scaled by two with nearest neighbour, so every pixel stays a pixel. Colours
are Tokyo Night, the theme the launcher ships with.

Needs Pillow. No other dependency, and nothing from the shell at runtime.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
FONT_RS = ROOT / "shell" / "src" / "font8x8.rs"

W, H = 640, 400
SCALE = 2

BG = (7, 9, 15)
PANEL = (26, 27, 38)
PANEL2 = (32, 34, 48)
FG = (192, 202, 245)
DIM = (86, 95, 137)
ACCENT = (122, 162, 247)
CYAN = (125, 207, 255)
MAGENTA = (187, 154, 247)
GREEN = (158, 206, 106)
ORANGE = (255, 158, 100)
RED = (247, 118, 142)
GLASS = (12, 16, 28)


def load_font() -> list[list[int]]:
    """The 8x8 font as 128 rows of 8 bytes, bit 0 being the leftmost pixel."""
    text = FONT_RS.read_text()
    rows = re.findall(r"\[((?:\s*0x[0-9a-fA-F]{2}\s*,?){8})\]", text)
    font = []
    for row in rows:
        font.append([int(b, 16) for b in re.findall(r"0x([0-9a-fA-F]{2})", row)])
    if len(font) < 128:
        raise SystemExit(f"font8x8.rs gave {len(font)} glyphs, expected 128")
    return font[:128]


FONT = load_font()


class Canvas:
    def __init__(self, w: int, h: int, bg):
        self.im = Image.new("RGB", (w, h), bg)
        self.d = ImageDraw.Draw(self.im)

    # -- primitives ------------------------------------------------------
    def rect(self, x, y, w, h, c):
        if w <= 0 or h <= 0:
            return
        self.d.rectangle([x, y, x + w - 1, y + h - 1], fill=c)

    def frame(self, x, y, w, h, c):
        self.d.rectangle([x, y, x + w - 1, y + h - 1], outline=c)

    def dither(self, x, y, w, h, c, step=2):
        """A checkerboard fill: the cheapest 16 bit shading there is."""
        for yy in range(y, y + h):
            for xx in range(x + (yy % step), x + w, step):
                if 0 <= xx < self.im.width and 0 <= yy < self.im.height:
                    self.im.putpixel((xx, yy), c)

    def text(self, x, y, s, c, scale=1):
        for i, ch in enumerate(s):
            code = ord(ch) if ord(ch) < 128 else ord("?")
            for ry, bits in enumerate(FONT[code]):
                for rx in range(8):
                    if bits & (1 << rx):
                        self.rect(
                            x + (i * 8 + rx) * scale, y + ry * scale, scale, scale, c
                        )

    def text_centered(self, cx, y, s, c, scale=1):
        self.text(cx - len(s) * 8 * scale // 2, y, s, c, scale)

    # -- pieces ----------------------------------------------------------
    def box(self, x, y, w, h, title, colour, fill=PANEL):
        """A panel with a shadow, a lit top edge and a title in its corner."""
        self.dither(x + 3, y + 3, w, h, (14, 16, 24))
        self.rect(x, y, w, h, fill)
        self.frame(x, y, w, h, colour)
        self.rect(x + 1, y + 1, w - 2, 1, tuple(min(255, v + 24) for v in fill))
        if title:
            self.rect(x + 1, y + 1, w - 2, 9, colour)
            self.text(x + 4, y + 2, title, BG)

    def arrow(self, x0, y0, x1, y1, c, label="", label_above=True):
        """Straight arrow, horizontal or vertical, with a chunky head."""
        if y0 == y1:
            step = 1 if x1 > x0 else -1
            for x in range(x0, x1, step):
                if (x // 2) % 2 == 0 or True:
                    self.rect(x, y0, 1, 1, c)
            for k in range(4):
                self.rect(x1 - step * k, y0 - k, 1, 2 * k + 1, c)
            if label:
                ly = y0 - 12 if label_above else y0 + 5
                self.text(min(x0, x1) + 4, ly, label, c)
        else:
            step = 1 if y1 > y0 else -1
            for y in range(y0, y1, step):
                self.rect(x0, y, 1, 1, c)
            for k in range(4):
                self.rect(x0 - k, y1 - step * k, 2 * k + 1, 1, c)
            if label:
                self.text(x0 + 6, min(y0, y1) + 6, label, c)

    def television(self, x, y, w, h):
        """A tube in pixels: cabinet, bezel, glass, scanlines, a stand."""
        self.dither(x + 4, y + 4, w, h, (14, 16, 24))
        self.rect(x, y, w, h, (38, 40, 54))
        self.frame(x, y, w, h, DIM)
        self.rect(x + 1, y + 1, w - 2, 1, (58, 62, 82))
        # Screen, inset, with rounded corners suggested by two clipped pixels.
        sx, sy, sw, sh = x + 6, y + 6, w - 12, h - 22
        self.rect(sx, sy, sw, sh, GLASS)
        self.frame(sx, sy, sw, sh, (18, 22, 36))
        for cx, cy in ((sx, sy), (sx + sw - 1, sy), (sx, sy + sh - 1), (sx + sw - 1, sy + sh - 1)):
            self.rect(cx, cy, 1, 1, (38, 40, 54))
        # Scanlines first, then the picture on top of them, or the wordmark
        # comes out too dark to read at this size.
        for yy in range(sy, sy + sh, 2):
            self.dither(sx, yy, sw, 1, (0, 0, 0), step=1)
        self.text_centered(sx + sw // 2, sy + sh // 2 - 6, "OMACRT", CYAN)
        self.rect(sx + 10, sy + sh // 2 + 11, sw - 20, 1, (40, 52, 84))
        self.rect(sx + 10, sy + sh // 2 + 15, sw - 34, 1, (30, 40, 64))
        # Glow spilling past the bezel, the way a tube does in a dark room.
        for k in range(1, 4):
            c = (10 + 4 * (4 - k), 14 + 5 * (4 - k), 26 + 8 * (4 - k))
            self.frame(sx - k, sy - k, sw + 2 * k, sh + 2 * k, c)
        # Speaker grille and stand.
        self.dither(x + 6, y + h - 13, w - 12, 6, (58, 62, 82), step=2)
        self.rect(x + w // 2 - 8, y + h, 16, 3, (38, 40, 54))
        self.rect(x + w // 2 - 16, y + h + 3, 32, 2, DIM)


def draw() -> Image.Image:
    c = Canvas(W, H, BG)

    # A faint grid, so the picture reads as a diagram and not a poster.
    for y in range(0, H, 8):
        c.dither(0, y, W, 1, (13, 15, 23), step=4)

    # ------------------------------------------------------------ title
    c.rect(0, 0, W, 16, PANEL2)
    c.rect(0, 16, W, 1, ACCENT)
    c.text(8, 4, "OMACRT", FG)
    c.text(8 + 7 * 8, 4, "ARCHITECTURE", ACCENT)
    right = "320x240 -> 3520x240 @ 15.7 kHz"
    c.text(W - 8 - len(right) * 8, 4, right, DIM)

    # ---------------------------------------------------------- desktop
    c.box(8, 32, 240, 152, "OMARCHY DESKTOP", ACCENT)
    c.text(16, 50, "Hyprland 0.56", DIM)
    c.box(16, 64, 224, 30, "", MAGENTA, fill=PANEL2)
    c.text(24, 70, "bar plugin", MAGENTA)
    c.text(24, 80, "panel + library overlay", DIM)
    c.box(16, 100, 224, 30, "", CYAN, fill=PANEL2)
    c.text(24, 106, "omacrt", CYAN)
    c.text(24, 116, "one CLI for everything", DIM)
    c.box(16, 136, 224, 30, "", GREEN, fill=PANEL2)
    c.text(24, 142, "cliamp --daemon", GREEN)
    c.text(24, 152, "radio, Spotify, spectrum", DIM)

    # ---------------------------------------------------------- display
    c.box(272, 32, 216, 152, "omacrt-display", ORANGE)
    c.text(280, 50, "Smithay compositor", DIM)
    c.box(280, 64, 200, 26, "", FG, fill=PANEL2)
    c.text(288, 72, "DRM modeset by hand", FG)
    c.box(280, 96, 200, 26, "", FG, fill=PANEL2)
    c.text(288, 104, "launcher, 320x240", FG)
    c.box(280, 128, 200, 26, "", FG, fill=PANEL2)
    c.text(288, 136, "RetroArch, mpv", FG)
    c.text(280, 164, "it owns the connector", ORANGE)

    # --------------------------------------------------------- the tube
    c.box(512, 32, 120, 34, "", DIM, fill=PANEL2)
    c.text(520, 38, "RGB-PI 2", ORANGE)
    c.text(520, 50, "HDMI to RGB", DIM)
    c.television(508, 84, 128, 92)
    c.text_centered(572, 186, "BEOCENTER 1", DIM)

    # ---------------------------------------------------------- wiring
    c.arrow(248, 79, 272, 79, MAGENTA)
    c.arrow(248, 113, 272, 113, CYAN)
    c.arrow(248, 151, 272, 151, GREEN)
    c.arrow(488, 49, 512, 49, ORANGE)
    c.arrow(572, 66, 572, 84, ORANGE)

    # ---------------------------------------------------------- legend
    c.box(8, 204, 624, 148, "HOW THE TELEVISION IS TAKEN AWAY FROM THE DESKTOP", ACCENT)
    lines = [
        (MAGENTA, "1", "an EDID override at boot marks the DAC connector non-desktop,"),
        (MAGENTA, "", "so Hyprland drops it from its monitors and offers it for lease"),
        (ORANGE, "2", "omacrt-display takes that lease, programs 15.7 kHz through"),
        (ORANGE, "", "DRM and runs its own Wayland show: nothing else can land there"),
        (CYAN, "3", "the CLI drives the launcher over a control pipe, the bar plugin"),
        (CYAN, "", "drives the CLI, and the launcher draws every pixel by hand"),
        (GREEN, "4", "music arrives from cliamp over its Unix socket, films through"),
        (GREEN, "", "mpv and yt-dlp, all of them clients of the tube's compositor"),
    ]
    y = 226
    for colour, num, text in lines:
        if num:
            c.rect(18, y - 1, 10, 10, colour)
            c.text(19, y, num, BG)
        c.text(34, y, text, FG if num else DIM)
        y += 15

    foot = "one output, leased away from the desktop, driven at 1998 timings"
    c.text_centered(W // 2, 366, foot, DIM)
    c.rect(0, H - 4, W, 1, ACCENT)
    return c.im


def main() -> None:
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "docs" / "architecture.png"
    im = draw()
    im = im.resize((W * SCALE, H * SCALE), Image.NEAREST)
    out.parent.mkdir(parents=True, exist_ok=True)
    im.save(out)
    print(out)


if __name__ == "__main__":
    main()
