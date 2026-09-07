#!/usr/bin/env bash
# Cut the follow-up video: title cards between the takes, everything to
# 1920x1080 30 fps with a stereo AAC track, concatenated losslessly.
#
#   scripts/montage.sh TAKES_DIR OUT.mp4
#
# TAKES_DIR holds tube-tour.mp4 (the launcher and games, captured by the
# display process with the tube's audio), tube-videos.mp4 (the video player)
# and desktop-plugin.mp4 (the bar plugin, a desktop screen recording).
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
    -vf "drawtext=fontfile=$font:text='$t':fontcolor=$fg:fontsize=72:x=(w-text_w)/2:y=(h/2)-90, drawtext=fontfile=$font2:text='$s':fontcolor=$accent:fontsize=36:x=(w-text_w)/2:y=(h/2)+20, drawtext=fontfile=$font2:text='omarchy-crt':fontcolor=$dim:fontsize=26:x=(w-text_w)/2:y=h-90, fade=t=in:st=0:d=0.4,fade=t=out:st=$(awk "BEGIN{print $3-0.4}"):d=0.4" \
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
desk() {  # source, start, duration (desktop recording, may have no audio)
  n=$((n+1)); f="$work/$(printf '%02d' $n)-desk.mp4"
  ffmpeg -hide_banner -loglevel error -y -ss "$2" -t "$3" -i "$1" -f lavfi -i "anullsrc=r=48000:cl=stereo" \
    -vf "crop=880:1000:2560:20,scale=-2:1080,pad=1920:1080:(ow-iw)/2:(oh-ih)/2:color=$bg,fade=t=in:st=0:d=0.3,fade=t=out:st=$(awk "BEGIN{print $3-0.3}"):d=0.3" \
    -map 0:v -map 1:a "${common[@]}" -shortest "$f"
  echo "file '$f'" >> "$list"
}

T="$dir/tube-tour.mp4"; VV="$dir/tube-videos.mp4"; D="$dir/desktop-plugin.mp4"
card "Omarchy CRT" "an Omarchy version for retro gaming on CRT · update, September 2026" 4.5
card "The bar plugin" "power, standard, sync, audio and library from the Omarchy bar" 3.5
desk "$D" 3 27
card "The tube is ours" "the DAC output is leased from the desktop and driven by our own compositor · 15 kHz timing set directly" 4.5
card "Boot" "the launcher draws at 320x240 on a 3520x240 super resolution" 3
tube "$T" 0 14
card "Systems and console art" "28,847 games indexed from an unsorted disk, any folder layout" 3.5
tube "$T" 14 10
card "Collections and box art" "curated lists, covers from the libretro thumbnails" 3
tube "$T" 24 16
card "Yie Ar Kung-Fu" "Konami, 1985 · MAME" 3
tube "$T" 42 26
card "Pause menu" "save state, load, reset, back to the launcher · hotkeys pressed by the compositor" 3.5
tube "$T" 68 22
card "Chrono Trigger" "Super Nintendo · the tube switches to 224 lines for this system" 3.5
tube "$T" 94 26
card "Sonic The Hedgehog 2" "Mega Drive" 3
tube "$T" 143 20
card "Metal Slug" "Neo Geo · FinalBurn Neo" 3
tube "$T" 178 22
card "Videos" "mpv on the tube, fitted to 15 kHz" 3
tube "$VV" 0 30
card "github.com/stefanomainardi/omarchy-crt" "#omarchyCRT · next: media player on cliamp, pad wizard, library GUI" 5
ffmpeg -hide_banner -loglevel error -y -f concat -safe 0 -i "$list" -c copy "$out"
echo "$out"; ffprobe -v error -show_entries format=duration -of csv=p=0 "$out"
