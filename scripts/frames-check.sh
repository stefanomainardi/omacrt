#!/usr/bin/env bash
# Render the launcher's own screens headlessly and compare them with the
# frames kept in the repository.
#
#   scripts/frames-check.sh            compare
#   scripts/frames-check.sh --bless    accept what it renders now
#
# The check this replaces asked only whether a frame compressed to more than
# 1500 bytes, which catches a blank screen and nothing else: a menu with every
# label drawn on top of every other passes it. This compares the pixels.
#
# Determinism: the clock is pinned with `--clock`, and the configuration and
# the scanned library are pointed at empty directories, because the home
# screen prints how many games are in the collection. That line is what made
# the first run of this differ by 353 pixels between a laptop with twenty
# thousand games and a runner with none, and it is a line of text eight pixels
# tall, which is what 353 pixels looks like.
#
# `--config-dir` and `--systems` do not cover it: the index is read through
# `crt::config_dir()` and `index::data_dir()`, which answer to OMACRT_CONFIG
# and XDG_DATA_HOME and not to those arguments.
#
# A screen that draws the weather or a photograph is not deterministic either
# and is deliberately not in here.
#
# A frame that differs is written to $FRAMES_OUT when that is set, so a
# machine that renders differently can be looked at rather than guessed at.
#
# The comparison allows sixteen pixels to differ, because a renderer that does
# any arithmetic in floating point is not obliged to round the same way on
# another machine. Sixteen is a quarter of one 8x8 character, so nothing a
# person could see fits inside it. The first tolerance written here was 0.5%
# of the frame, 384 pixels, and a 40x8 block of magenta drawn on purpose to
# test the check went straight through it.
#
# The count is printed whether it passes or not, so a machine that really does
# round differently says by how much instead of being guessed at.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/.." && pwd)"
want="$repo/shell/tests/frames"
shell_bin="$repo/shell/target/release/omacrt-shell"
times="6.0,11.0,12.5"
clock="12:34"
bless=0
[ "${1:-}" = "--bless" ] && bless=1

if [ ! -x "$shell_bin" ]; then
  echo "no launcher at $shell_bin: cargo build --release" >&2
  exit 1
fi

got="$(mktemp -d)"
empty="$(mktemp -d)"
trap 'rm -rf "$got" "$empty"' EXIT
mkdir -p "$empty/config" "$empty/data"

SDL_VIDEODRIVER=dummy \
  OMACRT_CONFIG="$empty/config" XDG_DATA_HOME="$empty/data" \
  "$shell_bin" --headless --no-audio \
  --clock "$clock" --dump "$times" --dump-dir "$got" >/dev/null || {
  echo "the launcher did not render its frames" >&2
  exit 1
}

if [ "$bless" = 1 ]; then
  mkdir -p "$want"
  rm -f "$want"/*.ppm.gz
  for f in "$got"/*.ppm; do
    gzip -9c "$f" > "$want/$(basename "$f").gz"
  done
  echo "blessed $(ls -1 "$want" | wc -l) frames into ${want#"$repo"/}"
  exit 0
fi

if [ ! -d "$want" ] || [ -z "$(ls -A "$want" 2>/dev/null)" ]; then
  echo "no frames to compare against: run $0 --bless" >&2
  exit 1
fi

bad=0
for ref in "$want"/*.ppm.gz; do
  name="$(basename "$ref" .gz)"
  if [ ! -f "$got/$name" ]; then
    echo "FAIL $name was not rendered" >&2
    bad=1
    continue
  fi
  gzip -dc "$ref" > "$got/$name.want"
  python3 - "$got/$name.want" "$got/$name" "$name" <<'PY' || bad=1
import sys

def read(path):
    with open(path, "rb") as f:
        data = f.read()
    # P6 <w> <h> <max>\n, the header's fields separated by whitespace.
    fields, i = [], 0
    while len(fields) < 4:
        while data[i:i + 1].isspace():
            i += 1
        if data[i:i + 1] == b"#":
            while data[i:i + 1] not in (b"\n", b""):
                i += 1
            continue
        start = i
        while not data[i:i + 1].isspace():
            i += 1
        fields.append(data[start:i])
    return fields[1].decode(), fields[2].decode(), data[i + 1:]

want_w, want_h, want = read(sys.argv[1])
got_w, got_h, got = read(sys.argv[2])
name = sys.argv[3]
if (want_w, want_h) != (got_w, got_h):
    print(f"FAIL {name} is {got_w}x{got_h}, the frame kept is {want_w}x{want_h}",
          file=sys.stderr)
    raise SystemExit(1)
pixels = int(want_w) * int(want_h)
differing = sum(
    1
    for i in range(0, min(len(want), len(got)), 3)
    if want[i:i + 3] != got[i:i + 3]
)
allowed = 16
if differing > allowed:
    print(
        f"FAIL {name}: {differing} of {pixels} pixels differ, more than the "
        f"{allowed} allowed. If the change is meant, scripts/frames-check.sh "
        f"--bless",
        file=sys.stderr,
    )
    raise SystemExit(1)
print(f"ok   {name}: {differing} of {pixels} pixels differ")
PY
done

# What this machine drew, for comparing against what the repository holds
# when the two disagree.
if [ "$bad" != 0 ] && [ -n "${FRAMES_OUT:-}" ]; then
  mkdir -p "$FRAMES_OUT"
  for f in "$got"/*.ppm; do
    gzip -9c "$f" > "$FRAMES_OUT/$(basename "$f").gz"
  done
  echo "what this machine drew is in $FRAMES_OUT"
fi

exit "$bad"
