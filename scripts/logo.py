#!/usr/bin/env python3
"""Draw the lockup: the mark, then the wordmark, from the project's own geometry.

The picture at the top of the README used to be a capture of the launcher's
own boot screen, scaled up. Pixel art that has been resampled looks like a
photograph of pixel art: soft edges, half tones along every diagonal, a
different weight at every size. This draws it instead, from the two places
the shapes actually live:

  shell/src/assets.rs        the mark: four bars, and the dark cut of the
                             beam's return stepping one pitch per bar
  shell/assets/wordmark.txt  the letterforms, as block characters

Every edge lands on a whole pixel at any scale, so a bigger file is a bigger
picture rather than a blurrier one.

    python3 scripts/logo.py docs/logo.png
"""

import re
import sys
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent

# The palette the launcher lights the word with, bottom to top, and the green
# the mark is drawn in. shell/src/theme.rs, Theme::tokyo_night.
BG = (0x0B, 0x0D, 0x14)
GREEN = (0x9E, 0xCE, 0x6A)
MAGENTA = (0xBB, 0x9A, 0xF7)
CYAN = (0x7D, 0xCF, 0xFF)
PAPER = (0xC0, 0xCA, 0xF5)

# The lockup, in wordmark pixels: the mark stands at the word's own height.
MARK_UNITS = 20
GAP_UNITS = 6
PAD_UNITS = 6
SCALE = 14


def retrace():
    """`RETRACE` as the Rust declares it, read rather than copied."""
    text = (ROOT / "shell/src/assets.rs").read_text()
    block = text[text.index("pub const RETRACE"):]
    block = block[: block.index("};")]
    fields = dict(re.findall(r"(\w+):\s*(-?\d+)", block))
    return {k: int(v) for k, v in fields.items()}


def segments(r, size, base):
    """`Retrace::segments`: the solid pieces of the four bars, in pixels."""
    k = size / r["grid"]
    at = lambda u: round(u * k)  # noqa: E731
    side = round(size)
    pitch = max(at(r["thick"] + r["gap"]), 2)
    thick = min(max(at(r["thick"]), 1), pitch - 1)
    while (r["bars"] - 1) * pitch + thick > side and pitch > 2:
        pitch -= 1
        thick = min(thick, pitch - 1)
    width = at(r["grid"])
    cut = max(at(r["cut"]), 1)
    minseg = max(at(r["minseg"]), 2)
    top = max((side - ((r["bars"] - 1) * pitch + thick)) // 2, 0)
    base_px = round(base * k)
    out = []
    for i in range(r["bars"]):
        y = top + i * pitch
        a = base_px + (r["bars"] - 1 - i) * pitch
        b = a + cut - 1
        if a < minseg:
            a = 0
        if b > width - 1 - minseg:
            b = width - 1
        if b < 0 or a > width - 1:
            out.append((0, y, width, thick))
            continue
        if a > 0:
            out.append((0, y, a, thick))
        if b < width - 1:
            out.append((b + 1, y, width - 1 - b, thick))
    return out


def wordmark():
    """The block drawing as a pixel grid: a block character is two rows."""
    lines = (ROOT / "shell/assets/wordmark.txt").read_text().rstrip("\n").split("\n")
    cols = max(len(line) for line in lines)
    rows = []
    for line in lines:
        pad = line.ljust(cols)
        rows.append([c in "█▀" for c in pad])
        rows.append([c in "█▄" for c in pad])
    return rows


def lerp(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def main(out_path):
    r = retrace()
    px = wordmark()
    word_h, word_w = len(px), len(px[0])
    height = PAD_UNITS * 2 + max(word_h, MARK_UNITS)
    width = PAD_UNITS * 2 + MARK_UNITS + GAP_UNITS + word_w
    im = Image.new("RGB", (width * SCALE, height * SCALE), BG)
    d = ImageDraw.Draw(im)

    mark_y = PAD_UNITS + (max(word_h, MARK_UNITS) - MARK_UNITS) // 2
    for x, y, w, h in segments(r, MARK_UNITS * SCALE, r["rest"]):
        x0 = PAD_UNITS * SCALE + x
        y0 = mark_y * SCALE + y
        d.rectangle([x0, y0, x0 + w - 1, y0 + h - 1], fill=GREEN)

    word_x = PAD_UNITS + MARK_UNITS + GAP_UNITS
    word_y = PAD_UNITS + (max(word_h, MARK_UNITS) - word_h) // 2
    for y, row in enumerate(px):
        f = 1.0 - y / max(word_h - 1, 1)
        colour = lerp(MAGENTA, CYAN, f * 2) if f < 0.5 else lerp(CYAN, PAPER, (f - 0.5) * 2)
        run = None
        for x, on in enumerate(row + [False]):
            if on and run is None:
                run = x
            elif not on and run is not None:
                d.rectangle(
                    [
                        (word_x + run) * SCALE,
                        (word_y + y) * SCALE,
                        (word_x + x) * SCALE - 1,
                        (word_y + y + 1) * SCALE - 1,
                    ],
                    fill=colour,
                )
                run = None
    im.save(out_path)
    print(f"{out_path}: {im.size[0]}x{im.size[1]}")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else str(ROOT / "docs/logo.png"))
