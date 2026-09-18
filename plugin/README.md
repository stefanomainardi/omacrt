# OmaCRT bar plugin

The bar widget shows the television's status. Its panel controls power,
NTSC or PAL, keyboard focus, DAC sync, audio routing and volume. It also
provides access to the game library and BIOS checks.

The widget reads `omacrt status --json`; each button runs an `omacrt` command
through the helper in `bin/` beside the plugin files.

## Install

From a checkout of the repository:

```sh
bin/omacrt-install
```

That builds the launcher and the CLI, copies them to `~/.local/bin`, copies
this plugin to `~/.config/omarchy/plugins/io.github.stefanomainardi.omacrt`
and enables it in the right section of the bar.

### The helper

Everything the widget and the panel do is a call to `omacrt`, through
`bin/omacrt` in this folder. That file is a two line launcher for the
installed program, written by `omacrt plugin sync`. It is not a copy.

The helper keeps the plugin on the installed CLI version after package
upgrades. Older installs copied the binary into the plugin directory, where
it could become stale. `omacrt doctor` checks this under `plugin up to date`.

The helper is a script because Omarchy's plugin validator rejects symlinks
inside plugin folders.

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
- `TV volume` sets the sink volume of the television (up to 150%) and keeps
  it in `crt.toml`.
- The `PADS` section lists the pads in port order with how each one is
  attached and its charge where the kernel reports one, and moves or forgets
  one on the spot. `Pads` opens the full screen overlay (a third plugin,
  `io.github.stefanomainardi.omacrt.pads`): the four ports as sockets,
  filled or not, each with the pad's serial, the index RetroArch will give
  it, and whether the letters on its face sit where SDL expects. Identify
  shakes the pad in that port, which is the only way to tell two of one model
  apart.
- `Library` opens the full screen library overlay (a second plugin,
  `io.github.stefanomainardi.omacrt.library`, installed and enabled by
  the installer): the folders the scan reads and disks that look like
  collections with one click "adopt and scan", every system with its games
  and core (a missing core offers its package, from the repositories or the
  AUR, in a floating terminal), the BIOS files the cores expect with "import
  from here" for any folder holding them, and the folders the scan could not
  place with a system picker. `Esc` or the scrim closes it. Everything runs
  `omacrt` commands, so the same can be typed in a terminal.

IPC for keybindings, all through `omarchy-shell -q <id> <method>`:
`power`, `on`, `off`, `ntsc`, `pal`, `focus`, `restart`, `library`,
`audio crt|desktop`, `csync and|xor`, `toggle` (panel), `refresh`.

```lua
-- ~/.config/omarchy/hypr/bindings.lua
hl.bind("SUPER + SHIFT + C", hl.dsp.exec_cmd("omarchy-shell -q io.github.stefanomainardi.omacrt power"))
hl.bind("SUPER + SHIFT + K", hl.dsp.exec_cmd("omarchy-shell -q io.github.stefanomainardi.omacrt focus"))
```

## Settings

`Refresh interval` for the bar icon (the panel refreshes every two seconds
while open) and `Hide when no DAC is connected`.
