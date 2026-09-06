#!/usr/bin/env bash
# Drive the launcher through the follow-up video, keyboard events included,
# so the person behind the phone only films. Steps print what is happening
# and wait where a human has to act (leaving a game: RetroArch reads the
# keyboard through udev, so a synthetic Esc never reaches it).
#
#   scripts/shoot.sh prep      film timing (60.00 Hz), tube on, launcher fresh
#   scripts/shoot.sh desktop   bar panel: power off, power on (records DP-2)
#   scripts/shoot.sh tour      systems, collections, covers, a game, a video
#   scripts/shoot.sh restore   back to the NTSC timing
set -u

id="io.github.stefanomainardi.omarchy-crt"
key() { wtype -k "$1"; sleep "${2:-0.3}"; }
say() { printf '\n\033[1;32m>> %s\033[0m\n' "$*"; }
cue() { printf '\n\033[1;33m!! %s\033[0m\n' "$*"; read -r -p "   press Enter when done "; }

case "${1:-}" in
  prep)
    say "film timing 60.00 Hz, tube on"
    omarchy-crt on >/dev/null
    omarchy-crt mode film
    sleep 1
    omarchy-crt dac status
    say "launcher restarted so the boot sequence is fresh"
    omarchy-crt shell restart >/dev/null
    say "ready: run 'desktop' with the phone on the desk, 'tour' with the phone on the TV"
    ;;
  desktop)
    hyprctl eval 'hl.dispatch(hl.dsp.focus({ monitor = "DP-2" }))' >/dev/null
    say "recording the desktop (stop with: omarchy screenrecord --stop-recording)"
    omarchy screenrecord --fullscreen >/dev/null 2>&1 &
    sleep 3
    say "open the panel"
    omarchy-shell shell summon "$id" '{}' >/dev/null 2>&1
    sleep 4
    say "power off from the panel"
    omarchy-shell -q "$id" off >/dev/null 2>&1
    sleep 6
    say "power on from the panel: the tube boots"
    omarchy-shell -q "$id" on >/dev/null 2>&1
    sleep 16
    omarchy-shell shell hide "$id" >/dev/null 2>&1
    omarchy screenrecord --stop-recording >/dev/null 2>&1
    say "desktop take done"
    ;;
  tour)
    say "fresh boot on the tube (laser etch, Mode 7 tag)"
    omarchy-crt shell restart >/dev/null
    sleep 14
    omarchy-crt focus >/dev/null
    sleep 0.5
    say "Games: the systems and their consoles"
    key Return 2.5
    for _ in 1 2 3 4 5 6 7 8; do key Down 1.1; done
    for _ in 1 2 3 4 5 6; do key Up 0.35; done
    say "Collections"
    key Return 2.5
    for _ in 1 2 3 4 5; do key Right 0.6; done
    for _ in 1 2 3 4 5; do key Down 0.35; done
    sleep 1.5
    say "SNES Must Play, covers"
    key Return 3.0
    for _ in 1 2 3 4; do key Down 1.6; done
    sleep 1.5
    say "Chrono Trigger"
    key Return 1
    cue "let the game run ~40 s, then press Esc on the real keyboard to leave"
    sleep 2
    omarchy-crt focus >/dev/null
    say "back home, then Videos"
    key Escape 0.8; key Escape 0.8; key Escape 0.8
    key Down 0.9
    key Return 2.5
    for _ in 1 2 3 4 5; do key Down 0.8; done
    sleep 1.2
    say "Omarchy intro, CRT ready file"
    key Return 1
    sleep 30
    key Escape 1.5
    key Escape 0.8; key Escape 0.8
    say "tour done"
    ;;
  restore)
    omarchy-crt mode ntsc
    ;;
  *) sed -n '2,10p' "$0" ;;
esac
