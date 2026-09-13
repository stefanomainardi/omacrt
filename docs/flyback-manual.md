<p align="center">
  <img src="media/mark-flyback.svg" width="120" alt="The Flyback mark">
</p>

# Flyback: driving it

How to run the compositor, everything it will answer to, and what to do when
it will not. What it *is*, and the measurements behind it, are in
[`flyback.md`](flyback.md); this page is the manual.

Flyback is not normally started by hand. `omacrt on` starts it, `omacrt off`
stops it, and everything below is reached through the CLI. The direct forms
are here because they are what a diagnosis needs.

---

## Running it

```
flyback run [connector]      take the lease and run the compositor
flyback probe [conn] [secs]  take the lease, set the mode, show a test card
flyback props [connector]    print the connector's DRM properties
flyback globals              what the desktop offers for leasing
```

`run` is what `omacrt on` calls. The connector defaults to
`output.connector` in `crt.toml`.

**`globals` takes nothing and is safe with the television running.** It is the
first thing to try when the tube will not come up, because it answers the
question the whole project turns on:

| it prints | it means |
| --- | --- |
| `lease: none` | the desktop compositor does not speak `wp_drm_lease_v1` |
| `lease: offered` | it speaks it and is offering nothing: the connector is not marked non-desktop |
| `lease: offered HDMI-A-1` | the connector is there to be taken |

If the third line is missing, the EDID override has not been applied or the
connector has not been re-probed, which `scripts/crt-lease-setup.sh` does, and
not this program. See [`15khz.md`](15khz.md#a-television-has-no-edid).

Clients reach it at `WAYLAND_DISPLAY=wayland-crt`.

## The control pipe

A named FIFO, 0600, in the user's own state folder
(`~/.local/state/omacrt/display.ctl`). One command a line. `omacrt display
send "<line>"` writes to it, and several CLI verbs are wrappers over it.

| line | what it does |
| --- | --- |
| `top <app-id>` | raise that client to the top of the stacking order |
| `key <name\|code> [ms]` | press a key into the focused client. Names: `rewind` `slow` `pause` `save` `load` `reset` `quit` `ff` `menu` `enter` `slot+` `slot-`, or a raw evdev code up to 0x2ff |
| `mode <modeline>` | set a timing, in Hyprland's modeline form. **Refused unless it passes `Modeline::fault`** |
| `rate <hz\|off>` | ask the television for a refresh without a mode change. Needs a variable refresh rate |
| `vrr <on\|off>` | ask the kernel for adaptive sync on the CRTC |
| `shot <file.png>` | write the current frame. Refuses a name that is not a `.png` |
| `record start <file.mp4\|.mkv\|.webm> [sink]` / `record stop` | film the tube, optionally with a sink's monitor as the audio track |
| `monitor <on\|off>` | a preview window on the desktop, so the tube can be watched without looking at it |
| `quit` | stop the compositor |

Anything else is refused instead of guessed at, and a line longer than the
read buffer arrives in two pieces and neither parses, the right
failure for a pipe that can reach a modeline.

## What it reads from `crt.toml`

| key | what the compositor does with it |
| --- | --- |
| `output.connector` | which connector to lease |
| `output.standard` | which of the shipped modelines to set at start-up |
| `output.csync` | whether to ask the converter for composite sync |
| `output.hfreq_khz` | **the safety band.** A timing whose line rate falls outside it is refused, at both places one can arrive. 15 to 16.5 by default |
| `output.vrr_min_hz` | how slow a refresh this set still follows. Nothing asks the tube for less, however slowly a program runs. A calibration of the room, 55 on a BeoCenter 1 |

`output.hfreq_khz` is the one setting in this project that can break hardware
if it is wrong. Widening it is deliberate and for a display that can take it:
a multisync monitor, an arcade chassis rated for 25 or 31 kHz.

## Switches, for measuring and not for daily use

Environment variables on the display process. Each one turns off a decision
described in
[`flyback.md`](flyback.md#it-schedules-frames-for-a-tube-not-for-a-desktop),
and that is how the numbers there were separated from each other.

| | |
| --- | --- |
| `FLYBACK_MARGIN_US` | how long before the vblank to start drawing. Auto by default, from what recent frames cost; a number sets it in microseconds; `off` goes back to drawing as soon as a client commits, which costs a frame |
| `FLYBACK_LATE_DRAW=off` | tell clients they may draw at the vblank, the conventional thing to tell them, instead of just in time |
| `FLYBACK_SLACK_US` | how much room a client gets on top of twice its measured drawing time. 3000 by default |
| `FLYBACK_TRACE` | print three lines a frame: commit to flip queued, flip queued to vblank, and the vblank interval. This is how the frame timeline in the study was measured, and it writes about 180 lines a second |
| `FLYBACK_ALL_PROPS` | make `props` print every DRM property and not the interesting ones |

All three switches off at once is this compositor with its scheduling removed,
the "before" in the study's numbers:

```
FLYBACK_LATE_DRAW=off FLYBACK_MARGIN_US=off omacrt on
omacrt display send "vrr off"
```

## Reading what it is doing

`omacrt status` while the tube is on:

```
Latency:    2.0 ms  0.12 frames  at 60.04 Hz  over the last 300
            95th 2.4 ms, worst 3.1; frame 16.62 to 16.69 ms
            every frame inside the mode, the longest 262.0 lines
```

The first line is the median from a client's commit to the start of its
scanout, and the rate the tube is actually being given, which under a
variable refresh rate is not the mode's own.

The second is the tail of the same two things: a frame that arrives late once
every few seconds is three samples in three hundred and does not move a median
at all, exactly what somebody watching calls a glitch.

**The third line is the one to read when the picture twitches**, and it exists
because the second could not show it. A television's vertical countdown accepts
sync inside a narrow window once it has locked, two lines either side on a
60 Hz standard. A field that leaves that window is retraced at the edge of the
window instead of on the sync that arrived, which is a visible jump for that
field. Three fields in three thousand six hundred left the window on the
afternoon this row was written, and the row above read `16.66 to 16.66 ms`
throughout: a median cannot show three samples, and a hundredth of a
millisecond is a quarter of a line.

So it counts instead. `N of 300 frames ran long, the worst L lines` means N
fields went more than two lines past the mode's vertical total, which for
NTSC here is 262. Anything above about 290 is past where this particular set
starts losing height, which is a property of the set and is written up in
[`15khz.md`](15khz.md). The window itself is a documented design for sets of
that kind, not a measurement of any one of them. So the row reports the count
and the number of lines, and leaves the reading to somebody who knows which
television is in the room.

A count above zero, with nothing asking for a rate other than the mode's own,
means the variable refresh rate is turning the compositor's own lateness into
physically longer fields. `FLYBACK_SLACK_US` makes that rarer at the cost of
latency, and the range worth trying is 1500 to about 4500.

The same two rows appear on the launcher's own Diagnostics page, so the tube
says it without a terminal.

The log is `~/.local/state/omacrt/display.log`. Lines to look for:

| line | meaning |
| --- | --- |
| `leased HDMI-A-1 (DRM connector 415)` | the lease was taken |
| `vrr: on (RequiresModeset)` | adaptive sync was asked for at start-up and the connector is capable |
| `vrr: the connector is not capable of it` | no FreeSync range in the EDID, or the range is too narrow. See [`15khz.md`](15khz.md#a-variable-refresh-rate) |
| `mode: ... set in 1.3 ms` | the timing was accepted. This is the test commit, not the modeset |
| `mode: first vblank 196.5 ms after the modeline was asked for` | how long the television was actually dark |
| `mode: refused, it asks the television for ...` | the guard stopped a timing. **This is the message that means the guard worked** |
| `render_frame: ...` | a flip was refused. Ten in a row and the watchdog gives up |

## When it will not come up

In this order, because each one rules out the one after it.

1. **`flyback globals`.** No connector named means the lease is not on offer
   and nothing else matters. Run `scripts/crt-lease-setup.sh on`.
2. **`omacrt status`.** No `CRT output` row means the connector is not
   connected as far as DRM is concerned; the converter may be off.
3. **The log's last ten lines.** A refused modeline says so in words.
4. **`omacrt doctor`.** It runs `globals` itself and prints the whole chain.

If the picture is there but wrong:

| what it looks like | where to look |
| --- | --- |
| rolling, no lock | the modeline's field rate, or csync not selected on the converter |
| a narrow strip in the middle | an interlaced timing was accepted and scanned out progressively. See [`15khz.md`](15khz.md#what-a-stock-kernel-still-cannot-do) |
| the whole picture twitches now and then | the frame length is moving. Read the third latency row: a count above zero is fields leaving the set's sync window. `FLYBACK_SLACK_US` between 1500 and 4500 trades latency for fewer of them |
| the picture loses height and keeps it | the refresh went below what this set follows. That is `output.vrr_min_hz` |

## What it will not do

One leased connector, one CRTC, one mode. No XWayland, no pointer, no touch,
no input devices at all: the pad is read by the programs themselves, and the
launcher presses keys by writing `key` on the pipe. No layer shell, no
decorations, no clipboard. Popups are configured so a client making one is
not broken, and are not drawn. The full list, with the reasons, is in
[`flyback.md`](flyback.md#what-it-does-not-do).

**It is experimental**, and it sets the line rate of a circuit tuned for one.
