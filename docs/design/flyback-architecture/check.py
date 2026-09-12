#!/usr/bin/env python3
"""Read a finished SVG back and refuse it if any text is outside its box.

This is deliberately a second check rather than a better one. The generator
already budgets text before it draws, but a budget models the drawing and
this reads the drawing: it catches anything the model does not know about,
which on this figure has so far been an added line that no height knew about
and an arrow label sitting on a panel's border.

Two rules, both learned the hard way:

- **Strict containment, no slack.** Pairing a baseline with the smallest rect
  that strictly contains it is the whole of the logic. An earlier version
  allowed thirty units below a box while looking for the owner, so every box
  heading matched the box above it and was reported as overflowing. A checker
  with a tolerance in it finds faults that are not there, which is the same
  failure as one with no vertical budget, pointed the other way.

- **Descenders count.** A baseline one unit above a rule still puts its
  descenders through it, so 0.22 em is added before comparing.

Free-standing text, the labels in the gaps between columns, is checked the
other way round: it belongs to no box, so what matters is how far it is from
the nearest one on each side.

    python3 check.py Main.dc.html [more.svg ...]
"""
import re
import sys
from pathlib import Path

ADVANCE = 0.6     # JetBrains Mono, so this is exact rather than an estimate
DESCENDER = 0.22
CLEAR = 16        # units a free-standing label wants on each side

RECT = re.compile(r'<rect x="([\d.-]+)" y="([\d.-]+)" width="([\d.]+)" height="([\d.]+)"')
# Parse a tag's attributes rather than matching them in order. The first
# version of this used one regex with an optional group for text-anchor, and
# a lazy quantifier in front of an optional group never matches it: every
# centred and right-aligned label was read as left-aligned, and the checker
# reported three faults that were its own.
TEXT = re.compile(r"<text\b([^>]*)>([^<]*)</text>")
ATTR = re.compile(r'([\w-]+)="([^"]*)"')


def faults(svg, name):
    out = []
    rects = [tuple(float(g) for g in m.groups()) for m in RECT.finditer(svg)]
    for m in TEXT.finditer(svg):
        at = dict(ATTR.findall(m.group(1)))
        body = m.group(2)
        if not body.strip() or "x" not in at:
            continue
        x, y = float(at["x"]), float(at["y"])
        size = float(at.get("font-size", 10))
        anchor = at.get("text-anchor", "start")
        w = len(body) * size * ADVANCE
        left = x - w / 2 if anchor == "middle" else (x - w if anchor == "end" else x)
        right, bottom = left + w, y + size * DESCENDER
        owners = [r for r in rects if r[0] <= left and right <= r[0] + r[2] + 0.5
                  and r[1] <= y <= r[1] + r[3]]
        inner = [r for r in rects if r[0] <= x <= r[0] + r[2] and r[1] <= y <= r[1] + r[3]]
        if inner:
            bx, by, bw, bh = min(inner, key=lambda r: r[2] * r[3])
            if bottom - (by + bh) > 0.5:
                out.append(f"{name}: {body[:40]!r} is {bottom - (by + bh):.1f} below its own rule")
            if right - (bx + bw) > 0.5:
                out.append(f"{name}: {body[:40]!r} is {right - (bx + bw):.1f} past its right edge")
        elif not owners:
            # A label in a gap between panels. Overlap has to be tested before
            # clearance: measuring to "the nearest panel on each side" quietly
            # skips a panel the label is sitting on top of, so a label long
            # enough to cross one entirely was reported as having four hundred
            # units of room. Test intersection first, then distance.
            wide = [r for r in rects if r[2] > 150 and r[1] <= y <= r[1] + r[3]]
            over = [r for r in wide if r[0] < right and left < r[0] + r[2]]
            if over:
                r = over[0]
                out.append(f"{name}: {body[:40]!r} spans {left:.0f}-{right:.0f} "
                           f"and runs over a panel at {r[0]:.0f}-{r[0] + r[2]:.0f}")
                continue
            before = max((r[0] + r[2] for r in wide if r[0] + r[2] <= left), default=None)
            after = min((r[0] for r in wide if r[0] >= right), default=None)
            for edge, gap in (("left", left - before if before is not None else None),
                              ("right", after - right if after is not None else None)):
                if gap is not None and gap < CLEAR:
                    out.append(f"{name}: {body[:40]!r} has {gap:.0f} clear on the {edge}, wants {CLEAR}")
    return out


def main(paths):
    bad = []
    for p in paths:
        text = Path(p).read_text()
        for svg in re.findall(r"<svg .*?</svg>", text, re.S):
            bad += faults(svg, Path(p).name)
    for line in bad:
        print(line, file=sys.stderr)
    if bad:
        sys.exit(f"{len(bad)} fault(s)")
    print(f"ok: {', '.join(Path(p).name for p in paths)}, no text outside its box")


if __name__ == "__main__":
    main(sys.argv[1:] or ["Main.dc.html", "Frame.dc.html"])
