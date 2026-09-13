# Flyback beside the others

There is a well-populated world of Linux retrogaming on CRTs, and most of it
is older, more complete and better tested than this project. This document
says what Flyback does differently, what it does worse, and where it does not
compete at all.

**One rule throughout.** Our numbers are measured on our own chain and the
instrument is in this repository. Everybody else's numbers here are either
quoted from their own documentation or absent, because we do not own their
hardware and will not invent a figure for it. A table that scored their
latency against ours from memory would be a marketing exercise.

---

## The short version

Every other system on this page answers the question *"how do I build a
machine for a CRT?"* Flyback answers a different one: *"how does a machine I
already have grow a CRT?"* That is the whole of the difference, and everything
below follows from it.

If you are willing to dedicate a box, stop reading and use
[GroovyArcade](https://github.com/substring/os) or
[Batocera](https://wiki.batocera.org/batocera-and-crt). They are mature, they
have communities, and they solve the problem completely. If you want the most
faithful hardware behaviour money can buy, use a
[MiSTer](https://mister-devel.github.io/MkDocs_MiSTer/advanced/lag/). Flyback
exists for the case those three do not cover: one powerful machine that is
also a desktop, with a television as one of its outputs.

---

## What each one is

| | what it is | the machine is |
| --- | --- | --- |
| **GroovyMAME / Switchres** | a MAME fork and the modeline engine under it, the origin of most of this craft | whatever you run it on |
| **GroovyArcade** | an Arch-based OS built around GroovyMAME, with a patched kernel for 15 kHz and LXDE on the tube | a CRT machine |
| **Batocera + the CRT script** | a general retro distribution plus a community script that adds 15/25/31 kHz through Switchres | a CRT machine |
| **Lakka** | a LibreELEC-based RetroArch appliance, with CRT-tuned Raspberry Pi images since 6.1 | a CRT machine |
| **RetroPie** | a Raspberry Pi frontend with long-standing community 15 kHz recipes | a CRT machine |
| **MiSTer** | an FPGA that reimplements the consoles themselves | a console |
| **Flyback** | a Wayland compositor that takes one connector from the desktop and drives it | still your desktop |

Flyback is the only row that is a component. It has no
frontend of its own, no game database, no installer and no community. It is
one process that owns one output.

## How the 15 kHz mode gets set

This is the technical fork in the road, and the precision matters
because it is where the design decisions come from.

| | mechanism | needs a patched kernel |
| --- | --- | --- |
| GroovyArcade | kernel mode setting with 15 kHz patches, Switchres driving the mode per game | yes |
| Batocera + script | Switchres through the distribution's own video stack; Intel iGPUs unsupported, NVIDIA needs the proprietary driver | no, but it is a build |
| Lakka / RetroPie | fixed modelines and firmware timings, chosen per system | no |
| Flyback | a leased DRM connector, modeline written straight to KMS from `crt.toml` | no |

The reason Flyback leases instead of patching is that a desktop compositor
will not set a 15 kHz mode, will not stop managing an output's timing on
request, and will not hand over the scanout. Those are three separate refusals
with one supported answer: `wp_drm_lease_v1`, marked non-desktop in the EDID.
The protocol was written for VR headsets, and its own specification describes
the general case: a compositor "will not use this output at all and will make
it available for leasing". Every user of it in the wild is still a headset. No
trace was found of a television being leased this way, which describes a
search and not a claim of primacy.

**Where Switchres is better than what we have.** Switchres computes a modeline
per game from a monitor preset, for any resolution a driver asks for, across
15, 25 and 31 kHz and dozens of documented arcade monitor types. Flyback ships
five modelines in `crt.toml` and picks between them. It is a real gap, and
the accumulated work of years, and if this project ever needs per-game timings
the sensible path is to use Switchres instead of rewriting it.

## What Flyback does that the others do not

Three things, and only three.

**1. The desktop survives.** The connector is leased; the other outputs are
still Hyprland, still doing whatever they were doing. No dual boot, no second
machine, no reboot to play. Every other row on the first table takes the
machine.

**2. The latency of the whole chain is measured and published.** A compositor
is the only place where both ends of the measurement exist: the client's
commit and the kernel's vblank timestamp. Flyback keeps both, writes the
median of the last three hundred frames where the launcher can read it, and
shows it on the television.

| | |
| --- | --- |
| commit to start of scanout, client drawing 1 ms | **3.79 ms** |
| the launcher end to end, in `omacrt status` | **2.0 ms** |
| kernel timestamp of a button press to start of scanout, 300 presses at random phase | best 1.46 ms, **median 10.19 ms** (0.61 frames), worst 18.21 ms |

Measured on a BeoCenter 1 through an RGB-Pi 2 at 3520x240 @ 60.04 Hz, over a
leased HDMI connector on Navi 32, with the launcher mapped underneath, which
is how the television actually runs, because a program on it is never the only
client. The method and the instrument are in
[`flyback.md`](flyback.md#reproducing-them).

For comparison, quoted from their own documentation: MiSTer's own
documentation puts its scaler at four display lines in its fastest mode, about
a third of a millisecond at 320x200, on top of a core that is the console's
own timing. A MiSTer is faster than this and always will be, because there is
no operating system in it. What the number above buys is a reference point
for the software path, which nobody had published for Linux into a real CRT.

**3. A variable refresh rate on a fixed-frequency television.** A set's
horizontal rate must never move; its vertical rate can, because the vertical
oscillator re-triggers on sync. So every refresh an emulation asks for is
reachable by stretching the vertical blanking alone, exactly what
adaptive sync does in hardware. This chain follows. Five rates were asked for
by name and measured as the median of the last 300 vblank intervals, after
fifteen seconds of settling at each step: **60.041 asked, 60.04 given; 59.92,
60.02; 57.5, 57.59; 55, 55.01**. 50 is held at 55, because that is where this
set stops following. The instrument
is a median over a five second window, and the numbers are reported to the
precision it has. And no mode change, where a mode change costs
**182 to 229 ms** inside the kernel's own call, and about **400 ms** of a black
screen on the television, whether it moves the whole standard or only the
vertical total.

Everybody else changes the mode. That is not a criticism: before this, nobody
had established that a consumer television would follow a stretched blanking
at all, or where it stops following. The measurement of where it stops, with
+9.2% of frame length holding and +20% losing 11.5% of the picture height, is in
[`15khz.md`](15khz.md#a-variable-refresh-rate), and it sets
`output.vrr_min_hz`.

The one prior report of adaptive sync on a CRT is on multisync PC monitors,
never on televisions. Again: no trace found, which describes a search.

## What Flyback does not do, that they do

An honest list, because the one above is short.

- **No frontend ecosystem.** Batocera and Lakka bring scrapers, themes,
  netplay, hundreds of systems, and years of per-core configuration. This
  project's launcher is one person's launcher.
- **No per-game modelines.** See Switchres above.
- **No 25 or 31 kHz, no arcade monitor presets.** The guard can be widened for
  a multisync display, and nothing has been tested on one.
- **One GPU vendor.** Measured on AMD Navi 32. The variable refresh rate in
  particular needs amdgpu and an injected FreeSync range in the EDID.
- **One television.** Every calibration in `crt.toml` is a property of a
  BeoCenter 1 in one room. The numbers will differ on your set; the method is
  the transferable part.
- **No interlace.** A 480i modeline is accepted on this connector and then
  scanned out progressively, because five gates in the kernel's display code
  stand between it and a picture. This project decided not to require a
  patched kernel, and a distribution that already patches its kernel for
  15 kHz is not paying that price, so the trade belongs to us.
- **No beam racing.** Which deserves its own section.

## Beam racing, said plainly

Drawing the frame just ahead of the electron beam is the last real latency win
available, and it exists: in GroovyMAME and in WinUAE, on Windows. On Linux it
has never worked. RetroArch has had
[a bounty standing for it since 2018](https://github.com/libretro/RetroArch/issues/6984),
and what landed instead in 2025 is the opposite thing, a
[shader that simulates a CRT's rolling scan](https://www.libretro.com/index.php/retroarch-first-program-to-support-blurbusters-crt-beam-racing-simulator-shader/)
on a high-refresh LCD.

The reason nobody built it on Linux is that beam racing needs ownership of the
scanout, and a desktop compositor denies it. A leased connector does not.
So this is a setup where it could be built, and it has not been built here
either. That sentence is the whole of the claim.

## Where this leaves it

Flyback is not a better Batocera. It is the piece that was missing between a
normal Linux machine and a 15 kHz television. It takes one output, drives it
as a television rather than as a monitor, schedules frames for a tube instead
of for a desktop, and says out loud how long its own picture took to arrive.

It is experimental, it sets the line rate of a circuit tuned for one, and it
has been run in one room. What it contributes that outlasts it is the
measurements and the method, both of which are written down here for anyone
who wants to disagree with them on their own set.

---

## Sources

- [GroovyArcade](https://github.com/substring/os) and
  [GroovyMAME](https://github.com/antonioginer/GroovyMAME)
- [Batocera's CRT documentation](https://wiki.batocera.org/batocera-and-crt)
  and the [community CRT script](https://github.com/ZFEbHVUE/Batocera-CRT-Script)
- [Lakka 6.1's CRT-optimized images](https://alternativeto.net/news/2026/2/lakka-6-1-brings-libreelec-12-2-retroarch-1-22-2-and-crt-optimized-raspberry-pi-images)
- [A RetroPie 15 kHz CRT recipe](https://bencao74.blogspot.com/2017/05/retropie-15khz-crt-tutorial-for.html)
- [MiSTer on lag](https://mister-devel.github.io/MkDocs_MiSTer/advanced/lag/)
- [The RetroArch beam racing bounty](https://github.com/libretro/RetroArch/issues/6984)
  and [the simulator shader that landed instead](https://www.libretro.com/index.php/retroarch-first-program-to-support-blurbusters-crt-beam-racing-simulator-shader/)
- [The DRM lease protocol](https://wayland.app/protocols/drm-lease-v1) and
  [its history from the XR side](https://indico.freedesktop.org/event/6/contributions/384/attachments/217/296/DRM-lease_Wayland_Monado.pdf)
