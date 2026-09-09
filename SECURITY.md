# Security

This project drives a television, indexes a game collection and talks to a few
programs on the same machine. It is a desktop tool for a single user, not a
service: nothing listens on a network port and no daemon accepts remote input.
One credential can exist, and only if you create it: an API key for your own
photograph server. What follows is everything it touches, so you can decide
whether you are comfortable running it.

It was read through for this on 2026-09-08, and what that pass changed is at
the end.

## Reporting a problem

Open an issue, or write to the address on
[stefanomainardi.com](https://www.stefanomainardi.com) if you would rather not
say it in public first. There is no bounty, and there is no security team: it
is one person and a television.

## What runs as root, and only that

One thing needs root, once, at install time:

- `bin/omacrt-install --system` installs
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
`~/.local/state/omacrt/shell.ctl`, created with mode `0600`, and the
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
- the files a libretro core needs and does not ship, from the libretro buildbot
- one line of weather from `wttr.in`, and a calendar from an `.ics` address,
  both only when the ambient page is set up
- your own photographs from your own Immich server, if you set one up

All eight go out through `curl`, and all eight ask one function for it:
`net::curl` restricts the protocol list to HTTP and HTTPS on the request and
on any redirect, sets a timeout and a size cap, fails on an error status
rather than saving the error page, and passes arguments as arguments. A
crafted URL cannot make it read a local file, a redirect cannot leave those
two protocols, and none of that is a decision at the call site: what a caller
chooses is how long and how large its own fetch may be.

A name that came from a server never becomes a path. The photograph server's
asset identifiers are checked against letters, digits and dashes before they
are used in a file name, so an answer of `../../.ssh/authorized_keys` is not a
photograph and is skipped. Everything lands in `~/.cache/omacrt`, written
beside its final name and moved into place, so a fetch that is interrupted
leaves nothing that looks finished.

## Your photograph server, if you have one

The photo frame reads an Immich server on your own network. It needs
`~/.config/omacrt/immich.toml` with an address and an API key, which you
create in Immich and which needs read access and nothing else. The file is
yours to write and this project only reads it.

The key is handed to curl **on its standard input**, never as an argument, so
it does not appear in the process list where every other program on the
machine could read it. It is not logged, not printed by any command, and never
sent anywhere but to the address in that file. Nothing else in the project
holds a credential.

## What is executed

The project runs external programs: `retroarch`, `mpv`, `yt-dlp`, `cliamp`,
`bluetoothctl`, `curl`, `ffmpeg`, `hyprctl`, `pactl`, `systemctl`. Every one of
them is given an argument list. **There is no shell anywhere in the launcher.**
The one place that used one, for a single constant command, was changed to an
argument list, so nothing a file, a server or a game's name contains can ever
be read as a command.

The one exception is on the desktop side: the library overlay opens a floating
terminal for the few commands that need a password or show long progress, and
quotes every path it passes.

Media targets are placed after `--` so a file or URL beginning with a dash
cannot become an option to the player.

## What it kills

`omacrt doctor --fix`, and the sweep that runs when the launcher starts
or stops, will stop an emulator. It decides in two steps and both have to
hold: the process's command line carries **this project's own configuration
file**, which nothing else on the machine passes, and **no launcher is above
it** in the process tree. `/proc` is read directly rather than through
`pgrep`, so the survey can never match the process doing the surveying. A
polite signal first, then a hard one after two seconds for a core that has
wedged. Nothing else on the machine is ever a candidate.

## Your files

The project writes `~/.config/omacrt`, `~/.local/state/omacrt`,
`~/.cache/omacrt`, and, when you ask it to, RetroArch's configuration
under `~/.config/retroarch`. Every durable file is written atomically through a
temporary file and a rename, and the copy being replaced is kept as `.bak`.

It never deletes a game, never writes inside your collection, and never uploads
anything anywhere.

## Dependencies

Thirteen direct crates and 157 in the locked tree, and the lock file is
committed so a build is the same build. `scripts/audit.py` checks every locked
version against the RustSec advisory database over OSV's API, needs nothing
installed but Python, and runs in CI. Two advisories stand today, both against
`cgmath`, which arrives through `smithay` for the compositor and is never
called from this code: one says it is unmaintained, the other that a matrix
column swap is unsound when both indices are the same. They are listed in that
script with those reasons, and anything new fails the build.

## What it does not do

No telemetry. No analytics. No crash reporting. No auto update. No account, no
token, no key. If you see it opening a connection to anything not listed above,
that is a bug worth reporting.

## What the read through of 2026-09-08 changed

- The shell is gone from the launcher: the one command that went through
  `sh -c` is an argument list now, and the module that ran it cannot run a
  command line at all.
- Identifiers from the photograph server are checked before they become file
  names.
- The photo cache is written beside its name and moved into place, like every
  other file this project owns.
- A note beside a picture cannot be broken by a name with a newline in it.
- The float sorts behind `--dump` use a total order, so a value that is not a
  number is sorted rather than a panic.
- A DRM lease that arrives without a file descriptor is an error the display
  process reports rather than a panic the watchdog would restart in a loop.
- The eighteen places that could panic outside the tests are ten: seven mutex
  locks, one value the compositor framework guarantees, one thread the
  launcher cannot start without, and one array a line above pushed to. Each
  says so where it is.
- `scripts/audit.py`, and the two advisories it accepts, with reasons.
