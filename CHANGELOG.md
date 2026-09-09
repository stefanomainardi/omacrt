# Changelog

All notable changes to this project are documented here, following
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html), with the usual
caveat for a 0.x project: anything may still move.

## [Unreleased]

### Fixed

- The display process notices a graphics device that has stopped answering.
  A page flip that is accepted is always followed by a vblank; one that is not
  left the process alive and idle, holding the lease, with a dark television
  and nothing in the log, because the frame in flight was never cleared and
  every render returned at its first line. Two seconds without the vblank now
  ends the process and the watchdog puts the television back.
- The display process notices a compositor that has died. Only a polite
  `Finished` from the lease protocol counted as revocation before, and a
  compositor that crashes sends no event at all, so the process stayed alive
  holding a dead lease with the television black and the watchdog, which
  watches the pid, seeing nothing wrong. An error on the connection now counts
  too.
- The Style screen no longer reads and parses a theme file for every visible
  row of every frame. The swatch beside each name is read once, when the list
  of installed themes is built.
- The list of busiest processes is gathered on a thread of its own. Walking
  every process on the machine means opening two files per process, and it was
  happening twice a second where the picture is drawn.
- `ffprobe` cannot freeze the launcher. A probe that has not answered in five
  seconds is killed and the video treated as unreadable, so a mount that has
  gone away costs one pause rather than the whole interface.
- The library is read again on a thread when a scan changes it under the
  launcher, instead of stopping the picture for as long as it takes.
- A weather loop is rendered on a thread and outside the voice lock. Four
  seconds of samples were being synthesised where the picture is drawn, with
  the lock the audio callback needs held throughout: a dropped frame and an
  audible gap on every weather change.
- The desktop preview window's Wayland source is removed from the event loop
  when the window closes. One socket and one live source were left behind by
  every `monitor on` and `monitor off`.
- Whether the preview window is open is answered from a file the display
  process writes, not by asking the compositor. Starting a game no longer
  spawns `hyprctl` to find out.
- Recording no longer allocates a frame of video memory and two buffers thirty
  times a second. The offscreen buffer is kept between frames, the picture is
  scaled straight out of it, and the buffers the writer has finished with come
  back to be filled again.
- A worker thread that has died is no longer indistinguishable from one with
  nothing to do. A dead cover worker resolves what it was asked for as no
  picture, a dead music worker says so on the list and clears the transport,
  and a dead photograph worker says so on the frame.
- A page flip the connector refuses no longer retries for ever. Nothing marked
  the frame as queued when the flip failed, so every client commit drew the
  whole scene again and logged another line, indefinitely. Ten refusals in a
  row end the display process, and the watchdog puts the television back.

## [0.4.0] - 2026-09-09

The release the repository opens with. 0.3.0 was tagged the day before, under
the old name and before the pass below, so it is not the one to start from.

### Security

- Five things that crossed a boundary are closed. The library overlay took the
  program it runs out of the message that summons it and started six of them
  on open; it resolves the binary from its own directory now. The two places
  on the desktop that build a shell command line quote what goes into them,
  through the function that existed for it and was called nowhere. The core
  data archive was downloaded to a predictable name in `/tmp`, where another
  user can leave a symlink; it is staged in the project's own cache, checked
  for the file it claims to carry, and moved into place only then.
  `shot` and `record start` on the control pipes wrote wherever they were
  pointed and now insist on a name that matches what they write.
  `output.position` and a modeline's trailing flags reached Lua evaluated
  inside the compositor and are checked before they get there.
- `bin/omacrt-pick` reads `OMACRT_PICKER` as a command with arguments instead
  of running it through `eval`.

### Fixed

- Eight panics reachable from data alone. A games list open on a system a
  rescan removed, a cover name from the thumbnail server with a character
  outside ASCII, a calendar's start time, a Bluetooth address that is not one,
  a playlist that names itself, a cover PNG declaring enormous dimensions, a
  preview window a few pixels wide, and an audio callback whose panic took the
  picture with it. Release builds now carry `overflow-checks`.
- The sweep no longer stops an editor that happens to have one of this
  project's files open: a process has to be one of the programs the launcher
  starts as well as carry one of its files. The launcher search is anchored to
  the binary and scoped to the user, so it no longer matches the installer's
  own copy command.
- `state.json`, the record `off` reads to undo everything `on` changed, and
  `watch-later.tsv` are written atomically like the rest.
- The tidy sweep looks in the cache the code actually writes to, which on a
  machine that sets `XDG_CACHE_HOME` was not the one it was checking, and it
  removes the five megabyte leftovers a crashed scan used to keep forever.
- `off` puts the CRT sink's volume back.
- `bios import` keeps a file it replaces as `.replaced`, and a failed video
  conversion deletes only a destination that run created.
- The installer's menu block is removed only when both markers are present, so
  a file with a dangling one is handed back untouched, and the old plugin
  folders are retired only on an unlocked session - moving them under the lock
  screen aborted Quickshell, which is what the lock check exists for.
  `--system` validates the connector name before it reaches a root owned unit
  file.
- The lease unit waits for the connector instead of sleeping through ten
  seconds of every boot, tells an interrupted run apart from a switched off
  television, and stands down when there is no tube rather than holding up the
  login screen.
- The display process says which of the two lease failures it hit. "Is it
  marked non-desktop?" was misleading when it is: the compositor picks what it
  offers for leasing when it starts, so an override applied to a running
  session takes effect at the next boot. `doctor` carries the same row.

### Removed

- The pause menu's UDP fallback. It spoke RetroArch's command interface, which
  this project disables at every launch because a datagram crashes it, and a
  UDP send to a closed port succeeds - so in a window the menu reported saves
  that never happened. `game cmd` goes with it; `game key` remains.

### Changed

- Every path, binary and identifier carries the new name. The commands are
  `omacrt`, `omacrt-shell`, `omacrt-display` and `omacrt-pick`, the unit is
  `omacrt-lease.service`, the plugins are
  `io.github.stefanomainardi.omacrt[.library]`, and the environment reads
  `OMACRT_CONFIG`, `OMACRT_LOG`, `OMACRT_SINK`, `OMACRT_VM_DIR` and
  `OMACRT_CONNECTOR`.
- A machine that ran the project under its old name keeps everything it had.
  On the first start of a renamed build, whatever sits in
  `~/.config/omarchy-crt`, `~/.cache/omarchy-crt`,
  `~/.local/share/omarchy-crt` and `~/.local/state/omarchy-crt` is moved into
  the `omacrt` folders, file by file. A name already taken on the new side is
  left alone on both sides, and nothing is deleted. The installer does the
  same for what it had put in place itself: the old plugins are disabled and
  the old plugin folders and binaries are moved to
  `~/.local/share/omacrt/retired`, and the old lease unit is disabled without
  being stopped, because stopping it takes the EDID override off a connector
  a running launcher is leasing.

### Fixed

- The lease unit is given a stop timeout of its own. Both directions of
  `crt-lease-setup.sh` simulate an unplug and a plug eight seconds apart, and
  a five second `DefaultTimeoutStopSec` killed the stop between the two,
  leaving the connector unplugged with no EDID for the next start to read.
  The script now plugs a connector that reads `disconnected` back in before
  it reads the EDID it is meant to patch.

### Added

- The launcher is called **OmaCRT**, and the name lives in one constant. The
  BIOS post signs itself `OmaCRT BIOS 4.01 / 15kHz` and credits Omarchy as
  what it runs on and is built for, rather than as who wrote it.
- A mark of the project's own: four horizontal bars crossed by the dark cut of
  the beam's return, which travels across them and stops where it started. It
  is described as a shape rather than stored as a bitmap, so every size is
  exact, and it stands in the boot, the gate, the home screen and the header
  of every inner page, returning once every nine seconds wherever it is.

- The pause menu chooses the picture and the shader. **Picture** is `fill the
  screen`, `as the core asks` or `square pixels`, and on the tube the
  viewport is worked out by the launcher from the size the core last drew
  (read from the emulator's log), because RetroArch would fit square pixels
  into the wide super resolution frame and leave the game in a sliver.
  **Shader** offers the installed presets among a curated few and brings the
  `glcore` driver with them, for the days a game runs in a window. Both are
  kept per system in `systems.toml` and take effect at the next start.
- `omacrt library set SYSTEM aspect=...` and `shader=...`.

- `omacrt-shell --clock HH:MM` draws a time of day that is not now, and
  `--clock-speed N` runs the clock faster than it is, which is what a
  time-lapse of the sky needs. Only what is drawn moves; a log line and a
  scan stamp stay real times.
- Weather arrives instead of switching. The server answers every half hour
  and the answer used to land in one frame: a sunny sky became a downpour
  between two sixtieths of a second. A change now takes twenty five seconds,
  and the sky's colours, the number of clouds, how much is falling and the
  fog all cross over together, so a shower starts with a few drops and the
  fog rolls in rather than appearing.
- Rain runs down the window this page pretends to be: a dozen drops cling to
  the glass, gather until they are heavy enough to slide, carry a lens with
  them that shows the picture from a little further down, and leave a trail
  the next one follows. They keep off the band where the clock is, because
  the clock has to stay readable.
- Somebody is on the Atomium's escalator. Every ten seconds or so a bead of
  light travels up one of the twelve tubes that have an escalator in them,
  which is what the real one does and what turns a monument into a place
  where somebody is.
- A tram goes by every couple of minutes, because Brussels is a tram city: a
  silhouette with its windows lit, a pantograph folded up to the overhead
  wire, a spark at the wire now and then, and at night its windows lay their
  light along the pavement as it passes.
- Five birds sit on that wire. They scatter when the lightning goes and when
  the tram passes under them, and they come back to their own places one at a
  time.
- The moon is the moon outside. The server sends the phase and the picture
  drew a crescent whatever it said; it draws the real one now, terminator and
  earthshine and craters only on the lit side, and a full moon silvers the
  tops of the clouds and puts a band of light on the wet street where a new
  one leaves them flat.
- Four street lamps stand along the pavement and light it. They come on with
  the town's windows and go off with them, they put a pool of warm light on
  the ground that the puddles pick up, and they are why anybody can see the
  man walking home at two in the morning: he brightens through each pool and
  goes back to a shadow between them.
- He is out in every sky but the overcast one, and each sky gives him one
  thing to do. The lightning stops him where he stands, and if the bolt took
  the Atomium's lights he waits in the dark until they come back. The fog
  swallows him band by band and lets him out the other side. The snow keeps
  his footprints until it fills them in, settles on his umbrella and slows
  him down. On a clear day the umbrella is folded under his arm and he looks
  up when the aeroplane goes over; on a clear night he stops under the
  Atomium to watch the lights, which put their colour on his face. A dry
  wind takes his hat and he goes after it. And one walk in four has the dog
  out with him.
- Somebody walks home in the rain. Thirteen pixels of him, head down,
  umbrella tilted into the wind, along the pavement at the foot of the town;
  he stops now and then to look up at the sky, which never helps, and on the
  hardest squall of a downpour the umbrella turns inside out and he stands
  there holding it until it turns back. The wet pavement takes his
  reflection and his feet ring the puddles.
- Rain leaves puddles, and a puddle shows what is above it: the town's lit
  windows and the Atomium's own colours, upside down, squashed into a few
  pixels of water the way a city a mile off ends up in a puddle at your feet,
  with rings where the drops land in them.
- Rain comes in squalls. It used to fall at one angle for as long as the wind
  held; the same two swells the breeze is made of now pass through it, so it
  leans harder and then eases off.
- A clear sky has an aeroplane in it now and then: a speck of metal with a
  contrail spreading and thinning behind it, twenty odd seconds to cross,
  and at night the navigation lights blinking on their own rhythms with
  nothing else to see. Brussels has an airport ten kilometres from the
  Atomium, so a clear sky there always has one.
- The sun notices clouds. The rays used to be switched off by the kind of
  weather, so a cloud could drift across the sun and nothing happened; it is
  geometry now, and as a cloud crosses, the halo dims, the rays pull in, and
  the cloud's shade travels across the roofs of the town.
- The sun lining up with a polished sphere of the Atomium puts a four
  pointed flare on it, which grows as it comes into line and goes as it
  leaves. That takes the best part of an hour of real sun.
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

- The boot presents the project's own mark instead of Omarchy's. The mark is
  written from the top down behind a beam, the beam runs back across it once,
  and then the laser etches the wordmark: showing somebody else's mark in the
  first two seconds of every boot was the opposite of the distance the rename
  was for.
- The boot is a second and a half shorter, and the two dead beats in it are
  gone. The icon used to finish appearing and then wait almost a second before
  the laser began, and after the phosphor turned there was another second of a
  still picture before the menu arrived. The floor now leaves while the menu
  rises, rather than after it.
- The floor unrolls from the horizon with the light that opens it, instead of
  fading up as a slab. A slab that appears out of nothing has no cause; and
  what travels across it is light rather than an edge of geometry, because a
  hard edge with black beyond it reads as a picture being cut off.
- The act's soundtrack is read off the same sheet as the picture. It used to
  start at the slam, so the floor arrived in silence and the whole rumble
  landed two seconds late.
- The home screen keeps the twenty two rows that were reserved under the
  wordmark for the CRT tag, which is not drawn any more, so the last menu row
  no longer sits in the overscan.
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
- Music speaks cliamp's version 2 socket. cliamp 2.0 made its socket version
  2 only: requests carry `version: 2` and an id, reads answer with a
  snapshot and everything else answers with a job to follow. A daemon from
  before 2.0 is still understood; which one is listening is learned from the
  first request.
- `docs/plan.md` is gone. The changelog says what landed and the README says
  what the thing is; a status file that had to be edited by hand said neither
  for long.

- About, Diagnostics and Style read the room instead of counting to a number.
  They showed fifteen, twelve and twelve rows whatever the set was; a PAL
  television has 48 lines a 240 line one does not, and those lines sat empty.

- The visualizer waits three minutes instead of six seconds, and the row that
  sets it offers minutes (never, 30 s, 1, 3, 5, 10 min). Six seconds is less
  than it takes to choose a station, so the picture arrived while you were
  still reading the list. A file that says six seconds is a file that took the
  old default, so it is moved to the new one; any other number was chosen and
  is kept.
- The visualizer says what is playing. It always carried the title, but only
  for two and a half seconds after a mode change, which on the default
  settings is two seconds out of every forty five. Now the station (or the
  artist) and the title show when the picture takes the screen, again whenever
  the title changes, and then for ten seconds of every minute, landing a few
  pixels away each time so a tube never keeps them.

### Removed

- The CRT tag: three letters that rose from the horizon spinning, slammed into
  the foreground and flew to a resting place under the wordmark. The wordmark
  says CRT itself now, so the tag said it twice.
- The effect's name from the screensaver. `tte slide` in the corner was a
  note to whoever was building the effects, and there are eighteen of them
  now: the wordmark can have the screen to itself.
- The Omarchy mark that walked up the CRT tag's Mode 7 floor. On the tube it
  read as a smear on the checkerboard rather than as a mark, and the floor is
  better as a floor.

### Fixed

- The laser comes from outside the picture. It stopped eight cell rows above
  the word, which during the etch is the middle of the screen, so the beam
  appeared to start in mid air. It is drawn until it has left the frame, which
  the `laseretch` screensaver gets as well.
- `PRESS START` is centred with its cursor. The words were centred and the
  blinking block appended after them, which pushed the pair to the left by
  half a cursor and its gap.
- A saved album on Spotify would not play. Albums come back in the same list
  as the playlists and only their id says so; asking the provider to load one
  leaves the album's own address in the queue as though it were a stream,
  which fails at playback and leaves the deck on a track that never starts.
  An album is expanded into its tracks now: the first plays, the rest queue
  behind it.
- Recently played was always empty: a history entry from cliamp wraps its
  track and the launcher was reading the envelope as one.
- The deck showed nothing between tracks: a version 2 snapshot names the
  sounding track and the playlist's own track apart and drops the first while
  nothing plays.

## [0.3.0] - 2026-09-08

The release the repository opens with.

### Added

- The weather has a sound, off by default (`[frame] weather_sound`, or
  Settings, Photo frame). Rain with drops on it, gusting wind, thunder behind
  a downpour, birds on a clear day, crickets at night, a horn in fog: eight
  loops, each synthesized and then held at 8 kHz and quantised to five bits,
  the way a sample was in 1990. It plays only while the ambient page is up
  and fades in and out. `omacrt-shell --dump-audio DIR` writes them all
  as WAVs.
- A photo frame: photographs from an Immich server on the same network, from
  what the server calls memories, an album, the favourites or anything at all,
  captioned with the place, the date and the faces the server already knows.
  Three amounts of furniture over the picture (`photos`, `clock`, `panel`),
  the ambient page carrying the weather, the next appointment from an `.ics`
  calendar and what is playing. Pictures close to 4:3 fill the screen and
  drift a pixel a frame; the rest are fitted whole against a blurred copy of
  themselves. The prepared pictures are the frame's own collection, so it
  works with the server off. `omacrt frame check|fill|clear`.
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
- `omacrt-shell --headless --realtime` renders offline at the real frame
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
- `omacrt shell screen NAME` opens a launcher screen by name, which is
  how the menu reaches it without counting rows.
- `omacrt play "metal slug"` starts a game on the television by name or
  by path, matched against the index, and `omacrt library games` lists
  every game the scan has seen. `omacrt-pick` puts a fuzzy picker in
  front of that on the desktop and plays what comes back, turning the tube on
  if it is off; it is the **Play a game...** row in the Omarchy menu and it is
  worth a keybinding. Anything that stops a launch arrives as a notification,
  and with a game already playing the picker asks whether to stop it, with
  the answers as its own rows. `play --force` stops it and waits for the tube.
  Every step goes to `~/.local/state/omacrt/pick.log`, and a walker
  already open is closed first: started alongside another instance it hands
  its arguments over and exits without printing, which looks exactly like a
  menu row that does nothing.
- `omacrt audio volume` takes a step (`+10`, `-10`) as well as a percent.
- `omacrt doctor` surveys what has been left behind as well as what is
  missing: an emulator no launcher owns, a watchdog pidfile whose process is
  gone, half fetched files in the caches. `doctor --fix` clears them.
- `AGENTS.md`: the working guide for the repository, for a coding agent or a
  person, with the rules that have each already cost a session.

- GameCube, through the Dolphin core: native internal resolution, 480 lines,
  the widescreen hacks off and the boot animation skipped. The files the core
  needs but nobody ships with it are fetched once, on the first launch.
- `omacrt library unpack` reads a ScummVM game out of its disc images
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
  failing that, from a translation of SDL's own mapping. `omacrt-shell
  --pads` reports what SDL makes of every connected pad.
- `omacrt setup`: lists the DRM connectors with what their EDID says,
  picks the one the DAC is on, works out the television standard from the
  locale and writes both to `crt.toml`.
- A watchdog started by `omacrt on` that puts the display process, the
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
  display bookkeeping is behind `OMACRT_LOG=debug`.
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
  machine, the tube and the output of `omacrt doctor`, a pull request
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

- Interlaced timings: `omacrt mode 480i` and `mode 576i`, at the line
  rates of the progressive standards.
- A resume prompt: a game left in the middle asks whether to carry on from the
  state RetroArch wrote on exit, or start a session that leaves it alone.
- Rewind, fast forward and slow motion in the pause menu, and `F1` to raise
  that menu from the keyboard.
- `omacrt game key <key> [ms]`: the compositor presses a key inside the
  running game and holds it as long as asked.
- A ten band equaliser page over the music engine's own bands and presets.
- Album art for Spotify tracks and logos for radio stations, on the cassette
  label, the record label and the radio dial.
- The home screen shows what plays, with a small spectrum, on the Music row.
- Arcade sets read as games: file names like `mslug` become titles through the
  databases RetroArch ships, so those systems sort, search and find box art.
- The library overlay runs the scan itself, reporting the folder it reads, and
  lets a system's folder be edited in place.
- `bin/omacrt-install --uninstall` and `--uninstall-system`.
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
- A screen asked for from outside, by the desktop menu or `omacrt shell
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

[Unreleased]: https://github.com/stefanomainardi/omacrt/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/stefanomainardi/omacrt/releases/tag/v0.4.0
[0.3.0]: https://github.com/stefanomainardi/omacrt/releases/tag/v0.3.0
[0.2.0]: https://github.com/stefanomainardi/omacrt/releases/tag/v0.2.0
[0.1.0]: https://github.com/stefanomainardi/omacrt/releases/tag/v0.1.0
