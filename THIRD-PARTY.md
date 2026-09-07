# Third party pieces

What this project uses that it did not write, and under what terms. Nothing
here is bundled except the font, which is public domain.

## In the binary

- **font8x8**, the 8x8 bitmap font the launcher draws every glyph with,
  including the diagrams in the documentation. Daniel Hepper's public domain
  reconstruction of the IBM PC BIOS face, from
  [font8x8](https://github.com/dhepper/font8x8) (public domain / CC0). The
  table lives in `shell/src/font8x8.rs`.
- The Rust crates listed in `shell/Cargo.toml`, each under its own licence
  (MIT or Apache 2.0 in every case at the time of writing). `cargo tree` and
  `cargo license` will print the current set.

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
  on demand into `~/.cache/omarchy-crt`. The images belong to their respective
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
- **SDL_GameControllerDB** format: the pad mapping the wizard writes follows
  the format of [SDL_GameControllerDB](https://github.com/mdqinc/SDL_GameControllerDB)
  (MIT). No mappings from that project are bundled.

## Names

Omarchy, RetroArch, RGB-Pi, Bang & Olufsen, Sega, Nintendo, Sony, SNK, Capcom
and every other name that appears in the documentation or in a screenshot
belongs to its owner. This project is not affiliated with, endorsed by or part
of any of them.
