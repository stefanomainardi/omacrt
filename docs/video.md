# Video on a CRT

The `Videos` entry plays films and captures through mpv on the tube. Modern
video is the wrong shape for a 15 kHz television in five ways, and the shell
fixes each of them the way the analog world did, either live in mpv or by
writing a CRT ready file with ffmpeg.

## The five conversions

- **Frame rate to refresh.** The tube runs at 59.94 or 50 Hz. Content at 25
  or 50 fps goes to 576i at 50 Hz, content at 30 or 60 fps to 480i at 59.94.
  Film at 24 fps takes one of the two historical routes: **3:2 pulldown** to
  59.94 (the American DVD) or the **PAL speed-up** of 25/24 with the audio
  time stretched (the European DVD).
- **Resolution to 720x480 or 720x576, interlaced.** Sources at field rate (50
  or 60 fps) are scaled to field height and woven two frames per interlaced
  frame, so every field carries its own instant and motion stays intact.
  Other sources are scaled to the full frame and flagged interlaced.
- **16:9 into 4:3.** `letterbox` pads black bars, `crop` cuts the sides,
  `anamorphic` squeezes for sets with a 16:9 mode.
- **Color.** BT.709 and BT.2020 sources are converted to SD matrices (SMPTE
  170M for NTSC, BT.470BG for PAL); HDR is tone mapped to SDR; mpv targets the
  tube's gamma 2.4.
- **Overscan and sound.** A 5% margin keeps titles and subtitles out of the
  hidden border; subtitles use a large font with margins; audio is downmixed
  to stereo and, in conversions, loudness normalized to -16 LUFS.

A sixth case is the fun one: **retro gameplay captures**. YouTube recordings
of 240p games arrive upscaled to 1080p. With `retro 240p` on, 4:3 sources are
downscaled back to 320x240 progressive with an area filter, which restores the
original pixels on the tube.

## Settings

Settings, Video fit:

| Row         | Values                            | Default     |
| ----------- | --------------------------------- | ----------- |
| standard    | `auto`, `ntsc`, `pal`             | `auto`      |
| film 24 fps | `pulldown`, `speedup`             | `pulldown`  |
| 16:9 to 4:3 | `letterbox`, `crop`, `anamorphic` | `letterbox` |
| overscan 5% | on, off                           | on          |
| retro 240p  | on, off                           | off         |

Saved under `[video]` in `~/.config/omarchy-crt/settings.toml`.

## Live playback

Selecting a video probes it with `ffprobe` (size, frame rate, field order,
transfer function), builds a plan and starts mpv with the matching options:
an `lavfi` scale and pad chain for the aspect and overscan, `--speed=1.04271`
with pitch correction for the PAL speed-up, SD target primaries, gamma 2.4,
BT.2390 tone mapping, stereo downmix, subtitle size and margins. The plan's
label (`480i 3:2`, `576i +4%`, `240p`) flashes in the list when playback
starts.

Real interlaced output needs the CRT stack (a 480i or 576i mode on the tube).
On a progressive desktop the same options still apply and the picture is
correct, just not interlaced.

## Convert for CRT

`X` on a video (the `X` key or the pad's third button) starts an ffmpeg
conversion in the background. The row shows the percentage while it runs and a
`CRT` badge when done; the file lands next to the original as
`<name>.crt.mp4` and is preferred at playback, with no live fitting needed.

The ffmpeg chain follows the plan:

- field rate sources: `fps=60000/1001` (or `50`), scale to `720x240` (or
  `720x288`) with the aspect chain, `tinterlace=merge`, `setfield=tff`;
- 24 fps with pulldown: `fps=24000/1001`, scale, `telecine=pattern=32`;
- 24 fps with speed-up: `setpts=PTS/1.04271,fps=25`, scale, `atempo=1.04271`;
- everything else: `fps` to the standard, scale, `setfield=tff`;
- HDR sources first pass through `zscale` linearization, `tonemap=hable` and
  back to BT.709;
- then `scale=out_color_matrix=<sd matrix>,format=yuv420p`, `libx264` at CRF
  18 with `+ilme+ildct` and `tff=1`, `-aspect 4:3`, AAC stereo with
  `loudnorm`.

Retro conversions skip interlacing and write 320x240 progressive.

## Files

- `~/.config/omarchy-crt/mpv-input.conf`: keys mpv applies while focused.
- `~/.config/omarchy-crt/mpv-osd.lua`: the themed on screen display.
- `~/.config/omarchy-crt/convert.progress`: ffmpeg progress of the running
  conversion.
- `<video>.crt.mp4`: the CRT ready sibling.
