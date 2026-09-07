#!/usr/bin/env bash
# Record the follow-up video. The tube is captured by the display process
# itself (picture and the HDMI sink's audio), the desktop by wf-recorder.
# Every step drives the launcher over its control pipe.
#
#   scripts/shoot.sh desktop OUT.mp4   bar, widget, panel, power off and on, PAL and back
#   scripts/shoot.sh boot OUT.mp4      the launcher booting on a black tube
#   scripts/shoot.sh tube OUT.mp4      boot, systems, collection, games, pause menu, video
#   scripts/shoot.sh music OUT.mp4     the deck, the turntable, a visualizer, the equaliser
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
# The pause menu has eight rows; "Back to launcher" is the last one.
quit_game() { omarchy-crt shell key menu; sleep 2.2; for _ in 1 2 3 4 5 6 7; do key down 0.35; done; key fire 1; wait_no_game; }

case "${1:-}" in
  desktop)
    # A clean take of the main monitor: an empty workspace, the bar with the
    # widget, the pointer arriving on it, the panel, power off and on, a live
    # line standard switch. Captured with wf-recorder (screencopy, the picture
    # the compositor shows): gpu-screen-recorder reads one DRM plane and this
    # Hyprland spreads wallpaper, bar and popups over several. The pointer
    # waits at the bottom edge first so it visibly travels to the widget. The
    # shell's idle screensaver would otherwise take the workspace mid-take.
    out="${2:?output file}"
    command -v wf-recorder >/dev/null || { echo "wf-recorder missing (pacman -S wf-recorder)" >&2; exit 1; }
    read -r mon mx my mw mh <<<"$(hyprctl monitors -j | python3 -c 'import json,sys; m=[m for m in json.load(sys.stdin) if m["focused"]][0]; print(m["name"], m["x"], m["y"], m["width"], m["height"])')"
    widget_x=$((mx + mw - 332)); widget_y=$((my + 14))   # the CRT widget in the bar's right group
    idle_before="$(omarchy toggle idle status | grep -c '"enabled":true' || true)"
    omarchy toggle idle stay-awake >/dev/null
    was_ws="$(hyprctl activeworkspace -j | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
    # A named workspace of our own: an empty one by construction. Numbered
    # workspaces may hold windows, and a take with someone's terminals in it
    # is a take that cannot be published.
    # Hyprland 0.56 takes dispatchers through its Lua API: the shell form
    # `hyprctl dispatch workspace 9` is a Lua syntax error and changes
    # nothing, which is how the first take ended up full of windows.
    hyprctl eval 'hl.dispatch(hl.dsp.focus({ workspace = "name:crt-shoot" }))' >/dev/null
    sleep 1
    open_windows="$(hyprctl activeworkspace -j | python3 -c 'import json,sys; print(json.load(sys.stdin)["windows"])')"
    if [ "$open_windows" != "0" ]; then
      echo "the workspace is not empty ($open_windows windows): not recording" >&2
      hyprctl eval "hl.dispatch(hl.dsp.focus({ workspace = \"$was_ws\" }))" >/dev/null
      [ "$idle_before" = 1 ] || omarchy toggle idle allow-idle >/dev/null
      exit 1
    fi
    hyprctl eval "hl.dispatch(hl.dsp.cursor.move({ x = $((mx + mw / 2)), y = $((my + mh - 2)) }))" >/dev/null
    sleep 1.5
    say "recording $mon"
    wf-recorder -o "$mon" -r 60 -c libx264 -p preset=ultrafast -p crf=16 -x yuv420p -f "$out" >/dev/null 2>&1 &
    rec=$!
    sleep 3.5
    say "pointer on the widget"; hyprctl eval "hl.dispatch(hl.dsp.cursor.move({ x = $widget_x, y = $widget_y }))" >/dev/null; sleep 4
    say "panel"; omarchy-shell shell summon "$id" '{}' >/dev/null 2>&1; sleep 5
    say "power off"; omarchy-shell -q "$id" off >/dev/null 2>&1; sleep 7
    say "power on"; omarchy-shell -q "$id" on >/dev/null 2>&1; sleep 17
    say "PAL 50"; omarchy-shell -q "$id" pal >/dev/null 2>&1; sleep 6
    say "NTSC 60"; omarchy-shell -q "$id" ntsc >/dev/null 2>&1; sleep 6
    omarchy-shell shell hide "$id" >/dev/null 2>&1
    sleep 2.5
    kill -INT "$rec"; wait "$rec" 2>/dev/null
    hyprctl eval "hl.dispatch(hl.dsp.cursor.move({ x = $((mx + mw / 2)), y = $((my + mh / 2)) }))" >/dev/null
    hyprctl eval "hl.dispatch(hl.dsp.focus({ workspace = \"$was_ws\" }))" >/dev/null
    [ "$idle_before" = 1 ] || omarchy toggle idle allow-idle >/dev/null
    say "desktop take: $out"
    ;;
  boot)
    # The whole boot, from a black tube: recording runs before the launcher starts.
    out="${2:?output file}"
    omarchy-crt shell stop >/dev/null 2>&1 || true
    sleep 1
    omarchy-crt record start "$out"
    sleep 1.5
    omarchy-crt shell start >/dev/null 2>&1 || omarchy-crt shell restart >/dev/null
    sleep 19
    omarchy-crt record stop
    say "boot take: $out"
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
  music)
    out="${2:?output file}"
    # The music screens, with the station or track already playing: radio
    # first for the dial and its logo, then the deck looks, a visualizer and
    # the equaliser. Nothing here needs the desktop.
    say "recording the music screens"
    omarchy-crt record start "$out"
    sleep 2
    omarchy-crt shell key home; sleep 1.2
    for _ in 1 2 3 4 5 6 7 8; do key up 0.2; done   # to the top of the list
    key down 0.4; key down 0.7      # Music, the third row
    key fire 2.0
    say "Radio: the country list"
    key fire 2.5                    # Radio hub
    key fire 3.0                    # the home country's stations
    for _ in 1 2 3; do key down 0.8; done
    key fire 6                      # tune in: the dial, the static, the logo
    say "the deck: cassette, turntable"
    key next 5                      # shoulder: the other deck look
    key next 5
    say "a visualizer"
    key alt 8                       # X: show the visualizer
    key next 6                      # the next mode
    key next 6
    key alt 2                       # back to the deck
    say "the equaliser"
    omarchy-crt shell key home; sleep 1.2
    for _ in 1 2 3 4 5 6 7 8; do key up 0.2; done
    key down 0.4; key down 0.7      # Music, the third row
    key fire 2.0
    # The root remembers the last row; up walks to the top (it stops there),
    # then the Equalizer is the last of the six rows with music playing.
    for _ in 1 2 3 4 5 6; do key up 0.2; done
    for _ in 1 2 3 4 5; do key down 0.35; done
    key fire 2.5
    # The page is shown, not used: walking the bands or the presets would
    # leave the listener's own curve changed.
    for _ in 1 2 3; do key right 0.9; done
    sleep 2
    key back 1.5
    omarchy-crt shell key home; sleep 2
    omarchy-crt record stop
    say "music take: $out"
    ;;
  restore) omarchy-crt mode ntsc ;;
  *) sed -n '2,13p' "$0" ;;
esac
