#!/usr/bin/env python3
"""Export the architecture artboard as a flat SVG for the page.

The artboard is laid out at 976 units, which is the width of the reading
column it sits in, so it exports one to one and the 9.5 px type stays 9.5 px.
Two earlier attempts are worth not repeating: rendering a 1120 unit drawing
into 920 is a true resize and nothing collides, but it takes that type to
7.8; and scaling the type back up without moving the boxes makes the labels
collide and the right-hand column run off its own panel. The fix was to make
the drawing narrower, not the type smaller or the type bigger.

Two colours change on the way out. #565f89 is fine as a rule or an arrow and
is not fine as small text on this ground, where it measures 3.1 to one; text
in it becomes #9aa5ce, which is 8.0. Nothing else is touched, so the diagram
on the page and the diagram on the canvas stay the same drawing.
"""
import re
from pathlib import Path

SRC = Path(__file__).resolve().parents[1] / "design/flyback-architecture/Main.dc.html"
OUT = Path(__file__).resolve().parent / "architecture.svg"

svg = re.search(r"<svg .*?</svg>", SRC.read_text(), re.S).group(0)

# text in the rule colour is the only thing recoloured, and only in <text>
svg = re.sub(r'(<text\b[^>]*?fill=")#565f89(")', r"\g<1>#9aa5ce\g<2>", svg)
svg = re.sub(r'(<text\b[^>]*?fill=")#3b4261(")', r"\g<1>#7f88ad\g<2>", svg)

svg = svg.replace(
    '<svg width="1120" height="500" viewBox="0 0 1120 500"',
    '<svg width="976" height="500" viewBox="0 0 976 500" '
    'role="img" aria-label="How Flyback is put together: Hyprland leases the connector to it, '
    'the clients reach it over a private Wayland socket, and it drives amdgpu, the converter and the television"',
    1,
)
# the artboard has no ground of its own; the page's would show through
svg = svg.replace(">", '><rect width="976" height="500" fill="#0b0d14"/>', 1)
OUT.write_text(svg)
print(f"wrote {OUT.name}: 976x500, one to one with the reading column, {len(svg)} bytes")
