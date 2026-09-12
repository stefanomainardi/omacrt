#!/usr/bin/env python3
"""Generate the two animated figures for the Flyback study.

Both are self-contained SVG with CSS keyframes, so they animate inside an
<img> tag with no script and no library, and both honour
prefers-reduced-motion by stopping on their first state.

Every number below is measured. `blanking.svg` uses the four refresh steps
filmed on the set, with the vertical totals computed from the modeline this
project actually runs. `frame-sweep.svg` uses the trace percentiles from the
compositor's own log. Re-run this after re-measuring rather than editing the
SVG, so a figure cannot drift away from its measurement.
"""
from pathlib import Path

BG, PANEL, LINE, DIM, FG = "#0b0d14", "#11141f", "#1f2335", "#565f89", "#9aa5ce"
# DIM is for strokes and rules, never for text. On these figures' own ground it
# measures 3.1 to one at 10 px, which is under the bar for small type. Captions
# take CAP (8.0 to one) and secondary numbers take SUB (5.6 to one). The figures
# carry their own dark ground on purpose, so these two are fixed rather than
# inherited from whatever page they land on.
CAP, SUB = "#9aa5ce", "#7f88ad"
PAPER, GREEN, BLUE, ORANGE, RED = "#c0caf5", "#9ece6a", "#7aa2f7", "#ff9e64", "#f7768e"
MONO = "JetBrains Mono, JetBrainsMono Nerd Font, ui-monospace, monospace"

CLOCK = 72e6
HTOTAL = 4577
LINE_RATE = CLOCK / HTOTAL          # 15730.8 Hz, and it never moves

# The four steps that were filmed, with the height measured against the
# picture's own width so the camera's drift cancels.
STEPS = [
    # vtotal, measured refresh, measured height change, label
    (262, 60.04, 0.0, "reference"),
    (274, 57.41, -0.4, "-0.4%"),
    (286, 55.00, -0.6, "-0.6%"),
    (315, 49.94, -11.5, "-11.5%"),
]
DWELL = 2.2                          # seconds per step
TOTAL = DWELL * len(STEPS)


def blanking():
    """The vertical blanking stretched at a constant line rate, and the
    moment the television stops following it."""
    W, H = 920, 430
    # the screen
    SX, SY, SW, SH = 40, 74, 340, 256
    # the time bar: one frame, active plus blanking, drawn to scale
    BX, BY, BH = 452, 150, 46
    BW_MAX = 430                     # the longest frame, vtotal 315
    px_per_line = BW_MAX / STEPS[-1][0]

    css = [
        f"@media (prefers-reduced-motion: reduce) {{ * {{ animation: none !important; }} }}",
        ".t { font-family: %s; }" % MONO,
    ]
    # picture height keyframes: the active area of the screen, shrinking only
    # at the last step
    def pct_frames(values, name, fmt):
        out = [f"@keyframes {name} {{"]
        for i, v in enumerate(values):
            a = 100.0 * (i * DWELL) / TOTAL
            b = 100.0 * ((i + 1) * DWELL - 0.001) / TOTAL
            out.append(f"  {a:.3f}%, {b:.3f}% {{ {fmt(v)} }}")
        out.append("}")
        return "\n".join(out)

    # Scale a group rather than animate the rect's own geometry: CSS geometry
    # properties on SVG shapes are not animatable everywhere, and a transform
    # is. The scan lines live inside the scaled group on purpose - when a
    # television loses picture height it is still drawing the same 240 lines,
    # so they close up rather than staying put.
    heights = [1 + h / 100.0 for _, _, h, _ in STEPS]
    css.append(pct_frames(heights, "pic", lambda v: f"transform: scaleY({v:.4f});"))
    widths = [v * px_per_line / (STEPS[0][0] * px_per_line) for v, _, _, _ in STEPS]
    css.append(pct_frames(widths, "bar", lambda v: f"transform: scaleX({v:.4f});"))
    css.append("#pic { transform-box: fill-box; transform-origin: center; "
               "animation: pic %.1fs steps(1, end) infinite; }" % TOTAL)
    css.append("#bar { transform-box: fill-box; transform-origin: left center; "
               "animation: bar %.1fs steps(1, end) infinite; }" % TOTAL)
    for i in range(len(STEPS)):
        a = 100.0 * (i * DWELL) / TOTAL
        b = 100.0 * ((i + 1) * DWELL - 0.001) / TOTAL
        css.append(
            f"@keyframes lab{i} {{ 0%, {max(a - 0.001, 0):.3f}% {{ opacity: 0 }} "
            f"{a:.3f}%, {b:.3f}% {{ opacity: 1 }} {min(b + 0.001, 100):.3f}%, 100% {{ opacity: 0 }} }}")
        css.append(f"#lab{i} {{ opacity: {1 if i == 0 else 0}; animation: lab{i} %.1fs steps(1, end) infinite; }}" % TOTAL)

    s = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}" role="img" '
         f'aria-label="The vertical blanking stretched at a constant line rate: the picture keeps its height down to 55 Hz and loses an eighth of it at 50">',
         f"<style>{chr(10).join(css)}</style>",
         f'<rect width="{W}" height="{H}" fill="{BG}"/>']

    def txt(x, y, t, size=11, fill=FG, weight=400, anchor="start", cls="t"):
        return (f'<text class="{cls}" x="{x}" y="{y}" font-size="{size}" font-weight="{weight}" '
                f'fill="{fill}" text-anchor="{anchor}">{t}</text>')

    s.append(txt(40, 34, "ONE LINE RATE, MANY FRAME LENGTHS", 12, PAPER, 700))
    s.append(txt(40, 52, f"{LINE_RATE:.1f} Hz horizontal, never moved. Only the vertical blanking changes.", 10.5, CAP))

    # the screen, with the picture inside it
    s.append(f'<rect x="{SX-8}" y="{SY-8}" width="{SW+16}" height="{SH+16}" rx="14" fill="{PANEL}" stroke="{LINE}"/>')
    s.append(f'<g id="pic">')
    s.append(f'<rect x="{SX}" y="{SY}" width="{SW}" height="{SH}" fill="{GREEN}" opacity="0.92"/>')
    # scan lines, inside the scaled group so they close up with the picture
    s.append(f'<g fill="{BG}" opacity="0.34">')
    for y in range(SY, SY + SH, 4):
        s.append(f'<rect x="{SX}" y="{y}" width="{SW}" height="1.6"/>')
    s.append("</g></g>")
    s.append(txt(SX + SW / 2, SY + SH + 40, "what the tube shows", 10, SUB, anchor="middle"))

    # the frame as a bar: active lines fixed, blanking growing
    s.append(txt(BX, 110, "ONE FRAME, TO SCALE", 10.5, PAPER, 700))
    s.append(f'<rect id="bar" x="{BX}" y="{BY}" width="{STEPS[0][0] * px_per_line:.2f}" height="{BH}" fill="{PANEL}" stroke="{LINE}"/>')
    s.append(f'<rect x="{BX}" y="{BY}" width="{240 * px_per_line:.2f}" height="{BH}" fill="{BLUE}" opacity="0.55"/>')
    s.append(txt(BX + 240 * px_per_line / 2, BY + BH / 2 + 4, "240 active lines", 10, BG, 700, "middle"))
    s.append(txt(BX, BY + BH + 22, "the blue block never changes. everything to the right of it is blanking.", 10, CAP))

    # the per-step readouts
    for i, (vt, hz, dh, lab) in enumerate(STEPS):
        g = [f'<g id="lab{i}">']
        g.append(txt(BX, 260, f"vtotal {vt}", 22, PAPER, 700))
        g.append(txt(BX, 288, f"{hz:.2f} Hz", 15, GREEN if dh > -5 else RED, 500))
        g.append(txt(BX + 150, 260, f"frame {1000 / hz:.2f} ms", 13, FG))
        g.append(txt(BX + 150, 288, f"picture height {lab}", 13,
                     FG if dh > -5 else RED, 500 if dh > -5 else 700))
        if dh < -5:
            g.append(txt(BX, 322, "past where this set follows.", 11, RED, 700))
            g.append(txt(BX, 340, "output.vrr_min_hz stops it before here.", 11, CAP))
        g.append("</g>")
        s.append("".join(g))

    s.append(txt(40, H - 22, "Filmed on a BeoCenter 1, eight seconds a step, height measured against the picture's own width.", 10, CAP))
    s.append("</svg>")
    return "".join(s)


# The trace, microseconds after the vblank. Measured marks are solid; the two
# derived from the scheduler's own estimates are drawn hollow and said so.
REGIMES = [
    ("variable rate", GREEN, 16655, [
        (0, "vblank", PAPER, True),
        (13100, "callbacks", SUB, False),
        (14150, "commit", ORANGE, False),
        (16655, "scanout", PAPER, True),
    ], (14150, 16655)),
    ("fixed rate", BLUE, 16655, [
        (0, "vblank", PAPER, True),
        (8400, "callbacks", SUB, False),
        (9500, "commit", ORANGE, False),
        (13644, "queued", BLUE, True),
        (16655, "scanout", PAPER, True),
    ], (9500, 16655)),
]
SWEEP = 4.0   # seconds for one 16.655 ms frame, so the eye can follow it


def frame_sweep():
    W, H = 920, 340
    X0, XW = 150, 720
    def x(us):
        return X0 + XW * us / 16655.0

    css = [f"@media (prefers-reduced-motion: reduce) {{ * {{ animation: none !important; }} }}",
           ".t { font-family: %s; }" % MONO,
           f"@keyframes sweep {{ from {{ transform: translateX(0) }} to {{ transform: translateX({XW}px) }} }}",
           f".head {{ animation: sweep {SWEEP}s linear infinite; }}"]
    s = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}" role="img" '
         f'aria-label="One frame swept twice: at a variable refresh the commit is queued at once, at a fixed one it waits for the deadline">']

    def txt(xx, yy, t, size=11, fill=FG, weight=400, anchor="start"):
        return (f'<text class="t" x="{xx}" y="{yy}" font-size="{size}" font-weight="{weight}" '
                f'fill="{fill}" text-anchor="{anchor}">{t}</text>')

    for row, (name, colour, frame_us, marks, span) in enumerate(REGIMES):
        y = 84 + row * 130
        for us, label, c, measured in marks:
            pct = 100.0 * us / frame_us
            css.append(f"@keyframes pop{row}{us} {{ 0%, {max(pct - 0.4, 0):.3f}% {{ opacity: .18 }} "
                       f"{pct:.3f}%, 100% {{ opacity: 1 }} }}")
            css.append(f"#m{row}{us} {{ opacity: 1; animation: pop{row}{us} {SWEEP}s linear infinite; }}")
        pa, pb = 100.0 * span[0] / frame_us, 100.0 * span[1] / frame_us
        css.append(f"@keyframes grow{row} {{ 0%, {pa:.3f}% {{ transform: scaleX(0) }} {pb:.3f}%, 100% {{ transform: scaleX(1) }} }}")
        css.append(f"#span{row} {{ transform-box: fill-box; transform-origin: left center; animation: grow{row} {SWEEP}s linear infinite; }}")

    s.append(f'<rect width="{W}" height="{H}" fill="{BG}"/>')
    s.append(txt(40, 34, "ONE FRAME, 16.655 ms, SWEPT TWICE", 12, PAPER, 700))
    s.append(txt(40, 52, "same client, drawing one millisecond, launcher mapped underneath", 10.5, CAP))

    for row, (name, colour, frame_us, marks, span) in enumerate(REGIMES):
        y = 84 + row * 130
        s.append(txt(40, y + 30, name, 12, colour, 700))
        s.append(txt(40, y + 46, "no deadline" if row == 0 else "a deadline", 10, SUB))
        s.append(f'<rect x="{X0}" y="{y}" width="{XW}" height="56" fill="{PANEL}" stroke="{LINE}"/>')
        s.append(f'<rect id="span{row}" x="{x(span[0]):.2f}" y="{y + 40}" width="{x(span[1]) - x(span[0]):.2f}" height="8" fill="{ORANGE}" opacity="0.85"/>')
        s.append(txt(x(span[1]) - 6, y + 34, f"{(span[1]-span[0])/1000:.2f} ms to scanout", 10.5, ORANGE, 700, "end"))
        for i, (us, label, c, measured) in enumerate(marks):
            xx = x(us)
            drop = 0 if i % 2 == 0 else 22
            dash = "" if measured else ' stroke-dasharray="3 3"'
            s.append(f'<g id="m{row}{us}"><path d="M{xx:.2f} {y - 6} V{y + 62 + drop}" stroke="{c}" stroke-width="1.4"{dash}/>'
                     + txt(xx + (4 if us < 1000 else (-4 if us > 15500 else 0)), y + 76 + drop, label, 9.5, c, 500,
                           "start" if us < 1000 else ("end" if us > 15500 else "middle"))
                     + "</g>")
        s.append(f'<g class="head"><path d="M{X0} {y - 10} V{y + 66}" stroke="{RED}" stroke-width="1.6" opacity="0.9"/></g>')

    # Two lines. One was 195 characters at font-size 10 from x 40 in a 920
    # wide box, and ran off the end of its own viewBox.
    s.append(txt(40, H - 32, "Solid marks are measured. The dashed ones are arithmetic from the scheduler's own estimates.", 10, CAP))
    s.append(txt(40, H - 18, "Under a variable rate the flip is queued 75 microseconds after the commit, which is three pixels at this scale.", 10, SUB))
    s.append(f'<style>{chr(10).join(css)}</style>')
    s.append("</svg>")
    return "".join(s)


def mark():
    """The Flyback mark doing what it depicts: the ramp sweeping out, and the
    stroke where it collapses back.

    Whole units, one colour, no easing on the return: a flyback is fast and a
    ramp is not, and the timing is the only thing that says so."""
    RAMP, BACK, HOLD = 1.05, 0.08, 0.10      # seconds: the blank must read as a beat, not a bug
    T = RAMP + BACK + HOLD
    bars = [(0, 8), (12, 16), (24, 24), (36, 32)]   # y, width, on the 44 grid
    css = ["@media (prefers-reduced-motion: reduce) { * { animation: none !important; } }"]
    # each bar grows in its own quarter of the ramp, then they all clear at once
    for i, (y, w) in enumerate(bars):
        a = 100.0 * (i * RAMP / len(bars)) / T
        b = 100.0 * ((i + 1) * RAMP / len(bars)) / T
        c = 100.0 * RAMP / T
        css.append(
            f"@keyframes b{i} {{ 0%, {a:.2f}% {{ transform: scaleX(0) }} "
            f"{b:.2f}%, {c:.2f}% {{ transform: scaleX(1) }} "
            f"{min(c + 0.01, 100):.2f}%, 100% {{ transform: scaleX(0) }} }}")
        css.append(f"#b{i} {{ transform-box: fill-box; transform-origin: left center; "
                   f"animation: b{i} {T:.2f}s linear infinite; }}")
    # the return stroke is lit only while the ramp collapses
    r0 = 100.0 * RAMP / T
    r1 = 100.0 * (RAMP + BACK) / T
    css.append(f"@keyframes ret {{ 0%, {r0:.2f}% {{ opacity: .22 }} "
               f"{min(r0 + 0.01, 100):.2f}%, {r1:.2f}% {{ opacity: 1 }} "
               f"{min(r1 + 0.01, 100):.2f}%, 100% {{ opacity: .22 }} }}")
    css.append(f"#ret {{ opacity: .22; animation: ret {T:.2f}s linear infinite; }}")
    out = ['<svg xmlns="http://www.w3.org/2000/svg" width="220" height="220" viewBox="0 0 44 44" '
           'shape-rendering="crispEdges" role="img" '
           'aria-label="The Flyback mark animating: four bars sweeping out in turn, then the full height stroke where the beam flies back">',
           f"<style>{chr(10).join(css)}</style>",
           f'<rect width="44" height="44" fill="{BG}"/>',
           f'<rect id="ret" x="0" y="0" width="8" height="44" fill="{GREEN}"/>']
    for i, (y, w) in enumerate(bars):
        out.append(f'<rect id="b{i}" x="12" y="{y}" width="{w}" height="8" fill="{GREEN}"/>')
    out.append("</svg>")
    return "".join(out)


for name, body in [("blanking.svg", blanking()), ("frame-sweep.svg", frame_sweep()),
                   ("mark-flyback.svg", mark())]:
    Path(name).write_text(body)
    print(f"wrote {name}, {len(body)} bytes")
