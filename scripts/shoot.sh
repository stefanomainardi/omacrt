#!/usr/bin/env bash
# Record the follow-up video. The tube is captured by the display process
# itself (picture and the HDMI sink's audio), the desktop by omarchy's
# screen recorder. Every step drives the launcher over its control pipe.
#
#   scripts/shoot.sh desktop OUT.mp4   bar plugin: panel, power off, power on
#   scripts/shoot.sh tube OUT.mp4      boot, systems, collection, games, pause menu, video
#   scripts/shoot.sh restore           NTSC timing
#
# The tour uses the collection "0 Tour" (~/.config/omarchy-crt/collections),
# whose first rows are: Yie Ar Kung-Fu, Chrono Trigger, Sonic 2, Pac-Man,
# Super Metroid, Streets of Rage 2, Metal Slug, Super Mario Bros. 3.
set -u
id="io.github.stefanomainardi.omarchy-crt"
key() { omarchy-crt shell key "$1"; sleep "${2:-0.3}"; }
say() { printf '\n\033[1;32m>> %s\033[0m\n' "$*"; }
wait_game() { for _ in $(seq 1 40); do sleep 0.5; pgrep -x retroarch >/dev/null && break; done; sleep "${1:-6}"; }
wait_no_game() { for _ in $(seq 1 30); do pgrep -x retroarch >/dev/null || break; sleep 0.5; done; sleep 1.5; }
play_row() {  # $1 = row in "0 Tour" (0 based), $2 = seconds to play, $3 = label
  say "$3"
  omarchy-crt shell key home; sleep 0.6
  key fire 1.6                       # Games
  key down 0.3; key down 0.5         # Collections row
  key fire 1.6                       # Collections list, "0 Tour" first
  key fire 2.5                       # open it
  for _ in $(seq 1 "$1"); do key down 0.5; done
  sleep 1.2
  key fire 1
  wait_game "$2"
}
quit_game() { omarchy-crt shell key menu; sleep 2.2; key down 0.4; key down 0.4; key down 0.4; key down 0.6; key fire 1; wait_no_game; }

case "${1:-}" in
  desktop)
    out="${2:?output file}"
    hyprctl eval 'hl.dispatch(hl.dsp.focus({ monitor = "DP-2" }))' >/dev/null
    say "recording the desktop"
    omarchy screenrecord --fullscreen >/dev/null 2>&1 &
    sleep 3
    say "panel"; omarchy-shell shell summon "$id" '{}' >/dev/null 2>&1; sleep 5
    say "power off"; omarchy-shell -q "$id" off >/dev/null 2>&1; sleep 7
    say "power on"; omarchy-shell -q "$id" on >/dev/null 2>&1; sleep 18
    omarchy-shell shell hide "$id" >/dev/null 2>&1
    omarchy screenrecord --stop-recording >/dev/null 2>&1
    sleep 2
    latest="$(ls -t ~/Videos/screenrecording-*.mp4 2>/dev/null | head -1)"
    [ -n "$latest" ] && mv "$latest" "$out" && say "desktop take: $out"
    ;;
  tube)
    out="${2:?output file}"
    say "fresh launcher, recording starts with the boot"
    omarchy-crt shell restart >/dev/null
    sleep 1
    omarchy-crt record start "$out"
    sleep 13
    say "Games: systems and console pictures"
    key fire 2.0
    for _ in 1 2 3 4 5 6 7 8; do key down 1.0; done
    for _ in 1 2 3 4 5 6; do key up 0.3; done
    say "Collections"
    key fire 2.0
    key fire 2.5
    say "covers"
    for _ in 1 2 3 4 5 6 7; do key down 1.3; done
    for _ in 1 2 3 4 5 6 7; do key up 0.35; done
    sleep 1
    say "Yie Ar Kung-Fu (arcade)"
    key fire 1; wait_game 28
    say "pause menu: save state, resume"
    omarchy-crt shell key menu; sleep 3
    key down 0.5; key fire 2.5
    key up 0.5; key fire 1; sleep 6
    quit_game
    play_row 1 22 "Chrono Trigger (Super Nintendo, 224 lines)"
    quit_game
    play_row 2 20 "Sonic The Hedgehog 2 (Mega Drive)"
    quit_game
    play_row 6 18 "Metal Slug (Neo Geo)"
    quit_game
    say "Videos: the Omarchy intro"
    omarchy-crt shell key home; sleep 0.6
    key down 0.6; key fire 2.5
    for _ in 1 2 3 4 5; do key down 0.7; done
    sleep 1; key fire 1; sleep 26
    key back 1.5; key back 0.8
    omarchy-crt shell key home; sleep 2
    omarchy-crt record stop
    say "tube take: $out"
    ;;
  restore) omarchy-crt mode ntsc ;;
  *) sed -n '2,12p' "$0" ;;
esac
