# Systems, launch policy and the TV profile

`~/.config/omacrt/systems.toml` describes every system the launcher
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
| gamecube                | dolphin           | none at all, on purpose: see below                                              |
| scummvm                 | scummvm           | pointer on the left stick, hardware acceleration off, copy protection off       |

The Nintendo 64 defaults follow the "authentic look" recipe documented by the
RePlayOS project: shader dithering and quantization on, RDRAM image dithering
off, native resolution.

### GameCube, and why it carries no options

Every graphics setting written for the Dolphin core here was a guess at a
value string, and each guess left it drawing a screen of magenta. With none of
them the core uses its own defaults and the games run. What decides the
picture is the line count the television is set to, which comes from
`default_lines`, not from the core.

The files Dolphin needs and does not ship, its `Sys` folder, are fetched once
from the libretro buildbot on the first launch.

### ScummVM, which is a folder rather than a file

The core wants a `.scummvm` launcher file holding a game id, beside the game's
own data. The scan works out what each folder holds from its data files and
writes that file itself, so a folder copied off a disc is playable without
anybody reading a manual from 1997. A game still inside a disc image is
reported rather than half configured, and `omacrt library unpack` reads
it out.

## The TV profile

`tv-profile` in the menu edits `~/.config/omacrt/profile.toml`:

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

## The index: any layout, scanned once

Nobody should have to rename folders for a launcher. `omacrt library
scan DIR` walks whatever you point it at (an external disk, `~/Games`, a
messy download folder) and decides the system of every file from the file
inward:

1. extensions that name a system on their own (`.sfc`, `.md`, `.z64`, `.pce`);
2. the words in the folder names above the file, tokenised, so `sega_dc`,
   `Sega - Dreamcast` and `DC games` all read as Dreamcast while `dcp` does
   not;
3. the file itself: disc signatures in `.cue`/`.bin`/`.iso` images (PlayStation,
   Saturn, Mega-CD, Dreamcast, PC Engine CD, Neo Geo CD, 3DO, CD-i), the file
   names inside a `.zip`, cartridge headers, arcade set names;
4. what you told it before with `omacrt library assign FOLDER SYSTEM`.

The result lands in `~/.local/share/omacrt/library.json` with title,
tags, region and disc number per game. The launcher lists from it: one entry
per title (regional variants collapse onto the preferred region, multi disc
games onto disc 1), systems appear when they have games and hide when they
do not, and folders no longer matter. Roots and your folder answers live in
`~/.config/omacrt/library.toml`. `systems.toml` keeps only what is
tuning: core, options, video policy, run-ahead. Systems the scan finds but
`systems.toml` does not mention take their core from the built in catalogue
(`omacrt library systems`).

A 28,000 game disk over USB scans in about four seconds.
