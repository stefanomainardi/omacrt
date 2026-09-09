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
display process has one of its own. Any process running as you can write to
them, which is the same trust level as your shell: nothing crosses a user
boundary. What they accept, in full:

- the launcher: the navigation inputs, `type <text>`, `screen <name>`,
  `play <path>` and `watch <file or url>`. `play` has to name something the
  scan has seen; `watch` has to be an `http`/`https` link or a file that
  exists.
- the display process: `quit`, `top <app id>`, `monitor on|off`,
  `mode <modeline>`, `key <name or evdev code> [ms]`, `shot <file.png>` and
  `record start <file.mp4|mkv|webm> [sink]`.

Two of those write a file where they are told to, so both insist on a name
that matches what they write: a program of yours can overwrite a picture or a
film of yours, and not the rest. `key` presses a key on the tube's keyboard,
which reaches whatever has focus there.

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

Two places on the desktop side do use a shell, and both are named here rather
than glossed over. The bar panel and the library overlay open a floating
terminal for the few commands that need a password or show long progress; that
terminal takes a command line, so everything variable in it is quoted. And
`bin/omacrt-pick` reads `OMACRT_PICKER` as a command with arguments - it used
to `eval` it, which granted nothing the caller did not already have but is not
a thing to leave in a script on somebody's PATH.

Media targets are placed after `--` so a file or URL beginning with a dash
cannot become an option to the player, and the same for a recording's output
file.

## What it kills

`omacrt doctor --fix`, and the sweep that runs when the launcher starts
or stops, will stop an emulator. It decides in three steps and all of them
have to hold: the process is **one of the programs this project starts**
(`retroarch`, `mpv`), its command line carries **this project's own
configuration file**, and **no launcher is above it** in the process tree.
`/proc` is read directly rather than through `pgrep`, so the survey can never
match the process doing the surveying. A polite signal first, then a hard one
after two seconds for a core that has wedged.

The first of those three is there because the second is not enough on its own:
an editor open on `~/.config/omacrt/retroarch.cfg` carries that path too, and
used to be read as an orphaned emulator.

## Your files

The project writes `~/.config/omacrt`, `~/.local/state/omacrt`,
`~/.cache/omacrt`, and, when you ask it to, RetroArch's configuration
under `~/.config/retroarch`. Every durable file is written atomically through a
temporary file and a rename, and the copy being replaced is kept as `.bak`.

It never deletes a game and never uploads anything anywhere. It does write
inside your collection, in three places, and it is better to say so than to
claim otherwise:

- a scan writes `<Title>.scummvm` into the folder of a ScummVM game, which is
  the file ScummVM needs to be launched by name;
- `library unpack` extracts a disc image into the folder that holds it, and
  never over a file that is already there;
- converting a video for the tube writes `<video>.crt.mp4` beside the source.
  A failed conversion removes that file only when the same run created it.

`bios import` copies into RetroArch's system directory. A file of the same
name and a different size is kept as `.replaced` rather than written over: a
BIOS is the one thing here that came off your own console.

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
  says so where it is. The read through of the next day found more, of a kind
  this one had not looked for: see below.
- `scripts/audit.py`, and the two advisories it accepts, with reasons.

## What the read through of 2026-09-09 changed

Three passes over the tree before it opened: how it behaves when things fail,
what it does to the machine that installs it, and what somebody looking for a
way in would find. The pattern in almost every finding was the same, and it is
worth naming: a guard existed and was not applied at every site.

Five things crossed a boundary and are closed.

- The library overlay took the program it runs out of the IPC payload that
  summons it, and started six of them the moment it opened. It resolves the
  binary from its own directory now, and the payload carries data only.
- The two places on the desktop that build a shell command line quote what
  goes into them, through the function that was written for it and called
  nowhere.
- The core data archive was downloaded to a predictable name in `/tmp`, where
  another user on the machine can put a symlink and have it followed. It is
  staged in this project's own cache, checked for the file it claims to carry,
  and moved into place only then. The EDID probe script had the same shape.
- `shot` and `record start` on the control pipes wrote wherever they were
  pointed; they insist on a name that matches what they write, and the
  recording's output is behind a `--`.
- `output.position` and a modeline's trailing flags reached Lua evaluated
  inside Hyprland. Both are checked before they get there.

Eight panics reachable from data alone are gone: a games list open on a system
a rescan removed, a cover name from the thumbnail server with a character
outside ASCII, a calendar's start time, a Bluetooth address that is not one, a
playlist that names itself, a cover PNG declaring enormous dimensions, a
preview window a few pixels wide, and an audio callback whose panic used to
take the picture with it. Release builds carry `overflow-checks`, so the class
below all of those fails where it happens.

The sweep no longer matches an editor that has one of our files open. Two pid
files are checked against `/proc` immediately before the signal. `state.json`
and `watch-later.tsv` go through the atomic write the rest of the project
uses. The tidy pass looks in the cache the code actually writes to and removes
the five megabyte leftovers a crashed scan used to keep forever. `off` puts
the sink's volume back. The installer's menu strip needs both markers before
it removes anything, retires the old plugin folders only on an unlocked
session, and validates the connector name before it reaches a root owned unit
file. The lease unit waits for the connector instead of sleeping through ten
seconds of the boot, and stands down when there is no television.

The pause menu's UDP fallback is deleted rather than documented. It could not
work - the interface it needed is disabled at every launch because a datagram
crashes RetroArch - and a UDP send to a closed port succeeds, so it reported
saves that had not happened.
