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
2. `omacrt-display` takes the lease, becomes DRM master for that
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
