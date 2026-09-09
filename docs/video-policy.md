# Video policy: how a game gets its display mode

Every system in `systems.toml` carries a `video` value that decides how the
display mode follows the game. The launcher turns it into RetroArch settings
written to a per launch file and passed with `--appendconfig`, so the base
`retroarch.cfg` stays untouched and each system can behave differently.

## The three policies

- **`super` (default).** A fixed wide frame, 2560 pixels by default, whose
  height and refresh rate follow the core: 224, 240, 256 or 288 lines at the
  game's own frequency. Horizontal scaling into a wide frame is invisible on a
  CRT, and it avoids a mode switch when a game changes only its width. Write
  `super:1920` or `super:3840` for other widths.
- **`native`.** Width, height and refresh all follow the core. The purest
  output, one mode switch per resolution change, integer scaling on.
- **`WxH` (pinned frame).** One fixed mode for the whole session, the core is
  scaled into it. Use it where a console mixes widths inside one game without
  a real mode change, such as `512x224` for the Super Nintendo, or where a
  vertical arcade game needs a taller frame than 240 lines.

## RetroArch keys per policy

| Policy   | Keys written to `launch.cfg`                                                                                                            |
| -------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `super`  | `crt_switch_resolution`, `crt_switch_resolution_super = 2560`, `aspect_ratio_index = 22` (core provided), `video_scale_integer = false` |
| `native` | `crt_switch_resolution`, `crt_switch_resolution_super = 0`, `aspect_ratio_index = 22`, `video_scale_integer = true`                     |
| `WxH`    | `crt_switch_resolution = 0`, `video_fullscreen_x/y`, `aspect_ratio_index = 23` (custom), `custom_viewport_*`                            |

`crt_switch_resolution` is `1` only when `switching = true` is set at the top
of `systems.toml`. Mode switching needs RetroArch on the KMS or X11 video
driver, and a kernel carrying the 15 kHz patches for the modes it picks. This
project does not use it: `omacrt` sets the mode itself on the leased connector,
on a stock kernel. On a Wayland desktop the setting does nothing, so it stays
off while testing in a window. Pinned frames work everywhere.

## Lines, and the tube following the game

The mode the television is in is decided by a line count, not by a mode name.
Every system carries one (`lines` in `systems.toml`, with a built-in default
per console: 240 for a NES, 224 for a Super Nintendo, 480 for a Dreamcast),
and the launcher switches the tube to it before the game starts.

A line count above 288 does not fit a progressive 15 kHz frame, so it selects
the interlaced mode of the same standard by itself: `--lines 480` on an NTSC
tube is 480i at 59.94 Hz, `--lines 576` on a PAL one is 576i at 50 Hz. Nobody
has to name a mode.

The table is only a starting point. While a game runs, the launcher reads the
emulator's own log, which announces the geometry the core is drawing and every
change to it:

```text
[INFO] [Core] Geometry: 640x480, Aspect: 1.333, FPS: 59.95, ...
[INFO] [Environ] SET_GEOMETRY: 320x240, Aspect: 1.333.
```

The tube follows those within a second. A PlayStation game whose menus go to
480 lines gets an interlaced menu and a progressive game; a Saturn game moving
between 224 and 240 lines gets both. What the console drew is what the
television draws, which is the whole point of the exercise: 640x480 squeezed
into 240 lines is how text becomes unreadable, and it is exactly what happens
when the mode is left alone.

## Interlace

Systems that draw 480 or 576 lines (Dreamcast, Naomi, PlayStation 2, some
PlayStation menus) would get a real interlaced mode from the same mechanism:
the core reports 480 lines and the timing calculator picks 480i at the game's
refresh. A stock `amdgpu` cannot scan that out, so `output.interlace` is off
and those games are shown at 240p. See
[`docs/15khz.md`](15khz.md) for what stands in the way and what it costs.

## Example

```toml
switching = false

[[system]]
name = "snes"
dir = "~/Games/roms/snes"
core = "snes9x"
extensions = ["sfc", "smc", "zip"]
video = "512x224"

[[system]]
name = "megadrive"
dir = "~/Games/roms/megadrive"
core = "genesis_plus_gx"
extensions = ["md", "bin", "zip"]
video = "super"

[[system]]
name = "dreamcast"
dir = "~/Games/roms/dreamcast"
core = "flycast"
extensions = ["gdi", "chd", "cdi"]
video = "native"
lines = 480     # 480i: what a Dreamcast drew on a television
```
