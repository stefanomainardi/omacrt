# Plan, September 2026

What comes next, in the order it will be built. Each phase ends with a test on
the BeoCenter 1. Items move to the README roadmap when they land.

## Phase A, stabilise the tube (now)

- **Subfolders.** RePlayOS collections keep ROMs in subfolders (`00 Clean
Romset`, `01 Other Romsets`, ...). The launcher browses folders inside a
  system and the library counts them, so Super Nintendo and Mega Drive show
  up from the external disk.
- **No desktop flash on launch.** RetroArch and mpv open on their own
  workspace of the CRT output; the launcher stays fullscreen underneath and
  takes the tube back the frame the game exits. No special workspaces (they
  froze the compositor once).
- **Centering per console, first cut.** Overscan crop options per core
  (PlayStation borders), pinned line counts where the console has one, and a
  geometry block per system in `systems.toml` (`lines`, `shift_x`,
  `shift_y`) applied to the modeline at launch. The TV profile screen in the
  launcher drives the same numbers.
- **Audio levels.** CRT sink boost as one setting, RetroArch at 0 dB, mpv
  loudness normalised. A PipeWire limiter before the sink if clipping shows.

## Phase A3, own the tube (done 2026-09-07)

Every problem fought on the tube had one cause: the desktop compositor
owned the CRT output. Now it does not. The DAC's connector is marked
non-desktop (a Microsoft "specialized display" block in an EDID override,
installed at boot by `omarchy-crt-lease.service`), Hyprland leaves it alone
and offers it through the DRM lease protocol, and `omarchy-crt-display`
takes the lease:

- **The display process** sets the 15 kHz modeline straight through DRM
  (any timing the kernel accepts, interlace included), keeps the mode up
  (the HDMI audio pin lives only while a mode is up) and switches it live
  (`mode`), so a 224 line system gets 224 lines.
- **A small Wayland compositor** (smithay) on that output hosts the
  launcher, RetroArch and mpv as ordinary clients
  (`WAYLAND_DISPLAY=wayland-crt`): one fullscreen client at a time, dmabuf
  import, GL rendering, frame callbacks to every mapped surface so a
  program under the pause overlay keeps running. Stacking is a `top`
  command on its control pipe.
- **Hotkeys, not network commands.** RetroArch 1.22 crashes when it
  processes a network command, so the pause menu presses the emulator's
  own hotkeys (P, F2, F4, H, Escape) on the compositor's keyboard, held for
  a frame. Pause, save state, load state, reset, resume and a clean exit
  are verified on the tube.

Nothing on the desktop is touched any more: no monitor, no window or
workspace rules, no layers, no pointer on the tube. The pinned-window
machinery from the night before stays as the fallback when the connector
is not leaseable (`omarchy-crt on` picks the path by the kernel's
non-desktop property).

Left in this phase: keyboard for the tube's clients (a desktop bind that
routes keys through `omarchy-crt-display key`), `omarchy-crt-display`
started by `omarchy-crt boot` at login, and a screenshot command that reads
the composited frame back (the desktop's tools cannot see the tube).

## Phase A2, playing well

Progress: the CRT workspace is now hardened (everything pinned floating on
`crt`, foreign windows pushed back to the desktop, the pointer parked off
the tube), and the launcher draws its own pause overlay (Resume, Save
state, Load state, Reset, Back to launcher) raised over the game. The
catch is that RetroArch's network commands only land while its window
renders, so save and quit from behind the overlay are best effort (Wayland
throttles an occluded window's loop); resume is solid. Still to do below.

- **In game menu, Omarchy style.** The plumbing is in: RetroArch runs with
  its network command interface on, and `omarchy-crt game
pause|save|load|reset|quit` drives it (proven on the tube). `omarchy-crt
shell key menu` pauses and resumes the running game today. What is left is
  the visual: RetroArch's own RGUI opens but renders nothing at the wide
  super-resolution, so the launcher must draw its own pause overlay. The
  clean path is to pause the game first (it then stops asking the compositor
  for frames), switch the tube to the launcher's workspace to show the
  overlay, and reverse both on resume, so the hidden game never triggers the
  not-responding dialog. Options to expose: resume, save state, load state,
  reset, fast forward, rewind, aspect, back to the launcher. The launcher
  already sees the pad in the background and the control pipe carries the
  bind.
- **Save states.** One slot per game with a timestamp, quick save and load
  from the pause menu, "resume where you left" on the game row.
  *Landed 2026-09-07:* `shell/src/states.rs` reads RetroArch's states per
  game (manual slot and the automatic one), rows carry an arrow, the box art
  panel and the pause menu say when the state was written; fast forward
  joined the pause menu. Rewind stays off (RetroArch's buffer costs CPU).
- **Pad recognition.** Identify the pad family and layout on plug (SDL
  database plus vendor and product ids), map RetroPad buttons per family,
  show the right glyphs everywhere, remember per pad. Unknown pads get a
  short mapping wizard on the tube.
  *Landed 2026-09-07:* `shell/src/padmap.rs`, the wizard on the tube for any
  joystick SDL has no mapping for, saved to `gamecontrollerdb.txt`; `X` on
  the Pads screen maps the current pad again.
- **Search.** *Landed 2026-09-07:* `/` filters any list as you type, from
  the home menu it searches the whole collection; pads use the left trigger
  and an on screen keyboard, shoulders jump by initial letter.

## Phase B, a launcher worth looking at

- **Box art.** Covers from the libretro thumbnail repository (No-Intro and
  Redump names, which the collection already uses), fetched in the background
  and cached at 320x240 friendly sizes. The selected game shows its cover and
  a screenshot while you move through the list, keyboard or pad.
- **Console images.** One picture per system on the systems screen and as a
  header in the game list, from the RetroArch `systematic` asset set (CC BY),
  recoloured to the theme.
- **Details that make it feel finished.** Region and player count from the
  file name tags, "last played", favourites star, a slow marquee for long
  titles, folder breadcrumbs.
  *Landed 2026-09-07:* tags, last played, marquee, star and breadcrumbs.
  Player count is not in the file names the collection uses (No-Intro,
  MAME short names), so it waits for a metadata source.

## Phase C, manage the collection from the bar

- **Library overlay in the plugin.** A full screen Omarchy style surface:
  systems with folder, core, count, BIOS state; disk discovery (mounted
  drives with recognisable folders) with one click "adopt this collection";
  BIOS import from a discovered `bios` folder; per system folder and core
  editing with a path field and a core picker. Everything writes
  `systems.toml` through `omarchy-crt`.
- **Cores on demand.** When a system in the index has no core installed
  (`mame2003` today), the library overlay says so and offers to install it
  (`libretro-mame2003-plus` through `omarchy pkg add` or `pacman`), then the
  system comes alive in the launcher. Same for RetroArch or mpv missing.
- **Output settings in the panel.** Connector, workspace, standard, csync and
  volume editable from the panel and stored in `crt.toml`.

*Landed 2026-09-07:* the library overlay as its own `overlay` plugin
(`plugin/library/`, summoned by the panel's `Library` button): sources with
disk adoption, systems with core state and an install offer (`library cores`
names the package, repository or AUR), BIOS report with import from any
folder that holds the files (`bios discover`), unplaced folders with a system
picker (`library assign`). `TV volume` slider in the panel (`audio volume`,
`config set`). Not done: per system folder and core editing beyond
assignment, and the connector picker (single DAC setups have nothing to pick).
Lesson from the day: a full screen surface cannot live inside a bar widget's
panel (it maps as a plain window), and plugin code must never be hot reloaded
while the session is locked (Quickshell aborts on its lock surfaces).

## Phase D, a television, not only a console

- **cliamp as the engine.** Omarchy's default music player, cliamp (a
  Winamp 2 tribute TUI), already speaks local files, YouTube, Spotify, Qobuz,
  Tidal, Plex, Jellyfin and tens of thousands of radio stations, runs as a
  daemon and exposes IPC and a CLI. The tube gets a 240p face on top of that
  daemon: the same pad and keyboard language as the games, a visualiser built
  for 15 kHz, album art, radio dial. Its skin language (Winamp playlist,
  equaliser, retro chrome) is the reference for the mood.
  *Landed 2026-09-07 (`shell/src/music.rs`):* the `Music` entry, radio by
  country and genre through Radio Browser, cliamp's catalog, the live queue,
  history, every configured provider's playlists, the now playing screen with
  cliamp's spectrum, pause on game launch, audio following the TV routing.
  cliamp 1.63 speaks the v1 socket protocol (`{"cmd": ...}`); the v2 envelope
  of newer releases is not accepted yet, so the client sticks to v1 commands
  (`status`, `bands`, `provider.*`, `track.play`, `queue.*`, `history`).
  *Later the same day:* search with the on screen keyboard on the music
  lists, favourite stations (`radio-favorites.tsv`), `omarchy-crt watch` for
  files and links with a watch later list in Videos. Still open: album art
  and station logos, an EQ page, Spotify and YouTube sign in (the user runs
  `cliamp setup` once; the providers then appear by themselves).
- **Video.** YouTube and local video through mpv with the Video fit pipeline
  (already in), a "watch later" list, 480i when the KMS session lands.

## Phase E, the native path

- `linux-crt` kernel package with the 15 kHz patches, the DisplayPort DAC
  tier, interlaced 480i and 576i, per game modelines through Switchres in a
  KMS session. The study in `docs/studio-15khz.md` remains the reference.
