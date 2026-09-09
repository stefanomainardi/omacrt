#!/usr/bin/env bash
# Hand the tube's connector to omacrt: mark it non-desktop through an
# EDID override so the desktop compositor stops configuring it and offers it
# for DRM leasing. Needs root (debugfs and the connector's sysfs files).
#
#   sudo scripts/crt-lease-setup.sh on   [connector]   override + simulated replug
#   sudo scripts/crt-lease-setup.sh off  [connector]   back to the real EDID
#
# Writing the sysfs `status` file only re-probes the connector and emits no
# hotplug event, so the compositor never notices; amdgpu's debugfs
# `trigger_hotplug` simulates an unplug (0) and a plug (1) with the events.
set -eu
conn="${2:-HDMI-A-1}"
case "$conn" in
  *[!A-Za-z0-9-]*|"") echo "not a connector name: $conn" >&2; exit 1 ;;
esac
[ -e "/sys/class/drm/card"*"-$conn" ] || { echo "no such connector: $conn" >&2; exit 1; }
card="$(basename "$(dirname "$(readlink -f /sys/class/drm/card*-"$conn")")")"
minor="${card#card}"
dbg="/sys/kernel/debug/dri/$minor/$conn"
status="/sys/class/drm/$card-$conn/status"
edid="/sys/class/drm/$card-$conn/edid"
here="$(cd "$(dirname "$0")" && pwd)"
replug() {
  echo 0 > "$dbg/trigger_hotplug"; sleep 5
  echo 1 > "$dbg/trigger_hotplug"; sleep 3
}
case "${1:-}" in
  on)
    # Start from the sink's own EDID, whatever override is in place now.
    printf reset > "$dbg/edid_override"; echo detect > "$status"; sleep 1
    # A run that was interrupted between the unplug and the plug leaves the
    # connector disconnected with nothing to read. Plug it back before
    # reading, or the patch has no EDID to work from.
    if [ "$(cat "$status")" = disconnected ]; then
      echo "connector reads disconnected; plugging it back"
      echo 1 > "$dbg/trigger_hotplug"; sleep 3; echo detect > "$status"; sleep 1
    fi
    out="/run/omacrt-$conn.edid"
    python3 "$here/edid-non-desktop.py" "$edid" "$out"
    cat "$out" > "$dbg/edid_override"
    echo detect > "$status"; sleep 1
    replug
    echo "live EDID Microsoft blocks: $(edid-decode "$edid" 2>/dev/null | grep -c 'Microsoft')"
    ;;
  off)
    printf reset > "$dbg/edid_override"; echo detect > "$status"; sleep 1
    replug
    ;;
  *) sed -n '2,8p' "$0"; exit 1 ;;
esac
