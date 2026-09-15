# RGB-Pi 2 on a PC

The RGB-Pi 2 is an HDMI to SCART DAC built around a Chrontel CH7101 receiver.
It is sold for the Raspberry Pi and RePlayOS, but it is a plain HDMI sink, so a
PC can drive it. This is the recipe that gets a picture out of it on a Bang &
Olufsen BeoCenter 1. The registers and the figures come from the device
itself and from the public parts of RePlayOS (the shell scripts under
`/opt/replay/extra` and the log strings of its frontend).

## What the device is

- Powered from the HDMI 5 V pin. No USB, no external supply.
- EDID: 256 bytes, product name `MORTACA DEV00`, vendor `ATG`, preferred
  1024x768, range 30 to 68 kHz. No 15 kHz mode is advertised; the DAC passes
  whatever timing it receives, the host is responsible for 15 kHz.
- Audio: the EDID carries an audio block (LPCM stereo, 32/44.1/48 kHz, up to
  24 bit). HDMI audio comes out on SCART pins 2 and 6 and on the minijack.
- Control: an I2C slave at 7 bit address `0x78` on the HDMI DDC bus. Standard
  `i2cdetect` stops at `0x77`, so use `-a` to see it. The EDID EEPROM sits at
  `0x50` as usual.

## Why the picture rolls out of the box

At power on the combiner is in "separated H/V" mode: SCART pin 20 carries only
horizontal sync. A TV locks horizontally and rolls vertically, and no modeline
tweak fixes it. The host has to select composite sync over I2C after every
power cycle of the DAC:

| Page (reg 0x00) | Register | Value | Meaning                        |
| --------------- | -------- | ----- | ------------------------------ |
| 4               | `0xB5`   | 0x06  | csync AND                      |
| 4               | `0xB5`   | 0x0C  | csync XOR                      |
| 4               | `0xB5`   | 0x00  | separated H and V (power on)   |
| 0               | `0x60`   | 0x00  | assert reset (hold about 2 s)  |
| 0               | `0x60`   | 0xFF  | release reset                  |
| 0               | `0x61`   | 0xFF  | locked; 0xEF after signal loss |

A reset clears the csync selection, so reset first and select csync after.
On the BeoCenter 1 only XOR locks. RePlayOS documents AND as the common TV mode
and XOR for PVM style monitors, so both are worth trying on another set.

RetroRGB reported a "jumpy screen" that the vendor attributes to a PLL
decoupling problem in the current hardware revision. RePlayOS works around it
with a background process that polls register `0x61` at 1 kHz and resets the
DAC when it drops to `0xEF`. `omacrt dac watch` does the same.

## The tool

```sh
omacrt dac status            # lock register and csync mode
omacrt dac reset             # 2 s reset pulse, csync restored after
omacrt dac csync xor         # or and, separate
omacrt dac watch             # auto reset on signal loss
```

`omacrt on` runs the csync selection for you after the modeline. The
connector defaults to the first connected HDMI output whose EDID names a
Mortaca device; the I2C bus is read from the connector's `ddc` link in sysfs.
The device node is usually `root:i2c`, so join the `i2c` group.

## Hyprland modelines that work

Hyprland 0.56 has two quirks that decide how a modeline is written:

- the modeline clock is truncated to whole MHz (`48.328` becomes `48`), so
  choose integer clocks and adapt `htotal` to hit 15.6 to 15.75 kHz;
- once a modeline is applied to an output it sticks: a later `mode =
"640x480@60"` is ignored, only another modeline replaces it. On the Lua
  config use `hyprctl eval 'hl.monitor({...})'`, `hyprctl keyword` is refused.

Modelines validated on the RGB-Pi 2 (negative sync on both, positive sync
breaks the horizontal lock):

```text
# 240p 60 Hz, 15.731 kHz, 48.9 us of picture
modeline 72 3520 3781 4119 4577 240 242 245 262 -hsync -vsync
# 288p 50 Hz, 15.625 kHz, the same 48.9 us
modeline 72 3520 3740 4078 4608 288 291 294 312 -hsync -vsync
# 240p 60 Hz at 48 MHz
modeline 48 2560 2632 2860 3051 240 244 247 262 -hsync -vsync
```

The picture is 48.9 us of the 63.556 line rather than the standard's 52.66,
and that is deliberate: it is the shape of Switchres's `generic_15` preset,
which is what both RGB-Pi OS and ReplayOS select for this DAC. See
[`docs/15khz.md`](15khz.md) for the three timing tables side by side.

The wide "super resolution" horizontals keep the HDMI pixel clock above the
TMDS floor and let the shell stretch its 320 pixel wide framebuffer by an
integer factor, eleven at 3520 samples.

### The pixel clock is a limit of the converter, not a preference

The CH7101 holds 48 and 72 MHz. **It does not hold 74.** Reading the
converter's lock register (page 0, `0x61`) a thousand times a second on a
still menu with nothing else running:

| pixel clock | timing | losses of lock |
| --- | --- | --- |
| 72 MHz | `72 3520 3781 4119 4577 240 242 245 262` | 0 in 60 s |
| 74 MHz | `74 3840 3966 4314 4736 288 291 294 312` | **80 in 60 s** |
| 72 MHz | `72 3840 3948 4290 4608 288 291 294 312` | 0 in 60 s |
| 72 MHz | `72 3520 3740 4078 4608 288 291 294 312` | 0 in 90 s |

Every loss lasted 276 to 281 ms, which is 276 ms of no picture. The only
variable between the second and third rows is the clock.

`omacrt dac listen` takes this reading: it polls the lock register at 1 kHz,
timestamps every change and resets nothing, so it can be left running while
somebody watches the screen. `omacrt dac watch` is the same poll with the
vendor's workaround attached, which resets the converter on a loss and costs
2.3 seconds of black each time.

## Audio routing

The DAC is one ELD pin on the GPU's HDMI audio function. `scripts/crt-probe.sh`
prints the pin, the PipeWire profile (`output:hdmi-stereo-extraN`) and the sink
name, and `--tone` plays a test tone there. One profile is active per card, so
the desktop monitor on the same GPU loses HDMI audio while the CRT has it.

## Caveats

- Green is reported about 100 mV low compared to red and blue on this
  hardware revision (RetroRGB scope measurement). Colors will not match a
  reference DAC.
- `mode 480i` and `mode 576i` reach the DAC through the leased output, and the
  DAC keeps its lock on them. The picture does not arrive: a stock `amdgpu`
  scans interlaced timings out progressively. See
  [`docs/15khz.md`](15khz.md).
- 74 MHz is outside what this converter holds, and a timing that uses it
  loses lock about once a second. Nothing in the picture says so: it looks
  like an intermittent fault somewhere else, which is where this project
  looked for two days. Keep the clock at 72.
- The device is an HDMI tier option: progressive 15 kHz modes from a stock
  kernel, no patching. Native 320x240 timings and interlace belong to the
  DisplayPort DAC tier instead.
