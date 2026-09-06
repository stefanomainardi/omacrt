# omarchy-crt, the CLI

`omarchy-crt` is the Rust binary that turns a desktop into a CRT station and
back. The bar plugin is a face over it, the launcher is started by it, and
everything it does can be typed in a terminal.

```text
omarchy-crt status [--json]          output, mode, DAC, audio, launcher, library, BIOS
omarchy-crt on [ntsc|pal]            modeline, DAC csync, audio to the TV, launcher
omarchy-crt off                      launcher closed, audio back, output disabled
omarchy-crt toggle
omarchy-crt mode ntsc|pal [--lines N]
omarchy-crt shell start|stop|restart|focus
omarchy-crt focus
omarchy-crt audio crt|desktop
omarchy-crt dac status|reset|csync and|xor|separate|watch
omarchy-crt bios [--json]
omarchy-crt bios import DIR [--all]
omarchy-crt library scan [--json]
omarchy-crt library link DIR [--write]
omarchy-crt doctor
omarchy-crt config
```

## What `on` does, in order

1. Applies the modeline of the chosen standard to the CRT output through
   `hyprctl eval` (Hyprland's Lua config API), with the position from the
   config.
2. Waits a second for the DAC to lock, then selects the composite sync mode
   over I2C (`output.csync`, `xor` by default).
3. Switches the GPU audio card to the DAC's HDMI profile, sets the sink volume
   (`audio.volume`, 100 by default), makes it the default sink and moves the
   launcher, RetroArch and mpv streams there. The previous profile and sink
   are remembered.
4. Tells Hyprland that a new fullscreen window takes over and that the window
   underneath gets fullscreen back when it exits, and pins the launcher,
   RetroArch and mpv to the CRT output.
5. Starts the launcher fullscreen and gives it keyboard focus.

`off` reverses the list: launcher stopped, audio restored, options reset,
output disabled.

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

`library scan` lists every system with its folder, game count, files with
unknown extensions and whether the core is installed. `library link DIR`
reads a ROM collection laid out the RePlayOS or Batocera way
(`nintendo_nes`, `sony_psx`, `sega_dc`, ...), maps each folder to a system
and core available on Arch, prints the resulting `systems.toml` and writes it
with `--write` (the previous file is kept as `systems.toml.bak`). Nothing is
copied, the systems point at the collection.

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

[audio]
route = true
volume = 100
```

State lives in `~/.local/state/omarchy-crt/state.json` and the launcher log
in `~/.local/state/omarchy-crt/shell.log`.
