#!/usr/bin/env bash
# Hand the tube's connector to omarchy-crt: mark it non-desktop through an
# EDID override so the desktop compositor stops configuring it and offers it
# for DRM leasing. Needs root (debugfs and the connector's status file).
#
#   sudo scripts/crt-lease-setup.sh on   [connector]   override + re-detect
#   sudo scripts/crt-lease-setup.sh off  [connector]   back to the real EDID
set -eu
conn="${2:-HDMI-A-1}"
card="$(basename "$(dirname "$(readlink -f /sys/class/drm/card*-"$conn")")")"
minor="${card#card}"
dbg="/sys/kernel/debug/dri/$minor/$conn"
status="/sys/class/drm/$card-$conn/status"
edid="/sys/class/drm/$card-$conn/edid"
here="$(cd "$(dirname "$0")" && pwd)"
case "${1:-}" in
  on)
    out="/run/omarchy-crt-$conn.edid"
    python3 "$here/edid-non-desktop.py" "$edid" "$out"
    cat "$out" > "$dbg/edid_override"
    echo off > "$status"; sleep 1; echo detect > "$status"
    sleep 1
    echo "override in place: $(edid-decode "$edid" 2>/dev/null | grep -c 'Microsoft') Microsoft block(s) in the live EDID"
    ;;
  off)
    echo reset > "$dbg/edid_override"
    echo off > "$status"; sleep 1; echo detect > "$status"
    ;;
  *) sed -n '2,8p' "$0"; exit 1 ;;
esac
