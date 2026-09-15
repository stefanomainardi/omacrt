#!/usr/bin/env bash
# The three things that can only be checked just after a reboot, with the
# machine still cold and nothing of this project running.
#
#   scripts/after-reboot.sh
#
# Run it before `omacrt on`. Two of the three answers are destroyed by
# starting the tube: the previous boot's shutdown is only in the journal
# until the next one, and the EDID has to be read while nobody is holding
# the connector, or the reading says what this project put there a moment
# ago rather than what the boot-time unit did on its own.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
conn="${OMACRT_CONNECTOR:-HDMI-A-1}"
bad=0
warned=0

say() {
  [ "$1" = WARN ] && warned=1
  printf '%-4s %-22s %s\n' "$1" "$2" "$3"
}

# Everything below is only worth reading while the machine is still cold.
# Run late in a session and the EDID says what this project wrote an hour
# ago, which is the question, not the answer.
up="$(cut -d. -f1 /proc/uptime)"
cold=1
if [ "${up:-0}" -gt 900 ]; then
  cold=0
  echo "This machine has been up for $((up / 60)) minutes. These checks want a"
  echo "cold boot: run this before starting the tube, not in the middle of a"
  echo "session, or the readings below say what was done since rather than"
  echo "what the boot did on its own."
  echo
fi

# 1. Did the lease unit hang on the way down? It has a stop action that talks
#    to debugfs, and a stop that times out adds ninety seconds to every
#    shutdown. Only the previous boot's journal knows.
if ! journalctl -b -1 -n 0 >/dev/null 2>&1; then
  say WARN "shutdown clean" "no previous boot in the journal to read"
else
  log="$(journalctl -b -1 -u omacrt-lease.service --no-pager 2>/dev/null)"
  hits="$(printf '%s\n' "$log" | grep -c "timed out")"
  # No timeout is only good news if the unit was asked to stop at all. A boot
  # that was cut short leaves no "Stopping" line, and counting zero timeouts
  # in it proves nothing.
  stopped="$(printf '%s\n' "$log" | grep -c "Stopping\|Stopped\|Deactivated")"
  if [ "${stopped:-0}" -eq 0 ]; then
    say WARN "shutdown clean" "the lease unit was never stopped last boot: nothing to read"
  elif [ "${hits:-0}" -eq 0 ]; then
    say OK "shutdown clean" "the lease unit stopped without timing out"
  else
    say FAIL "shutdown clean" "$hits timeout(s) stopping omacrt-lease.service last boot"
    bad=1
  fi
fi

# 2. Is the FreeSync range in the EDID because the boot-time unit put it
#    there, rather than because somebody ran something? Nine bytes,
#    `68 1a 00 00 01 01 <min> <max> 00`, written by edid-non-desktop.py.
edid="$(ls -1 /sys/class/drm/card*-"$conn"/edid 2>/dev/null | head -1)"
if [ -z "$edid" ]; then
  say FAIL "freesync range" "no EDID to read for $conn: is it connected?"
  bad=1
else
  range="$(python3 - "$edid" <<'PY'
import sys
raw = open(sys.argv[1], "rb").read()
blk = bytes([0x68, 0x1A, 0x00, 0x00, 0x01, 0x01])
i = raw.find(blk)
print("" if i < 0 else f"{raw[i + 6]}-{raw[i + 7]}")
PY
)"
  case "$range" in
  "") say FAIL "freesync range" "no AMD block in the EDID: the override did not run at boot"; bad=1 ;;
  48-62)
    if [ "$cold" -eq 1 ]; then
      say OK "freesync range" "48-62 Hz, written at boot with nothing running"
    else
      say WARN "freesync range" "48-62 Hz, but this machine is not cold: who wrote it is unknown"
    fi
    ;;
  *) say FAIL "freesync range" "$range Hz, and this project writes 48-62"; bad=1 ;;
  esac
fi

# 3. Are the files the root side installed the ones this version carries? A
#    checkout that has moved on from what is in /etc is how an evening gets
#    spent on a bug that was fixed days ago.
omacrt="$(command -v omacrt || echo "$here/../shell/target/release/omacrt")"
row="$("$omacrt" doctor --plain 2>/dev/null | grep "lease files up to date")"
if [ -z "$row" ]; then
  say WARN "lease files" "doctor did not run"
elif [ "${row:0:2}" = "OK" ]; then
  say OK "lease files" "the files this version carries"
else
  say FAIL "lease files" "${row#* }"
  bad=1
fi

echo
if [ "$bad" -eq 0 ] && [ "$warned" -eq 0 ]; then
  echo "All three. Nothing owed from the reboot: omacrt on."
elif [ "$bad" -eq 0 ]; then
  echo "Nothing wrong, but not everything could be read. What is marked WARN"
  echo "above is still owed at the next reboot."
else
  echo "Something is off above. Fix it before starting the tube, or the"
  echo "reading is gone until the next reboot."
fi
exit "$bad"
