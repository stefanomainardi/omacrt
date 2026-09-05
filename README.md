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
  pass the low pixel clocks 15 kHz needs. HDMI is out: its 25 MHz floor makes
  240p impossible. Validated units: CableDeconn DP to VGA (non 4K, no audio),
  Cable Matters 102026, biaze ZH277.
- **Sync and SCART.** The VGA H and V sync must be combined into composite sync
  at 0.3 to 1 V, and the TV needs 1 to 3 V on SCART pin 16 to switch to RGB.
  Preferred: VideoAmp (also emulates an EDID), then UMSA, sirMagb F-15,
  VGA2SCART. Passive VGA to SCART cables and MiSTer cables do not work.
- **Kernel.** `amdgpu` refuses low dot clocks and mishandles interlace without
  the 15 kHz patch set maintained in
  [D0023R/linux_kernel_15khz](https://github.com/D0023R/linux_kernel_15khz).
  The plan is a `linux-crt` package built from `linux-lts` with those patches,
  living next to the stock kernels through Omarchy's Limine and UKI setup.
- **Mode switching.** Wayland cannot set arbitrary modelines and Hyprland drops
  the interlace flag. The desktop shows fixed progressive 15 kHz modes;
  RetroArch and GroovyMAME run through KMS/DRM on a separate virtual terminal
  with Switchres, which is where per-game switching happens.

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
- **Sound everywhere.** A quiet ambient pad in the menus, clicks under the
  BIOS typewriter, whooshes on submenus, crackle under the laser etch, all
  synthesized at startup.
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

## Repository layout

- **`docs/`** research and decisions: the 15 kHz study, the video policy, the
  systems file and TV profile, controllers.
- **`scripts/crt-probe.sh`** read-only probe of a DRM connector: status, EDID,
  kernel mode list, Hyprland view. Used to test DACs.
- **`scripts/demo-video.sh`** renders the shell offline from `scripts/demo.txt`
  (scripted input) and encodes an MP4 with the synthesized audio, frame exact.
- **`scripts/vm.sh`** throwaway Omarchy VM for kernel packaging tests.
- **`shell/`** the native launcher.

## Roadmap

1. **First light.** Test the launcher on a CRT through an HDMI DAC (RGB-Pi 2),
   which needs no kernel patch but only fixed 240p.
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
