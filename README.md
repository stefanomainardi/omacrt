<p align="center">
  <img src="docs/logo.png" alt="The OmaCRT mark, four bars crossed by the dark cut of a beam returning, beside the word OMACRT in block letters" width="560">
</p>

<h1 align="center">OmaCRT</h1>

<p align="center">
  <a href="https://omacrt.com">omacrt.com</a>
</p>

**What it is.** A Wayland compositor that takes one output away from the
desktop and drives a 15 kHz CRT television on it. Your other screens keep
working.

**What it's for.** Emulated games on a real tube at native 240p, with the
frame timing written for a CRT. A client's commit reaches the start of scanout
in 3.79 ms. A button press reaches the picture in 10.19 ms, of which 8.33 is
the half frame nobody can remove.

**When you don't need it.** The 15 kHz kernel patches let your desktop drive a
CRT as one of its outputs. If that suits you, use them. This takes the
connector away from the desktop instead, so nothing of the session can land on
the tube.

**What it costs.** Nothing. MIT, no account, no telemetry.

**What state it is in.** Experimental. Stock kernel, AMD only, one television.

Native lines, native refresh, real scanlines, no scaler in between. A launcher
on the tube, and on [Omarchy](https://omarchy.org) a bar plugin and a menu
entry on the desktop as well. Written in Rust, drawn at 320x240.

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
signal, the kernel, a compositor, an emulator and a music player fit together.
The old screen gets a second life out of it: the games look the way they were
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
it alone and offers it through the DRM lease protocol. **Flyback**, this
project's own Wayland compositor, takes the lease, programs the 15 kHz timing
straight into the kernel and owns the scanout of that output. It is named
after what a tube does between two lines. The launcher, RetroArch
and mpv are its clients, forced fullscreen at the output's size. No bar,
notification, pointer or stray window can reach the tube. Any timing the
kernel accepts is one command away, live, including a different line count per
system: 224 lines for a Super Nintendo game, 240 for a NES one.

The bar plugin and the CLI stay on the desktop and talk to the tube over a
control pipe: the panel is the remote control, the overlay manages the
collection, the CLI does everything from a terminal.

The same thing with every process and channel named is a flowchart in
[`docs/architecture.md`](docs/architecture.md); the study behind the timings
is [`docs/15khz.md`](docs/15khz.md).

## Flyback

<p align="center">
  <img src="docs/flyback-logo.png" width="460" alt="The Flyback mark, four bars stepping out beside the full-height stroke of the beam flying back, beside the word Flyback">
</p>

A Wayland compositor that owns the scanout of a fifteen kilohertz television.
One process, one thread, one event loop, 3616 lines on smithay. It is named
after what a tube does between two lines, and it is the piece of this project
that touches hardware.

A kiosk compositor puts one window on a monitor the system already knows how
to drive. This one takes a connector the desktop has been told to leave alone,
programs a timing no desktop would set, and schedules every frame for a
cathode ray tube. The desktop keeps running on its other outputs. GroovyArcade
and Batocera take the whole machine; this takes one connector.

Owning the scanout is also what lets it answer a question nobody else here
can. Flyback times every frame from the client's own commit to the kernel's
vblank timestamp. On a set with no panel and no scaler between the connector
and the phosphor, that second moment is very nearly the picture on the
glass. Measured on a BeoCenter 1 through an RGB-Pi 2, with the launcher
mapped underneath, the way the television actually runs:

| | |
| --- | --- |
| commit to the start of scanout, client drawing 1 ms | **3.79 ms** |
| the launcher end to end, in `omacrt status` | **2.0 ms** |
| the same compositor with all three of its scheduling decisions removed | **33.4 ms**, two frames exactly |
| a button press to the start of scanout, 300 presses at random points of the frame | median **10.19 ms**, 0.61 of a frame |

It offers `wp_presentation`, so a client that cares about timing is told
instead of left guessing, and it runs the television at a **variable refresh
rate**: every refresh an emulation asks for is reachable by stretching the
vertical blanking alone, with the line rate never moving, so a program gets
its own rate without the fifth of a second of darkness a mode change costs.
How far a set follows that is a property of the set, measured from film and
written down as one number in `crt.toml`.

**It is experimental**, and it sets the line rate of a television, a circuit
tuned for one rate and under no obligation to work at another. No timing
reaches the kernel without passing a guard that checks the line rate against
the band `crt.toml` allows for that set, 15 to 16.5 kHz as shipped, the field
rate between 40 and 90 Hz, and every timing in order.

| | |
| --- | --- |
| [`docs/flyback.md`](docs/flyback.md) | what it is: the lease, the EDID, the scheduler, the variable rate, what it costs, what it deliberately does not do, and where it sits beside GroovyArcade, Batocera and a MiSTer |
| [`docs/flyback-manual.md`](docs/flyback-manual.md) | how to drive it: every line the control pipe answers to, the configuration it reads, the switches that turn each decision off, and what to do when it will not come up |
| [`docs/comparison.md`](docs/comparison.md) | the same comparison on its own |
| [`docs/sets.md`](docs/sets.md) | the televisions this has been pointed at, one so far. If you have a set, that page is the ask |

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
| Compositor | Hyprland 0.56 or later, and that is what Omarchy ships | Anything without DRM leasing. wlroots, KWin and Mutter all have the protocol, none of them has been tried |
| Desktop | Omarchy for the bar widget, the library overlay and the menu entry | Plain Hyprland gets the television and the command line: [`docs/hyprland.md`](docs/hyprland.md) |

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

Requirements, and the distribution is not one of them: an AMD card on
`amdgpu`, Hyprland 0.56 or later (the Lua configuration), systemd for the boot
time override, RetroArch with libretro cores, mpv, cliamp (a terminal music
player), yt-dlp for YouTube, ffmpeg, curl, a stable Rust toolchain. What
decides whether a machine can run this is the card, the compositor and the
DAC, whatever else is on the disk. `omacrt doctor` says which of them this
machine has, in its own words, before anything is bought.

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

Every verb, with its flags, is in [`docs/cli.md`](docs/cli.md). `shell key`,
`shell type` and `shell screen` drive the launcher over its control pipe, and
every screenshot and video in this repository was made.

### omacrt doctor

<p align="center">
  <img src="docs/screens/doctor-full.gif" width="620" alt="The whole self test in a terminal: the OmaCRT wordmark cut out of the dark by a laser one branch at a time, the mark under it with the beam running back across its four bars, the checks landing one line at a time, and then the finished report">
</p>

The command to run before buying anything. Its first four answers decide
whether a machine can drive a television at all: which driver the card is on,
whether the compositor offers DRM leasing and for which connector, whether
systemd is there for the boot time override, and whether debugfs is mounted.
None of them needs a DAC to be plugged in.

In a terminal it is the launcher's own power on self test. The mark and the
wordmark stand side by side, the way the boot screen holds them. The word is
cut by the same laser while the machine is being asked, which is the slow part.
The beam runs back across the mark's four bars every time one of those answers
lands. Then the rest of the answers arrive, one line at a time as they come,
and last the modeline drawn rather than listed, the card's outputs and what
each is for, and the DAC's lock as a lamp. It is drawn with
[ratatui](https://ratatui.rs), inline rather than on the alternate screen, so
the report stays in the scrollback where it can be read again and pasted into
an issue. Piped, redirected, under `NO_COLOR`, or with `--plain`, it prints the
lines it always printed.

## Repository layout

| Path | What |
| --- | --- |
| `shell/` | The Rust workspace: `omacrt-shell` (launcher), `omacrt` (CLI), `flyback` (the compositor that owns the tube), shared library |
| `plugin/` | The bar widget and panel; `plugin/library/` the library overlay |
| `menu/` | The Television rows the installer writes into Omarchy's menu extension file |
| `bin/omacrt-install` | Build and install everything, `--system` for the boot time lease |
| `bin/omacrt-pick` | Pick a game with the desktop's runner, play it on the tube |
| `scripts/` | The EDID override and lease setup, DRM probing, the offline demo renderer, the video takes and montage |
| `systemd/` | The oneshot unit that hands the tube over at boot |
| `docs/` | The 15 kHz study, [the compositor](docs/flyback.md) and [how to drive it](docs/flyback-manual.md), [how it sits beside the others](docs/comparison.md) and [what re-checking every published number found](docs/audit-2026-09-12.md), [how the mark was drawn](docs/identity.md), what is on the tube screen by screen, what Omarchy lends it, [the same without Omarchy](docs/hyprland.md), hardware notes, systems and video policy, controllers, CLI, troubleshooting |
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
welcome. One rule is not negotiable, because half of this cannot be checked any
other way: it has to have run on a real television, and the pull request has to
say what it ran on.

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
