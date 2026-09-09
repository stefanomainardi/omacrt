#!/usr/bin/env bash
# Cut the follow-up video: title cards between the takes, everything to
# 1920x1080 30 fps with a stereo AAC track, concatenated losslessly.
#
#   scripts/montage.sh TAKES_DIR OUT.mp4
#
# TAKES_DIR holds tube-boot.mp4 (the launcher booting), tube-tour.mp4 (the
# launcher and games), tube-videos.mp4 (the video player), all captured by the
# display process with the tube's audio, and desktop-plugin.mp4 (the bar
# plugin, a 3440x1440 desktop screen recording, see scripts/shoot.sh).
set -eu
dir="${1:?takes dir}"; out="${2:?output}"
work="$dir/montage"; mkdir -p "$work"
font=/usr/share/fonts/TTF/CaskaydiaMonoNerdFont-Bold.ttf
font2=/usr/share/fonts/TTF/CaskaydiaMonoNerdFont-Regular.ttf
bg=0x0b0d14; fg=0xc0caf5; accent=0x7aa2f7; dim=0x565f89
n=0
list="$work/list.txt"; : > "$list"
common=(-c:v libx264 -preset medium -crf 18 -pix_fmt yuv420p -r 30 -c:a aac -b:a 192k -ar 48000 -ac 2)
esc() { printf '%s' "$1" | sed "s/'/\\\\\\\\\\\\'/g; s/:/\\\\:/g; s/%/\\\\%/g"; }
card() {  # title, subtitle, seconds
  n=$((n+1)); f="$work/$(printf '%02d' $n)-card.mp4"
  local t; t="$(esc "$1")"; local s; s="$(esc "$2")"
  ffmpeg -hide_banner -loglevel error -y -f lavfi -i "color=c=$bg:s=1920x1080:r=30:d=$3" -f lavfi -i "anullsrc=r=48000:cl=stereo" -t "$3" \
    -vf "drawtext=fontfile=$font:text='$t':fontcolor=$fg:fontsize=72:x=(w-text_w)/2:y=(h/2)-90, drawtext=fontfile=$font2:text='$s':fontcolor=$accent:fontsize=36:x=(w-text_w)/2:y=(h/2)+20, drawtext=fontfile=$font2:text='omacrt':fontcolor=$dim:fontsize=26:x=(w-text_w)/2:y=h-90, fade=t=in:st=0:d=0.4,fade=t=out:st=$(awk "BEGIN{print $3-0.4}"):d=0.4" \
    "${common[@]}" -shortest "$f"
  echo "file '$f'" >> "$list"
}
tube() {  # source, start, duration
  n=$((n+1)); f="$work/$(printf '%02d' $n)-tube.mp4"
  ffmpeg -hide_banner -loglevel error -y -ss "$2" -t "$3" -i "$1" \
    -vf "scale=1440:1080:flags=neighbor,pad=1920:1080:(ow-iw)/2:(oh-ih)/2:color=$bg,fade=t=in:st=0:d=0.3,fade=t=out:st=$(awk "BEGIN{print $3-0.3}"):d=0.3" \
    -af "afade=t=in:st=0:d=0.3,afade=t=out:st=$(awk "BEGIN{print $3-0.3}"):d=0.3" "${common[@]}" "$f"
  echo "file '$f'" >> "$list"
}
desk() {  # source, start, duration: the desktop take, 3440x1440, no audio.
  # The whole desktop first, then a push toward the bar's right end where the
  # widget and its panel live: from the full frame (letterboxed) to a 1678x944
  # window right aligned on the bar, between ZOOM_FROM and ZOOM_TO seconds of
  # the clip. The perspective filter does the moving crop since crop and
  # zoompan cannot resize per frame without distorting the picture.
  n=$((n+1)); f="$work/$(printf '%02d' $n)-desk.mp4"
  local zf="${ZOOM_FROM:-3}" zt="${ZOOM_TO:-6}" Z=2.05
  local e="clip((in/60-$zf)/($zt-$zf),0,1)"; e="($e*$e*(3-2*$e))"
  local sc="(1/(1+($Z-1)*$e))" x0 y0 x1 y2
  x0="(W-W*$sc-40*$e)"; y0="(247*$e)"; x1="(W-40*$e)"; y2="(247*$e+H*$sc)"
  ffmpeg -hide_banner -loglevel error -y -ss "$2" -t "$3" -i "$1" -f lavfi -i "anullsrc=r=48000:cl=stereo" \
    -vf "fps=60,setsar=1,pad=3440:1935:0:(oh-ih)/2:color=$bg,perspective=x0='$x0':y0='$y0':x1='$x1':y1='$y0':x2='$x0':y2='$y2':x3='$x1':y3='$y2':sense=source:eval=frame:interpolation=linear,scale=1920:1080:flags=bicubic,setsar=1,fade=t=in:st=0:d=0.3,fade=t=out:st=$(awk "BEGIN{print $3-0.3}"):d=0.3" \
    -map 0:v -map 1:a "${common[@]}" -shortest "$f"
  echo "file '$f'" >> "$list"
}

T="$dir/tube-tour.mp4"; VV="$dir/tube-videos.mp4"; D="$dir/desktop-plugin.mp4"; B="$dir/tube-boot.mp4"
card "OmaCRT" "an Omarchy version for retro gaming on CRT · update, September 2026" 4.5
card "The bar plugin" "power, standard, sync, audio and library, from the bar" 3.5
desk "$D" 1 48
card "The tube is ours" "leased from the desktop, driven by our own compositor at 15 kHz" 4.5
card "Boot" "drawn at 320x240 on a 3520x240 super resolution" 3
tube "$B" 0.5 18.5
card "Systems and console art" "indexed from an unsorted disk, any folder layout" 3.5
tube "$T" 14 10
card "Collections and box art" "curated lists, covers matched by title" 3
tube "$T" 24 16
card "Yie Ar Kung-Fu" "Konami, 1985 · MAME" 3
tube "$T" 42 26
card "Pause menu" "save, load, reset, back · hotkeys pressed by the compositor" 3.5
tube "$T" 68 22
card "Chrono Trigger" "Super Nintendo · the tube switches to 224 lines" 3.5
tube "$T" 94 26
card "Sonic The Hedgehog 2" "Mega Drive" 3
tube "$T" 143 20
card "Metal Slug" "Neo Geo · FinalBurn Neo" 3
tube "$T" 178 22
card "Videos" "mpv on the tube, fitted to 15 kHz" 3
tube "$VV" 0 30
card "github.com/stefanomainardi/omacrt" "#omarchyCRT · music, video and the library, from the bar" 5
ffmpeg -hide_banner -loglevel error -y -f concat -safe 0 -i "$list" -c copy "$out"
echo "$out"; ffprobe -v error -show_entries format=duration -of csv=p=0 "$out"
