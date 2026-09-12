#!/usr/bin/env python3
"""Export the architecture artboard as a flat SVG for the page.

At its native 1120 units the smallest type in it is 9.5 px and every line
fits the box it is in. Rendering it at 920 would take that type to 7.8, and
scaling the type back up without re-laying out the boxes makes the labels
collide and the right-hand column run off its own panel, which is what
happened on the first attempt. So this exports at the size it was drawn at,
and the page should give it the break-out width rather than the measure.

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
    '<svg width="1120" height="500" viewBox="0 0 1120 500" '
    'role="img" aria-label="How Flyback is put together: Hyprland leases the connector to it, '
    'the clients reach it over a private Wayland socket, and it drives amdgpu, the converter and the television"',
    1,
)
# the artboard has no ground of its own; the page's would show through
svg = svg.replace(">", '><rect width="1120" height="500" fill="#0b0d14"/>', 1)
OUT.write_text(svg)
print(f"wrote {OUT.name}: 1120x500 native, {len(svg)} bytes")
