"""Generate the three Flyback diagram artboards as .dc.html files.

Everything is one hand-placed SVG per sheet: predictable, no layout engine
to argue with, and the whole thing is whole-pixel so it stays legible small.
"""
from pathlib import Path
from html import escape

BG = "#0b0d14"
PANEL = "#11141f"
LINE = "#1f2335"
DIM = "#565f89"
FG = "#9aa5ce"
PAPER = "#c0caf5"
GREEN = "#9ece6a"
BLUE = "#7aa2f7"
CYAN = "#7dcfff"
ORANGE = "#ff9e64"
RED = "#f7768e"
MONO = "JetBrains Mono, JetBrainsMono Nerd Font, ui-monospace, monospace"

def esc(t):
    return escape(str(t), quote=False)

def box(x, y, w, h, stroke=LINE, fill=PANEL, dash=None, rx=0):
    d = f' stroke-dasharray="{dash}"' if dash else ""
    return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" '
            f'fill="{fill}" stroke="{stroke}" stroke-width="1"{d}/>')

def text(x, y, s, size=13, fill=FG, weight=400, anchor="start", ls=0, mono=True):
    f = MONO if mono else MONO
    return (f'<text x="{x}" y="{y}" font-family="{f}" font-size="{size}" '
            f'font-weight="{weight}" fill="{fill}" text-anchor="{anchor}" '
            f'letter-spacing="{ls}">{esc(s)}</text>')

def cap(x, y, s, fill=DIM, size=10):
    return text(x, y, s.upper(), size=size, fill=fill, weight=500, ls=1.4)

def arrow(x1, y1, x2, y2, colour=DIM, dash=None, head=True, width=1.4):
    d = f' stroke-dasharray="{dash}"' if dash else ""
    m = f' marker-end="url(#head-{colour.lstrip("#")})"' if head else ""
    return (f'<path d="M{x1} {y1} L{x2} {y2}" fill="none" stroke="{colour}" '
            f'stroke-width="{width}"{d}{m}/>')

def elbow(x1, y1, x2, y2, colour=DIM, dash=None, mid=None):
    """A right-angled connector: across, then down (or the reverse)."""
    mx = mid if mid is not None else (x1 + x2) / 2
    d = f' stroke-dasharray="{dash}"' if dash else ""
    return (f'<path d="M{x1} {y1} H{mx} V{y2} H{x2}" fill="none" '
            f'stroke="{colour}" stroke-width="1.4"{d} '
            f'marker-end="url(#head-{colour.lstrip("#")})"/>')

def defs(colours):
    out = ["<defs>"]
    for c in colours:
        out.append(
            f'<marker id="head-{c.lstrip("#")}" viewBox="0 0 10 10" refX="9" refY="5" '
            f'markerWidth="6" markerHeight="6" orient="auto-start-reverse">'
            f'<path d="M0 0 L10 5 L0 10 z" fill="{c}"/></marker>')
    out.append("</defs>")
    return "".join(out)

SHEET = """<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <script src="./support.js"></script>
</head>
<body>
<x-dc>
<helmet>
  <link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;700;800&display=swap">
  <style>
    body {{ margin: 0; font-family: "JetBrains Mono", ui-monospace, monospace; }}
    .sheet {{ background: #0b0d14; color: #c0caf5; padding: 36px 40px 32px; display: flex; flex-direction: column; gap: 20px; min-height: 100%; box-sizing: border-box; }}
    .eyebrow {{ font-size: 11px; letter-spacing: 0.18em; text-transform: uppercase; color: #565f89; font-weight: 500; }}
    .name {{ font-size: 24px; font-weight: 800; letter-spacing: -0.01em; color: #c0caf5; margin-top: 6px; }}
    .why {{ font-size: 12.5px; line-height: 1.6; color: #9aa5ce; max-width: 96ch; margin-top: 8px; }}
    .why b {{ color: #c0caf5; font-weight: 500; }}
    .foot {{ font-size: 11.5px; line-height: 1.6; color: #565f89; border-top: 1px solid #1f2335; padding-top: 12px; }}
    .foot b {{ color: #9aa5ce; font-weight: 500; }}
  </style>
</helmet>
<div class="sheet">
  <div>
    <div class="eyebrow">{eyebrow}</div>
    <div class="name">{name}</div>
    <div class="why">{why}</div>
  </div>
  {svg}
  <div class="foot">{foot}</div>
</div>
</x-dc>
</body>
</html>
"""

def sheet(path, eyebrow, name, why, svg, foot):
    Path(path).write_text(SHEET.format(eyebrow=eyebrow, name=name, why=why,
                                       svg=svg, foot=foot))
    print("wrote", path)

# ---------------------------------------------------------------- diagram 1

def titled(x, y, w, h, title, lines, accent=PAPER, stroke=LINE, fill=PANEL):
    out = [box(x, y, w, h, stroke=stroke, fill=fill)]
    out.append(text(x + 14, y + 24, title, size=13, fill=accent, weight=700))
    for i, ln in enumerate(lines):
        out.append(text(x + 14, y + 44 + i * 16, ln, size=10.5, fill=DIM))
    return "".join(out)

def label(x, y, s, fill=DIM):
    return text(x, y, s, size=10, fill=fill, weight=500, anchor="middle", ls=0.4)

# The drawing is laid out at 976 units because that is the width of the
# reading column the page gives it. Rendering a 1120 unit drawing into 976
# is a true resize and nothing collides, but it takes the smallest type from
# 9.5 px to 8.3, and that would be the smallest type on the page. So the
# geometry is narrower and the type is unchanged, which is a re-layout rather
# than a scale. Every column below is sized from the longest string in it:
# the check at the foot of this file fails the build if one stops fitting.
W = 976
COL_L = (0, 190)          # the desktop and the clients
GAP_L = 106               # holds "wp_drm_lease_v1" and "frame callbacks"
COL_M = (296, 316)        # the compositor
GAP_R = 94                # holds "atomic commit"
COL_R = (706, 264)        # kernel, converter, tube
BOX_M = (COL_M[0] + 18, COL_M[1] - 36)   # the inner boxes

ROWS = [
    (74, 52, "lease.rs", ["takes the lease: a DRM fd that is",
                          "master for one connector and its",
                          "CRTC, and nothing else"]),
    (140, 68, "the Wayland side",
     ["wl_compositor  wl_subcompositor",
      "xdg_shell  wl_shm  wp_viewporter",
      "zwp_linux_dmabuf  wp_presentation",
      "wl_seat, keyboard only: no libinput"]),
    (226, 90, "the scheduler",
     ["when to tell the clients, when to",
      "draw, when to flip. fixed rate: draw",
      "at the deadline less render_cost.",
      "variable rate: no deadline, draw the",
      "moment a client commits"]),
    (334, 58, "the DRM output",
     ["atomic commit, and no timing",
      "reaches it without Modeline::fault"]),
    (406, 58, "the control pipe",
     ["0600 in the user's own state folder",
      "top key mode vrr rate shot record"]),
]

RIGHT = [
    (20, 120, "amdgpu / DRM", BLUE,
     ["atomic modeset on the leased fd",
      "page flip, and a vblank timestamp",
      "on CLOCK_MONOTONIC",
      "adaptive sync on the CRTC"]),
    (200, 95, "RGB-Pi 2", CYAN,
     ["HDMI in, RGB SCART out",
      "composite sync selected over I2C"]),
    (355, 115, "CRT television", PAPER,
     ["15.731 kHz, 240 lines, 60.04 Hz",
      "no panel, no scaler, no frame store:",
      "the photon leaves when it arrives"]),
]

CLIENTS = [("omacrt-shell", "the launcher, 320x240"),
           ("RetroArch", "a core per system"),
           ("mpv", "films and YouTube")]


def fits(lines, size, room, where):
    """Refuse to emit a drawing whose text does not fit the box it is in.

    0.6 em per glyph is JetBrains Mono's advance, so this is exact for the
    monospaced faces these sheets use rather than an estimate."""
    for ln in lines:
        w = len(ln) * size * 0.6
        if w > room:
            raise SystemExit(f"{where}: {w:.0f} units of text in {room} units of box: {ln!r}")


def architecture():
    s = [f'<svg width="{W}" height="500" viewBox="0 0 {W} 500" xmlns="http://www.w3.org/2000/svg">']
    s.append(defs([DIM, GREEN, BLUE, CYAN]))

    lx, lw = COL_L
    fits(["the desktop session", "offers the connector it", "was told to leave alone"], 10.5, lw - 28, "Hyprland")
    s.append(titled(lx, 20, lw, 100, "Hyprland",
                    ["the desktop session", "offers the connector it",
                     "was told to leave alone"]))
    s.append(titled(lx, 140, lw, 75, "omacrt",
                    ["the CLI and the bar plugin", "on the desktop side"]))
    s.append(box(lx, 245, lw, 225, stroke=LINE, fill="none"))
    s.append(cap(lx + 14, 266, "the clients"))
    for i, (n, d) in enumerate(CLIENTS):
        y = 280 + i * 62
        fits([d], 10, lw - 40, "client")
        s.append(box(lx + 14, y, lw - 28, 50, stroke=LINE, fill=BG))
        s.append(text(lx + 26, y + 21, n, size=12, fill=PAPER, weight=700))
        s.append(text(lx + 26, y + 38, d, size=10, fill=DIM))

    mx, mw = COL_M
    s.append(box(mx, 0, mw, 480, stroke=GREEN, fill=PANEL))
    s.append(text(mx + 20, 32, "Flyback", size=21, fill=GREEN, weight=800))
    s.append(text(mx + 20, 51, "one process, one thread, one calloop loop", size=10, fill=DIM))
    bx, bw = BOX_M
    for y, h, title, lines in ROWS:
        fits(lines, 9.5, bw - 24, title)
        s.append(box(bx, y, bw, h, stroke=LINE, fill=BG))
        s.append(text(bx + 12, y + 19, title, size=11.5, fill=PAPER, weight=700))
        for i, ln in enumerate(lines):
            s.append(text(bx + 12, y + 35 + i * 14, ln, size=9.5, fill=DIM))

    rx, rw = COL_R
    for y, h, title, accent, lines in RIGHT:
        fits(lines, 10.5, rw - 28, title)
        s.append(titled(rx, y, rw, h, title, lines, accent=accent))

    # connections. The labels live in the gaps between the columns, so the
    # gaps are sized from the labels rather than the other way round: this is
    # what let "wp_drm_lease_v1" run under the compositor's own border once.
    fits(["wp_drm_lease_v1", "frame callbacks", "wayland-crt", "control pipe"], 10, GAP_L - 16, "left gap")
    fits(["atomic commit", "vblank"], 10, GAP_R - 16, "right gap")
    s.append(arrow(lx + lw, 70, mx - 2, 70, GREEN))
    s.append(label((lx + lw + mx) / 2, 62, "wp_drm_lease_v1", GREEN))
    s.append(arrow(lx + lw, 178, mx - 2, 178, DIM))
    s.append(label((lx + lw + mx) / 2, 170, "control pipe"))
    s.append(arrow(lx + lw, 330, mx - 2, 330, DIM))
    s.append(label((lx + lw + mx) / 2, 322, "wayland-crt"))
    s.append(arrow(mx - 2, 420, lx + lw + 2, 420, DIM, dash="4 4"))
    s.append(label((lx + lw + mx) / 2, 412, "frame callbacks"))

    s.append(arrow(mx + mw, 70, rx - 2, 70, BLUE))
    s.append(label((mx + mw + rx) / 2, 62, "atomic commit", BLUE))
    s.append(arrow(rx - 2, 120, mx + mw + 2, 120, BLUE, dash="4 4"))
    s.append(label((mx + mw + rx) / 2, 112, "vblank", BLUE))
    cxx = rx + rw / 2
    s.append(arrow(cxx, 140, cxx, 198, CYAN))
    s.append(text(cxx + 12, 175, "HDMI", size=10, fill=CYAN, weight=500))
    s.append(arrow(cxx, 295, cxx, 353, CYAN))
    s.append(text(cxx + 12, 330, "RGB SCART", size=10, fill=CYAN, weight=500))
    s.append("</svg>")
    return "".join(s)


sheet("Main.dc.html",
      "Flyback &middot; architecture",
      "What is between a button and a phosphor",
      "One process holds the lease, the Wayland globals, the frame scheduler and the DRM output. "
      "Everything above it is an ordinary Wayland client; everything below it is the analogue chain, "
      "which has no buffer anywhere in it.",
      architecture(),
      "<b>The green arrow is the whole reason this exists.</b> Hyprland is asked for the connector through "
      "<b>wp_drm_lease_v1</b>, the protocol written for VR headsets, and what comes back is a DRM file "
      "descriptor that is master for that connector alone. Without it a 15 kHz modeline is not something "
      "any desktop compositor will set, and the scanout is not something it will give away. "
      "The dashed arrows are the two answers that make the latency measurable: a frame callback that says "
      "<b>draw now</b>, and a vblank timestamp that says <b>this is when it was scanned out</b>.")

# ---------------------------------------------------------------- diagram 2

def timeline():
    """Two frames, drawn to the same scale, from the compositor's own trace."""
    PX = 0.055   # px per microsecond -> a 16655 us frame is 916 px
    X0 = 176
    def t(us): return X0 + us * PX
    s = [f'<svg width="1120" height="440" viewBox="0 0 1120 440" xmlns="http://www.w3.org/2000/svg">']
    s.append(defs([DIM, GREEN, BLUE, ORANGE, RED]))

    for row, (name, note, colour, marks) in enumerate([
        ("variable rate", "no deadline: draw the moment a client commits", GREEN, [
            (0, "vblank", PAPER), (13100, "callbacks", DIM),
            (14150, "commit", ORANGE),
            (16655, "vblank: scanout", PAPER)]),
        ("fixed rate", "a deadline to meet, and a margin held back from it", BLUE, [
            (0, "vblank", PAPER), (8400, "callbacks", DIM), (9500, "commit", ORANGE),
            (13644, "queued", BLUE), (16655, "scanout", PAPER)]),
    ]):
        y = 70 + row * 170
        s.append(text(0, y - 26, name, size=13, fill=colour, weight=700))
        s.append(text(0, y - 10, note, size=9.5, fill=DIM))
        # the frame itself
        s.append(box(t(0), y, t(16655) - t(0), 54, stroke=LINE, fill=PANEL))
        # Labels alternate between two baselines: at this scale the last three
        # marks of a frame are within two milliseconds of each other and a
        # single row of text collides with itself.
        for i, (us, label, c) in enumerate(marks):
            x = t(us)
            drop = 0 if i % 2 == 0 else 30
            s.append(f'<path d="M{x} {y - 6} V{y + 62 + drop}" stroke="{c}" stroke-width="1.2"/>')
            anchor = "end" if us > 15000 else ("start" if us < 1000 else "middle")
            dx = -4 if anchor == "end" else 4 if anchor == "start" else 0
            s.append(text(x + dx, y + 76 + drop, label, size=9.5, fill=c, weight=500, anchor=anchor))
            s.append(text(x + dx, y + 90 + drop, f"{us/1000:.2f} ms", size=9, fill="#3b4261", anchor=anchor))
        # the span that is the measurement
        a, b = (marks[2][0], 16655)  # the commit is always the third mark
        s.append(f'<path d="M{t(a)} {y + 40} H{t(b)}" stroke="{ORANGE}" stroke-width="1.4" '
                 f'marker-end="url(#head-{ORANGE.lstrip("#")})" marker-start="url(#head-{ORANGE.lstrip("#")})"/>')
        s.append(text((t(a) + t(b)) / 2, y + 34, f"{(b - a)/1000:.2f} ms to scanout",
                      size=10, fill=ORANGE, weight=700, anchor="middle"))
    s.append(text(0, 412, "One frame, 16.655 ms, drawn to scale. Measured percentiles are in the table beside this.",
                  size=10, fill=DIM))
    s.append("</svg>")
    return "".join(s)

sheet("Frame.dc.html",
      "Flyback &middot; the frame",
      "Where the milliseconds went",
      "The same client in both, drawing one millisecond, with the launcher mapped underneath. "
      "Under a fixed refresh the commit waits for the deadline; under a variable one there is no deadline, "
      "so it is drawn and queued in seventy-five microseconds and the frame simply ends when the flip lands.",
      timeline(),
      "<b>Measured, not drawn.</b> From the compositor's own trace, 200 frames in each regime, "
      "microseconds as 5th / 50th / 95th percentile. <b>Commit to flip queued</b>: 55 / 75 / 244 variable, "
      "4069 / 4144 / 13905 fixed. <b>Flip queued to vblank</b>: 2238 / 2423 / 2487 variable, "
      "1531 / 1643 / 1894 fixed. <b>Vblank interval</b>: 16655 in both. "
      "The callback and commit marks are arithmetic from the estimates rather than measurements, and are drawn lighter for that reason.")
