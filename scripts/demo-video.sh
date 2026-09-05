#!/usr/bin/env bash
# Render the shell demo offline (frames + mixed audio) and encode it for X.
# Usage: scripts/demo-video.sh [OUT.mp4] [SECONDS] [SCRIPT]
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:-$here/demo.mp4}"
secs="${2:-33}"
script="${3:-$here/scripts/demo.txt}"
work="$(mktemp -d -t omarchy-crt-demo.XXXX)"
trap 'rm -rf "$work"' EXIT
bin="$here/shell/target/release/omarchy-crt-shell"
[ -x "$bin" ] || (cd "$here/shell" && cargo build --release)
"$bin" --record "$work" --record-secs "$secs" --script "$script"
ffmpeg -y -loglevel error -framerate 60 -i "$work/frame_%05d.ppm" -i "$work/audio.wav" \
  -vf "scale=960:720:flags=neighbor,format=yuv420p" \
  -c:v libx264 -preset slow -crf 18 -pix_fmt yuv420p -r 60 \
  -c:a aac -b:a 192k -movflags +faststart -shortest "$out"
echo "$out"
