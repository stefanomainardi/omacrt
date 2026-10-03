# Systems, launch policy and the TV profile

`~/.config/omacrt/systems.toml` describes every system the launcher
shows: where the ROMs are, which libretro core runs them and what RetroArch
should do so the game looks and plays right on first launch. When the file is
missing the shell uses built-in defaults for the cores Arch ships.

## File layout

```toml
# Enable mode switching in RetroArch (CRT SwitchRes). Needs the KMS or X11
# video driver, and a kernel carrying the 15 kHz patches for the modes it
# picks. Off by default: pinned frames still work.
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
- **`player`.** `retroarch` (default), `mpv` or `supermodel`. An `mpv`
  system is a video folder: files play fullscreen through mpv, the shell
  keeps the pad and draws the overlay. The built-in `videos` system points at
  `~/Videos`. `supermodel` is the built-in `model3` system, below.

## Built-in defaults

Run-ahead and rewind follow the RGB-Pi frontend's whitelists: on for 8 and 16
bit consoles and handhelds, off for arcade, PlayStation, Nintendo 64 and
Dreamcast. Video is `super` everywhere except the Super Nintendo (`512x224`,
absorbs the hi-res modes without switching) and the Dreamcast (`native`,
because 480 line content wants interlacing).

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

The Dolphin core takes a graphics option only in the exact value string it
expects, and a wrong one leaves it drawing a screen of magenta instead of
refusing the setting. With no options at all the core uses its own defaults and
the games run. What decides the picture is the line count the television is set
to, which comes from `default_lines`, not from the core.

The files Dolphin needs and does not ship, its `Sys` folder, are fetched once
from the libretro buildbot on the first launch.

### Games taller than the tube

Some boards draw more lines than a 15 kHz television holds. Sega's Model 2,
which runs in the MAME core (Sega Rally Championship, Daytona USA, Virtua
Fighter 2), draws 496x384 at 57.52 Hz for a 24 kHz monitor. The tube is given
its whole frame, 240 lines in NTSC, at the board's own rate, and the emulator
reduces the picture to it.

A reduction by a whole number is left sharp: a Dreamcast's 480 lines into 240
keeps every other line. Anything else is filtered, because the emulator's
nearest neighbour drops lines unevenly, three in every eight from 384 to 240,
and the letters of a timer come out with rows missing. The filter applies from
the second launch of a game, once the launcher knows how many lines it draws.

### Sega Model 3, through Supermodel

Model 3 is the one board on the tube that RetroArch does not run: no libretro
core emulates it. The `model3` system (Sega Rally 2, Daytona USA 2, Scud Race,
Virtua Fighter 3) is played by Supermodel, a program of its own, which the
launcher starts the way it starts RetroArch and stops with Select and Start.

The board draws 496x384 at 57.524 Hz for a 24 kHz monitor. Supermodel can draw
its 3D at any size, so it is handed the tube's whole frame and the scene comes
out drawn at 240 lines rather than reduced to them; only the text and gauges,
a 2D layer, are scaled, with Supermodel's own filter. The tube runs at the
board's rate.

Supermodel wants its sets flat in one folder, because a variant such as
`dayto2pe` loads the files it shares with `daytona2` from beside it. The scan
finds them in a folder called `model3`, `arcade_model3` or `Sega Model 3`.

On a first launch the launcher writes `~/.config/supermodel/Config/Supermodel.ini`
for a pad: Select is the coin, Start starts, the left stick steers, the right
trigger accelerates and the left one brakes, the shoulders change gear. It
replaces the file Supermodel writes for itself on a first run, which puts
Start on a button a pad does not have, and leaves alone one anybody has
edited.

Scud Race and Daytona USA 2 were built for two cabinets linked by a network
board, and out of the box each is the master of a pair: alone, it stops at
"network board not present". An operator set LINK ID to SINGLE in the test
menu. The launcher does the same in Supermodel's saved copy of the game's
EEPROM before every start: the link word and a flag beside it, in both
copies of the settings, and the CRC-16 over them. A first run has nothing
saved yet and stops at the error once; the file it leaves behind is set
right when it ends, and the launcher says to start it again.

Supermodel's sound ignores the sink the launcher asks for, so the launcher
moves the stream to the television, through the loudness leveller, once it
opens.

The AUR package does not start on the tube: the display process is a Wayland
compositor with no X server, and Supermodel's copy of GLEW gives up when it
finds no X display although it has everything it needs. `packaging/supermodel-omacrt`
builds the same upstream release with that one check relaxed, and replaces the
AUR package.

### ScummVM, where a game is a folder

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
- **h shift, v shift.** Picture centering, -16 to 16, in the launcher's
  pixels and in lines. This is the television's own level, applied to every
  mode the tube is given.

### Where a picture sits

A picture is placed at three levels, each a correction on the one before:
the TV profile for the set, `shift_x` and `shift_y` in a system's entry in
`systems.toml` for a console whose timing puts its picture somewhere else,
and `~/.config/omacrt/centring.tsv` for a single game. A game's entry
replaces its system's rather than adding to it. All three are set from the
pause menu's **Centre picture** page while the game runs, and the system's
also with `omacrt library set SYS shift_x=N`. Moving the picture changes the
porches of the timing and not its line rate, so the set keeps its lock.
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
some pads: a pad asks in its own descriptor how often it is read, and cheap
ones ask for every eight milliseconds. `sudo bin/omacrt-install --system`
offers to set it. On Omarchy it writes `usbhid.jspoll=1` into
`/etc/limine-entry-tool.d/omacrt-usbhid.conf` and rebuilds the boot image with
`limine-update`; `OMACRT_JSPOLL=1` answers yes without the question, and
`--uninstall-system` takes it back out. Elsewhere, add `usbhid.jspoll=1` to the
kernel command line by hand. Wired pads over the game controller API keep the
shell itself under a frame of input lag.

## The index: any layout, scanned once

Nobody should have to rename folders for a launcher. `omacrt library
scan DIR` walks whatever you point it at (an external disk, `~/Games`, a
messy download folder) and decides the system of every file from the file
inward:

1. extensions that name a system on their own (`.sfc`, `.md`, `.z64`, `.pce`);
2. the words in the folder names above the file, tokenised, so `sega_dc`,
   `Sega - Dreamcast` and `DC games` all read as Dreamcast while `dcp` does
   not;
3. the file itself, where disc signatures in `.cue`/`.bin`/`.iso` images (PlayStation,
   Saturn, Mega-CD, Dreamcast, PC Engine CD, Neo Geo CD, 3DO, CD-i), the file
   names inside a `.zip`, cartridge headers, arcade set names;
4. what you told it before with `omacrt library assign FOLDER SYSTEM`.

The result lands in `~/.local/share/omacrt/library.json` with title, tags,
region and disc number per game. The launcher lists from it: one entry per
title, with regional variants collapsed onto the preferred region and multi
disc games onto disc 1. Systems appear when they have games and hide when they
do not, and the folder a game sits in does not decide which system it belongs
to. Roots and your folder answers live in `~/.config/omacrt/library.toml`.
`systems.toml` keeps only what is tuning: core, options, video policy,
run-ahead. Systems the scan finds but `systems.toml` does not mention take
their core from the built in catalogue (`omacrt library systems`).

A 28,000 game disk over USB scans in about four seconds.
