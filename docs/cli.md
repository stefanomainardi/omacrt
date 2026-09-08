# omarchy-crt, the CLI

`omarchy-crt` is the Rust binary that turns a desktop into a CRT station and
back. The bar plugin is a face over it, the launcher is started by it, and
everything it does can be typed in a terminal.

```text
omarchy-crt setup [--connector NAME] [--standard ntsc|pal] [--dry-run] [--force]
                                     first run: the DAC's connector and the standard
omarchy-crt status [--json]          output, mode, DAC, audio, launcher, library, BIOS
omarchy-crt on [ntsc|pal]            modeline, DAC csync, audio to the TV, launcher
omarchy-crt off                      launcher closed, audio back, output disabled
omarchy-crt boot                     login reset: output off, audio back to the desktop
omarchy-crt toggle
omarchy-crt mode ntsc|pal|film|480i|576i [--lines N] [--shift-x X] [--shift-y Y]
omarchy-crt shell start|stop|restart|focus
omarchy-crt shell key <input>...   # home up down left right fire back fav alt start
                                   # search osk del next prev first last
omarchy-crt shell type <text>      # type into the search bar
omarchy-crt game key <key> [ms]    # press a key inside the running game, held that long
                                   # (enter is start, rshift the coin, or an evdev code)
omarchy-crt watch <file|url> [--later [TITLE]]   # play on the tube now, or keep it in Videos
omarchy-crt focus
omarchy-crt audio crt|desktop|all|apps   # our own streams only, unless `all`
omarchy-crt audio volume N           # TV sink volume, percent up to 150, kept in crt.toml
omarchy-crt dac status|reset|csync and|xor|separate|watch
omarchy-crt bios [--json]
omarchy-crt bios import DIR [--all]
omarchy-crt bios discover [--json]   # folders on the roots and disks that hold BIOS files
omarchy-crt library [--json]
omarchy-crt library cores [--json]   # the core each system needs, installed or not, its package
omarchy-crt library set SYS core=X|dir=D   # change a system's core or folder in systems.toml
omarchy-crt library covers [SYS...] [--limit N] [--force]   # box art for the collection, titles matched
omarchy-crt library scan [DIR...] [--progress]   # --progress: one plain line per folder
omarchy-crt library discover [--json] | roots add|remove DIR | assign DIR SYSTEM | unknown | systems
omarchy-crt watchdog                 put the display back if it dies; `on` starts it, `off` stops it
omarchy-crt doctor
omarchy-crt config
omarchy-crt config set KEY VALUE     # output.csync, output.standard, audio.volume, ...
```

`--json` answers are what the bar plugin and its library overlay render.
`library cores` names the package for a missing core (`libretro-mesen` from the
Arch repositories, `libretro-mame2003-plus-git` from the AUR) so the overlay
can offer the install through `omarchy pkg add` or `omarchy pkg aur add`.
`config set` edits one key of `crt.toml` in place and keeps the comments.

`setup` is the first run on a machine that is not the author's. It lists the
DRM connectors with their EDID name, whether the kernel already marks them
non-desktop, and whether the desktop is drawing on them; picks the one with a
DAC it recognises, or a connected HDMI output the desktop is not using; takes
the standard from `--standard`, else from what is already configured, else
from the country in the locale; and writes `output.connector` and
`output.standard`. `--dry-run` only looks. A connector already named in
`crt.toml` is not replaced without `--force`.

## Files, and what happens to them

`settings.toml` and `systems.toml` carry a `version` key. An older file is
read as it is, since every key added so far has a default. A file written by a
*newer* build is copied aside as `<name>.v<N>` before anything touches it,
because saving it would drop the keys this build does not know about. Every
durable file is written through a temporary file and a rename, with the copy
being replaced kept as `.bak`, and read back through the backup when the
current one is unreadable.

Logs live in `~/.local/state/omarchy-crt`. They rotate past 8 MB, keeping one
generation as `<name>.1`. The display process writes a line per second about
frames and timing only when `OMARCHY_CRT_LOG=debug` is set; otherwise its log
holds warnings, errors and what it is doing.

Arcade collections name their files after the emulated set, `mslug.zip` for
Metal Slug, while both the lists and the thumbnail repository work in titles.
The databases RetroArch ships (`/usr/share/libretro/database/rdb`, or the
user's own copy) pair the two, so those systems show real titles, sort and
search by them, and find their box art. Without those databases the set names
stay as they are.

`library covers` walks the collection and fetches the libretro thumbnail of
every game into `~/.cache/omarchy-crt/art/<system>/`. The exact file name is
tried first; when the repository has no such name (collections without
region tags), the name index of that system (downloaded once a fortnight into
`art/_index/`) is searched for the same title, in the region order of
`[music] country` in `settings.toml`, then for the closest title by words.
The launcher does the same lazily for any cover it misses. Covers are shrunk
to 320 pixels on the way in.

## What `on` does, in order

1. Applies the modeline of the chosen standard to the CRT output through
   `hyprctl eval` (Hyprland's Lua config API), with the position from the
   config.
2. Waits a second for the DAC to lock, then selects the composite sync mode
   over I2C (`output.csync`, `xor` by default).
3. Switches the GPU audio card to the DAC's HDMI profile and sets the sink
   volume (`audio.volume`, 100 by default). The launcher starts with
   `PULSE_SINK` and `PIPEWIRE_NODE` pointing at that sink, so it, RetroArch and
   mpv play on the television while the desktop keeps its own output.
   Streams already open are moved. `audio.system_default = true` (or `audio
all` from the panel) makes the CRT the default sink for everything; the
   previous profile and sink are remembered.
4. Tells Hyprland that a new fullscreen window takes over and that the window
   underneath gets fullscreen back when it exits, and pins the launcher,
   RetroArch and mpv to the CRT output.
5. Starts the launcher fullscreen and gives it keyboard focus.

`off` reverses the list: launcher stopped, audio restored, options reset,
output disabled.

## Boot and workspaces

Hyprland lights every connected output with its preferred mode at login, which
for the DAC means 1024x768 (a flickering television) and a numbered desktop
workspace stolen by the CRT. Two lines in the user config keep the tube quiet
until asked and put audio back on the desktop after a reboot:

```lua
-- ~/.config/hypr/monitors.lua, before the fallback rule
hl.monitor({ output = "desc:ATG MORTACA DEV00", disabled = true })
-- ~/.config/hypr/autostart.lua
o.launch_on_start("omarchy-crt boot")
```

`on` binds a named workspace `crt` to the CRT output, so terminals and
browsers keep opening on the desktop monitors and `off` gives nothing back to
sort out. `boot` restores the previous default sink, disables the output when
the compositor lit it, and clears the state.

## Modelines and lines

`mode ntsc --lines 224` keeps the line rate and refresh of the NTSC modeline
but shows 224 active lines centred in the frame, so a Super Nintendo game
lands on the tube one line per line. The launcher will use this per system.

Modeline clocks must be whole MHz because Hyprland 0.56 truncates them, and a
stored modeline can only be replaced by another modeline (`mode = "WxH"` is
ignored). Validated on the RGB-Pi 2:

```text
ntsc = "72 3520 3695 4033 4577 240 242 245 262 -hsync -vsync"   # 15.73 kHz, 60.04 Hz
pal  = "72 3840 3948 4290 4608 288 291 294 312 -hsync -vsync"   # 15.63 kHz, 50.08 Hz
```

Two interlaced timings sit next to them, for video rather than games. They
keep the same line rate and draw two fields per frame, so the tube shows 480
or 576 lines:

```text
ntsc_i = "72 3520 3695 4033 4577 480 484 490 525 -hsync -vsync interlace"
pal_i  = "72 3840 3948 4290 4608 576 582 588 625 -hsync -vsync interlace"
```

`mode 480i` and `mode 576i` apply them. The refresh in `status` is the frame
rate, half the field rate: 29.96 Hz means 59.93 fields. The launcher draws at
the full line count in these modes. Both were programmed on the RGB-Pi 2;
whether a source looks better as fields than as a deinterlaced 240p picture
is a judgement for the eye, in front of the tube.

## Library and BIOS

`library scan DIR...` indexes every game under the given folders, whatever
their layout, and remembers the folders as roots. With `--progress` it prints
`scanning <folder>` as it goes, which is how the library overlay shows a scan
without opening a terminal. Without arguments it rescans the roots, or discovers mounted disks that look like collections.
`library` shows systems with counts and sources, `library unknown` the files
it could not place and `library assign FOLDER SYSTEM` teaches it. See
[docs/systems.md](systems.md) for the detection rules.

`bios` checks the files each core expects in RetroArch's system directory,
`MISS` for required files of systems you actually have. `bios import DIR`
copies them from another collection; `--all` also copies the known folders
(`dc`, `fbneo`, `neocd`, `Machines`, ...).

## Config

`~/.config/omarchy-crt/crt.toml`, written with comments on first run:

```toml
[output]
connector = ""        # empty: first HDMI output with a Mortaca (RGB-Pi 2) EDID
position = "auto"
csync = "xor"         # and | xor | separate
standard = "ntsc"

[modelines]
ntsc = "72 3520 3695 4033 4577 240 242 245 262 -hsync -vsync"
pal = "72 3840 3948 4290 4608 288 291 294 312 -hsync -vsync"
ntsc_i = "72 3520 3695 4033 4577 480 484 490 525 -hsync -vsync interlace"
pal_i = "72 3840 3948 4290 4608 576 582 588 625 -hsync -vsync interlace"

[shell]
bin = "omarchy-crt-shell"
args = ["--fullscreen", "--stretch", "--auto-boot"]
autostart = false     # true: `boot` switches the tube on at login when the DAC is there

[audio]
route = true
volume = 100
```

State lives in `~/.local/state/omarchy-crt/state.json` and the launcher log
in `~/.local/state/omarchy-crt/shell.log`.

## Driving the launcher

The launcher listens on a named pipe, `~/.local/state/omarchy-crt/shell.ctl`,
and treats every line as a key press or a pad button:

```
omarchy-crt shell key down down fire   # two rows down, open
omarchy-crt shell key back             # B
```

Inputs: `home` (top of the main menu), `up`, `down`, `left`, `right`, `fire`
(A, Enter), `back` (B, Esc), `fav` (Y), `alt` (X), `start`. Aliases such as `enter`, `esc`, `a`, `b` work
too. This is how `scripts/shoot.sh` records the tour and how tests drive the
menu; a game already running keeps the real keyboard and pad, nothing from
the pipe reaches it.

A whole screen can be asked for by name, which is what the desktop menu does
rather than counting rows:

```
omarchy-crt shell screen music
omarchy-crt shell screen frame
```

Names: `home`, `games`, `videos`, `youtube`, `music`, `favorites`, `recent`,
`frame`, `ambient`, `monitor`, `processes`, `settings`, `picture`, `style`,
`pads`, `diagnostics`, `about`, `power`. Nothing happens while a game or a film is on:
a menu entry pressed by accident must not take the television away from what
it is doing.

## The photo frame

```
omarchy-crt frame check       # is the server there, does it take the key
omarchy-crt frame fill [N]    # prepare N photographs ahead of an evening
omarchy-crt frame clear       # throw the prepared ones away
```

The frame needs `~/.config/omarchy-crt/immich.toml`:

```toml
url = "https://immich.example.lan"
key = "an API key with read access to assets, albums and memories"
```

Make the key in Immich under the account menu, API Keys. It is handed to curl
on its standard input, never on a command line, so it does not show up in the
process list. Without that file the frame says so and everything else works
as before.

What the frame shows and for how long is in `settings.toml`, under `[frame]`,
and on the television under Settings, Photo frame: `style` (`photos`, `clock`,
`panel`), `seconds`, `source` (`memories`, `favorites`, `album`, `all`),
`album`, `pan`, `weather` (a place name for wttr.in, empty asks by address)
and `calendar` (an `.ics` address).

### Where the frame is

Four ways in, and none of them is the settings page:

- **On the television**, Videos, Photo frame. Videos is where the pictures
  live: films, YouTube, the frame, the clock.
- **From the settings page** for it, Settings, Photo frame: A shows it now.
  That page is where it is set up, not where it runs.
- **On its own, when the television is left alone**: Settings, Screensaver,
  set `when idle` to `photos`, or to `mix` to take turns with the other pages.
- **From the desktop**: the Omarchy menu, Television, Channel, Photo frame, or
  `omarchy-crt shell screen frame` in a terminal.

## What an idle television shows

The screensaver is not only a text effect on the wordmark any more. Four pages
can have the screen, and they can take turns:

| Page | What |
| --- | --- |
| `effects` | The wordmark taken apart and put back, one of nine effects |
| `photos` | The photo frame |
| `ambient` | A window with the weather drawn in it, and the time under it |
| `system` | The system monitor |

```toml
[screensaver]
enabled = true
idle_secs = 60      # seconds of nothing before it starts
effect = "mix"      # a page by name, an effect by name, random, or mix
cycle_secs = 240    # while mixing, how long each page stays; 0 keeps one
off = ["system"]    # pages left out of the mix
```

The ambient page draws what the weather is doing: the sun on its real arc
between sunrise and sunset, the moon on the same path at night, clouds at the
speed of the real wind, rain that slants with it and breaks on the ground,
snow, fog, lightning, and a town along the horizon whose windows come on after
dark. It needs no photograph server. `[frame] weather` names the town; with that
empty the town is the city in the machine's own timezone, so
`Europe/Brussels` asks about Brussels. The Photo frame settings page on the
television says which town it is using and where the name came from.

`effect` takes one page name (`effects`, `photos`, `ambient`, `system`), one
effect name (`laseretch`, `rain`, `beams`, `burn`, `slide`, `decrypt`,
`expand`, `unstable`, `vhstape`), `random` for any effect, or `mix` to take
turns between the pages that are on. The same page is on the television under
Settings, Screensaver, where every one of those is a row.

Two rules decide who wins:

- **Music playing takes the screen**, and the visualizer stands in, whatever
  the mix says. A page that shows the time is a poor answer to a room with
  music in it. Turn it off with `[music] saver = false`.
- **The first page of an evening is picked at random** among the ones that are
  on, so a television left alone twice does not open the same way twice.
  After that they go round in order.

The first key press puts back the screen that was up before, not the top of
the menu: a page the idle timer started is a screensaver, whatever else it can
do.

## The Omarchy menu

`bin/omarchy-crt-install` adds a **Television** entry to Omarchy's own menu by
writing the block in `menu/omarchy-menu.jsonc` into
`~/.config/omarchy/extensions/omarchy-menu.jsonc`, between two markers, with
the previous file kept beside it as `.omarchy-crt.bak`. That path is the
extension point Omarchy offers: nothing is patched and nothing is forked.
`--uninstall` takes the block out again.

The rows are power, the link in the clipboard, then Channel (games, music,
video, the photo frame, the system monitor), Picture (standard, lines,
centring), Sound (volume, where the audio goes), Library (scan, box art,
BIOS, fill the frame), Capture (screenshot, record), Pads and Diagnostics.
Every one of them runs a command from this page.

## The display process

```
omarchy-crt-display run [connector]       # lease the connector, hold the mode, host clients
omarchy-crt-display probe [connector] [s] # take the lease, show a test card, release
omarchy-crt-display props [connector]     # the kernel's view: state, modes, non-desktop
```

`omarchy-crt on` starts `run` itself when the connector is leaseable. Its
control pipe, `~/.local/state/omarchy-crt/display.ctl`, takes one line at a
time: `top <app_id>` (stacking), `mode <modeline>` (live timing change),
`key <name>` (press a key on the tube's keyboard: pause, save, load, reset,
quit, ff, menu, or an evdev code) and `quit`.
