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
# The film is 2.39:1 inside a 1920x1080 frame. It is the television that is
# being filmed, so nearly every shot is the whole set, held still; the tighter
# framing on the screen alone is kept for a few moments of play. The picture
# always comes from the phone, the sound always from the capture, at the same
# instant: the two takes were aligned to within four hundredths of a second.
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

# The room: the whole phone frame cropped to the wide ratio. The camera
# never moves, and neither does the shot: a slow push looked like a slideshow
# effect on a picture that is already still.
wide() {  # start, seconds, caption
  n=$((n+1)); local f="$work/$(printf '%02d' $n)-wide.mp4"
  local d="$2" cap; cap="$(caption "${3:-}" "$2")"
  local vf="crop=1920:${H}:0:138,setsar=1"
  [ "$cap" = "null" ] || vf="$vf,$cap"
  ffmpeg -hide_banner -loglevel error -y -ss "$1" -t "$d" -i "$P" \
    -ss "$1" -t "$d" -i "$T" -map 0:v:0 -map 1:a:0 \
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
    -ss "$1" -t "$d" -i "$T" -map 0:v:0 -map 1:a:0 \
    -vf "$vf,$(pad),$(fades "$d")" -af "$(afades "$d")" "${common[@]}" "$f"
  echo "file '$f'" >> "$list"
}

# The framebuffer itself. Kept for the record: this cut does not use it,
# because the point of the film is the television, not the pixels.
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
# Every shot is the television in the room, held still: the subject is the set,
# not the pixels. The boot starts 27 seconds into the phone's own file, which
# is 7.4 here because the head was trimmed to match the capture.
wide  20.5 3.2 "A Bang & Olufsen television from 1998, driven by an Omarchy PC"
card  "OMARCHY CRT" "an Omarchy version for retro gaming on CRT" 2.4
wide   7.4 10.5 "the launcher boots on the tube: BIOS, wordmark, Mode 7 floor"
wide    28 3.5 "the collection, with box art matched by title"
wide    34 3.0 "a game left in the middle asks before it starts"
wide    46 3.0 ""
wide   100 5.0 "Sega Rally Championship · Saturn"
wide   108 3.0 "save, load, rewind, slow motion: hotkeys the compositor presses"
wide   145 3.0 ""
wide   176 4.0 "Marvel vs. Capcom 2 · Naomi"
wide   190 3.5 "Super Mario 64 · Nintendo 64"
wide   221 3.5 "Super Metroid · 224 lines, one for each line of the tube"
wide   328 3.0 "any disk, any folder layout, every system with its core"
wide   370 3.5 "radio and Spotify through cliamp, on a hi-fi deck"
wide   401 4.0 ""
wide   415 3.0 "the album art comes from Spotify itself"
wide   505 3.5 ""
card  "github.com/stefanomainardi/omarchy-crt" "#omarchyCRT" 3.0

ffmpeg -hide_banner -loglevel error -y -f concat -safe 0 -i "$list" -c copy "$out"
echo "$out"
ffprobe -v error -show_entries format=duration -of csv=p=0 "$out"
