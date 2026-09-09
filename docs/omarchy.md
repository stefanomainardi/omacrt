# Omarchy on the tube

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
  `omacrt watch`, all through the same player with the tube's own fit
  pipeline (480p streams, 480i or 576i by frame rate when the modeline lands).
- **RetroArch.** Driven without its menu: a configuration written per launch
  from `systems.toml`, hotkeys pressed by the compositor, save states read
  back for the launcher's rows.
- **The menu.** A **Television** entry in Omarchy's own menu, through the
  extension file Omarchy reads for exactly that
  (`~/.config/omarchy/extensions/omarchy-menu.jsonc`): power, channels,
  picture, sound, the library, capture, pads and diagnostics, each row calling
  the same CLI a terminal would. Nothing patched.
- **Walker.** *Play a game...* in that menu hands the whole collection to
  Omarchy's own runner and plays what comes back on the television, turning
  the tube on if it is off. Twenty thousand games belong in a fuzzy finder,
  not in a nested menu.
- **Immich.** The photo frame reads the house's own photograph server: this
  day in the years before, an album, the favourites, with where and when and
  who from the server's own metadata. The pictures never leave the network.
- **The rest of the box.** PipeWire routes the launcher, RetroArch, mpv and
  cliamp to the television's audio and back; the I2C bus of the HDMI port
  configures the DAC's sync; systemd hands the tube over at boot; pads come
  through SDL with a wizard for the unknown ones.

## The bar plugin

<p align="center">
  <img src="screens/panel.png" width="300" alt="The bar panel">
  <img src="screens/library-overlay.png" width="540" alt="The library overlay">
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

See [`plugin/README.md`](../plugin/README.md).
