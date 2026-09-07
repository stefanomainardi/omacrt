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
# The film is 2.39:1 inside a 1920x1080 frame: wide shots of the set fill it,
# pictures of the screen stand in the middle with the frame dark either side.
# Every clip carries its own sound. Cuts are hard, the two dissolves are where
# the subject changes.
set -eu
dir="${1:?takes dir}"; out="${2:?output}"
P="$dir/phone-proxy.mp4"; T="$dir/tube-live-1.mp4"
for f in "$P" "$T"; do [ -f "$f" ] || { echo "missing $f" >&2; exit 1; }; done
work="$dir/montage-live"; mkdir -p "$work"
list="$work/list.txt"; : > "$list"
n=0

font=/usr/share/fonts/TTF/CaskaydiaMonoNerdFont-Bold.ttf
font2=/usr/share/fonts/TTF/CaskaydiaMonoNerdFont-Regular.ttf
bg=0x07090f; fg=0xc0caf5; accent=0x7aa2f7; dim=0x565f89
# 2.39:1 in a 1080 frame: 804 lines of picture, 138 of black above and below.
H=804
common=(-c:v libx264 -preset medium -crf 19 -pix_fmt yuv420p -r 30
        -c:a aac -b:a 192k -ar 48000 -ac 2)
esc() { printf '%s' "$1" | sed "s/'/\\\\\\\\\\\\'/g; s/:/\\\\:/g; s/%/\\\\%/g"; }

# A caption low in the frame, fading in and out with the shot.
caption() {  # text, seconds
  local t; t="$(esc "$1")"; local d="$2"
  [ -z "$1" ] && { printf '%s' "null"; return; }
  printf "drawtext=fontfile=%s:text='%s':fontcolor=%s:fontsize=28:x=90:y=h-96:alpha='if(lt(t,0.4),t/0.4,if(lt(t,%s),1,max(0,(%s-t)/0.5)))'" \
    "$font2" "$t" "$fg" "$(awk "BEGIN{print $d-0.5}")" "$d"
}

pad() { printf "pad=1920:1080:(ow-iw)/2:(oh-ih)/2:color=%s" "$bg"; }
fades() { printf "fade=t=in:st=0:d=0.12,fade=t=out:st=%s:d=0.16" "$(awk "BEGIN{print $1-0.16}")"; }
afades() { printf "afade=t=in:st=0:d=0.12,afade=t=out:st=%s:d=0.2" "$(awk "BEGIN{print $1-0.2}")"; }

# The room: the whole phone frame, cropped to the wide ratio, pushing in
# slowly. Photographic material, so a gentle zoom does not hurt it.
wide() {  # start, seconds, caption
  n=$((n+1)); local f="$work/$(printf '%02d' $n)-wide.mp4"
  local d="$2" cap; cap="$(caption "${3:-}" "$2")"
  local vf="crop=1920:${H}:0:138,scale=2400:-2,zoompan=z='min(1+0.00055*on,1.10)':x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)':d=1:s=1920x${H}:fps=30"
  [ "$cap" = "null" ] || vf="$vf,$cap"
  ffmpeg -hide_banner -loglevel error -y -ss "$1" -t "$d" -i "$P" \
    -vf "$vf,$(pad),$(fades "$d")" -af "$(afades "$d")" "${common[@]}" "$f"
  echo "file '$f'" >> "$list"
}

# The screen alone, as the camera saw it: standing in the middle of the wide
# frame, its own light the only thing in the picture.
screen() {  # start, seconds, caption
  n=$((n+1)); local f="$work/$(printf '%02d' $n)-screen.mp4"
  local d="$2" cap; cap="$(caption "${3:-}" "$2")"
  local vf="crop=730:548:585:125,scale=-2:${H},setsar=1"
  vf="$vf,pad=1920:${H}:(ow-iw)/2:0:color=$bg"
  [ "$cap" = "null" ] || vf="$vf,$cap"
  ffmpeg -hide_banner -loglevel error -y -ss "$1" -t "$d" -i "$P" \
    -vf "$vf,$(pad),$(fades "$d")" -af "$(afades "$d")" "${common[@]}" "$f"
  echo "file '$f'" >> "$list"
}

# The framebuffer itself, for everything made of text: menus, lists, the deck.
clean() {  # start, seconds, caption
  n=$((n+1)); local f="$work/$(printf '%02d' $n)-clean.mp4"
  local d="$2" cap; cap="$(caption "${3:-}" "$2")"
  local vf="scale=-2:${H}:flags=neighbor,setsar=1,pad=1920:${H}:(ow-iw)/2:0:color=$bg"
  [ "$cap" = "null" ] || vf="$vf,$cap"
  ffmpeg -hide_banner -loglevel error -y -ss "$1" -t "$d" -i "$T" \
    -vf "$vf,$(pad),$(fades "$d")" -af "$(afades "$d")" "${common[@]}" "$f"
  echo "file '$f'" >> "$list"
}

card() {  # title, subtitle, seconds
  n=$((n+1)); local f="$work/$(printf '%02d' $n)-card.mp4"
  local t; t="$(esc "$1")"; local s; s="$(esc "$2")"; local d="$3"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i "color=c=$bg:s=1920x1080:r=30:d=$d" \
    -f lavfi -i "anullsrc=r=48000:cl=stereo" -t "$d" \
    -vf "drawtext=fontfile=$font:text='$t':fontcolor=$fg:fontsize=78:x=(w-text_w)/2:y=(h/2)-80,drawtext=fontfile=$font2:text='$s':fontcolor=$accent:fontsize=34:x=(w-text_w)/2:y=(h/2)+30,drawtext=fontfile=$font2:text='omarchy-crt':fontcolor=$dim:fontsize=24:x=(w-text_w)/2:y=h-110,$(fades "$d")" \
    "${common[@]}" -shortest "$f"
  echo "file '$f'" >> "$list"
}

# ---------------------------------------------------------------- the film
# Timestamps read off both takes frame by frame: the phone for anything with
# a picture in it, the capture for anything made of text.
wide    99  5.0 "A Bang & Olufsen television from 1998, driven by an Omarchy PC"
card    "OMARCHY CRT" "an Omarchy version for retro gaming on CRT" 2.6
clean  10.6 4.6 "the launcher boots on the tube"
clean  18.4 2.6 "320x240, in the Omarchy look"
clean   29  3.4 "the collection: box art matched by title"
clean  33.2 2.8 "a game left in the middle asks before it starts"
screen  55  2.2 ""
screen  70  2.4 ""
screen 100  5.0 "Sega Rally Championship · Saturn"
clean 107.5 3.4 "save, load, rewind, slow motion: hotkeys the compositor presses"
screen 145  2.6 ""
screen 176  4.2 "Marvel vs. Capcom 2 · Naomi"
screen 190  3.8 "Super Mario 64 · Nintendo 64"
screen 221  3.8 "Super Metroid · 224 lines, one for each line of the tube"
screen 281  2.6 ""
clean  318  3.2 "any disk, any folder layout, every system with its core"
clean  364  3.4 "radio and Spotify through cliamp, on a hi-fi deck"
screen 401  3.8 ""
clean 404.5 3.0 "the album art comes from Spotify itself"
clean 416.2 2.4 "ten bands, moved from the sofa"
wide   505  4.0 ""
card   "github.com/stefanomainardi/omarchy-crt" "#omarchyCRT" 3.2

ffmpeg -hide_banner -loglevel error -y -f concat -safe 0 -i "$list" -c copy "$out"
echo "$out"
ffprobe -v error -show_entries format=duration -of csv=p=0 "$out"
