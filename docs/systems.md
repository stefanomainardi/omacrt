# Systems, launch policy and the TV profile

`~/.config/omarchy-crt/systems.toml` describes every system the launcher
shows: where the ROMs are, which libretro core runs them and what RetroArch
should do so the game looks and plays right on first launch. When the file is
missing the shell uses built-in defaults for the cores Arch ships.

## File layout

```toml
# Enable mode switching in RetroArch (CRT SwitchRes). Needs the KMS or X11
# video driver and the 15 kHz kernel. Off by default: pinned frames still work.
switching = false

# Optional: RetroArch binary and core directory.
# retroarch = "retroarch"
# core_dir = "/usr/lib/libretro"

[[system]]
name = "snes"
dir = "~/Games/roms/snes"
core = "snes9x"
extensions = ["sfc", "smc", "zip"]
video = "512x224"
runahead = 1
rewind = true
devices = ["1:1", "2:1"]

[system.options]
snes9x_overclock_cycles = "disabled"
snes9x_superscope_crosshair = "0"
```

## Fields

- **`name`.** Shown in the listing with a trailing slash, `ls` style. Also the
  key used by the recent and favorites lists.
- **`dir`.** ROM directory, `~` expands to the home directory. Files with an
  accepted extension become games; `.m3u` playlists hide the disc images they
  reference so a multi disc game appears once.
- **`core`.** A core name resolved to `<core_dir>/<core>_libretro.so`, or a
  full path to a `.so`.
- **`extensions`.** Lowercase, without the dot. Empty means every file.
- **`video`.** `super`, `super:1920`, `native` or a pinned `WxH`. See
  [`video-policy.md`](video-policy.md).
- **`options`.** libretro core options. The shell writes them to `cores.cfg`
  before each launch and points RetroArch at it with `core_options_path`.
  Anything the core exposes in its options menu can go here; names are the
  ones RetroArch itself writes to its core options file.
- **`devices`.** RetroArch `--device=PORT:TYPE` pairs. Type ids are the
  libretro device constants: `1` joypad, `2` mouse, `3` keyboard, `4` light
  gun, `5` analog, `6` pointer, plus core specific subclasses such as `260`
  (`(1 << 8) | 4`, a light gun variant). Check the core's documentation for
  the exact ids; the defaults leave this empty so RetroArch autodetects.
- **`runahead`.** Frames of run-ahead. `1` removes one frame of input lag on 8
  and 16 bit systems at the cost of running the core twice per frame. Leave
  `0` on 3D systems.
- **`rewind`.** Enables the rewind buffer. Off for 3D systems.
- **`player`.** `retroarch` (default) or `mpv`. An `mpv` system is a video
  folder: files play fullscreen through mpv, the shell keeps the pad and draws
  the overlay. The built-in `videos` system points at `~/Videos`.

## Built-in defaults

Run-ahead and rewind follow the RGB-Pi frontend's whitelists: on for 8 and
16 bit consoles and handhelds, off for arcade, PlayStation, Nintendo 64 and
Dreamcast. Video is `super` everywhere except the Super Nintendo (`512x224`,
absorbs the hi-res modes without switching) and the Dreamcast (`native`,
because 480 line content should be interlaced, not scaled).

| System                  | Core              | Notable options                                                                 |
| ----------------------- | ----------------- | ------------------------------------------------------------------------------- |
| nes                     | mesen             | no stretching, no overclock                                                     |
| snes                    | snes9x            | overclock off, crosshair off, hires blend off                                   |
| megadrive, mastersystem | genesis_plus_gx   | overscan off, NTSC filter off, YM2413 auto                                      |
| pcengine                | mednafen_pce_fast |                                                                                 |
| gb, gba                 | mgba              | model autodetect, DMG palette                                                   |
| neogeo                  | fbneo             | MVS Europe BIOS, 60 Hz forcing off                                              |
| arcade                  | fbneo             | native refresh, CPU at 100 %, patched sets off                                  |
| psx                     | mednafen_psx_hw   | 1x internal resolution, native dithering, analog toggle                         |
| n64                     | mupen64plus_next  | 320x240, native resolution factor 1, hardware dithering on, RDRAM dithering off |
| dreamcast               | flycast           | 640x480 internal, no widescreen hack                                            |

The Nintendo 64 defaults follow the "authentic look" recipe documented by the
RePlayOS project: shader dithering and quantization on, RDRAM image dithering
off, native resolution.

## The TV profile

`tv-profile` in the menu edits `~/.config/omarchy-crt/profile.toml`:

- **monitor.** A Switchres preset: `generic_15`, `ntsc`, `pal`, `arcade_15`,
  `arcade_15_25`, `arcade_15_25_31`, `arcade_31`.
- **h shift, v shift.** Picture centering, -16 to 16. Written as
  `crt_switch_center_adjust` and `crt_switch_porch_adjust` for RetroArch and
  as `h_shift` and `v_shift` in `switchres.ini`.
- **h size.** Horizontal size 0.80 to 1.20 for `switchres.ini`.
- **invert sync.** Flips sync polarity for sets that need it.
- **test pattern.** Runs the first ROM whose title contains `240p` (the free
  240p Test Suite) through its system's core.

Leaving the screen saves the profile and rewrites `switchres.ini` in the
config directory. Point RetroArch and GroovyMAME at that file once the CRT
stack is in place.

## Recent and favorites

The systems screen starts with two virtual folders. `recent/` keeps the last
20 launched games, `favorites/` the ones toggled with `F` or the `Y` button in
a game list. Both are plain text files in the config directory, one
`system<TAB>path` per line, so they survive reordering of `systems.toml`.

## Latency recipe

The base `retroarch.cfg` enables automatic frame delay and leaves vsync on;
run-ahead is per system. On the host side, USB polling at 1 kHz helps with
some pads: add `usbhid.jspoll=1` to the kernel command line of the CRT boot
entry. Wired pads over the game controller API keep the shell itself under a
frame of input lag.
