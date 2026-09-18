<p align="center">
  <img src="docs/logo.png" alt="The OmaCRT mark, four bars crossed by the dark cut of a beam returning, beside the word OMACRT in block letters" width="560">
</p>

<h1 align="center">OmaCRT</h1>

<p align="center">
  <a href="https://omacrt.com">omacrt.com</a>
</p>

Retro gaming on a real 15 kHz CRT television from a Hyprland desktop.
OmaCRT dedicates one output to the television through DRM leasing, while your
other screens keep working. Its compositor, Flyback, drives the CRT; a
launcher provides games, music and video at 320x240 or 320x288.

[Omarchy](https://omarchy.org) adds a bar plugin, desktop overlays and a menu
entry. Plain Hyprland uses the same launcher and command line.

**Experimental:** tested on one television with an AMD GPU and a stock kernel.
Written in Rust, licensed under MIT. No account or telemetry.

<p align="center">
  <img src="docs/screens/boot.gif" width="560" alt="The launcher booting on the tube">
</p>

<p align="center">
  <img src="docs/screens/coverflow.png" width="270" alt="Cover flow on the tube">
  <img src="docs/screens/systems.png" width="270" alt="Systems with console pictures">
  <img src="docs/screens/deck-radio.png" width="270" alt="The music deck tuned to a radio">
</p>

Images were captured from the framebuffer with `omacrt shot`; the boot animation
uses `omacrt record`. Scanlines come from the television and are absent from
these captures. More demonstrations are on [omacrt.com](https://omacrt.com).

## Why

OmaCRT began as a way to play games on a CRT from an everyday desktop and
learn how video timings, the kernel and emulators fit together. It also
provides a place to experiment with interfaces designed for a low resolution
television.

The project is developed with AI assistance, including Claude, for design,
code, tests and documentation. Hardware behaviour is tested on a real CRT.

OmaCRT includes no games or BIOS files and provides no download links for them.
Use your own files where you have the right to do so.

## How it works

<p align="center">
  <img src="docs/architecture.png" width="720" alt="OmaCRT architecture, drawn as a 16 bit illustration">
</p>

At boot a systemd unit installs an
EDID override that marks the DAC's connector _non-desktop_, so Hyprland leaves
it alone and offers it through the DRM lease protocol. **Flyback**, this
project's own Wayland compositor, takes the lease, programs the 15 kHz timing
straight into the kernel and owns the scanout of that output. It is named
after what a tube does between two lines. The launcher, RetroArch
and mpv are its clients, forced fullscreen at the output's size. Desktop
windows and notifications stay on the other outputs. The line count can
change per system: 224 lines for a Super Nintendo game, 240 for a NES one.
Each timing must pass Flyback's validation before it reaches the kernel.

The bar plugin and the CLI stay on the desktop and talk to the tube over a
control pipe: the panel is the remote control, the overlay manages the
collection, the CLI does everything from a terminal.

The processes and their communication channels are documented in
[`docs/architecture.md`](docs/architecture.md); the study behind the timings
is [`docs/15khz.md`](docs/15khz.md).

## Flyback

<p align="center">
  <img src="docs/flyback-logo.png" width="460" alt="The Flyback mark, four bars stepping out beside the full-height stroke of the beam flying back, beside the word Flyback">
</p>

Flyback uses smithay and a single event loop to schedule frames for the CRT.

It measures the interval from a client's commit to the kernel's vblank
timestamp. These are software and kernel measurements, not click-to-photon
measurements: no photodiode was used. The measurement methods are described in
[`docs/flyback.md`](docs/flyback.md#what-kind-of-measurement-each-of-these-is).

Measured on a BeoCenter 1 through an RGB-Pi 2, with the launcher mapped beneath
the test client:

| Measurement                                                                       | Result                               |
| --------------------------------------------------------------------------------- | ------------------------------------ |
| commit to the start of scanout, client drawing 1 ms                               | **3.79 ms**                          |
| the launcher end to end, in `omacrt status`                                       | **2.0 ms**                           |
| the same compositor with all three of its scheduling decisions removed            | **33.4 ms**, two frames exactly      |
| a button press to the start of scanout, 300 presses at random points of the frame | median **10.19 ms**, 0.61 of a frame |

Flyback reports presentation timing through `wp_presentation`. It can vary
the refresh rate by extending vertical blanking while keeping the horizontal
rate fixed, avoiding a full mode change within the supported range. The lower
refresh limit depends on the television and is configured in `crt.toml`.

**Timing limits matter.** Flyback validates the horizontal rate against the
configured band, which defaults to 15 to 16.5 kHz, the field rate against
40 to 90 Hz, and the ordering of the timing fields. These checks do not
establish compatibility with an untested television.

| Documentation                                        | Contents                                                    |
| ---------------------------------------------------- | ----------------------------------------------------------- |
| [`docs/flyback.md`](docs/flyback.md)                 | Architecture, scheduling, variable refresh and measurements |
| [`docs/flyback-manual.md`](docs/flyback-manual.md)   | Commands, configuration and troubleshooting                 |
| [`docs/comparison.md`](docs/comparison.md)           | Comparison with other CRT setups                            |
| [`docs/sets.md`](docs/sets.md)                       | Hardware test reports and how to submit one                 |
| [Development study](https://omacrt.com/log/flyback/) | Design and measurement notes, videos and diagrams           |

## What is on the television

- **Games.** A collection indexed from any disk in any folder layout, box art,
  console pictures, a cover flow, search across tens of thousands of titles,
  pads mapped on the tube, save states in sight, a pause menu over the game.
- **Music.** cliamp as the engine: radio by country and genre, Spotify and the
  other providers it knows, a hi-fi deck with cassette, turntable and VU
  meters, seven visualizers, synced lyrics, a ten band equaliser.
- **Video.** Local films fitted to the tube, YouTube searched and played from
  the television, a link sent from the desktop.
- **When nothing is playing.** Four pages take turns: the wordmark under a
  text effect, a photo frame reading the house's own Immich server, the
  weather drawn as a window, and a system monitor as a 16 bit status screen.

See the [launcher guide](docs/launcher.md), [Omarchy integration](docs/omarchy.md)
and [keyboard and pad bindings](docs/input.md).

## Hardware

| Part       | What worked                                                                                            |
| ---------- | ------------------------------------------------------------------------------------------------------ |
| GPU        | AMD Radeon RX 7700/7800 XT, stock kernel                                                               |
| DAC        | [RGB-Pi 2](docs/rgb-pi-2.md), HDMI in, SCART RGB out, composite sync selected over I2C, audio on SCART |
| Television | Bang & Olufsen BeoCenter 1, RGB SCART                                                                  |
| Pads       | Anything SDL knows; unknown pads get a mapping wizard on the tube                                      |

The tested desktop runs Omarchy 4 with Hyprland 0.56. Other hardware
combinations have not been tested. The following notes describe requirements
and expected compatibility:

| Part       | Where it should work                                                                                              | Where it will not                                                                                         |
| ---------- | ----------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| GPU        | Any AMD card on `amdgpu`: the lease and the 15 kHz timings are kernel side, not vendor side                       | Nvidia's proprietary driver does not offer non-desktop connectors for leasing. Intel is untested          |
| DAC        | Anything that takes HDMI and puts RGB on a SCART or VGA pin. `omacrt setup` recognises the RGB-Pi 2 from its EDID | Sync mode is set over I2C only on the RGB-Pi 2; on anything else set it on the device itself              |
| Television | Any 15 kHz set with RGB in, PAL or NTSC. The standard follows your locale, `--standard` overrides it              | A VGA monitor: 15 kHz is below what it will lock onto                                                     |
| Compositor | Hyprland 0.56 or later, and that is what Omarchy ships                                                            | Anything without DRM leasing. wlroots, KWin and Mutter all have the protocol, none of them has been tried |
| Desktop    | Omarchy for the bar widget, the library overlay and the menu entry                                                | Plain Hyprland gets the television and the command line: [`docs/hyprland.md`](docs/hyprland.md)           |

Run `omacrt setup` first on a machine that is not this one: it lists the
connectors with what their EDID says, picks the one the DAC is on, works out
the standard from the locale, and writes those two lines to `crt.toml`.
`omacrt doctor` says what is still missing. The DisplayPort DAC tier, for
native 320x240 timings instead of the wide ones, is in
[`docs/15khz.md`](docs/15khz.md).

## Install

```sh
git clone https://github.com/stefanomainardi/omacrt.git
cd omacrt
bin/omacrt-install                # builds, installs to ~/.local/bin, installs both plugins
omacrt setup                      # find the DAC's connector, write crt.toml
sudo bin/omacrt-install --system  # once: the boot time EDID override that hands the tube over
omacrt library scan ~/Games       # index your collection, any folder layout
omacrt on                         # tube on: 15 kHz timing, DAC sync, audio, launcher
```

There is an Arch package in [`packaging/`](packaging/README.md) for people who
would rather not build by hand, and `bin/omacrt-install --uninstall`
(plus `--uninstall-system` as root) puts everything back.

Requirements: an AMD card on
`amdgpu`, Hyprland 0.56 or later (the Lua configuration), systemd for the boot
time override, RetroArch with libretro cores, mpv, cliamp (a terminal music
player), yt-dlp for YouTube, ffmpeg, curl and a stable Rust toolchain.
`omacrt doctor` checks the installed hardware and software. No particular
Linux distribution is required.

After `--system` the tube is handed over at every boot, and the handover
starts working at the first boot after that. Hyprland decides which connectors
it offers for leasing when it starts, so the override has to be in place
before it is. Set `shell.autostart = true` in
`~/.config/omacrt/crt.toml` and the television boots straight into the
launcher along with the desktop.

The project was called `omarchy-crt` until it became OmaCRT. Upgrading needs
nothing: the first start moves the four old folders into their `omacrt` names
and the installer retires the plugins and binaries it had put there itself,
deleting nothing.

## The CLI

Everything the plugin and the launcher do can be typed:

```text
omacrt on | off | status | doctor | mode ntsc|pal|film|480i|576i
omacrt library scan DIR | play <title> | watch <file|url>
omacrt shell key <input>... | shot out.png | record start out.mp4
```

Commands and flags are documented in [`docs/cli.md`](docs/cli.md).
`shell key`, `shell type` and `shell screen` drive the launcher over its
control pipe.

### omacrt doctor

<p align="center">
  <img src="docs/screens/doctor-full.gif" width="620" alt="omacrt doctor displaying hardware checks and a modeline diagram in a terminal">
</p>

Run `omacrt doctor` to check the graphics driver, DRM leasing, systemd and
debugfs before connecting a DAC. With the hardware connected, it also reports
the outputs, configured modeline and DAC lock status.

The terminal interface uses [ratatui](https://ratatui.rs) and leaves the report
in scrollback so it can be copied into an issue. Use `--plain` for text output;
pipes, redirection and `NO_COLOR` also select it.

## Repository layout

| Path                 | What                                                                                                                         |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `shell/`             | The Rust workspace: `omacrt-shell` (launcher), `omacrt` (CLI), `flyback` (the compositor that owns the tube), shared library |
| `plugin/`            | The bar widget and panel; `plugin/library/` the library overlay                                                              |
| `menu/`              | The Television rows the installer writes into Omarchy's menu extension file                                                  |
| `bin/omacrt-install` | Build and install everything, `--system` for the boot time lease                                                             |
| `bin/omacrt-pick`    | Pick a game with the desktop's runner, play it on the tube                                                                   |
| `scripts/`           | The EDID override and lease setup, DRM probing, the offline demo renderer, the video takes and montage                       |
| `systemd/`           | The oneshot unit that hands the tube over at boot                                                                            |
| `docs/`              | Hardware, compositor design, measurements, launcher, desktop integration and command reference                               |
| `packaging/`         | The Arch `PKGBUILD` and what it installs where                                                                               |
| `THIRD-PARTY.md`     | Everything here that somebody else wrote, and under what terms                                                               |
| `.github/workflows/` | The build, the lints, the tests and a headless render of the launcher's own frames                                           |

## State

See [`CHANGELOG.md`](CHANGELOG.md) for changes. Interlaced modes require a
patched kernel; progressive modes work on the tested stock kernel.

## Contributing

This is a personal project maintained in spare time. Contributions are welcome.
Changes to display timings, rendering or emulator configuration need testing
on a real CRT, with the hardware listed in the pull request. Other changes can
be checked without a television; report what you tested.

- [`CONTRIBUTING.md`](CONTRIBUTING.md): what gets merged, what does not, the
  standards, and the commands CI runs.
- [`AGENTS.md`](AGENTS.md): module layout, development procedures, headless
  rendering and operational precautions.
- [`SECURITY.md`](SECURITY.md): what runs as root, what is downloaded and from
  where, what is executed, and how credentials are handled.
- [`docs/troubleshooting.md`](docs/troubleshooting.md): what has gone wrong so
  far and what it turned out to be.

Bugs are issues, with the template filled in and the output of `omacrt
doctor`. Ideas and questions are discussions.

## Credits and licenses

Third-party components and their licences are listed in
[`THIRD-PARTY.md`](THIRD-PARTY.md). The wordmark uses Omarchy's letterforms,
Copyright (c) David Heinemeier Hansson, under MIT; the notice is included in
[`shell/assets/LICENSE.omarchy`](shell/assets/LICENSE.omarchy). The icon is
OmaCRT's own design.

OmaCRT is independent of Omarchy and is not endorsed by the Omacom Foundation.

License: MIT, and it covers the pictures and the documentation as well as
the code. Reusing the wordmark means carrying Omarchy's notice with it:
[`LICENSE-SCOPE.md`](LICENSE-SCOPE.md) says what that means in practice.
