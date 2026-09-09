#!/usr/bin/env bash
# Render the shell demo offline (frames + mixed audio) and encode it for X.
# Uses a throwaway copy of the config directory so the tour never touches
# your profile, settings, recent or favorites.
# Usage: scripts/demo-video.sh [OUT.mp4] [SECONDS] [SCRIPT]
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:-$here/demo.mp4}"
secs="${2:-33}"
script="${3:-$here/scripts/demo.txt}"
work="$(mktemp -d -t omacrt-demo.XXXX)"
trap 'rm -rf "$work"' EXIT
cfg="$work/config"
mkdir -p "$cfg"
if [ -d "$HOME/.config/omacrt" ]; then
  cp -r "$HOME/.config/omacrt/." "$cfg/"
  rm -f "$cfg/recent.txt" "$cfg/favorites.txt" "$cfg/profile.toml" "$cfg/settings.toml"
fi
bin="$here/shell/target/release/omacrt-shell"
[ -x "$bin" ] || (cd "$here/shell" && cargo build --release)
"$bin" --record "$work/frames" --record-secs "$secs" --script "$script" --config-dir "$cfg"
ffmpeg -y -loglevel error -framerate 60 -i "$work/frames/frame_%05d.ppm" -i "$work/frames/audio.wav" \
  -vf "scale=960:720:flags=neighbor,format=yuv420p" \
  -c:v libx264 -preset slow -crf 18 -pix_fmt yuv420p -r 60 \
  -c:a aac -b:a 192k -movflags +faststart -shortest "$out"
echo "$out"
