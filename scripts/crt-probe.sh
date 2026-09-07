#!/usr/bin/env bash
# Probe an HDMI/DP connector for a CRT DAC: connection status, EDID, kernel modes,
# Hyprland view, HDMI audio path (ELD, PipeWire profile and sink).
# Read-only unless --tone is given, which plays a 2 s test tone on the matching sink.
# Usage: crt-probe.sh [--tone] [CONNECTOR]   (default: first connected HDMI)
set -u

tone=0
conn=""
for arg in "$@"; do
  case "$arg" in
    --tone) tone=1 ;;
    -h|--help) sed -n '2,5p' "$0"; exit 0 ;;
    *) conn="$arg" ;;
  esac
done

pick_connector() {
  for c in /sys/class/drm/card*-HDMI-A-*; do
    [ "$(cat "$c/status")" = connected ] && { basename "$c"; return; }
  done
}

[ -n "$conn" ] || conn="$(pick_connector)"
if [ -z "$conn" ]; then
  echo "no connected HDMI connector found; connectors:"
  for c in /sys/class/drm/card*-*; do echo "  $(basename "$c") $(cat "$c/status")"; done
  exit 1
fi

sys="/sys/class/drm/$conn"
[ -d "$sys" ] || { echo "no such connector: $conn"; exit 1; }
echo "== $conn: status=$(cat "$sys/status") enabled=$(cat "$sys/enabled") dpms=$(cat "$sys/dpms" 2>/dev/null)"
echo "== driver: $(basename "$(readlink "$sys/device/driver" 2>/dev/null || readlink "$sys/../device/driver")")"

echo "== kernel mode list ($sys/modes):"
sed 's/^/  /' "$sys/modes"

edid_bytes=$(wc -c < "$sys/edid")
echo "== EDID: $edid_bytes bytes"
edid_name=""
edid_audio="no"
if [ "$edid_bytes" -gt 0 ]; then
  out="${CRT_PROBE_OUT:-/tmp}/edid-${conn}.bin"
  cp "$sys/edid" "$out" && echo "  saved to $out"
  if command -v edid-decode >/dev/null; then
    decoded=$(edid-decode "$sys/edid")
    printf '%s\n' "$decoded" | sed 's/^/  /'
    edid_name=$(printf '%s\n' "$decoded" | sed -n "s/.*Display Product Name: '\(.*\)'.*/\1/p" | head -n 1)
    if printf '%s\n' "$decoded" | grep -qE "Basic audio support|Audio Data Block"; then
      edid_audio="yes"
    fi
  fi
  echo "  EDID advertises audio: $edid_audio"
  [ "$edid_audio" = yes ] || echo "  (without an audio descriptor the kernel exposes no HDMI audio pin for this sink)"
else
  echo "  no EDID: the kernel falls back to its default mode list and offers no HDMI audio"
fi

# HDMI audio lives on the GPU's PCI audio function (.1). Each connector with a
# sink is an ELD pin; PipeWire exposes pin 0 as output:hdmi-stereo and pin N as
# output:hdmi-stereo-extraN, one active profile per card at a time.
echo "== HDMI audio:"
gpu_pci=$(basename "$(readlink -f "$sys/device/device")")
audio_pci="${gpu_pci%.*}.1"
alsa_dir=$(ls -d "/sys/bus/pci/devices/$audio_pci/sound/card"* 2>/dev/null | head -n 1)
if [ -z "$alsa_dir" ]; then
  echo "  no ALSA card on $audio_pci (GPU $gpu_pci)"
else
  alsa_card=$(basename "$alsa_dir")
  pw_card="alsa_card.pci-${audio_pci//:/_}"
  echo "  GPU $gpu_pci, audio $audio_pci, ALSA $alsa_card, PipeWire card $pw_card"
  match_pin=""
  for eld in "/proc/asound/$alsa_card"/eld#*; do
    [ -e "$eld" ] || continue
    present=$(sed -n 's/^monitor_present[[:space:]]*//p' "$eld")
    pin="${eld##*#0.}"
    if [ "$present" = 1 ]; then
      name=$(sed -n 's/^monitor_name[[:space:]]*//p' "$eld")
      ctype=$(sed -n 's/^connection_type[[:space:]]*//p' "$eld")
      rates=$(sed -n 's/^sad0_rates[[:space:]]*//p' "$eld")
      bits=$(sed -n 's/^sad0_bits[[:space:]]*//p' "$eld")
      echo "  pin $pin: present, name='$name' via $ctype, LPCM rates $rates bits $bits"
      if [ -n "$edid_name" ] && [ "$name" = "$edid_name" ]; then match_pin="$pin"; fi
    else
      echo "  pin $pin: no sink"
    fi
  done
  if [ -n "$match_pin" ]; then
    if [ "$match_pin" = 0 ]; then profile="output:hdmi-stereo"; else profile="output:hdmi-stereo-extra$match_pin"; fi
    sink="alsa_output.pci-${audio_pci//:/_}.${profile#output:}"
    echo "  $conn is pin $match_pin: profile $profile, sink $sink"
    if command -v pactl >/dev/null; then
      active=$(pactl list cards 2>/dev/null | awk -v c="$pw_card" '$1=="Name:"{cur=$2} cur==c && /Active Profile:/{print $3}')
      echo "  active profile on this card: ${active:-unknown}"
      if [ "$active" != "$profile" ]; then
        echo "  to route audio here: pactl set-card-profile $pw_card $profile"
      fi
      echo "  to test: paplay -d $sink <file.wav>   (or crt-probe.sh --tone $conn)"
      if [ "$tone" = 1 ]; then
        wav="${CRT_PROBE_OUT:-/tmp}/crt-probe-tone.wav"
        python3 - "$wav" <<'PY'
import math, struct, sys, wave
rate, secs, freq = 48000, 2.0, 440.0
with wave.open(sys.argv[1], "wb") as w:
    w.setnchannels(2); w.setsampwidth(2); w.setframerate(rate)
    frames = bytearray()
    for i in range(int(rate * secs)):
        t = i / rate
        env = min(1.0, t * 20, (secs - t) * 20)
        s = int(12000 * env * math.sin(2 * math.pi * freq * t))
        frames += struct.pack("<hh", s, s)
    w.writeframes(bytes(frames))
PY
        if [ "$active" != "$profile" ]; then
          echo "  switching profile for the test (restore with: pactl set-card-profile $pw_card $active)"
          pactl set-card-profile "$pw_card" "$profile"
          sleep 1
        fi
        echo "  playing 440 Hz for 2 s on $sink"
        paplay -d "$sink" "$wav" && echo "  tone done" || echo "  paplay failed"
      fi
    fi
  else
    echo "  no ELD pin matches the EDID product name '${edid_name:-?}'"
    echo "  (a DAC with a plain video EDID has no audio pin; use the minijack of another card or a USB DAC)"
  fi
fi

if command -v hyprctl >/dev/null && hyprctl monitors all -j >/dev/null 2>&1; then
  echo "== Hyprland view:"
  hyprctl monitors all -j | python3 -c '
import json,sys
want=sys.argv[1]
for m in json.load(sys.stdin):
    if m["name"]!=want: continue
    print("  name:",m["name"],"| desc:",m["description"])
    print("  current:",m["width"],"x",m["height"],"@",m["refreshRate"],"| disabled:",m["disabled"])
    print("  availableModes:")
    for am in m.get("availableModes",[]): print("    ",am)
' "${conn#card*-}"
fi

echo "== recent amdgpu/drm kernel log lines:"
journalctl -k -b --no-pager 2>/dev/null | grep -iE "drm|amdgpu|hdmi|edid" | tail -n 15 | sed 's/^/  /'
