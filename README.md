# omarchy-crt

Plug an [Omarchy](https://omarchy.org) PC into a 15 kHz CRT television over
RGB SCART and play retro games the way they were drawn: native resolutions,
native refresh rates, real scanlines, no scaler in between.

The project has two halves. A hardware and kernel recipe that gets a 15 kHz
signal out of a modern AMD GPU, and a native boot screen and launcher that
brings the Omarchy look to a 320x240 tube, in the spirit of
[crt.omarchy.org](https://crt.omarchy.org/).

Status: early prototype. The launcher runs in a desktop window, browses a
ROM library and starts games through RetroArch; the first real CRT test is
pending hardware. Development happens on `develop`, `main` holds what works.

## Why

Emulators can already output 240p on Linux. What is missing is an opinionated,
Omarchy-flavoured setup that a user can install in one step: the right cable,
the right kernel, a boot entry that does not touch the daily desktop, and a
launcher that feels like a console rather than a file manager. This repository
collects the research and builds that setup.

## What "no compromises" means here

- **Native modelines per game.** 224, 240, 256 or 288 lines, 60.00, 59.94, 57.5
  or 50 Hz, chosen by the emulator at launch and on the fly, not one fixed mode
  for everything.
- **Real interlace.** 480i and 576i for the systems that used them.
- **Analog RGB out of a modern GPU.** A DisplayPort DAC that accepts pixel
  clocks down to a few MHz, into a SCART TV with proper composite sync and
  blanking voltage.
- **The desktop stays untouched.** A separate kernel and boot entry carry the
  15 kHz patches. The default Omarchy kernel and Limine entry never change.

## Signal chain

```text
GPU (DisplayPort) -> DAC (Realtek RTD2166/2168) -> sync combiner (VGA to SCART RGBS) -> CRT TV
```

- **DAC.** Only adapters built on the Realtek RTD2166 or RTD2168 are known to
  pass the low pixel clocks 15 kHz needs. HDMI is out for native 320x240: its
  25 MHz floor makes it impossible. Validated units: CableDeconn DP to VGA
  (non 4K, no audio), Cable Matters 102026, biaze ZH277.
- **HDMI tier.** An HDMI to SCART DAC such as the RGB-Pi 2 works with wide
  "super resolution" modelines (3520x240 at 72 MHz) and carries audio, but
  needs its sync combiner configured over I2C first. See
  [docs/rgb-pi-2.md](docs/rgb-pi-2.md) and `omarchy-crt dac`. First light
  on the BeoCenter 1 came this way on 2026-09-06.
- **The tube is ours.** The DAC's connector is marked non-desktop (an EDID
  override installed at boot by `omarchy-crt-lease.service`), so the desktop
  compositor leaves it alone and offers it through the DRM lease protocol.
  `omarchy-crt-display` takes the lease, sets the timing straight through
  DRM and runs a small Wayland compositor of its own on that output; the
  launcher, RetroArch and mpv are its clients. No desktop bar, notification,
  pointer or window can reach the tube, and any timing the kernel accepts
  is available, interlace included. Working since 2026-09-07.
- **Sync and SCART.** The VGA H and V sync must be combined into composite sync
  at 0.3 to 1 V, and the TV needs 1 to 3 V on SCART pin 16 to switch to RGB.
  Preferred: VideoAmp (also emulates an EDID), then UMSA, sirMagb F-15,
  VGA2SCART. Passive VGA to SCART cables and MiSTer cables do not work.
- **Kernel.** `amdgpu` refuses low dot clocks and mishandles interlace without
  the 15 kHz patch set maintained in
  [D0023R/linux_kernel_15khz](https://github.com/D0023R/linux_kernel_15khz).
  The plan is a `linux-crt` package built from `linux-lts` with those patches,
  living next to the stock kernels through Omarchy's Limine and UKI setup.
- **Lines per system.** On the HDMI tier the launcher asks `omarchy-crt` to
  switch the tube to a system's pinned line count before a game starts (224
  lines for Super Nintendo) and back to the full frame after, so pixels land
  one line per line without a kernel patch.
- **Mode switching.** With the connector leased, modelines are set by our
  own display process through DRM, live (`omarchy-crt mode`, or the launcher
  before each game), with no compositor in between. The separate KMS
  session with Switchres remains the plan for per-game timings on the
  DisplayPort tier.

The full study, with sources and the verification plan, is in
[`docs/studio-15khz.md`](docs/studio-15khz.md) (Italian).

## The launcher: `shell/`

A Rust and SDL2 program that renders a 320x240 framebuffer and shows it
fullscreen on the CRT, or in a scaled window while developing. It does not fake
scanlines or curvature. The tube provides those.

- **Boot sequence.** Power surge and vertical roll, a BIOS style POST with live
  data (host, kernel, video mode, theme), the Omarchy icon revealed band by
  band, a systems-online chime, and the wordmark etched by a laser.
- **Laser etch.** A pixel port of the `laseretch` effect from
  TerminalTextEffects, the same effect crt.omarchy.org runs: a depth-first
  random walk decides the etch order, cells flash and cool from yellow to their
  final gradient color, sparks fly along Bezier arcs and pile up on the
  baseline.
- **CRT tag.** Introduced SNES title screen style: a Mode 7 checkerboard
  floor rushes toward the viewer, giant CRT letters rise from the horizon
  spinning in fake 3D, slam into the foreground with a shake, a copper bar and
  a burst of dust, then shrink and fly to their spot under the wordmark. Riser,
  slam, chord stab and landing bells come from the same timeline.
- **Menu.** Omarchy style: a vertical list with pixel icons, a selection band
  in the theme's `selection` color, accent colored text, chevrons for
  submenus, slide-in transitions and a whoosh. Home: Games, Favorites, Recent,
  Settings, About, Power. Settings holds the TV profile, pads, the
  screensaver (on/off, idle time, effect), Style and a diagnostics page;
  About explains the goals and credits; Power goes back to the desktop or
  powers off after a confirming second press.
- **Style.** Every installed Omarchy theme, previewed live as the cursor
  moves: icon, wordmark, bands and text blend to the new palette in a third of
  a second. `system` follows the desktop theme.
- **Launch ritual.** Selecting a game slides a cartridge into its slot (or
  spins a disc up for CD systems) with a scrape and a click, then the picture
  collapses to a line and the emulator takes over. System logos in the Games
  list carry each console's signature color.
- **Videos.** A `Videos` entry plays any folder of films through mpv with the
  shell still in charge: keyboard and pad controls (pause, seek, volume, stop),
  a themed on screen display drawn by mpv itself (title, progress, times,
  state, volume, hints) that appears on every command and stays while paused,
  position saved on quit. Deinterlacing stays off so 480i sources reach the
  tube as fields.
- **A console from login.** `shell.autostart = true` in `crt.toml` makes the
  login reset switch the tube on when the DAC is connected: the television
  boots into the launcher with the desktop.
- **Details.** Region and revision tags under the box art, when a game was
  last played, folder breadcrumbs in the header, a marquee for long titles,
  the favourites star.
- **Any pad.** Pads SDL knows just work; an unknown one gets a button by
  button wizard on the tube the moment it is plugged in, and the mapping is
  kept for next time. `X` on the Pads screen maps the current pad again.
- **Save states in sight.** RetroArch saves on exit and resumes on start; the
  launcher shows it: a small arrow on every game that has a state, "left
  12:03" or "saved Sat 21:10" under the box art, the same line when the pause
  menu opens. The pause menu (Select + Start, or the home button) offers
  resume, save state, load state, fast forward, reset and back to the
  launcher, all through real hotkeys pressed by the compositor.
- **Search.** `/` filters the open list as you type, or, from the home menu,
  searches the whole collection across systems (21,880 titles answer in a
  frame). Every word must appear in the title, titles starting with the first
  word come first. Pads get the same through the left trigger and an on
  screen keyboard, and the shoulder buttons jump letter by letter.
- **Music.** A `Music` entry turns the tube into a radio set on top of
  cliamp, Omarchy's music player, started as a daemon when needed: the
  stations of your country (Radio Browser, most voted first), every country
  and genre of the directory, cliamp's own picks, the live queue and the
  recently played list, plus any provider configured in cliamp (Spotify,
  YouTube Music, Qobuz, Plex, Jellyfin: run `cliamp setup` once and their
  playlists appear). A now playing screen shows the station or song, the
  elapsed time and a ten band spectrum straight from cliamp's analyser; a
  strip with a small spectrum follows the music on every music screen. `/`
  (or the left trigger) filters any station list as you type, `Y` stars a
  station into a favourites list. Music keeps playing while you browse,
  pauses by itself when a game or a video starts, and follows the launcher's
  audio routing to the TV. `country` under `[music]` in `settings.toml` picks
  the home country (the locale otherwise).
- **Watch anything.** `omarchy-crt watch URL` plays a YouTube link (or any
  file) on the tube through mpv and yt-dlp; `--later` keeps it at the top of
  the Videos list for the evening.
- **Video fit.** Modern video adapted to the tube the way the analog world did
  it: 480i or 576i by frame rate, 3:2 pulldown or PAL speed-up for film,
  letterbox, crop or anamorphic, SD color with HDR tone mapping, a 5% safe
  area, and a `retro 240p` mode that turns upscaled gameplay captures back
  into 320x240 pixels. Applied live in mpv, or baked into a `CRT` ready file
  by ffmpeg with field based scaling. See [`docs/video.md`](docs/video.md).
- **Sound everywhere.** Clicks under the BIOS typewriter, whooshes on
  submenus, crackle under the laser etch, a scrape and a click when a cartridge
  goes in, all synthesized at startup. No background music.
- **Game browser.** `games/` lists the systems from `systems.toml`, then the
  ROMs of a system as a paged list. A game starts in RetroArch with a dedicated
  config: no RetroArch menu, no notifications, save state on exit and resume on
  start. The shell waits behind the game and comes back when it ends.
- **Video policy.** Each system declares how the display mode follows the
  game: `super` (wide frame, height and refresh follow the core), `native`, or
  a pinned frame such as `512x224`. See
  [`docs/video-policy.md`](docs/video-policy.md).
- **Right on first launch.** Per system libretro core options, input device
  types, run-ahead and rewind whitelists, written into RetroArch's config at
  every launch. Built-in defaults cover the cores Arch ships. See
  [`docs/systems.md`](docs/systems.md).
- **Recent and favorites.** Two virtual folders on top of the systems list;
  `F` or the `Y` button stars a game. Multi disc games appear once through
  `.m3u` playlists.
- **Controllers.** SDL game controller API in the shell (hotplug, left stick
  as d-pad, button hints in the pad's own vocabulary, extra mappings from a
  `gamecontrollerdb.txt`), udev autoconfig profiles in RetroArch, analog to
  d-pad per system, and a `pair-pad` screen that drives `bluetoothctl`. See
  [`docs/input.md`](docs/input.md).
- **TV profile.** Monitor preset (`generic_15`, `ntsc`, `pal`, arcade
  chassis), centering, horizontal size and sync polarity, saved as
  `switchres.ini` plus RetroArch centering keys, with the 240p Test Suite one
  press away as a test pattern.
- **Screensaver.** After an idle period the wordmark cycles through text
  effects (laser etch, rain, beams, burn, slide, decrypt, expand, unstable,
  vhstape), like Omarchy's own screensaver does in the terminal. Nine of the
  TerminalTextEffects catalog so far; effect and idle time are configurable
  from Settings and saved in `settings.toml`.
- **Theme aware.** Colors come from `~/.config/omarchy/current/colors.toml`.
  Icon and wordmark are Omarchy's own assets.

```sh
cd shell
cargo build --release
./target/release/omarchy-crt-shell              # 3x window, press Space
./target/release/omarchy-crt-shell --auto-boot  # skip the gate
./target/release/omarchy-crt-shell --screensaver laseretch
./target/release/omarchy-crt-shell --fullscreen --stretch   # on the CRT output
```

See [`shell/README.md`](shell/README.md) for every flag, the menu file format
and the timeline.

## The library scans anything

Point `omarchy-crt library scan` at a disk and it works out what every file
is: extension, the words in the folder names, disc image signatures, the
names inside zips. No renaming, no fixed folder scheme. Regional variants
collapse onto one title, systems show up when they have games. Details in
[docs/systems.md](docs/systems.md).

## Repository layout

- **`docs/`** research and decisions: the 15 kHz study, the video policy, the
  systems file and TV profile, controllers, video on a CRT.
- **`scripts/crt-probe.sh`** read-only probe of a DRM connector: status, EDID,
  kernel mode list, Hyprland view, and the HDMI audio path (ELD pin, PipeWire
  profile and sink for that connector). `--tone` plays a 2 s test tone on the
  matching sink. Used to test DACs.
- **`bin/omarchy-crt-install`** builds the launcher and the CLI, installs
  them in `~/.local/bin` and installs the bar plugin.
- **`plugin/`** the Omarchy bar plugin (`io.github.stefanomainardi.omarchy-crt`)
  and, in `plugin/library/`, the full screen library overlay it summons:
  a television glyph in the bar and a panel drawn like a TV on screen display
  with power, NTSC or PAL, launcher focus, DAC sync, audio and library health.
  See [plugin/README.md](plugin/README.md).
- **`scripts/demo-video.sh`** renders the shell offline from `scripts/demo.txt`
  (scripted input) and encodes an MP4 with the synthesized audio, frame exact.
- **`scripts/vm.sh`** throwaway Omarchy VM for kernel packaging tests.
- **`shell/`** the native launcher (`omarchy-crt-shell`) and the CLI
  (`omarchy-crt`) that turns the desktop into a CRT station: modeline, DAC
  composite sync, audio routing, launcher, window rules, the game index and
  BIOS checks. See [docs/cli.md](docs/cli.md).

## Roadmap

1. **First light.** Done on 2026-09-06: the launcher runs on the BeoCenter 1
   through the RGB-Pi 2 at 240p and 288p with audio over SCART. Next: centering
   and overscan safe area, color levels, per game modes.
2. **Kernel.** Package `linux-crt` from `linux-lts` with the 15 kHz patches;
   automate rebuilds.
3. **Real DAC.** RTD2166 adapter plus VideoAmp or UMSA on a SCART TV; verify
   240p, 288p and 480i with `switchres`.
4. **Game launch on the CRT.** RetroArch in KMS/DRM on a second virtual
   terminal with mode switching on, GroovyMAME next; the desktop launch path
   already works.
5. **Omarchy integration.** `omarchy-crt-install`, menu entries, TV profiles
   (NTSC, PAL, generic 15 kHz), geometry test patterns.
6. **More effects.** Port the rest of the TerminalTextEffects catalog to the
   screensaver.

## Contributing

Work lands on `develop` and is merged to `main` when it runs. Commits follow
Conventional Commits. The launcher builds with a stable Rust toolchain and
SDL2; `cargo build --release` in `shell/` is all it takes.

## Credits and licenses

- Omarchy icon and wordmark: Omacom Foundation, MIT.
- `font8x8` bitmap font: Daniel Hepper, public domain, after the IBM VGA fonts.
- `laseretch` and the effect catalog: inspired by
  [TerminalTextEffects](https://github.com/ChrisBuilds/terminaltexteffects) by
  ChrisBuilds.
- 15 kHz kernel patches: Calamity and D0023R. Switchres and GroovyMAME:
  Antonio Giner and the GroovyArcade community.
- Hardware research: the Batocera CRT Script wiki by ZFEbHVUE and the
  RetroRGB, shmups and arcadecontrols communities.
- Launcher flow (systems, then games, RetroArch without its menu): inspired
  by the GPL frontend of RGB-Pi OS by rtomasa; the display mode policy is our
  own on top of upstream RetroArch CRT SwitchRes.

License: MIT.
