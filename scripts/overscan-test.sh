#!/usr/bin/env bash
# Show a pattern on the television that answers one question with the eye:
# how much of the picture the tube throws away at each edge.
#
#   scripts/overscan-test.sh          # the mode the tube is in now
#   scripts/overscan-test.sh ntsc     # switch first
#
# Five nested rectangles at 100, 96, 92, 88 and 84 percent of the frame, each
# a different colour, with a cross at the centre. Name the outermost
# rectangle whose four sides you can all see and that is the safe area of
# this set, to within four percent. A rectangle whose left side shows and
# whose right side does not means the picture is off centre, not overscanned,
# and `omacrt mode --shift-x` is the answer to that one.
#
# Press q to close it.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
omacrt="$(command -v omacrt || echo "$here/../shell/target/release/omacrt")"

if [ "$#" -gt 0 ]; then
  "$omacrt" mode "$1" >/dev/null || exit 1
  sleep 1
fi

read -r w h < <("$omacrt" status --json | python3 -c '
import json, sys
m = json.load(sys.stdin)["mode"]
print(m["width"], m["height"])
')
if [ -z "${w:-}" ]; then
  echo "no mode on the tube: omacrt on" >&2
  exit 1
fi

png="$(mktemp -t overscan.XXXX.png)"
python3 - "$w" "$h" "$png" <<'PY'
import sys
from PIL import Image, ImageDraw

w, h, out = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
img = Image.new("RGB", (w, h), (0, 0, 0))
d = ImageDraw.Draw(img)

# The frame is stretched horizontally by eleven or twelve, so a one pixel
# line drawn here is one pixel on the tube vertically and eleven across.
# The rectangles are drawn thick enough to survive that and to be seen on a
# set whose focus is not a monitor's.
thick_x = max(2, w // 320)
thick_y = 2

bands = [
    (100, (255, 255, 255), "white"),
    (96, (255, 255, 0), "yellow"),
    (92, (0, 255, 255), "cyan"),
    (88, (255, 0, 255), "magenta"),
    (84, (0, 255, 0), "green"),
]
for pct, colour, _ in bands:
    dx = int(w * (100 - pct) / 200)
    dy = int(h * (100 - pct) / 200)
    for t in range(thick_x):
        d.line([(dx + t, dy), (dx + t, h - 1 - dy)], fill=colour)
        d.line([(w - 1 - dx - t, dy), (w - 1 - dx - t, h - 1 - dy)], fill=colour)
    for t in range(thick_y):
        d.line([(dx, dy + t), (w - 1 - dx, dy + t)], fill=colour)
        d.line([(dx, h - 1 - dy - t), (w - 1 - dx, h - 1 - dy - t)], fill=colour)

# The centre, for judging whether the picture sits in the middle of the tube
# rather than how much of it is left.
cx, cy = w // 2, h // 2
d.line([(cx - w // 20, cy), (cx + w // 20, cy)], fill=(255, 255, 255), width=thick_y)
d.line([(cx, cy - h // 12), (cx, cy + h // 12)], fill=(255, 255, 255), width=thick_x)

img.save(out)
PY

echo "white 100%  yellow 96%  cyan 92%  magenta 88%  green 84%"
echo "Name the outermost rectangle whose four sides you can all see."
echo "q closes it."
WAYLAND_DISPLAY=wayland-crt mpv --no-config --really-quiet --fullscreen \
  --no-audio --loop=inf --image-display-duration=inf \
  --scale=nearest --dscale=nearest --video-unscaled=yes \
  "$png" 2>/dev/null
rm -f "$png"
