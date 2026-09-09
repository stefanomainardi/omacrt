# Troubleshooting

Things that went wrong on the reference machine, what they turned out to be,
and where to look. Every emulator run leaves its own output in
`~/.local/state/omacrt/game.log`; the launcher itself writes
`~/.local/state/omacrt/shell.log`, including each game's exit status.

## Games die with SIGKILL, seconds or minutes in

`shell.log` says `game exited: signal: 9 (SIGKILL)`, `game.log` stops
mid-flight, nothing in the journal, no core dump. The same game started
from a terminal runs for hours.

Cause: SDL's PipeWire backend loads PipeWire's `module-rt` into the launcher,
which sets a 200 ms `RLIMIT_RTTIME` on the process so rtkit will grant its
audio thread realtime priority. Children inherit that limit and cannot raise
it back. With the limit in place rtkit also grants realtime to RetroArch's
PulseAudio thread, and the first time that thread runs 200 ms without
sleeping the kernel kills the whole process with SIGKILL. The launcher now
runs SDL on `SDL_AUDIODRIVER=pulseaudio`, which leaves the limits alone.
Check with:

```
grep "realtime timeout" /proc/$(pgrep -x retroarch)/limits   # must read unlimited
```

## RetroArch crashes when a game ends

A SIGSEGV core dump right after `[Core] Unloading core...` in `game.log`,
seen with snes9x and Genesis Plus GX alike. The launcher ran run-ahead with
a secondary core instance, whose teardown at exit crashes several cores.
Run-ahead now uses a single instance. Sending SIGTERM to a RetroArch that is
already shutting down can still crash it; the launcher never does that.

## "Application Not Responding" for a game that was fine

A fullscreen client whose workspace is not shown on its monitor blocks on
its next frame, and Hyprland reports it unresponsive. This happened when
something focused the launcher (its `crt` workspace) while the game ran on
`crtgame`. `omacrt focus` now focuses the running game, not the
launcher, and the launcher does not re-assert its fullscreen while a game
runs. Two launcher processes fighting over the tube produce the same
symptom; `omacrt shell stop` waits for the launcher to exit and kills a
stuck one.

## The CRT workspace is protected

Everything the launcher puts on the tube is a floating, pinned window on
the `crt` workspace: the launcher, the emulator, the video player. Pinned
windows draw above everything on their monitor and stay where they are
whatever the desktop does, so no stray desktop window, and no mouse pointer,
lands on the tube. The `window.open` handler (`isolate`) sends any foreign
window that opens on `crt`/`crtgame` back to the desktop, and pins the
emulator the launcher started (an "expected game" flag set right before the
spawn covers the frame before its class is known). The pointer is parked on
the desktop monitor after every focus. Verify:

```
hyprctl clients -j | jq '.[] | select(.workspace.name=="crt") | {class, pinned}'
```

## Talking to the running emulator over the network crashes it

RetroArch 1.22.2 dies with SIGSEGV inside its input poll the moment it
processes a network command (`network_cmd_enable`); an unknown command is
logged and survives, a known one kills the process. The launcher does not
use that interface any more: on the leased tube it presses the emulator's
own hotkeys through the compositor (`omacrt-display` control pipe,
`key pause|save|load|reset|quit`), and RetroArch runs with
`network_cmd_enable = false`. If you see `omacrt game ...` misbehave
outside the lease path, that is why.

## Is the tube ours?

```
omacrt-display props HDMI      # non-desktop = 1 means Hyprland leaves it alone
omacrt status --json | jq .connector
systemctl status omacrt-lease.service
tail ~/.local/state/omacrt/display.log
```

`hyprctl monitors` must not list the connector. If it does, the EDID
override is not in place: `sudo bin/omacrt-install --system` installs
it for every boot, `sudo scripts/crt-lease-setup.sh on` for now. `off`
gives the connector back to the desktop.

## The in-game menu (RGUI) is blank

RetroArch's own menu opens and pauses the game (the picture freezes) but
draws nothing over it on this gl/Wayland super-resolution path. So the
launcher does not rely on RGUI: it drives the emulator over the network
command interface instead (`omacrt game pause|save|load|reset|quit`),
and its own pause overlay is still to be built. Commands only reach
RetroArch when its input driver is `wayland`; a saved config carrying the
`x` (X11) driver silently disables both keyboard input and the command
loop under Wayland, so the launcher forces `input_driver = wayland`.

## RetroArch crashes are logged to the desktop

Every core dump makes Omarchy pop a "Process crashed: retroarch" toast.
The launcher's own crashes are fixed (the realtime limit and the run-ahead
secondary instance above); what remains is an intermittent SIGSEGV inside
RetroArch or a libretro core on this fresh install, unrelated to
omacrt (it happens from a plain terminal too). A core dump cannot be
suppressed per process here: `RLIMIT_CORE = 0` is ignored when
`kernel.core_pattern` pipes to systemd-coredump, and `PR_SET_DUMPABLE(0)`
is reset by `execve`. Clear the toasts with `omarchy-shell -q notifications
dismissAll`.

## Wrong game starts from a script

The control pipe (`omacrt shell key`) is stateless: inputs land on
whatever screen the launcher shows, and the main menu remembers its cursor.
Send `home` first to return to the top of the main menu, then navigate.

## Blue tint, garbage register reads, lost lock

The RGB-Pi 2 sits at 0x78 on the HDMI DDC bus. Omarchy's shell probes every
DDC bus with `ddcutil ... detect` and `getvcp` for brightness control,
including this one, so bus traffic to 0x37 interleaves with the launcher's
own register access. The launcher reads registers in a single I2C
transaction (page select and read together) and serializes its own users
with a lock on the bus device. A DAC reset is the last resort: it clears the
sync selection, and writing a garbage value back to it is what produced the
blue picture once.

## The picture died in the middle of a game

The display process owns the leased connector, so everything on the tube is
one of its Wayland clients: when it goes, the launcher, the emulator and the
picture go with it. `omacrt on` leaves a watchdog behind for exactly
this, and it puts the display, the timing, the DAC and the launcher back
within a second. If the tube stayed black, the watchdog gave up:

```sh
tail ~/.local/state/omacrt/watchdog.log   # why it stood down
tail ~/.local/state/omacrt/display.log    # what the display said as it died
omacrt on                                 # start again by hand
```

Three restarts inside two minutes is the limit. A display that cannot stay up
that long is a bug in the log, not something to restart for ever.

## A setting went back to its old value

Every durable file is written through a temporary file and a rename, and the
copy being replaced is kept next to it as `.bak`. A file that cannot be read
falls back to that backup, so a truncated `settings.toml` costs the last
change rather than every setting.

A file written by a newer build than the one running is not overwritten: it is
copied aside as `settings.toml.v<N>` first, and a line about it goes to the
launcher's log. Going back to the newer build and moving that copy over the
current file restores what it held.

## The photo frame says "reading the collection..."

That line is the frame with nothing to show yet, and it names its own reason
underneath. In order:

- **`no immich.toml in ~/.config/omacrt`.** The frame is not set up. Write
  the file with `url` and `key`; `docs/cli.md` says where the key comes from.
- **`... did not answer, or refused the key`** from `omacrt frame check`.
  Either the address is wrong or the key has been revoked. The address must
  carry no trailing slash and no path: `https://immich.example.lan`.
- **`the memories source has no photographs`.** The server has no memory for
  today, which happens on a young collection. Set `source` to `favorites` or
  `all` in `[frame]`, or name an album.

With the server reachable, the first picture takes as long as one fetch and
one ffmpeg pass. `omacrt frame fill 60` does that work ahead of an
evening, and after it the frame fills in the first second even with the server
switched off, because the prepared pictures are its own collection.

A photograph with nothing written under it was prepared before the caption
files existed. `omacrt frame clear` and one `frame fill` puts them back.

## The frame's pictures are all fetched again

The cache is keyed by the size of the screen: pictures prepared for 240 lines
are not the pictures for 288. Changing standard, or the line count of the
launcher, is a new set. `omacrt frame fill` prepares for whatever the
tube is set to now, so run it after changing standard, not before.

## The weather line is the wrong town

wttr.in guesses from the address when `[frame] weather` is empty, and a VPN
moves that guess a country. Put a place in the setting. The line is cached
half an hour under the name of the place it was asked for, so a change shows
up on the next fetch rather than on the next picture.

## The monitor says the graphics have no counters

`gpu_busy_percent` and the video memory files are an amdgpu feature. On
anything else that panel says so and the rest of the page carries on: nothing
here is worth a driver-specific dependency.

Temperatures come from whichever chip `hwmon` names: `k10temp` and `coretemp`
for the processor, `amdgpu` for the card, `nvme` for the disk. A machine that
names none of them leaves those numbers out.

The processor number is high the moment the monitor opens because the
launcher is drawing it, which is honest: at 320 by 240 with a bank of meters
moving, the launcher really is the busiest thing on the machine.

## "Something is playing" with the launcher on its own home menu

An emulator that outlived its launcher. The signal that stopped the launcher
went to the launcher alone: the child was reparented to systemd and kept
running, holding the audio sink and answering every "is anything playing"
with yes. It also refuses every launch from the desktop, since `omacrt
play` asks that question first.

```
omacrt doctor        # lists it, with its pid
omacrt doctor --fix  # stops it, with SIGKILL if a wedged core ignores the first signal
```

`shell stop`, `off`, `on` and `shell start` all do that sweep now, and the
watchdog does it once a minute while the tube is on, so this should not
happen again. An emulator is ours when its command line carries our own
RetroArch configuration, and an orphan when no launcher is above it in the
process tree, so nothing else on the machine is ever touched.

## A command from the desktop does nothing at all

Three things have caused this, in the order they are worth checking.

**The launcher is an older build than the CLI.** A command that goes down the
control pipe has to be understood at both ends: `omacrt` sends it and
`omacrt-shell` reads it. Install both and restart the launcher
(`omacrt shell restart`); the launcher's log says `control: unknown
input <name>` when this is what happened.

**Something is already playing.** `omacrt play` refuses, and says so in
the terminal. From a menu row there is no terminal: the picker turns that into
a notification and offers to stop what is playing.

**walker was already open.** It is a single instance application, so a second
invocation hands its arguments to the first and exits without printing, which
is indistinguishable from a row that does nothing. `omacrt-pick` closes
whatever walker is open first. Its own log is
`~/.local/state/omacrt/pick.log`, and it names every step.
