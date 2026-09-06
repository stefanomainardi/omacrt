# Troubleshooting

Things that went wrong on the reference machine, what they turned out to be,
and where to look. Every emulator run leaves its own output in
`~/.local/state/omarchy-crt/game.log`; the launcher itself writes
`~/.local/state/omarchy-crt/shell.log`, including each game's exit status.

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
`crtgame`. `omarchy-crt focus` now focuses the running game, not the
launcher, and the launcher does not re-assert its fullscreen while a game
runs. Two launcher processes fighting over the tube produce the same
symptom; `omarchy-crt shell stop` waits for the launcher to exit and kills a
stuck one.

## Wrong game starts from a script

The control pipe (`omarchy-crt shell key`) is stateless: inputs land on
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
