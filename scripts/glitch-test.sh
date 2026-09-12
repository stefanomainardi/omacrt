#!/usr/bin/env bash
# Put the tube into one of the four states the "the picture twitches" hunt
# needs, and read back what it was actually given.
#
# The thing being chased is a brief disturbance of the whole picture, as if
# the set were re-acquiring the signal. On this chain the only thing that can
# do that without a mode change is the frame length moving, which is what a
# variable refresh rate does when it follows a client whose own pace wanders.
#
#   scripts/glitch-test.sh 1     fixed refresh: the frame cannot move at all
#   scripts/glitch-test.sh 2     variable, and the rate asked for by name
#   scripts/glitch-test.sh 3     variable, following the program (the control)
#   scripts/glitch-test.sh report        what the tube has been given since
#   scripts/glitch-test.sh watch         the same, once a second
#
# Each state stays until the next one is asked for, and none of them survives
# a restart of the display process, which puts the variable rate back on.
set -u

omacrt=${OMACRT:-omacrt}
send() { "$omacrt" display send "$1" >/dev/null 2>&1 || printf 'vrr %s\n' "${1#vrr }" > "$HOME/.local/state/omacrt/display.ctl"; }
ctl() { printf '%s\n' "$1" > "$HOME/.local/state/omacrt/display.ctl"; }

# The rate a GameCube core actually runs at, which its own log declares as
# "Target refresh rate changed: 119.8801 Hz -> 59.9401 Hz". A different system
# wants a different number and the point of the test is unchanged.
rate=${RATE:-59.94}

case "${1:-}" in
  1)
    ctl "rate off"; sleep 1; ctl "vrr off"
    echo "state 1: fixed refresh. The tube is pinned to the mode's own rate and"
    echo "         the frame length cannot move by a microsecond, whatever the"
    echo "         game does."
    echo
    echo "  Play for ten minutes. If the twitch is GONE it is the variable"
    echo "  refresh rate, and state 2 is the cure. If it is STILL THERE the"
    echo "  pacing is innocent and the next place to look is the converter."
    ;;
  2)
    ctl "vrr on"; sleep 1; ctl "rate $rate"
    echo "state 2: variable refresh, rate asked for: $rate Hz."
    echo "         The compositor stops following the program's own pace and"
    echo "         holds the frame length on a number instead."
    echo
    echo "  If the twitch does not come back, this is the fix, and the"
    echo "  launcher can ask for the right rate by itself: it already knows"
    echo "  which system is running."
    ;;
  3)
    ctl "rate off"; sleep 1; ctl "vrr on"
    echo "state 3: variable refresh, following the program. THE CONTROL."
    echo
    echo "  This is the state the twitch was first seen in. It is here to be"
    echo "  sure it comes back: without that, state 2 proves nothing, because"
    echo "  a fault that has stopped on its own looks exactly like a fault"
    echo "  that has been fixed."
    ;;
  report|"")
    "$omacrt" status 2>/dev/null | sed -n '/^Mode:/p;/^Latency:/,+1p'
    echo
    echo "The second line is the one to read. 'frame X to Y' is the shortest"
    echo "and the longest frame the tube was actually given over the last few"
    echo "hundred: those two far apart is the tube being asked for frames of"
    echo "different lengths, which is what it answers with a twitch."
    ;;
  watch)
    echo "Ctrl-C to stop."
    while true; do
      "$omacrt" status 2>/dev/null | sed -n '/^Latency:/,+1p' | tr '\n' ' '
      echo
      sleep 1
    done
    ;;
  *)
    sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
    ;;
esac
