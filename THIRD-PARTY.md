# Third party pieces

What this project uses that it did not write, and under what terms. Three
things are compiled into the binary; everything else is spoken to, read or
fetched at run time.

The project's own licence, MIT, covers the whole repository: the code, this
documentation, and the screenshots and recordings under `docs/`. Where a
piece below carries somebody else's copyright as well, reusing that piece
means carrying their notice with it. [`LICENSE-SCOPE.md`](LICENSE-SCOPE.md)
says the same thing beside the licence itself.

## In the binary

- **The wordmark's letterforms.** `shell/assets/wordmark.txt` is a block
  character drawing of the word OMACRT, compiled in with `include_str!` and
  redrawn by the launcher as a two pixel grid. It is not Omarchy's drawing,
  and it is not a copy of it: it spells a different word, and its T was built
  by hand because the source has none. What it does take is the letterforms
  of the letters the two words share, out of Omarchy's own `logo.txt`.
  Copyright (c) David Heinemeier Hansson, MIT, and the licence travels with
  it as [`shell/assets/LICENSE.omarchy`](shell/assets/LICENSE.omarchy) because
  that is what MIT asks of a derivative as much as of a copy.

  **Omarchy's icon is not here.** The mark the boot opens on, and the one on
  every inner page, is the project's own: four bars crossed by the dark cut of
  the beam's return, described in `shell/src/assets.rs` as geometry rather
  than stored as a picture.

  This project is a fan project. It is not part of Omarchy, not endorsed by
  the Omacom Foundation, and does not speak for either.
- **TerminalTextEffects**, ported rather than linked: the boot sequence's
  laser etch and the screensaver's nine effects are pixel reimplementations of
  effects from
  [TerminalTextEffects](https://github.com/ChrisBuilds/terminaltexteffects)
  (MIT) by ChrisBuilds. No code is copied; the behaviour is followed.
- **font8x8**, the 8x8 bitmap font the launcher draws every glyph with,
  including the diagrams in the documentation. Daniel Hepper's public domain
  reconstruction of the IBM PC BIOS face, from
  [font8x8](https://github.com/dhepper/font8x8) (public domain / CC0). The
  table lives in `shell/src/font8x8.rs`.
- The Rust crates listed in `shell/Cargo.toml`, each under its own licence,
  MIT or Apache 2.0 in every case. `cargo tree` and `cargo license` print the
  current set.

## Programs it drives

None of these are included; the project runs whatever is installed and speaks
to it over a documented interface.

- [RetroArch](https://www.retroarch.com) (GPL 3.0) and its cores, each under
  its own licence.
- [mpv](https://mpv.io) (GPL 2.0 or later) and
  [yt-dlp](https://github.com/yt-dlp/yt-dlp) (Unlicense).
- [cliamp](https://github.com/bjarneo/cliamp), the music engine, spoken to over
  its Unix socket.
- [Hyprland](https://hyprland.org), [Quickshell](https://quickshell.org) and
  the rest of [Omarchy](https://omarchy.org).
- `ffmpeg`, `curl`, `pactl`, `bluetoothctl`, `hyprctl`.

## Data it reads and downloads

- **libretro thumbnails** (<https://thumbnails.libretro.com>): box art fetched
  on demand into `~/.cache/omacrt`. The images belong to their respective
  publishers; the collection's own terms are in the
  [libretro-thumbnails](https://github.com/libretro-thumbnails) repositories.
  Nothing is redistributed by this project.
- **RetroArch's databases** (`/usr/share/libretro/database/rdb`, or the user's
  own copy): read to turn an arcade set name into a title. Part of the
  [libretro-database](https://github.com/libretro/libretro-database) project.
- **Radio Browser** (<https://www.radio-browser.info>): the station directory,
  queried over its public API.
- **Spotify's oEmbed endpoint**: the album art of a playing track, through the
  public endpoint, without a key or an account.
- **RetroArch's `systematic` assets** (`/usr/share/retroarch/assets/xmb/systematic`):
  the console illustrations on the systems screen, read from the installed
  RetroArch. Part of RetroArch's asset set, CC BY, and nothing is
  redistributed here.
- **The libretro buildbot** (<https://buildbot.libretro.com>): the extra files
  a core needs and does not ship, Dolphin's `Sys` folder among them, fetched
  once on a first launch.
- **wttr.in** (<https://wttr.in>): one line of weather for the ambient page,
  no key and no account.
- **An Immich server**, if you set one up: your own photographs, over your own
  network, with a key you make yourself.
- **SDL_GameControllerDB** format: the pad mapping the wizard writes follows
  the format of [SDL_GameControllerDB](https://github.com/mdqinc/SDL_GameControllerDB)
  (MIT). No mappings from that project are bundled.

## Names

Omarchy, RetroArch, RGB-Pi, Bang & Olufsen, Sega, Nintendo, Sony, SNK, Capcom
and every other name that appears in the documentation or in a screenshot
belongs to its owner. This project is not affiliated with, endorsed by or part
of any of them.
