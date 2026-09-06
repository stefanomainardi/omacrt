# Omarchy CRT bar plugin

A television in the Omarchy bar. The glyph shows whether the 15 kHz tube is
on the air and the panel is the remote control: power, NTSC or PAL, keyboard
focus to the launcher, DAC sync mode and reset, audio to the TV, and the
health of the game library and BIOS files.

Everything real happens in the `omarchy-crt` binary that ships in `bin/`
next to these files. The widget runs `omarchy-crt status --json` and renders
it; every button runs one `omarchy-crt` command.

## Install

From a checkout of the repository:

```sh
bin/omarchy-crt-install
```

That builds the launcher and the CLI, copies them to `~/.local/bin`, copies
this plugin to `~/.config/omarchy/plugins/io.github.stefanomainardi.omarchy-crt`
and enables it in the right section of the bar.

## Use

- Left click opens the panel, middle click toggles the tube, right click
  refreshes.
- The panel hero is an on screen display: `ON AIR`, `STANDBY`, `NO SIGNAL`
  or `SYNC LOST`, with line rate, refresh, resolution, DAC lock, composite
  sync mode and where the audio goes.
- Power on: 15 kHz modeline on the CRT output, DAC composite sync, HDMI
  audio to the TV as default sink, launcher fullscreen with keyboard focus.
  Power off undoes all of it.
- `Keys to the launcher` gives the keyboard back to the tube when the desktop
  stole it.

IPC for keybindings, all through `omarchy-shell -q <id> <method>`:
`power`, `on`, `off`, `ntsc`, `pal`, `focus`, `restart`, `audio crt|desktop`,
`csync and|xor`, `toggle` (panel), `refresh`.

```lua
-- ~/.config/omarchy/hypr/bindings.lua
hl.bind("SUPER + SHIFT + C", hl.dsp.exec_cmd("omarchy-shell -q io.github.stefanomainardi.omarchy-crt power"))
hl.bind("SUPER + SHIFT + K", hl.dsp.exec_cmd("omarchy-shell -q io.github.stefanomainardi.omarchy-crt focus"))
```

## Settings

`Refresh interval` for the bar icon (the panel refreshes every two seconds
while open) and `Hide when no DAC is connected`.
