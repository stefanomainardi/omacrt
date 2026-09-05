#!/usr/bin/env bash
# Probe an HDMI/DP connector for a CRT DAC: connection status, EDID, kernel modes,
# Hyprland view. Read-only. Usage: crt-probe.sh [CONNECTOR]  (default: first connected HDMI)
set -u

pick_connector() {
  for c in /sys/class/drm/card*-HDMI-A-*; do
    [ "$(cat "$c/status")" = connected ] && { basename "$c"; return; }
  done
}

conn="${1:-$(pick_connector)}"
if [ -z "$conn" ]; then
  echo "no connected HDMI connector found; connectors:"
  for c in /sys/class/drm/card*-*; do echo "  $(basename "$c") $(cat "$c/status")"; done
  exit 1
fi

sys="/sys/class/drm/$conn"
echo "== $conn: status=$(cat "$sys/status") enabled=$(cat "$sys/enabled") dpms=$(cat "$sys/dpms" 2>/dev/null)"
echo "== driver: $(basename "$(readlink "$sys/device/driver" 2>/dev/null || readlink "$sys/../device/driver")")"

echo "== kernel mode list ($sys/modes):"
sed 's/^/  /' "$sys/modes"

edid_bytes=$(wc -c < "$sys/edid")
echo "== EDID: $edid_bytes bytes"
if [ "$edid_bytes" -gt 0 ]; then
  out="${CRT_PROBE_OUT:-/tmp}/edid-${conn}.bin"
  cp "$sys/edid" "$out" && echo "  saved to $out"
  if command -v edid-decode >/dev/null; then
    edid-decode "$sys/edid" | sed 's/^/  /'
  fi
fi

if command -v hyprctl >/dev/null && hyprctl monitors all -j >/dev/null 2>&1; then
  echo "== Hyprland view:"
  hyprctl monitors all -j | python3 -c '
import json,sys
want=sys.argv[1]
for m in json.load(sys.stdin):
    if m["name"]!=want: continue
    print("  name:",m["name"],"| desc:",m["description"])
    print("  current:",m["width"],"x",m["height"],"@",m["refreshRate"],"| disabled:",m["disabled"])
    print("  availableModes:")
    for am in m.get("availableModes",[]): print("    ",am)
' "$conn"
fi

echo "== recent amdgpu/drm kernel log lines:"
journalctl -k -b --no-pager 2>/dev/null | grep -iE "drm|amdgpu|hdmi|edid" | tail -n 15 | sed 's/^/  /'
