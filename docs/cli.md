# omarchy-crt, the CLI

`omarchy-crt` is the Rust binary that turns a desktop into a CRT station and
back. The bar plugin is a face over it, the launcher is started by it, and
everything it does can be typed in a terminal.

```text
omarchy-crt status [--json]          output, mode, DAC, audio, launcher, library, BIOS
omarchy-crt on [ntsc|pal]            modeline, DAC csync, audio to the TV, launcher
omarchy-crt off                      launcher closed, audio back, output disabled
omarchy-crt boot                     login reset: output off, audio back to the desktop
omarchy-crt toggle
omarchy-crt mode ntsc|pal|film [--lines N] [--shift-x X] [--shift-y Y]
omarchy-crt shell start|stop|restart|focus
omarchy-crt shell key <input>...   # home up down left right fire back fav alt start
                                   # search osk del next prev first last
omarchy-crt shell type <text>      # type into the search bar
omarchy-crt watch <file|url> [--later [TITLE]]   # play on the tube now, or keep it in Videos
omarchy-crt focus
omarchy-crt audio crt|desktop|all|apps
omarchy-crt audio volume N           # TV sink volume, percent up to 150, kept in crt.toml
omarchy-crt dac status|reset|csync and|xor|separate|watch
omarchy-crt bios [--json]
omarchy-crt bios import DIR [--all]
omarchy-crt bios discover [--json]   # folders on the roots and disks that hold BIOS files
omarchy-crt library [--json]
omarchy-crt library cores [--json]   # the core each system needs, installed or not, its package
omarchy-crt library set SYS core=X|dir=D   # change a system's core or folder in systems.toml
omarchy-crt library scan [DIR...]
omarchy-crt library discover [--json] | roots add|remove DIR | assign DIR SYSTEM | unknown | systems
omarchy-crt doctor
omarchy-crt config
omarchy-crt config set KEY VALUE     # output.csync, output.standard, audio.volume, ...
```

`--json` answers are what the bar plugin and its library overlay render.
`library cores` names the package for a missing core (`libretro-mesen` from the
Arch repositories, `libretro-mame2003-plus-git` from the AUR) so the overlay
can offer the install through `omarchy pkg add` or `omarchy pkg aur add`.
`config set` edits one key of `crt.toml` in place and keeps the comments.

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

## Library and BIOS

`library scan DIR...` indexes every game under the given folders, whatever
their layout, and remembers the folders as roots; without arguments it
rescans the roots, or discovers mounted disks that look like collections.
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
