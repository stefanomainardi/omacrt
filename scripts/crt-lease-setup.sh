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

# This unit runs Before=display-manager.service, so every second it spends
# here is a second the login screen waits. `settle` replaces a blind sleep
# with waiting for the connector to actually answer, which on a machine with
# the television on takes a fraction of what the sleeps cost.
settle() {  # $1 = the state to wait for, $2 = seconds to give it
  local want="$1" until_n="${2:-5}" i=0
  while [ "$i" -lt "$((until_n * 10))" ]; do
    [ "$(cat "$status" 2>/dev/null)" = "$want" ] && return 0
    sleep 0.1
    i=$((i + 1))
  done
  return 1
}

# A replug is an unplug and a plug with the compositor noticing in between.
# The marker exists so an interrupted run can be told apart from a television
# that is simply switched off: without it, `on` used to try to recover from a
# perfectly normal cold boot and spend four seconds failing.
marker="/run/omacrt-replugging-$conn"
replug() {
  : > "$marker"
  echo 0 > "$dbg/trigger_hotplug"
  settle disconnected 5 || true
  echo 1 > "$dbg/trigger_hotplug"
  settle connected 5 || true
  rm -f "$marker"
}
case "${1:-}" in
  on)
    # A run interrupted between the unplug and the plug leaves the connector
    # disconnected with nothing to read. That is the only case worth
    # recovering from, and the marker is what says so.
    if [ -e "$marker" ]; then
      echo "a previous run stopped mid replug; plugging the connector back"
      echo 1 > "$dbg/trigger_hotplug"
      settle connected 5 || true
      rm -f "$marker"
    fi
    # Start from the sink's own EDID, whatever override is in place now.
    printf reset > "$dbg/edid_override"; echo detect > "$status"
    settle connected 2 || true
    # A television that is switched off is not a failure, and it is the
    # normal state at boot. Saying so and standing down keeps the login
    # screen from waiting on a tube that is not there.
    if [ "$(cat "$status")" != connected ]; then
      echo "no television on $conn; nothing to hand over"
      exit 0
    fi
    out="/run/omacrt-$conn.edid"
    python3 "$here/edid-non-desktop.py" "$edid" "$out"
    cat "$out" > "$dbg/edid_override"
    echo detect > "$status"
    settle connected 2 || true
    replug
    echo "live EDID Microsoft blocks: $(edid-decode "$edid" 2>/dev/null | grep -c 'Microsoft')"
    ;;
  off)
    printf reset > "$dbg/edid_override"; echo detect > "$status"
    settle connected 2 || true
    replug
    ;;
  *) sed -n '2,8p' "$0"; exit 1 ;;
esac
