#!/usr/bin/env bash
# Cut the presentation film from one live session, filmed twice: the phone
# looking at the television (room, glass, phosphor) and the tube's own
# framebuffer with the television's audio (crisp, unreadably clean).
#
#   scripts/montage-live.sh TAKES_DIR OUT.mp4
#
# TAKES_DIR holds:
#   phone-proxy.mp4   1080p30 of the phone take, upright, its head trimmed so
#                     it starts at the same instant as the capture
#   tube-live-1.mp4   the capture (1280x960 with the TV's audio)
#
# The film is 2.39:1 inside a 1920x1080 frame, and it is the television that is
# being filmed: every shot is the whole set, held still. The picture always
# comes from the phone, the sound always from the capture at the same instant,
# which the alignment makes exact. Shots are joined by a cross dissolve, never
# by a cut: nothing in this film should arrive abruptly.
set -eu
dir="${1:?takes dir}"; out="${2:?output}"
P="$dir/phone-proxy.mp4"; T="$dir/tube-live-1.mp4"
for f in "$P" "$T"; do [ -f "$f" ] || { echo "missing $f" >&2; exit 1; }; done
work="$dir/montage-live"; mkdir -p "$work"
rm -f "$work"/*.mp4
n=0
clips=()   # the files, in order
lens=()    # their durations, in the same order

font=/usr/share/fonts/TTF/CaskaydiaMonoNerdFont-Bold.ttf
font2=/usr/share/fonts/TTF/CaskaydiaMonoNerdFont-Regular.ttf
bg=0x07090f; fg=0xc0caf5; accent=0x7aa2f7; dim=0x565f89
# 2.39:1 in a 1080 frame: 804 lines of picture, 138 of black above and below.
H=804
# How long each dissolve lasts. Every shot is cut this much longer than it
# reads, since the dissolve eats into both sides.
X=0.7
common=(-c:v libx264 -preset medium -crf 19 -pix_fmt yuv420p -r 30
        -c:a aac -b:a 192k -ar 48000 -ac 2)
# Text for a drawtext filter: colons, percent signs and backslashes are
# escaped, and an apostrophe must close and reopen the quoted argument or
# everything after it is read as filter syntax.
esc() {
  python3 - "$1" <<'PY'
import sys
t = sys.argv[1]
t = t.replace(chr(92), chr(92)*2).replace(':', chr(92)+':').replace('%', chr(92)+'%')
t = t.replace(chr(39), chr(39)+chr(92)+chr(39)+chr(39))
sys.stdout.write(t)
PY
}

# A caption low in the frame, easing in and out well inside the shot so it
# never appears or vanishes on a dissolve.
caption() {  # text, seconds
  local t; t="$(esc "$1")"; local d="$2"
  [ -z "$1" ] && { printf '%s' "null"; return; }
  printf "drawtext=fontfile=%s:text='%s':fontcolor=%s:fontsize=28:x=90:y=h-96:alpha='if(lt(t,0.9),max(0,(t-0.3)/0.6),if(lt(t,%s),1,max(0,(%s-t)/0.6)))'" \
    "$font2" "$t" "$fg" "$(awk "BEGIN{print $d-0.9}")" "$d"
}

pad() { printf "pad=1920:1080:(ow-iw)/2:(oh-ih)/2:color=%s" "$bg"; }

add() { clips+=("$1"); lens+=("$2"); }

# The room: the whole phone frame cropped to the wide ratio. The camera never
# moves, and neither does the shot.
wide() {  # start, seconds, caption
  n=$((n+1)); local f="$work/$(printf '%02d' $n)-wide.mp4"
  local d="$2" cap; cap="$(caption "${3:-}" "$2")"
  local vf="crop=1920:${H}:0:138,setsar=1"
  [ "$cap" = "null" ] || vf="$vf,$cap"
  ffmpeg -hide_banner -loglevel error -y -ss "$1" -t "$d" -i "$P" \
    -ss "$1" -t "$d" -i "$T" -map 0:v:0 -map 1:a:0 \
    -vf "$vf,$(pad)" "${common[@]}" "$f"
  add "$f" "$d"
}

# The screen alone, as the camera saw it. Kept for the record; this cut does
# not use it, because the subject is the set and not the pixels.
screen() {  # start, seconds, caption
  n=$((n+1)); local f="$work/$(printf '%02d' $n)-screen.mp4"
  local d="$2" cap; cap="$(caption "${3:-}" "$2")"
  local vf="crop=730:548:585:125,scale=-2:${H},setsar=1,pad=1920:${H}:(ow-iw)/2:0:color=$bg"
  [ "$cap" = "null" ] || vf="$vf,$cap"
  ffmpeg -hide_banner -loglevel error -y -ss "$1" -t "$d" -i "$P" \
    -ss "$1" -t "$d" -i "$T" -map 0:v:0 -map 1:a:0 \
    -vf "$vf,$(pad)" "${common[@]}" "$f"
  add "$f" "$d"
}

card() {  # title, subtitle, seconds
  n=$((n+1)); local f="$work/$(printf '%02d' $n)-card.mp4"
  local t; t="$(esc "$1")"; local s; s="$(esc "$2")"; local d="$3"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i "color=c=$bg:s=1920x1080:r=30:d=$d" \
    -f lavfi -i "anullsrc=r=48000:cl=stereo" -t "$d" \
    -vf "drawtext=fontfile=$font:text='$t':fontcolor=$fg:fontsize=78:x=(w-text_w)/2:y=(h/2)-80,drawtext=fontfile=$font2:text='$s':fontcolor=$accent:fontsize=34:x=(w-text_w)/2:y=(h/2)+30" \
    "${common[@]}" -shortest "$f"
  add "$f" "$d"
}

# ---------------------------------------------------------------- the film
# Timestamps read off the phone take frame by frame. The boot begins 27
# seconds into the phone's own file, which is 7.4 here because the head was
# trimmed to meet the capture.
wide  20.2 4.2 "A Bang & Olufsen television from 1998, driven by an Omarchy PC"
card  "OMARCHY CRT" "an Omarchy version for retro gaming on CRT" 3.4
wide   7.2 11.5 "the launcher boots on the tube: BIOS, wordmark, Mode 7 floor"
wide  27.6 4.5 "the collection, with box art matched by title"
wide  33.6 4.2 "a game left in the middle asks before it starts"
wide  45.6 4.0 ""
wide  99.5 6.5 "Sega Rally Championship · Saturn"
wide 107.5 4.2 "save, load, rewind, slow motion: hotkeys the compositor presses"
wide 144.5 4.0 ""
wide 175.5 5.5 "Marvel vs. Capcom 2 · Naomi"
wide 189.5 5.0 "Super Mario 64 · Nintendo 64"
wide 220.5 5.0 "Super Metroid · 224 lines, one for each line of the tube"
wide 280.5 4.0 ""
wide 327.5 4.2 "any disk, any folder layout, every system with its core"
# --- the music half, given room
wide 351.5 5.0 "music runs on cliamp: its radio directory, Spotify, every provider it knows"
wide 369.5 6.0 "radio by country and genre, on a dial that tunes through static"
wide 393.5 6.0 "Spotify on a turntable, with the album art Spotify itself provides"
wide 400.8 6.0 "the visualizers run on cliamp's own spectrum, with a kick detector"
wide 421.5 5.0 "cliamp's ten band equaliser, moved from the sofa"
# --- and the films
wide 439.5 5.5 "video on the same screen: local files and YouTube, played by mpv"
wide 445.5 6.0 "yt-dlp fetches the stream, the fit pipeline squeezes it into 240 lines"
wide 504.5 4.5 ""
card  "OMARCHY CRT" "#omarchyCRT" 3.6

# ------------------------------------------------------------- the assembly
# One pass, every clip dissolving into the next, picture and sound together.
inputs=(); for f in "${clips[@]}"; do inputs+=(-i "$f"); done
filter=""; prev="0:v"; preva="0:a"; acc="${lens[0]}"
for i in $(seq 1 $(( ${#clips[@]} - 1 ))); do
  off="$(awk "BEGIN{printf \"%.3f\", $acc-$X}")"
  filter+="[$prev][$i:v]xfade=transition=fade:duration=$X:offset=$off[v$i];"
  filter+="[$preva][$i:a]acrossfade=d=$X:c1=tri:c2=tri[a$i];"
  prev="v$i"; preva="a$i"
  acc="$(awk "BEGIN{printf \"%.3f\", $acc+${lens[$i]}-$X}")"
done
ffmpeg -hide_banner -loglevel error -y "${inputs[@]}" \
  -filter_complex "${filter%;}" -map "[$prev]" -map "[$preva]" \
  -c:v libx264 -preset medium -crf 19 -pix_fmt yuv420p -r 30 \
  -c:a aac -b:a 192k -ar 48000 -ac 2 "$out"
echo "$out"
ffprobe -v error -show_entries format=duration -of csv=p=0 "$out"
