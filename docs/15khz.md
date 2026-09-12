# Driving a 15 kHz television from a modern PC

This is the research the project rests on: what a television with a SCART
socket wants on its pins, why a modern graphics card cannot give it that on its
own, and how a DAC, a leased connector and a wide modeline answer each of those
in turn. It is worth reading before buying anything.

## What a 15 kHz television actually wants

A consumer CRT with a SCART socket is not a monitor with a different plug. It
expects, on those pins:

| Pin | Signal | Level |
| --- | --- | --- |
| 15, 11, 7 | Red, green, blue | 0.7 Vpp each |
| 20 | Composite sync | 0.3 to 1 V |
| 16 | RGB blanking | 1 to 3 V, or the set stays on composite and shows black and white |
| 8 | AV switching | 9.5 to 12 V, convenient, not required |

Two of those are the ones people get wrong. A PC's VGA output has separate
horizontal and vertical sync at TTL level, five volts: fed straight to pin 20
it is out of specification by a factor of five, and the two signals have to be
combined into one anyway. And without a volt or two on pin 16 the television
never switches its decoder off, so a perfect RGB signal arrives and is shown
as monochrome composite.

The line rate is the other half. 15.734 kHz for NTSC, 15.625 kHz for PAL,
which is a horizontal frequency roughly half of what the oldest VGA monitor
will lock onto. It is not a resolution, it is a scan rate: a television draws
262 or 312 lines per field and it cannot draw more.

## Why a modern GPU cannot do this on its own

Three walls, in the order they stop you.

**No analogue output.** The last AMD card with an analogue path through
DVI-I was the R9 380X. Everything since is digital only, so something has to
convert.

**Converters refuse the clock.** 320 by 240 at 15 kHz needs a pixel clock
around 6 or 7 MHz. Almost every active DisplayPort to VGA adapter refuses
anything under about 25 MHz. The exception is the Realtek RTD2166 and its
successor the RTD2168, which take roughly 8 to 210 MHz, have no automatic
deinterlacer and no crystal of their own. That is why the arcade community
names those two chips and no others.

**HDMI has a floor.** HDMI's TMDS encoding starts at 25 MHz, and `amdgpu`
will not program a mode below it on an HDMI connector. Taken at face value that
leaves one road, DisplayPort into an RTD2166 with a kernel carrying the 15 kHz
patches, and an emulator on KMS rather than on a desktop.

## What this project does

The third wall has a door in it. **The DAC is the sink, not the television.**
An HDMI to SCART DAC that accepts arbitrary timings presents itself as a
normal HDMI monitor: the 25 MHz floor is satisfied by the link to the DAC,
which then produces the 15 kHz analogue signal on the other side. The mode the
GPU programs is wide rather than slow:

```
NTSC   72 3520 3695 4033 4577  240 242 245 262  -hsync -vsync
PAL    72 3840 3948 4290 4608  288 291 294 312  -hsync -vsync
```

72 MHz over 4577 pixels of total line length is 15.731 kHz, and 262 lines of
that is 60.04 Hz. The clock is high, the line rate is a television's. The
active width, 3520 pixels for a 240 line frame, is the arcade world's
"super resolution" trick: horizontal resolution to spare, so a console's 256
or 320 pixels land on a whole number of them and the television, which only
ever draws 4:3, does the rest. One game line on one television line, no
scaling anywhere.

A mode change for each game, which no desktop protocol can express, is answered
by **DRM leasing**. It is a Wayland protocol built for virtual reality headsets,
where a compositor hands one connector over to an application that then owns
it. The desktop keeps every other output; the leased one belongs to whatever
took it. So:

1. A systemd oneshot marks the DAC's connector *non-desktop* by overriding its
   EDID at boot. Hyprland sees that flag and stops configuring the output,
   offering it for leasing instead.
2. `flyback` takes the lease, becomes DRM master for that
   connector alone, programs the modeline through DRM, and runs a small
   Wayland compositor of its own on it.
3. The launcher, RetroArch and mpv are its clients, forced fullscreen at the
   output's size. Nothing from the desktop can land there, and the timing can
   be changed live, per console, while the desktop carries on untouched.

No patched kernel, no second virtual terminal, no `chvt`, no root for the mode
change. What is genuinely impossible under a compositor is asking it for a
timing: `wlr-output-management`, the protocol for that, carries a width, a
height and a refresh rate and nothing else, so a modeline cannot be expressed
through it at all. Leasing sidesteps the question by handing over the connector
rather than describing a mode to somebody else.

## What a stock kernel still cannot do

**Interlace.** `amdgpu` without the 15 kHz patches accepts a 480i modeline,
programs something, and scans out a narrow strip. On a Radeon RX 7700/7800 XT
(Navi 32, DCN 3.2) running the stock Arch kernel 7.2.4, through the leased
connector rather than through a compositor, `omacrt mode 480i` **succeeds**:
the modeset returns without error, the DAC keeps its lock, `status` reports
3520x480 at 59.927 Hz, and the television shows a narrow image in the middle of
the screen. Nothing rejects the mode on the leased path. It is programmed with
interlaced timings and scanned out progressively, which at a 525 line vertical
total and 15.731 kHz is 29.96 Hz, and no television locks to that.

Five things in the current upstream tree stand between that modeline and a
picture on DCN 3.2.

1. `fill_stream_properties_from_drm_display_mode` in `amdgpu_dm.c` never
   copies `DRM_MODE_FLAG_INTERLACE` into `timing_out->flags.INTERLACE`, so
   everything downstream believes the timing is progressive, which is what
   produces the strip.
2. `optc1_validate_timing` in `dcn10_optc.c` returns false for any interlaced
   timing, under the comment *"Temporarily blocking interlacing mode until
   it's supported"*. Forcing the flag alone turns a wrong picture into no
   picture.
3. The OTG register that enables interlacing is missing from the per-ASIC
   tables for DCN 3.x, though the code that writes it is already there:

   ```c
   /* Interlace */
   if (REG(OTG_INTERLACE_CONTROL)) {
           if (patched_crtc_timing.flags.INTERLACE == 1)
                   REG_UPDATE(OTG_INTERLACE_CONTROL, OTG_INTERLACE_ENABLE, 1);
   ```

   DCN 1 and 2 carry an address for it and interlace works there; for DCN 3.2
   the entry is absent, so the offset is zero and the block is skipped.
4. DML1, which is what DCN 3.2 validates through (`using_dml2 = false` in
   `dcn32_resource.c`), doubles `VRatio` for an interlaced timing when the
   ASIC does not claim `ptoi_supported`, and `dcn3_2_ip` sets that to false.
   The doubling then fails the scaler taps validation.
5. `interleave_en` on the scaler's line buffer is never set from the timing,
   in `dcn10_hwseq.c` and `dcn20_hwseq.c`.

Most of the work is upstream already and needs nothing: the OTG programming
code, the front porch workaround, the field number handling, `dest.interlaced`
reaching DML from `dcn20_fpu.c`, and the HDMI stream encoder, which the 15 kHz
patches do not touch at all. The hardware is not the limit either, since AMD's
own public headers for this ASIC carry the register and its bit:

```c
#define regOTG0_OTG_INTERLACE_CONTROL                     0x1b44
#define OTG0_OTG_INTERLACE_CONTROL__OTG_INTERLACE_ENABLE__SHIFT  0x0
#define OTG0_OTG_INTERLACE_CONTROL__OTG_INTERLACE_ENABLE_MASK    0x00000001L
```

For a DCN 3.2 card the whole of patch 03 comes to about ten lines: one register
table entry, one shift and mask entry, three in `amdgpu_dm.c`, one deletion in
`dcn10_optc.c`, two in `dcn20_hwseq.c` and two in `display_mode_vba.c`.

With those changes the display engine does interlace. An owner of an RX 7700S,
which is RDNA 3 and the same DCN 3.2 display engine as the card here, reported
interlaced output as a black screen (`D0023R/linux_kernel_15khz#11`), then
"Works great" with the version of patch 03 that covers DCN 3, and left a note
for anybody arriving with the same problem:

> for those reading who have an issue with a horizontally squished interlaced
> image or no image on any AMD GPU newer than the RX 5x00 series, try this
> patch

A horizontally squished image is what this machine shows. Two warnings come
with the patches. Vertical sync values want odd numbers on DCN 3, since on a
6700 XT the field order came out wrong until they were made odd
(`D0023R/linux_kernel_15khz#16`, `1280 1360 1536 1664 480 489 493 525`), while
the interlaced modelines in `crt.toml` are even (`480 484 490 525`). And
`amdgpu.dc=0` gives working interlace only on cards old enough to have the
legacy DCE path, so it is not an option on Navi, where every part requires the
Display Core.

This project does not patch the kernel or the driver. `output.interlace` is off
by default, 480 line consoles are shown at 240p instead, which is what the line
count per console exists for, and this section is here to say what that costs
and what changing it would take.

What it costs is narrower than it first looks. Interlace concerns one group of
systems, the Dreamcast, Naomi, GameCube and the PlayStation 2 if it ever
arrives, and not the rest of a collection, which at 240p is already in its
native shape. On those systems the gain is the full vertical resolution in
text and menus. The price is the shimmer of alternating fields, which is the
authentic look of that era and which many people find worse than 240p, and
that is the whole of the trade.

For anybody who wants the interlaced modes, the patches are maintained at
`D0023R/linux_kernel_15khz`, which tracks Calamity's original work from
GroovyMAME and Switchres. The eight of them, in the order they are applied:

1. Removes the minimum dot clock limits and enables 15, 25 and 31 kHz modes in DRM.
2. A general interlace correction.
3. Interlace for the DCN 1 to 4 display engines, which is RDNA 1 to 4 and the APUs.
4. Interlace for the older DCE engines.
5. A correct PLL calculation at low clocks.
6. User modes added through an ioctl without Xorg, which is what Switchres needs.
7. DDC.
8. Forces even line counts in interlaced modes.

They touch `drivers/gpu/drm/amd/display`, so an LTS update that moves that
code can break them until the repository catches up. Its history says it
follows stable within days. Building one is a package that coexists with the
stock kernel, its own UKI and its own boot entry, and the default entry stays
the stock one.

## What must never reach the connector

A television is not a monitor that shrugs at a signal it cannot use. Its
horizontal deflection is a tuned circuit - a flyback transformer and an
output transistor sized for one line rate - and driving it well above that
destroys both. Everything here runs at 15.6 or 15.7 kHz, so any other rate
is a mistake rather than an intention: a typo in `crt.toml`, a bug here, or
anything else on the machine writing to the control pipe. That pipe is
owner-only - a named pipe created 0600 in the user's own state folder - so
the danger is a mistake and not a stranger, which is exactly the kind of
danger that reaches hardware.

So no timing reaches the kernel without passing `Modeline::fault`, at the
two places one can arrive: the modeline read from `crt.toml` at start-up,
and the `mode` command. It refuses a line rate outside `output.hfreq_khz`,
which is 15 to 16.5 kHz by default and is the one setting in this project
that can break hardware if it is wrong. It also refuses what cannot work at
all - sync outside blanking, blanking inside the picture, a field rate no 15
kHz set locks to - because the kernel is not obliged to notice those before
the television does.

Widening the band is deliberate and in one place, for a display that can
take it: a multisync monitor, an arcade chassis rated for 25 or 31 kHz.

The variable refresh rate below is on the other axis and cannot do this: it
stretches the vertical blanking and never moves the line rate by a single
hertz. That is why the worst seen from it is a picture that loses height.

## A variable refresh rate

A television's horizontal rate must not move: the flyback transformer and the
deflection circuit are tuned for one. The vertical rate is another matter,
because the vertical oscillator re-triggers on sync. So every refresh
emulation asks for - 49.70 for PAL, 59.92 for a Mega Drive, 60.0988 for a
NES, 57.5 for one arcade board or another - is reachable by changing the
vertical total alone and leaving the line rate at 15.731 kHz. That is exactly
what adaptive sync does in hardware: it stretches the vertical blanking and
touches nothing else.

Today a change of refresh costs a mode change. Measured through the leased
connector on Navi 32, an atomic commit that carries a new modeline blocks for
**166 to 190 ms**, whether it moves the whole standard or only the vertical
total, and the television is dark for it. Adaptive sync would cost nothing.

**The analogue chain takes it.** Switching between vertical totals of 262,
274, 288 and 312 lines at a constant 15.731 kHz - 60.04 Hz down to 50.4 Hz -
the RGB-Pi 2 keeps its lock at every step. A stretched vertical blanking
reaches the set intact. What the tube itself does about picture height across
that range is a question for a camera, not for software: the one report of
adaptive sync on a CRT, on multisync PC monitors rather than televisions,
says some sets change vertical size in proportion to the blanking interval.

**And so does the driver, with two things set.** The pieces:

1. `vrr_capable` on an HDMI connector comes from an AMD vendor block in the
   EDID, which the display microcontroller parses. Nine bytes,
   `68 1a 00 00 01 01 <min> <max> 00`, and since this project writes the
   connector's EDID anyway, `OMACRT_FREESYNC=48:62 crt-lease-setup.sh on`
   adds it. The range matters: `mod_freesync_build_vrr_params` caps the
   declared maximum at the mode's own nominal rate and then wants
   `refresh_range >= MIN_REFRESH_RANGE`, which is 10 Hz, so at a nominal
   60.04 Hz the minimum has to be 50 or below. A range of 55 to 66 collapses
   to five and is refused in silence.
2. `amdgpu.freesync_video=1` on the kernel command line. Without it the
   config computed for the CRTC is overwritten a few lines further on and
   the state falls back to `VRR_STATE_INACTIVE`.

With both, the timing generator is programmed with room: `amdgpu_dm_dtn_log`
reports `vmin 261 vmax 327` for the tube's OTG, which is 60.04 Hz down to
48 Hz of vertical blanking at an unchanged 15.731 kHz, and the driver logs
`VRR packet update: enabled=1 state=3`, which is `VRR_STATE_ACTIVE_VARIABLE`.

The scanout then follows the program, frame by frame. A client committing at
a fixed rate, and the interval between two vblanks measured from the
compositor's own DRM events:

| client | scanout |
| --- | --- |
| 60 Hz | 60.05 Hz |
| 57 Hz | 56.80 Hz |
| 55 Hz | 54.82 Hz |
| 50 Hz | 49.92 Hz |

**And the picture keeps its height.** A camera on the tube through seven
steps of vertical total, measured as the ratio of picture height to picture
width so that the camera's own drift cancels - the width cannot change,
because the line rate never does:

| vtotal | Hz | frame longer by | height |
| --- | --- | --- | --- |
| 262 | 60.04 | reference | - |
| 264 | 59.59 | +0.8% | +0.07% |
| 266 | 59.14 | +1.5% | +0.32% |
| 270 | 58.26 | +3.1% | -0.12% |
| 276 | 57.00 | +5.3% | -0.10% |
| 286 | 55.00 | +9.2% | +0.06% |

If the vertical amplitude followed the frame period, the last row would read
+9.2%. The seventh step repeats the first, and its 0.69% disagreement over
forty-two seconds is the camera moving; the residuals above have that drift
taken out linearly. The BeoCenter 1 regulates its vertical size, and every
refresh emulation asks for on this side of the world - 60.0988 for a NES,
59.92 for a Mega Drive, 59.6 to 61.7 for the arcade boards - is inside
±1.8%, five times inside what was measured.

## What the compositor does with it

Flyback, the compositor itself, has a document of its own:
[`flyback.md`](flyback.md).

A variable refresh rate turns the scheduling problem inside out. With a fixed
one, a frame that misses the deadline is shown a whole frame late, so the
compositor draws at the last safe moment and no later. With a variable one
there is no deadline at all: a flip that arrives after the frame's minimum
length simply makes that frame longer. Nothing is dropped and nothing
judders.

So under a variable rate Flyback gives a client the whole frame rather than
the frame less a margin, halves the slack it holds back, and stops counting a
commit that arrives with a flip in the air as late - under a fixed rate that
is a client to give more room to, and here it is the normal case. The one
thing it must not do is draw at the vblank: anything committed while the last
flip was in flight is about to be superseded by the frame the client is
drawing now, and flipping it puts a stale picture in the air that the fresh
one then waits behind. That single mistake cost a frame and a half.

Measured on the television, commit to the start of scanout, for a client that
draws on the frame callback the way a program paces itself:

| the client takes | fixed rate | variable rate |
| --- | --- | --- |
| 1 ms | 16.6 ms | **3.6 ms** |
| 2 ms | 16.6 ms | **4.5 ms** |
| 4 ms | 16.6 ms | **6.3 ms** |

The launcher itself, end to end through `omacrt on`, reports **2.0 ms** in
`omacrt status`, and 4.5 ms with every core on the machine busy.

**How far it can be stretched is a property of the set.** A television's
vertical deflection follows a longer frame only so far, and past that the
picture loses height and keeps it for as long as the rate does. Filmed and
measured on the BeoCenter 1, with the refresh held at each step for eight
seconds and the height taken against the picture's own width:

| refresh | frame longer by | height | brightness |
| --- | --- | --- | --- |
| 59.92 Hz | +0.2% | reference | 170.4 ±1.7 |
| 57.5 Hz | +4.4% | -0.4% | 169.4 ±1.4 |
| 55 Hz | +9.2% | -0.6% | 169.6 ±3.2 |
| 50 Hz | +20.1% | **-11.5%** | 162.8 ±4.3 |
| back to 60.04 Hz | +0.0% | -0.1% | 170.7 ±0.8 |

So the useful range on this set is 60.04 Hz down to about 55, and the whole
NTSC family is four times inside it: 60.0988 for a NES, 59.92 for a Mega
Drive, 59.6 to 61.7 for the arcade boards. PAL at 49.70 is outside, and does
not need to be inside: a PAL frame is 288 active lines and wants its own
modeline for that anyway.

Where the set gives up is a calibration, like the picture shift, and
`output.vrr_min_hz` in `crt.toml` is where it goes. Nothing asks the tube for
a slower rate than that, however slowly a program runs. The EDID has to
declare a wider range - amdgpu refuses one narrower than ten hertz and caps
the top at the mode's own, so the declared minimum has to be 50 or below -
but what is declared only turns the feature on. What is used is the
calibration.

The brightness in that table is the other half of the answer: steady to
within two parts in a hundred at every step. A television whose frame length
keeps changing *will* pulse, and this is what that looks like when it is not
happening.

A program that runs at a rate of its own is where this stops being simple.
A television whose frame length is the one thing that varies will pulse if
that length keeps changing, because the brightness of a phosphor depends on
how long it is left between refreshes. Put an emulator on the tube whose
content rate is not the compositor's cadence and the two beat: half the
frames end at the hardware's minimum vertical total and half run all the way
to its maximum, four milliseconds apart, and the picture flickers visibly.
Pacing the frame callbacks to the client's own commit interval rather than to
the mode's period removes it - measured on a Mega Drive core, the step
between one frame and the next falls from 4283 microseconds to 24, and the
tube runs at the core's own 59.95 Hz.

What that does not do is let a program choose a rate. A client paced by frame
callbacks runs at the cadence it is given, and the cadence is taken from the
cadence it runs at, so whatever it settles on is where it stays. Taking the
brake off - `video_vsync=false` with `vrr_runloop_enable=true` - lets the
core set the rate, and on an NTSC title it does exactly that; on a PAL one
RetroArch then had nothing holding it at all and ran at 80 Hz, faster than
the hardware's shortest frame, and the pacing collapsed again. The rate has
to be asked for rather than discovered: the launcher knows which system is
running and what that system's refresh is, and telling the compositor is one
line on the control pipe it already has.

Asking for the range in the EDID is the switch. There is nothing else a
television leased to this compositor would want a variable refresh rate for,
so when the kernel says the connector is capable, Flyback turns it on.

## Modelines and interlace in Hyprland

Two things are true of Hyprland 0.56 and its aquamarine backend, and both are
in the source.

**Modelines work, the interlace flag does not.** The monitor rule takes a full
modeline: `monitor = DP-2, modeline 6.400 320 336 368 400 240 244 247 262
-hsync -vsync, 0x0, 1`. The parser reads the clock, eight timings and then the
flags. The flag table holds the key `Interlace` with a capital letter while
the parser lowercases every flag before looking it up, so the flag is never
found, is logged as invalid, and the mode is applied as progressive with the
wrong timings. Issue 4607 covers it and was closed as not planned in 2024. A
one line fix upstream, and independent of everything here.

**aquamarine drops interlaced modes.** Every interlaced mode read from an
EDID is skipped by name. Custom modes go through a different path, but with
the flag bug above the flag never arrives anyway.

Neither matters to this project any more, because the leased output is
programmed through DRM directly rather than through the compositor. They
matter to anybody trying to get a 15 kHz picture on the desktop itself.

## A television has no EDID

A SCART set says nothing about itself, and a DAC in front of it passes on
whatever it wants. The kernel sees a connector that is either disconnected or
describing the DAC. Three remedies, in the order they cost effort:

- `video=<connector>:320x240eS` on the kernel command line: `e` forces the
  connector active with no EDID, `S` allows the low dot clock modes, `i` asks
  for interlace.
- `drm.edid_firmware=<connector>:edid/crt15.bin` with an EDID built for the
  purpose, in `/usr/lib/firmware/edid/` and included in the initramfs.
  Switchres can generate one.
- Hardware that emulates an EDID, which is what a VideoAmp does.

This project overrides the EDID for the opposite reason: not to describe a
television, but to add the flag that marks the connector non-desktop so the
compositor leaves it alone. `scripts/edid-non-desktop.py` adds it to the
DAC's own EDID and `scripts/crt-lease-setup.sh` installs it through debugfs,
with a simulated unplug and replug so the compositor notices. Writing the
sysfs `status` file re-probes but emits no event, which is why the replug is
needed.

## Hardware

What this was built on, and what the alternatives are.

| Part | This machine | Alternatives |
| --- | --- | --- |
| GPU | AMD Radeon RX 7700/7800 XT (Navi 32, DCN 3.2), `amdgpu` | Any AMD card on `amdgpu`: leasing and the timings are kernel side. Nvidia's proprietary driver offers no non-desktop connectors to lease. Intel untested |
| DAC | [RGB-Pi 2](rgb-pi-2.md): HDMI in, SCART RGB out, sync mode over I2C, audio on SCART | Anything that takes HDMI and puts RGB on SCART or VGA. On DisplayPort: an RTD2166 or RTD2168 adapter, plus a sync combiner |
| Sync | Handled inside the DAC | On a VGA path: VideoAmp, sirMagb F-15, UMSA, VGA2SCART. Never a passive VGA to SCART cable: no shielding and a weak sync |
| Television | Bang & Olufsen BeoCenter 1, SCART RGB | Any 15 kHz set with an RGB input, PAL or NTSC |

Fixed mode HDMI DACs exist too, HDMI2SCART among them: they expose one
resolution and one refresh rate and convert anything. They need no patched
kernel and no leasing, and they cannot give a console its own line count.
Worth knowing about as a cheaper first step.

The two adapters to avoid on a VGA path, for reasons the arcade forums
document at length: anything with a slow PIC that loses sync when the timing
changes, and any cable built for a MiSTer, which expects 3.3 to 5 V on pin 9
and TTL composite sync on pin 13.

## Bringing one up, in order

The order matters: everything that can be proved without spending money comes
first.

1. Switchres dry, for the modelines: `switchres 320 240 60 -c -m ntsc`. No
   hardware needed.
2. A modeline applied to a DisplayPort output with an LCD attached. The
   monitor says out of range, which is the correct answer; `hyprctl monitors`
   and `dmesg` say whether the driver took the mode. This tests the GPU
   without a television in the room.
3. The DAC, on the television, with the picture coming from the kernel
   console.
4. The lease: the connector marked non-desktop, offered, taken, and the mode
   programmed by our own process.
5. RetroArch as a client of that compositor, with the line count following the
   console.
6. Interlace, which is where a stock kernel stops.

## Sources

- 15 kHz kernel patches: <https://github.com/D0023R/linux_kernel_15khz>
- Supported AMD GPUs: <https://github.com/ZFEbHVUE/Batocera-CRT-Script/wiki/Supported-AMD-dGPUs-&-APUs>
- DACs: <https://github.com/ZFEbHVUE/Batocera-CRT-Script/wiki/Digital-to-Analog-(DAC)>
- Sync solutions and transcoders: <https://github.com/ZFEbHVUE/Batocera-CRT-Script/wiki/Recommended-Adapters,-Sync-Solutions-&-Transcoders>
- Switchres, and its Wayland discussion: <https://github.com/antonioginer/switchres> and <https://github.com/antonioginer/switchres/issues/102>
- RetroArch CRT SwitchRes: <https://docs.libretro.com/guides/crtswitchres/>
- GroovyArcade: <https://github.com/substring/os>
- Hyprland modelines: <https://github.com/hyprwm/Hyprland/pull/2254>, the interlace flag: <https://github.com/hyprwm/Hyprland/issues/4607>
- SCART pin 16: <http://martin.hinner.info/vga/scart.html>
- HDMI2SCART: <https://github.com/c0pperdragon/HDMI2SCART>
- RGB-Pi 2: <https://retrorgb.com/rgb-pi-2-released.html>
