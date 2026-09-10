<p align="center">
  <img src="docs/logo.png" alt="OmaCRT" width="504">
</p>

<h1 align="center">OmaCRT</h1>

<p align="center">
  <a href="https://omacrt.com">omacrt.com</a>
</p>

A PC plugged into a 15 kHz CRT television over RGB SCART, playing retro games
the way they were drawn: native lines, native refresh, real scanlines, no
scaler in between. A launcher on the tube, a display process that owns the
television outright, and on [Omarchy](https://omarchy.org) a bar plugin and a
menu entry on the desktop as well. Written in Rust, drawn at 320x240.

<p align="center">
  <img src="docs/screens/boot.gif" width="560" alt="The launcher booting on the tube">
</p>

<p align="center">
  <img src="docs/screens/coverflow.png" width="270" alt="Cover flow on the tube">
  <img src="docs/screens/systems.png" width="270" alt="Systems with console pictures">
  <img src="docs/screens/deck-radio.png" width="270" alt="The music deck tuned to a radio">
</p>

Every picture here came off the tube's own framebuffer with `omacrt shot`, and
the boot with `omacrt record`; the television adds the scanlines.
[omacrt.com](https://omacrt.com) has the rest of it moving, including the
film.

## Why

A television that marked a couple of generations sits in a corner, still
perfect at what it was built for, and a modern PC can feed it the exact signal
its tube was made to show. This project is a way to learn in the open how that
signal, the kernel, a compositor, an emulator and a music player fit together,
and to give the old screen a second life: the games look the way they were
drawn, the radio sounds like a radio, an evening's video plays without a
scaler in between. It is also a playground, because a system this malleable
invites the experiments the big frontends never bothered with.

> **Three things worth knowing.**
>
> **This project is written with an AI.** Design, code, tests and this very
> README are the work of a human directing Claude, commit after commit, on a
> real television in a real living room. If a codebase built that way is not
> for you, no hard feelings: there are many other repositories.
>
> **This project is built for Omarchy and runs on plain Hyprland.** The
> television, the launcher, the timings and the whole command line need
> Hyprland and nothing else. The bar widget, the library overlay and the
> Television menu entry are Omarchy plugins, and without Omarchy there is
> the command line in their place. It is not a generic Linux CRT frontend
> and will not become one: what it will not do is pretend the desktop half
> is the whole thing. [`docs/hyprland.md`](docs/hyprland.md) is the second
> channel.
>
> **This project is about preservation, not piracy.** It ships no games, no
> BIOS files and no copyrighted material, and it links to none. It is a
> frontend for hardware and software you already own: a television, a DAC,
> emulators, and whatever you are entitled to run on them. Old machines and
> the things made for them are disappearing into landfill and rot; keeping
> them readable, and keeping a tube alive to show them on, is the point.
> Where you get your files, and whether you have the right to them, is
> between you and the law of your country.
>
> OmaCRT is a fun project by one user. It is not affiliated with, endorsed
> by or part of the official Omarchy project. Omarchy, RetroArch, RGB-Pi and
> every other name here belong to their owners.
>
> **Tested on one setup so far:** Omarchy 4 with Hyprland 0.56, an AMD Radeon
> RX 7700/7800 XT, an RGB-Pi 2 DAC and a Bang & Olufsen BeoCenter 1. Other
> GPUs, DACs and televisions are uncharted: the code is written to cope with
> them, and none of them has been tried.

## How it works

<p align="center">
  <img src="docs/architecture.png" width="720" alt="OmaCRT architecture, drawn as a 16 bit illustration">
</p>

The desktop never touches the television. At boot a systemd unit installs an
EDID override that marks the DAC's connector *non-desktop*, so Hyprland leaves
it alone and offers it through the DRM lease protocol. `omacrt-display` takes
the lease, programs the 15 kHz timing straight into the kernel and runs a
small Wayland compositor of its own on that output. The launcher, RetroArch
and mpv are its clients, forced fullscreen at the output's size. No bar,
notification, pointer or stray window can reach the tube, and any timing the
kernel accepts is one command away, live, including a different line count per
system (224 lines for a Super Nintendo game, 240 for a NES one).

The bar plugin and the CLI stay on the desktop and talk to the tube over a
control pipe: the panel is the remote control, the overlay manages the
collection, the CLI does everything from a terminal.

The same thing with every process and channel named is a flowchart in
[`docs/architecture.md`](docs/architecture.md); the study behind the timings
is [`docs/15khz.md`](docs/15khz.md).

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

Screen by screen, with what each one does and why:
[`docs/launcher.md`](docs/launcher.md). What the desktop lends it, from the
bar plugin to the menu entry: [`docs/omarchy.md`](docs/omarchy.md). Keyboard
and pad bindings: [`docs/input.md`](docs/input.md).

## Hardware

| Part | What worked |
| --- | --- |
| GPU | AMD Radeon RX 7700/7800 XT, stock kernel |
| DAC | [RGB-Pi 2](docs/rgb-pi-2.md), HDMI in, SCART RGB out, composite sync selected over I2C, audio on SCART |
| Television | Bang & Olufsen BeoCenter 1, RGB SCART |
| Pads | Anything SDL knows; unknown pads get a mapping wizard on the tube |

That is the machine everything here was written on. Nothing in the code is
tied to it, but nothing else has been tried either, so this is what to expect
elsewhere:

| Part | Where it should work | Where it will not |
| --- | --- | --- |
| GPU | Any AMD card on `amdgpu`: the lease and the 15 kHz timings are kernel side, not vendor side | Nvidia's proprietary driver does not offer non-desktop connectors for leasing. Intel is untested |
| DAC | Anything that takes HDMI and puts RGB on a SCART or VGA pin. `omacrt setup` recognises the RGB-Pi 2 from its EDID | Sync mode is set over I2C only on the RGB-Pi 2; on anything else set it on the device itself |
| Television | Any 15 kHz set with RGB in, PAL or NTSC. The standard follows your locale, `--standard` overrides it | A VGA monitor: 15 kHz is below what it will lock onto |
| Compositor | Hyprland 0.56 or later, which is what Omarchy ships | Anything without DRM leasing. wlroots, KWin and Mutter all have the protocol, none of them has been tried |
| Desktop | Omarchy for the bar widget, the library overlay and the menu entry | Plain Hyprland gets the television and the command line: [`docs/hyprland.md`](docs/hyprland.md) |

Run `omacrt setup` first on a machine that is not this one: it lists the
connectors with what their EDID says, picks the one the DAC is on, works out
the standard from the locale, and writes those two lines to `crt.toml`.
`omacrt doctor` says what is still missing. The DisplayPort DAC tier, for
native 320x240 timings rather than the wide ones, is in
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

Requirements, and the distribution is not one of them: an AMD card on
`amdgpu`, Hyprland 0.56 or later (the Lua configuration), systemd for the boot
time override, RetroArch with libretro cores, mpv, cliamp (a terminal music
player), yt-dlp for YouTube, ffmpeg, curl, a stable Rust toolchain. What
decides whether a machine can run this is the card, the compositor and the
DAC, not what is on the rest of the disk. `omacrt doctor` says which of them
this machine has, in its own words, before anything is bought.

After `--system` the tube is handed over at every boot, and the first boot
after it is when the handover starts working: Hyprland decides which
connectors it offers for leasing when it starts, so the override has to be in
place before it is. Set `shell.autostart = true` in
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

Every verb, with its flags, is in [`docs/cli.md`](docs/cli.md). `shell key`,
`shell type` and `shell screen` drive the launcher over its control pipe,
which is how every screenshot and video in this repository was made.

## Repository layout

| Path | What |
| --- | --- |
| `shell/` | The Rust workspace: `omacrt-shell` (launcher), `omacrt` (CLI), `omacrt-display` (lease and compositor), shared library |
| `plugin/` | The bar widget and panel; `plugin/library/` the library overlay |
| `menu/` | The Television rows the installer writes into Omarchy's menu extension file |
| `bin/omacrt-install` | Build and install everything, `--system` for the boot time lease |
| `bin/omacrt-pick` | Pick a game with the desktop's runner, play it on the tube |
| `scripts/` | The EDID override and lease setup, DRM probing, the offline demo renderer, the video takes and montage |
| `systemd/` | The oneshot unit that hands the tube over at boot |
| `docs/` | The 15 kHz study, what is on the tube screen by screen, what Omarchy lends it, the same without Omarchy, hardware notes, systems and video policy, controllers, CLI, troubleshooting |
| `packaging/` | The Arch `PKGBUILD` and what it installs where |
| `THIRD-PARTY.md` | Everything here that somebody else wrote, and under what terms |
| `.github/workflows/` | The build, the lints, the tests and a headless render of the launcher's own frames |

## State

[`CHANGELOG.md`](CHANGELOG.md) keeps what has landed. In short: the tube is
ours, games, music and video run on it, the collection is managed from the
bar, and the interlaced timings are there for a kernel that can scan them.

## Contributing

This is one person's television, given away because it turned out well: not a
product, and worked on when it is fun to work on. A good change is still
welcome, and there is one rule that is not negotiable, because half of this
cannot be checked any other way: **it has to have run on a real television**,
and the pull request has to say what it ran on.

- [`CONTRIBUTING.md`](CONTRIBUTING.md): what gets merged, what does not, the
  standards, and the commands CI runs.
- [`AGENTS.md`](AGENTS.md): the working guide. The modules, how to render a
  screen without a television and look at it, how to add a screen, a console,
  a setting or a CLI verb, and the rules that do damage when they are broken.
  Written for a coding agent and fine for a person.
- [`SECURITY.md`](SECURITY.md): what runs as root, what is downloaded and from
  where, what is executed, and the one credential that can exist.
- [`docs/troubleshooting.md`](docs/troubleshooting.md): what has gone wrong so
  far and what it turned out to be.

Bugs are issues, with the template filled in and the output of `omacrt
doctor`. Ideas and questions are discussions.

## Credits and licenses

Every piece of this that somebody else wrote, and under what terms, is in
[`THIRD-PARTY.md`](THIRD-PARTY.md). The one that needs saying here: the
wordmark spells OMACRT with the letterforms of Omarchy's own drawing, which
are Copyright (c) David Heinemeier Hansson, MIT, with the licence shipped
beside it at
[`shell/assets/LICENSE.omarchy`](shell/assets/LICENSE.omarchy). Omarchy's icon
is not here at all: the mark in the boot and on every inner page is the
project's own. OmaCRT is a fan project, not part of Omarchy, not endorsed by
the Omacom Foundation, and speaks for neither.

License: MIT, and it covers the pictures and the documentation as well as
the code. Reusing the wordmark means carrying Omarchy's notice with it:
[`LICENSE-SCOPE.md`](LICENSE-SCOPE.md) says what that means in practice.
