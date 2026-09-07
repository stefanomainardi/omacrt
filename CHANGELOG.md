# Changelog

All notable changes to this project are documented here, following
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html), with the usual
caveat for a 0.x project: anything may still move.

## [Unreleased]

### Added

- GameCube, through the Dolphin core: native internal resolution, 480 lines,
  the widescreen hacks off and the boot animation skipped. The files the core
  needs but nobody ships with it are fetched once, on the first launch.
- ScummVM games are prepared on the way in: the scan works out what each
  folder holds from its data files and writes the `.scummvm` launcher the core
  needs, beside the data. The pointer is on the left stick with the settings a
  point and click game wants, and a game still inside a disc image is reported
  rather than half configured.
- Vibration where the pad has it and the console had it: the Rumble Pak on a
  Nintendo 64, the Purupuru pack on a Dreamcast, a DualShock rather than a
  plain pad on a PlayStation, the flag on a GameCube. Nothing is turned on for
  a pad without force feedback.
- A setting for whether the desktop preview window stays up while a game runs.
  It does not, by default.
- Pads are taught to RetroArch as well as to the launcher: the profile
  directory RetroArch reads is kept filled from the profiles it ships or,
  failing that, from a translation of SDL's own mapping. `omarchy-crt-shell
  --pads` reports what SDL makes of every connected pad.
- `omarchy-crt setup`: lists the DRM connectors with what their EDID says,
  picks the one the DAC is on, works out the television standard from the
  locale and writes both to `crt.toml`.
- A watchdog started by `omarchy-crt on` that puts the display process, the
  timing, the DAC and the launcher back when the display dies, and stands
  down after three restarts in two minutes.
- A `version` key in `settings.toml` and `systems.toml`, with a file written
  by a newer build copied aside before it is touched.
- A hardware compatibility table in the README.

### Changed

- The Games browser no longer lists the videos folder as if it were a
  console; it has its own row on the home menu.
- The television follows the resolution the core is drawing. Every system has
  a line count (480 for a Dreamcast, 224 for a Super Nintendo), a count above
  288 selects the interlaced mode of the standard on its own, and the launcher
  reads the emulator's log while a game runs and follows every change. A
  640x480 console is no longer squeezed into 240 lines.
- `misc:exit_window_retains_fullscreen` is never left set on the desktop, and
  `misc:on_focus_under_fullscreen` is put back to whatever it was.
- Log files rotate past 8 MB, keeping one older generation; the per-second
  display bookkeeping is behind `OMARCHY_CRT_LOG=debug`.
- `crt.toml` and `systems.toml` are written atomically and read through the
  durable store, falling back to their backup.
- The library overlay writes a source folder as `~/...`, or from the name of
  the removable disk it sits on, rather than as a full path carrying the
  user's name.

## [0.2.0] - 2026-09-07

The first release meant for somebody else's machine.

### Added

- Interlaced timings: `omarchy-crt mode 480i` and `mode 576i`, at the line
  rates of the progressive standards.
- A resume prompt: a game left in the middle asks whether to carry on from the
  state RetroArch wrote on exit, or start a session that leaves it alone.
- Rewind, fast forward and slow motion in the pause menu, and `F1` to raise
  that menu from the keyboard.
- `omarchy-crt game key <key> [ms]`: the compositor presses a key inside the
  running game and holds it as long as asked.
- A ten band equaliser page over the music engine's own bands and presets.
- Album art for Spotify tracks and logos for radio stations, on the cassette
  label, the record label and the radio dial.
- The home screen shows what plays, with a small spectrum, on the Music row.
- Arcade sets read as games: file names like `mslug` become titles through the
  databases RetroArch ships, so those systems sort, search and find box art.
- The library overlay runs the scan itself, reporting the folder it reads, and
  lets a system's folder be edited in place.
- `bin/omarchy-crt-install --uninstall` and `--uninstall-system`.
- `SECURITY.md`, a test suite that runs without a television, and a CI
  workflow that builds, lints, tests and renders frames headless.

### Changed

- Every durable file is written atomically and keeps its previous copy as
  `.bak`; a truncated file falls back to that copy when read.
- Downloads are restricted to HTTP and HTTPS with a size cap, and media targets
  are passed after the option terminator.
- Bluetooth pairing no longer builds a shell command; addresses are validated
  before use.
- The boot time unit runs with the usual systemd hardening, and the helper it
  runs refuses a connector name that is not one.
- `doctor` checks the music engine, the download tools, the lease unit, the bar
  plugin and whether its own directories can be written.

### Fixed

- I2C transfers time out instead of waiting for ever, and the bus lock is taken
  without blocking: a television that was off used to leave one stuck process
  per poll of the bar plugin, until everything that touched the DAC hung.
- Recordings play at the speed they were shot: dropped frames are held rather
  than shortening the film and drifting away from its own sound.
- Escape reaches the video player again, and the desktop window forwards every
  key it is given.
- The desktop's own video player keeps its sound: our player names its audio
  client, so PipeWire stops sending every mpv to the television.
- A link from the recent list plays: the timestamp column was being read as
  part of the path.
- The cover flow's title no longer runs into the save state label.

## [0.1.0] - 2026-09-06

First light: the television leased away from the desktop and driven by its own
compositor, a launcher drawn at 320x240, games at native line counts, the bar
plugin, music through cliamp and video through mpv.

[Unreleased]: https://github.com/stefanomainardi/omarchy-crt/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/stefanomainardi/omarchy-crt/releases/tag/v0.2.0
[0.1.0]: https://github.com/stefanomainardi/omarchy-crt/releases/tag/v0.1.0
