# Security

This project drives a television, indexes a game collection and talks to a few
programs on the same machine. It is a desktop tool for a single user, not a
service: there is no network listener, no daemon accepting remote input and no
credential of any kind stored by it. What follows is what it does touch, so you
can decide whether you are comfortable running it.

## Reporting a problem

Open an issue, or write to the address on
[stefanomainardi.com](https://www.stefanomainardi.com) if you would rather not
say it in public first. There is no bounty, and there is no security team: it
is one person and a television.

## What runs as root, and only that

One thing needs root, once, at install time:

- `bin/omarchy-crt-install --system` installs
  `scripts/crt-lease-setup.sh`, `scripts/edid-non-desktop.py` and a systemd
  unit that runs the first of those at boot.

That script marks the DAC's connector as *non-desktop* by writing an EDID
override through debugfs, so the compositor offers the output for DRM leasing
instead of managing it. It writes to `/sys/kernel/debug/dri/*`, the connector's
`status` file under `/sys/class/drm`, and one file under `/run`. It reads the
connector's own EDID, flips a bit and writes it back. It refuses a connector
name that is not made of letters, digits and dashes, and refuses one that does
not exist.

The unit runs with `NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome`,
`PrivateNetwork`, a capability bounding set of `CAP_DAC_OVERRIDE` and
`CAP_SYS_ADMIN`, and write access to nothing beyond the three paths above.

Everything else, the launcher, the CLI, the display process and the bar plugin,
runs as your own user.

## The control pipe

The launcher listens on a named pipe at
`~/.local/state/omarchy-crt/shell.ctl`, created with mode `0600`, and the
display process has one of its own. They accept a small vocabulary: navigation
inputs, `type <text>`, `watch <file or url>`, `key <name>`, `record`, `mode`.
Any process running as you can write to them, which is the same trust level as
your shell: nothing more is granted, and nothing crosses a user boundary.

## What is downloaded, and from where

Nothing is downloaded during installation. While running, the project fetches:

- box art and the per system name index from `thumbnails.libretro.com`
- radio stations and their logos from the Radio Browser directory
- album art through Spotify's public oEmbed endpoint
- YouTube results and streams through `yt-dlp`, when you ask for them

Every fetch goes through `curl` with the protocol list restricted to HTTP and
HTTPS, a size cap of 25 MB, a timeout, and arguments passed as arguments: a
crafted URL cannot make it read a local file, and a redirect cannot leave those
protocols. Downloads land in `~/.cache/omarchy-crt` and are shrunk with ffmpeg
before use.

## What is executed

The project runs external programs: `retroarch`, `mpv`, `yt-dlp`, `cliamp`,
`bluetoothctl`, `curl`, `ffmpeg`, `hyprctl`, `pactl`. All of them are invoked
with an argument list, never through a shell, with two exceptions:

- `menu::launch` runs one constant command (`systemctl poweroff`) through
  `sh -c`;
- the library overlay opens a floating terminal for the few commands that need
  a password or show long progress, quoting every path it passes.

Media targets are placed after `--` so a file or URL beginning with a dash
cannot become an option to the player.

## Your files

The project writes `~/.config/omarchy-crt`, `~/.local/state/omarchy-crt`,
`~/.cache/omarchy-crt`, and, when you ask it to, RetroArch's configuration
under `~/.config/retroarch`. Every durable file is written atomically through a
temporary file and a rename, and the copy being replaced is kept as `.bak`.

It never deletes a game, never writes inside your collection, and never uploads
anything anywhere.

## What it does not do

No telemetry. No analytics. No crash reporting. No auto update. No account, no
token, no key. If you see it opening a connection to anything not listed above,
that is a bug worth reporting.
