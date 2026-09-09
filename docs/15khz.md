# Driving a 15 kHz television from a modern PC

This is the research the project rests on, and the correction it needed. The
first version of this page was a feasibility study written before anything
worked, and its two main conclusions were both wrong: it said HDMI was out of
the question, and that a Wayland compositor could never change modes for each
game. What ships does both. The parts of the study that were right are still
right, and worth reading before buying anything, so they are here too, with
the reasoning that replaced the rest.

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
will not program a mode below it on an HDMI connector.

The study concluded from the third wall that HDMI was out, and that the only
road was DisplayPort into an RTD2166, with a kernel carrying the 15 kHz
patches, and the emulator taking KMS on a second virtual terminal because a
compositor cannot change modes per game.

## What this project does instead

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

The second wall the study raised, per game mode changes under a compositor, is
answered by **DRM leasing**. It is a Wayland protocol built for virtual
reality headsets, where a compositor hands one connector over to an
application that then owns it. The desktop keeps every other output; the
leased one belongs to whatever took it. So:

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
change. The study's "not possible under a compositor" was true of the
protocols it looked at, `wlr-output-management`, which carries a width, a
height and a refresh rate and nothing else. Leasing sidesteps the question by
handing over the connector rather than describing a mode to somebody else.

## What a stock kernel still cannot do

**Interlace.** `amdgpu` without the 15 kHz patches will accept a 480i
modeline, program something, and scan out a narrow strip. This is not a
subtlety to work around: the project carries `output.interlace`, off by
default, precisely because turning it on with a stock kernel gives a broken
picture. 480 line consoles are shown at 240p instead, which is what the line
count per console exists for.

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

## Two findings about Hyprland

Both verified by reading the source on 2026-09-05, and both still true.

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

## What was checked, and in what order

The order mattered: everything that could be proved without spending money was
proved first.

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
6. Interlace, which is where the stock kernel stopped.

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
