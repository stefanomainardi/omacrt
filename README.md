<p align="center">
  <img src="docs/logo.png" alt="Omarchy CRT" width="504">
</p>

# Omarchy CRT

An [Omarchy](https://omarchy.org) PC plugged into a 15 kHz CRT television over
RGB SCART, playing retro games the way they were drawn: native lines, native
refresh, real scanlines, no scaler in between. A launcher on the tube in the
Omarchy look, a bar plugin on the desktop, and a display process that owns the
television outright. Written in Rust, drawn at 320x240.

> **Three things to know before you read on.**
>
> **This project is written with an AI.** Design, code, tests and this very
> README are the work of a human directing Claude, commit after commit, on a
> real television in a real living room. If a codebase built that way is not
> for you, no hard feelings: there are many other repositories.
>
> **This project is for Omarchy.** It leans on Omarchy's shell, bar, theme
> files, plugins and music player on purpose. It is not a generic Linux CRT
> frontend and will not become one. If Omarchy is not your thing, this is not
> either.
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
> Omarchy CRT is a fun project by one user. It is not affiliated with, endorsed
> by or part of the official Omarchy project. Omarchy, RetroArch, RGB-Pi and
> every other name here belong to their owners.
>
> **Tested on one setup so far:** Omarchy 4 with Hyprland 0.56, an AMD Radeon
> RX 7700/7800 XT, an RGB-Pi 2 DAC and a Bang & Olufsen BeoCenter 1. Other
> GPUs, DACs and televisions are uncharted; the code is written to cope, the
> author has not seen them work.

## At a glance

- **The television is a client of its own compositor.** The desktop hands the
  DAC's connector over at boot; `omarchy-crt-display` sets the 15 kHz timing
  and runs the tube. Nothing from the desktop can land on it.
- **Games.** A collection indexed from any disk in any folder layout, box
  art, console pictures, a cover flow, search across everything, letter
  jumps, pads mapped on the tube, save states in sight, a pause menu over the
  game, the line count of each system set live.
- **Music.** cliamp as the engine: radio by country and genre, Spotify (and
  any provider cliamp knows) with search, a hi-fi deck with cassette,
  turntable and VU meters, album art and station logos, seven visualizers,
  synced lyrics, a ten band equaliser, sleep timer, all tuned from a Settings
  page on the tube.
- **Video.** Local films fitted to the tube, YouTube searched and played from
  the television, a link sent from the desktop.
- **Omarchy all the way.** Theme colours, the bar widget and its panel, a full
  screen library overlay, the shell's plugin system, cliamp, mpv, yt-dlp: the
  desktop's own tools drive the CRT.
- **One CLI for everything**, a control pipe to drive the launcher from a
  script, screenshots and recordings straight from the tube's framebuffer.

## Why

A television that marked a couple of generations sits in a corner, still
perfect at what it was built for, and a modern PC can feed it the exact
signal its tube was made to show. This project is a way to learn, in the
open, how that signal, the kernel, a compositor, an emulator and a music
player fit together, and to give the old screen a second life that is both
useful and fun: the games look the way they were drawn, the radio sounds like
a radio, an evening's video plays without a scaler in between.

It is also a playground. A system this malleable invites experiments the
big frontends never bothered with: a cassette that turns with the song, a
Mode 7 floor under the spectrum, covers on a shelf that reflect on the floor,
a laser etching the wordmark at boot. Some of it is nostalgia, some of it is
just the pleasure of drawing pixels at 320x240 and seeing them glow on
glass. Both are the point.

## What it looks like

<p align="center">
  <img src="docs/screens/boot.gif" width="560" alt="The launcher booting on the tube">
</p>
<p align="center">
  <img src="docs/screens/coverflow.png" width="270" alt="Cover flow on the tube">
  <img src="docs/screens/systems.png" width="270" alt="Systems with console pictures">
  <img src="docs/screens/pause.png" width="270" alt="Pause menu over a game">
  <img src="docs/screens/deck-radio.png" width="270" alt="The music deck tuned to a radio">
  <img src="docs/screens/equalizer.png" width="270" alt="The ten band equaliser">
  <img src="docs/screens/visual-mode7.png" width="270" alt="Mode 7 equalizer visualizer">
</p>

Every picture above was captured from the tube's own framebuffer by
`omarchy-crt shot`, and the boot by `omarchy-crt record`; the television adds
the scanlines.

## How it works

<p align="center">
  <img src="docs/architecture.png" width="720" alt="Omarchy CRT architecture, drawn as a 16 bit illustration">
</p>

The same thing as a flowchart, for the parts a picture cannot hold:

```mermaid
flowchart LR
  subgraph desktop["Omarchy desktop (Hyprland)"]
    bar["Bar plugin<br/>Quickshell panel + library overlay"]
    cli["omarchy-crt<br/>CLI"]
    cliamp["cliamp --daemon<br/>music engine"]
    bar --> cli
  end

  subgraph tube["The tube (leased DRM connector)"]
    display["omarchy-crt-display<br/>own Wayland compositor (smithay)<br/>sets 15 kHz modelines through DRM"]
    shell["omarchy-crt-shell<br/>launcher, 320x240"]
    ra["RetroArch"]
    mpv["mpv"]
    display --- shell
    display --- ra
    display --- mpv
  end

  cli -- "on / off / mode<br/>lease + hotkeys" --> display
  cli -- "control pipe:<br/>keys, type, watch" --> shell
  shell -- "launch, pause menu<br/>(hotkeys pressed by the compositor)" --> ra
  shell -- "JSON IPC" --> mpv
  shell -- "Unix socket IPC<br/>status, spectrum, lyrics" --> cliamp
  display -- "HDMI, 3520x240 @ 15.73 kHz<br/>+ audio" --> dac["RGB-Pi 2 DAC<br/>csync over I2C"]
  dac -- "RGB SCART" --> tv["CRT television"]
  shell -. "covers, radio directory,<br/>YouTube via yt-dlp" .-> net["Internet"]
```

The desktop never touches the television. At boot a systemd unit installs an
EDID override that marks the DAC's connector *non-desktop*, so Hyprland leaves
it alone and offers it through the DRM lease protocol. `omarchy-crt-display`
takes the lease, programs the 15 kHz timing straight into the kernel and runs a
small Wayland compositor of its own on that output. The launcher, RetroArch and
mpv are its clients, forced fullscreen at the output's size. No bar,
notification, pointer or stray window can reach the tube, and any timing the
kernel accepts is one command away, live, including a different line count per
system (224 lines for a Super Nintendo game, 240 for a NES one).

The bar plugin and the CLI stay on the desktop and talk to the tube over a
control pipe: the panel is the remote control, the overlay manages the
collection, the CLI does everything from a terminal.

## Omarchy on the tube

The interesting part is not the emulator, it is what an integrated desktop
can send to a television once the television is just another output it owns.

- **The shell.** The bar widget, the panel and the library overlay are
  Omarchy shell plugins, in Quickshell like the rest of the bar, using the
  same components, colours and popout behaviour. A keybinding can talk to them
  through `omarchy-shell` IPC (`power`, `on`, `off`, `ntsc`, `pal`, `focus`,
  `library`).
- **Themes.** The launcher reads Omarchy's theme colours and offers every
  installed theme; switching one blends the whole screen, icon and wordmark
  included.
- **cliamp.** Omarchy's music player runs as a daemon and the launcher is its
  face on the CRT over a Unix socket: radio through the Radio Browser
  directory it ships, Spotify, YouTube Music and the other providers set up
  once with `cliamp setup`, its spectrum analyser feeding the visualizers, its
  lyrics on the screen. What plays on the desktop can play on the tube and
  the other way round.
- **mpv and yt-dlp.** Local films, a YouTube search typed on the tube, a
  link copied on the desktop and sent with one click from the panel or with
  `omarchy-crt watch`, all through the same player with the tube's own fit
  pipeline (480p streams, 480i or 576i by frame rate when the modeline lands).
- **RetroArch.** Driven without its menu: a configuration written per launch
  from `systems.toml`, hotkeys pressed by the compositor, save states read
  back for the launcher's rows.
- **The rest of the box.** PipeWire routes the launcher, RetroArch, mpv and
  cliamp to the television's audio and back; the I2C bus of the HDMI port
  configures the DAC's sync; systemd hands the tube over at boot; pads come
  through SDL with a wizard for the unknown ones.

## Hardware

| Part | What worked |
| --- | --- |
| GPU | AMD Radeon RX 7700/7800 XT, stock Omarchy kernel |
| DAC | [RGB-Pi 2](docs/rgb-pi-2.md), HDMI in, SCART RGB out, composite sync selected over I2C, audio on SCART |
| Television | Bang & Olufsen BeoCenter 1, RGB SCART |
| Pads | Anything SDL knows; unknown pads get a mapping wizard on the tube |

The HDMI path works with wide "super resolution" modelines (3520x240 at 72 MHz,
15.73 kHz, 60.04 Hz for NTSC; 3840x288 for PAL). The tube turns the wide frame
back into 4:3, the emulator fills it, and every game line lands on one TV line.
A DisplayPort DAC tier (Realtek RTD2166 adapters plus a VGA to SCART sync
combiner) for native 320x240 timings is documented in
[`docs/studio-15khz.md`](docs/studio-15khz.md) (Italian) and has not been
needed so far.

## Install

```sh
git clone https://github.com/stefanomainardi/omarchy-crt.git
cd omarchy-crt
bin/omarchy-crt-install                # builds, installs to ~/.local/bin, installs both plugins
sudo bin/omarchy-crt-install --system  # once: the boot time EDID override that hands the tube over
omarchy-crt library scan ~/Games       # index your collection, any folder layout
omarchy-crt on                         # tube on: 15 kHz timing, DAC sync, audio, launcher
```

Requirements: Omarchy with Hyprland 0.56 or later (the Lua configuration),
RetroArch with libretro cores, mpv, cliamp (Omarchy's music player), yt-dlp
for YouTube, ffmpeg, curl, a stable Rust toolchain. `omarchy-crt doctor`
tells what is missing.

After `--system` the tube is handed over at every boot. Set
`shell.autostart = true` in `~/.config/omarchy-crt/crt.toml` and the
television boots straight into the launcher along with the desktop.

## The launcher

A 320x240 framebuffer drawn sixty times a second, no shader faking a tube. The
theme comes from Omarchy's own colors; every installed theme is available and
switches with a blend.

- **Boot.** Power surge and roll, a BIOS style POST with live data, the icon
  revealed band by band, a chime, the wordmark etched by a laser
  (TerminalTextEffects' `laseretch`, ported pixel by pixel), then the CRT tag
  slams in SNES title screen style over a Mode 7 floor. Both logos glint
  every few seconds afterwards.
- **Games.** Systems with console pictures, games with box art from the
  libretro thumbnails (matched by title when the file names carry no region
  tags, so a RePlayOS style set gets its covers too; `omarchy-crt library
  covers` fetches them all at once), collections, favourites, recent. Arcade
  files named after the emulated set, `mslug` for Metal Slug, are read
  through the databases RetroArch ships, so those lists show titles and find
  their covers as well. `X`
  opens the **cover
  flow**: the selected cover large on a shelf, the neighbours receding at an
  angle, everything mirrored on the floor, sliding with inertia.
- **Search.** `/` filters the open list as you type; from the home menu it
  searches the whole collection, tens of thousands of titles answering within
  a frame. Pads get
  the same with the left trigger and an on screen keyboard, and jump letter
  by letter with the shoulder buttons.
- **Launch ritual.** A cartridge slides in (a disc spins up for CD systems),
  scrape and click, the picture collapses to a line, the emulator takes over
  with the line count the system wants. RetroArch runs with its own menu and
  notifications off, save state on exit and resume on start.
- **Resume or start again.** A game left in the middle asks which it is to be:
  carry on from the state written on exit, or a new session that leaves that
  state alone.
- **Pause menu.** Select + Start, the home button, or F1: resume, save state,
  load state, rewind, fast forward, slow motion, reset, back to the launcher. The compositor presses
  RetroArch's real hotkeys, so nothing depends on a network command. Every
  game with a save state carries a small arrow and says when it was left.
- **Music.** On top of cliamp, started as a daemon when needed: the radio
  stations of your country, every country and genre of the Radio Browser
  directory, favourites, history, and any provider set up in cliamp (Spotify,
  YouTube Music, Qobuz, Plex, Jellyfin). The now playing screen is a **hi-fi
  deck**: a cassette whose reels turn with the music, or a radio dial whose
  needle glides to the station through a burst of static, two VU meters with
  inertia. Six idle seconds later the screen becomes a **visualizer** driven
  by cliamp's spectrum and a kick detector: Mode 7 equalizer, silk ribbons,
  oscilloscope, starfield, plasma, pixel fire, spectrum tower. Synced lyrics
  when cliamp has them, a sleep timer, and the selection band of every list
  breathing with the beat. The album art of a Spotify track and the logo of a
  radio station arrive on the cassette label, the record label and the dial.
  An **equaliser** page moves the engine's ten bands one decibel at a time,
  or takes one of its presets.

<p align="center">
  <img src="docs/screens/music.png" width="270" alt="The music screen">
  <img src="docs/screens/visual-ribbons.png" width="270" alt="Silk ribbons visualizer">
  <img src="docs/screens/youtube.png" width="270" alt="YouTube search on the tube">
</p>
- **Videos.** Local films through mpv with a themed on screen display and a
  fit pipeline for the tube (480i or 576i by frame rate, pulldown or PAL
  speed-up for film, letterbox or crop, a safe area, a retro 240p mode).
  **YouTube** on the television: search from the tube, watch later, recently
  watched, the link in the clipboard, or `omarchy-crt watch URL` from a
  terminal; streams are fetched at 480p, all a 240 line tube can show.
- **Pads.** SDL's database plus a wizard on the tube for the pads it does not
  know: press each control once and it is mapped for good.
- **Sound.** Every click, whoosh, crackle and scrape is synthesized at
  startup. No background music of its own.

Keyboard and pad bindings are in [`docs/input.md`](docs/input.md); the
launcher's flags and offline rendering in [`shell/README.md`](shell/README.md).

## The bar plugin

<p align="center">
  <img src="docs/screens/panel.png" width="300" alt="The bar panel">
  <img src="docs/screens/library-overlay.png" width="540" alt="The library overlay">
</p>

A television glyph in the Omarchy bar shows whether the tube is on the air
and at how many lines. The panel is the remote control: power, NTSC or PAL,
the keyboard to the launcher, DAC sync mode, audio to the TV, TV volume, a
field to send a link to the television. The **Library** overlay manages the
collection full screen: the folders the scan reads, disks that look like
collections with one click "adopt and scan", every system with its games and
core (a missing core offers its package, from the repositories or the AUR),
the BIOS files the cores expect with import from any folder that holds them,
and the folders the scan could not place with a system picker.

See [`plugin/README.md`](plugin/README.md).

## The CLI

Everything the plugin and the launcher do can be typed:

```text
omarchy-crt on | off | status | mode ntsc|pal|film [--lines N]
omarchy-crt shell start|stop|restart | shell key <input>... | shell type <text>
omarchy-crt shot out.png | record start out.mp4 | record stop | monitor on|off
omarchy-crt game menu|pause|save|load|reset|quit
omarchy-crt watch <file|url> [--later [TITLE]]
omarchy-crt library scan|discover|covers|cores|set|assign|unknown | bios [import DIR|discover]
omarchy-crt audio crt|desktop|all|apps | audio volume N | dac csync and|xor | doctor | config set KEY VALUE
```

Details in [`docs/cli.md`](docs/cli.md). `shell key` and `shell type` drive
the launcher over its control pipe, which is how every screenshot and video in
this repository was made.

## The library scans anything

Point `omarchy-crt library scan` at a disk and it works out what every file
is: extension, the words in the folder names, disc image signatures, the
names inside zips. No renaming, no fixed folder scheme. Regional variants
collapse onto one title and systems show up when they have games. Details in
[`docs/systems.md`](docs/systems.md), the display mode per system in
[`docs/video-policy.md`](docs/video-policy.md).

## Repository layout

| Path | What |
| --- | --- |
| `shell/` | The Rust workspace: `omarchy-crt-shell` (launcher), `omarchy-crt` (CLI), `omarchy-crt-display` (lease and compositor), shared library |
| `plugin/` | The bar widget and panel; `plugin/library/` the library overlay |
| `bin/omarchy-crt-install` | Build and install everything, `--system` for the boot time lease |
| `scripts/` | The EDID override and lease setup, DRM probing, the offline demo renderer, the video takes and montage |
| `systemd/` | The oneshot unit that hands the tube over at boot |
| `docs/` | The 15 kHz study, hardware notes, systems and video policy, controllers, CLI, troubleshooting, state of the project |

## State and what is next

[`docs/plan.md`](docs/plan.md) keeps the current state. In short: the tube is
ours, games, music and video run on it, the collection is managed from the
bar, and the interlaced modes are in. Next: aspect and shader choices in the
pause menu, more screensaver effects.

## Contributing

Work lands on `develop` and is merged to `main` when it runs on the
television. Commits follow Conventional Commits. `cargo build --release` in
`shell/` builds everything; `cargo test` runs the unit tests. Read the two
notes at the top before opening an issue about either.

## Credits and licenses

- Omarchy icon and wordmark: Omacom Foundation, MIT. Used here as the theme
  of a fan project; Omarchy CRT is not part of Omarchy.
- `font8x8` bitmap font: Daniel Hepper, public domain, after the IBM VGA fonts.
- `laseretch` and the effect catalog: inspired by
  [TerminalTextEffects](https://github.com/ChrisBuilds/terminaltexteffects) by
  ChrisBuilds.
- Box art: the [libretro thumbnails](https://github.com/libretro-thumbnails)
  repositories. Console pictures: RetroArch's `systematic` assets (CC BY).
- Radio directory: [Radio Browser](https://www.radio-browser.info/). Music
  engine: [cliamp](https://github.com/bjarneo/cliamp) by bjarneo.
- 15 kHz kernel patches: Calamity and D0023R. Switchres and GroovyMAME:
  Antonio Giner and the GroovyArcade community.
- Hardware research: the Batocera CRT Script wiki by ZFEbHVUE and the
  RetroRGB, shmups and arcadecontrols communities.
- Launcher flow (systems, then games, RetroArch without its menu): inspired
  by the GPL frontend of RGB-Pi OS by rtomasa; the display mode policy is our
  own on top of upstream RetroArch CRT SwitchRes.

License: MIT.
