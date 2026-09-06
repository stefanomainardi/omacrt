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

## Phase A2, playing well

- **In game menu, Omarchy style.** A combo on the pad (Select + Start, or a
  home button) and a key on the keyboard pause the game and bring the
  launcher's pause screen to the tube: resume, save state, load state,
  reset, a few emulator options that matter (fast forward, rewind, run
  ahead, aspect), and back to the launcher. RetroArch is driven through its
  network command interface; the launcher already sees the pad in the
  background and Hyprland carries the keyboard bind.
- **Save states.** One slot per game with a timestamp, quick save and load
  from the pause menu, "resume where you left" on the game row.
- **Pad recognition.** Identify the pad family and layout on plug (SDL
  database plus vendor and product ids), map RetroPad buttons per family,
  show the right glyphs everywhere, remember per pad. Unknown pads get a
  short mapping wizard on the tube.

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

## Phase D, a television, not only a console

- **Media player for the tube.** Music from local files with a visualiser
  built for 240p, playlists, album art; the same pad and keyboard language as
  the games.
- **YouTube.** Search and play through `yt-dlp` and mpv with the Video fit
  pipeline, a "watch later" list, 480i when the KMS session lands.
- **Spotify.** `librespot` as a Spotify Connect endpoint on the television
  plus a player screen, for Premium accounts.

## Phase E, the native path

- `linux-crt` kernel package with the 15 kHz patches, the DisplayPort DAC
  tier, interlaced 480i and 576i, per game modelines through Switchres in a
  KMS session. The study in `docs/studio-15khz.md` remains the reference.
