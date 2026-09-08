# Changelog

All notable changes to this project are documented here, following
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html), with the usual
caveat for a 0.x project: anything may still move.

## [Unreleased]

### Added

- `omarchy-crt-shell --clock HH:MM` draws a time of day that is not now, and
  `--clock-speed N` runs the clock faster than it is, which is what a
  time-lapse of the sky needs. Only what is drawn moves; a log line and a
  scan stamp stay real times.
- One bolt in four goes for the Atomium rather than the ground, because it is
  the tallest thing for miles with a lightning rod in every tube. It lands on
  the top sphere, the lights go out, and then the nine come back from the
  ground up while the street behind them fills in one window at a time.
- The weather over Brussels has the Atomium in it: nine spheres, twenty
  tubes, a cube standing on one corner behind the roofline. By day it is
  metal with the reflection where the real sun is, and the sun passes behind
  it; at night a wave of colour walks through the spheres, a lamp chases
  round each one, and the red light on top answers to aircraft. Snow caps
  it, fog eats it from the bottom, rain leaves it wet and lightning turns it
  into a silhouette. Only Brussels, however the city is spelled.
- The home menu says a little more without saying it eight times over: the
  row under the cursor carries what it holds on its own right (the size of
  the collection, the last thing played, how many pages an idle television
  shows, the theme), the icon of that row breathes, and the two corners the
  wordmark leaves empty carry the picture and line rate on the left and the
  time on the right.
- **Settings, Sound**: one page for every switch the launcher's noises have.
  `[sound] menu` turns off the beep, the click and the page turn while leaving
  the boot show alone; `deck` is the needle on a track change; `weather` is the
  clock and weather page's own sound. The boot show keeps its sounds whatever
  they say.
- **Settings, Clock and weather**: the ambient page has settings of its own,
  where the town and the calendar were only ever reachable through the photo
  frame's page. `A` opens the page itself, and the third row is whether it
  takes its turn on an idle television.

### Changed

- Twilight is a ramp rather than a switch. The stars used to vanish, the sky
  change colour, the town's windows go out and the Atomium's lights die in
  the single frame the sun crossed the horizon in. All four now fade across
  forty minutes either side of sunrise and sunset, which is what dawn looks
  like.
- The settings page is two grouped columns rather than a list of twelve:
  **THE PICTURE** (TV profile, Video fit) and **CHANNELS** (Music, Videos,
  Photo frame, Clock & weather) on the left, **THE SET** (Sound, Screensaver,
  Style, Pads) and **THIS MACHINE** (Diagnostics, About) on the right. Up and
  down stay in a column, left and right cross to the other one, the headings
  are skipped, and everything is still one button away.
- Leaving the Pads page put the cursor on Video fit, and leaving About put it
  on Clock and weather. Screens named the row they came from by number and two
  of the numbers were wrong; they name the page now.
- The town, the calendar and the sounds moved out of the sections they were
  camping in: `[frame] weather` and `[frame] calendar` are `[ambient] place`
  and `[ambient] calendar`, `[frame] weather_sound` is `[sound] weather`, and
  `[music] change_sound` is `[sound] deck`. A file written before that keeps
  what it asked for: the old keys are read once, moved, and never written
  again.
- The library overlay's buttons say what they do: "Rescan sources" for the
  one that rescans the folders already listed, and "Reload" for the one that
  reloads what the overlay is showing.

## [0.3.0] - 2026-09-08

The release the repository opens with.

### Added

- The weather has a sound, off by default (`[frame] weather_sound`, or
  Settings, Photo frame). Rain with drops on it, gusting wind, thunder behind
  a downpour, birds on a clear day, crickets at night, a horn in fog: eight
  loops, each synthesized and then held at 8 kHz and quantised to five bits,
  the way a sample was in 1990. It plays only while the ambient page is up
  and fades in and out. `omarchy-crt-shell --dump-audio DIR` writes them all
  as WAVs.
- A photo frame: photographs from an Immich server on the same network, from
  what the server calls memories, an album, the favourites or anything at all,
  captioned with the place, the date and the faces the server already knows.
  Three amounts of furniture over the picture (`photos`, `clock`, `panel`),
  the ambient page carrying the weather, the next appointment from an `.ics`
  calendar and what is playing. Pictures close to 4:3 fill the screen and
  drift a pixel a frame; the rest are fitted whole against a blurred copy of
  themselves. The prepared pictures are the frame's own collection, so it
  works with the server off. `omarchy-crt frame check|fill|clear`.
- A system monitor drawn as a 16 bit status screen: a bank of meters, one per
  logical processor, memory, graphics, load and network history, the rates and
  the busiest processes, with a second page of processes and their pids. Read
  from `/proc` and `/sys` only.
- The screensaver is a rotation, not one choice. Four pages can have an idle
  television: the wordmark under a text effect, the photo frame, the weather
  drawn with the time under it, and the system monitor. `[screensaver] pages`
  is which of them are in it, `cycle_secs` is how long each one keeps the
  screen, and `effect` means only which text effect the wordmark page uses.
  One page keeps the screen; several take turns in the order they are drawn
  in, starting at a random one so a television left alone twice does not open
  the same way twice. Music playing still takes the screen for the
  visualizer, and the first key press puts back the screen that was up. A
  file from an older build is read once and written back as a rotation.
- The ambient page is a window with the weather in it, drawn the way a 16 bit
  game drew a sky. The sun crosses the arc between the real sunrise and sunset
  and the moon takes the same path at night; clouds drift in three layers at
  the speed of the real wind; rain slants with it and breaks on the ground;
  snow wanders down; fog rolls over the clouds in bands; lightning lights the
  whole frame and leaves a bolt behind; and a town sits along the horizon with
  its windows coming on after dark. Every gradient is a 4x4 ordered dither,
  the way a console with a fixed palette faked one. The time, the temperature,
  the day and what is next are in the dark band underneath, where they can be
  read.
- With no place set, the weather is asked about the city in the machine's own
  timezone (`Europe/Brussels` is Brussels) rather than about wherever the
  address seems to be, which a VPN moves a country. The Photo frame settings
  page says which town the page is showing, and where the name came from.
- `omarchy-crt-shell --headless --realtime` renders offline at the real frame
  rate. Without it the loop runs about sixty times faster than the clock,
  which is what made the system monitor read zero: it asked the kernel for its
  counters more often than the kernel moves them.
- The Television rows in Omarchy's menu carry icons, taken from the code
  points Omarchy's own menu uses so the font is known to have them.
- The weather is read in full rather than as one line: the place, the
  temperature, the condition, the wind and the two times the sun crosses the
  horizon, in about sixty bytes from wttr.in. `[frame] weather` names the
  place; empty leaves wttr.in guessing from the address, which a VPN moves a
  country.
- A **Television** entry in Omarchy's own menu, written into the extension
  file Omarchy reads for it, with power, channels, picture, sound, library,
  capture, pads and diagnostics. `--uninstall` takes it back out.
- `omarchy-crt shell screen NAME` opens a launcher screen by name, which is
  how the menu reaches it without counting rows.
- `omarchy-crt play "metal slug"` starts a game on the television by name or
  by path, matched against the index, and `omarchy-crt library games` lists
  every game the scan has seen. `omarchy-crt-pick` puts a fuzzy picker in
  front of that on the desktop and plays what comes back, turning the tube on
  if it is off; it is the **Play a game...** row in the Omarchy menu and it is
  worth a keybinding. Anything that stops a launch arrives as a notification,
  and with a game already playing the picker asks whether to stop it, with
  the answers as its own rows. `play --force` stops it and waits for the tube.
  Every step goes to `~/.local/state/omarchy-crt/pick.log`, and a walker
  already open is closed first: started alongside another instance it hands
  its arguments over and exits without printing, which looks exactly like a
  menu row that does nothing.
- `omarchy-crt audio volume` takes a step (`+10`, `-10`) as well as a percent.
- `omarchy-crt doctor` surveys what has been left behind as well as what is
  missing: an emulator no launcher owns, a watchdog pidfile whose process is
  gone, half fetched files in the caches. `doctor --fix` clears them.
- `AGENTS.md`: the working guide for the repository, for a coding agent or a
  person, with the rules that have each already cost a session.

- GameCube, through the Dolphin core: native internal resolution, 480 lines,
  the widescreen hacks off and the boot animation skipped. The files the core
  needs but nobody ships with it are fetched once, on the first launch.
- `omarchy-crt library unpack` reads a ScummVM game out of its disc images
  into the folder beside them, which is what ScummVM needs and what the manual
  told you to do in 1997.
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

- The home menu carries an **Ambient** row where **About** was, holding the
  three pages for a television with nothing playing on it: the photo frame,
  the clock and weather, and the system monitor. About moved into Settings.
  Eight rows is what fits under the wordmark on a 240 line screen, so the
  three share one row rather than sitting under Videos, where photographs do
  not belong.
- The television follows whichever picture is being looked at: a console
  drawing 224 lines gets 224 lines while it plays, and the tube goes back to
  the launcher's own 240 for as long as the pause menu is up.
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
- The screensaver is two settings instead of one field meaning four things:
  which pages are in the rotation, and which text effect the wordmark uses.
- The turntable's track change sound is off by default, and quieter when it
  is on (`[music] change_sound`).
- `scene.rs` is a directory of ten files. Not a line was rewritten: the mover
  proved it could put the file back together first, and every screen was
  rendered before and after.
- Contribution rules (`CONTRIBUTING.md`), a bug template that asks for the
  machine, the tube and the output of `omarchy-crt doctor`, a pull request
  template that asks what a change ran on, and Discussions in place of blank
  issues.
- The 15 kHz research is in English, as `docs/15khz.md`, and corrected where
  what shipped disproved it.

### Security

- All eight network fetches go out through one function that restricts the
  protocol list to HTTP and HTTPS before and after a redirect, sets a
  timeout and a size cap, and fails on an error status. Three of them used
  to save an error page as if it were the answer.
- An identifier from the photograph server is checked against letters,
  digits and dashes before it becomes a file name, so an answer of
  `../../.ssh/authorized_keys` cannot decide where a file is written.
- There is no shell anywhere in the launcher: the one command that went
  through `sh -c` is an argument list, and the module that ran it cannot run
  a command line at all.
- The panics outside the tests are ten, each a mutex lock or a value the
  framework guarantees, each with its reason written beside it. `--dump nan`
  was one of them and is not.
- `scripts/audit.py` asks the advisory database about every locked crate with
  nothing installed but Python, and runs in CI.
- `SECURITY.md` says what the project reads, runs, writes, downloads and
  listens on, and how the photograph server's key is handled.

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

- The deck no longer makes a noise every time the track changes. Stepping
  through a list on the turntable played the radio's static, seven tenths of
  a second of broadband noise at four times the level of any other sound in
  the launcher, over the music. There is a sound for it now, a needle set
  down, at a tenth of that energy, and `[music] change_sound` decides whether
  anything is played at all. It is off.

- An emulator no longer outlives the launcher that started it. The signal went
  to the launcher alone, so the child was reparented to systemd and kept
  running, holding the audio and answering "something is playing" for as long
  as the machine was up; one from a morning's testing blocked every launch
  from the desktop for eleven hours. `shell stop` and `off` take them with
  them, `on` and `shell start` clear what a previous life left, and the
  watchdog sweeps once a minute while the tube is on.

- The idle timer leaves alone a page that is already one of the screensaver's
  own: sitting on the photo frame used to get the wordmark over it after a
  minute. A page that draws itself is a screensaver already, and one opened on
  purpose is the one that was wanted. Somewhere static, a list of games, is
  what the timer is for.
- A screen asked for from outside, by the desktop menu or `omarchy-crt shell
  screen`, puts the screensaver away first. It used to change the screen
  underneath an effect that went on drawing, so choosing a channel from the
  Omarchy menu looked as if it had done nothing. `shell key home` had the same
  fault.

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
