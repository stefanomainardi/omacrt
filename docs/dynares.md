# DynaRes, what it is and how we get the same result on a PC

RGB-Pi OS and RePlayOS launch every game at its native resolution and refresh
rate and follow the core when it changes mode mid game (Dreamcast 240p to 480i,
SNES 224 to 239 lines, arcade 256p). They call the mechanism DynaRes. This note
records what we could learn from the public parts of the stack and maps each
piece to what already exists on Linux for a modern GPU.

## What is public and what is not

- **The frontend is public.** `rtomasa/rgb-pi-frontend` (GPL-3.0, Python)
  writes a RetroArch config per launch and starts a patched RetroArch with
  `--appendconfig`. It shows every DynaRes option and how each system uses it.
- **The DAC driver is public.** `rtomasa/rpi-dpidac` (GPL-3.0, C) is a DRM
  bridge for the Pi 5 DPI output. It fakes an EDID, lets a module parameter
  force one exact timing (`force_mode=` in `video=` syntax) and picks a
  preferred mode. It does not compute timings itself.
- **The RetroArch fork is not public.** `rtomasa/RetroArch` on GitHub is a
  plain 2023 fork of upstream with no DynaRes code. The binary ships in the OS
  image and through the OTA repository. RePlayOS describes DynaRes 2.0 as
  "instant timing changes in 1 to 3 frames", against 108 to 120 frames in 1.0.

So DynaRes proper is a private video subsystem inside RetroArch. Its options
and behaviour are visible from the frontend; its internals are not.

## The options the frontend writes

From `launcher.py`, function `make_common` and the per system builders:

| Key                         | Values                       | Meaning                                                                                                                                            |
| --------------------------- | ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| `dynares_mode`              | `superx`, `native`, `custom` | Width strategy: super resolution (fixed wide mode, height follows the core), true native, or a fixed `video_fullscreen_x/y` with a custom viewport |
| `dynares_crt_type`          | `generic_15` and friends     | Monitor preset for the timing calculator                                                                                                           |
| `dynares_video_info`        | bool                         | On screen overlay with the active mode                                                                                                             |
| `dynares_flicker_reduction` | bool                         | Softens content that would flicker on a progressive tube                                                                                           |
| `dynares_overscan`          | `0` or `8`                   | Extra border in native mode                                                                                                                        |
| `dynares_handheld_full`     | bool                         | Stretch handheld systems to the full frame                                                                                                         |

Per system logic worth copying:

- **Super resolution is the default** (`config.ini`: `dynares = superx`), with
  `aspect_ratio_index` set to core provided and integer scaling off. Fonts are
  swapped for a 240 line or 480 line version so the OSD stays legible.
- **Dreamcast and Naomi use the 480 line variant.** Flycast reports 640x480; the
  frontend keeps super resolution and picks the `superx480` font, so the
  mode is a wide 480 line frame, interlaced where the hardware allows.
- **SNES in native mode is pinned to 512x224.** A fixed frame with a 512x224
  viewport absorbs the console's 256 and 512 wide modes without switching.
- **Vertical arcade without rotation is pinned to 640x480 with a 336x448
  viewport**, because a 240 line frame cannot show a 320 line tate game.
- **Pi 5 cannot interlace over GPIO.** RGUI-Pi documents that systems needing
  480i run at half height on the Pi 5. Interlace is a Pi 4 feature.

## Mapping to the PC stack

Everything DynaRes does has an upstream equivalent. RetroArch's CRT SwitchRes
is Switchres by Calamity linked into the frontend; it reacts to
`retro_get_system_av_info` and geometry changes, computes a modeline from a
monitor preset and sets it through X11 or KMS.

| DynaRes                            | Upstream RetroArch and Linux                                                                                |
| ---------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `dynares_mode = superx`            | `crt_switch_resolution = 1`, `crt_switch_resolution_super = 2560` (or 1920, 3840)                           |
| `dynares_mode = native`            | `crt_switch_resolution = 1`, `crt_switch_resolution_super = 0`                                              |
| `dynares_mode = custom`            | `crt_switch_resolution = 0`, fixed `video_fullscreen_x/y`, `aspect_ratio_index` custom, `custom_viewport_*` |
| `dynares_crt_type`                 | `switchres.ini` monitor preset: `ntsc`, `pal`, `generic_15`, `arcade_15`                                    |
| `dynares_overscan`, centering      | `crt_switch_center_adjust`, `crt_switch_porch_adjust`, Switchres geometry                                   |
| `rpi-dpidac` forcing timings       | Patched `amdgpu` (15 kHz patch set) accepting arbitrary modelines, DAC RTD2166 passing low pixel clocks     |
| On the fly switch inside RetroArch | Same, via KMS/DRM atomic modeset; the kernel patch 06 lets Switchres add user modes                         |
| Dreamcast 480i                     | Real interlace on AMD with the patches; no half height compromise                                           |

The one thing we cannot copy is the 1 to 3 frame switch time claimed by DynaRes
2.0. AMD KMS modesets take longer and the CRT itself needs a moment to relock.
That is a cosmetic difference, not a functional one.

## What this means for omarchy-crt

- **No fork of RetroArch.** Upstream CRT SwitchRes, KMS video driver, our own
  `retroarch.cfg` and a `switchres.ini` per TV profile give the DynaRes feature
  set.
- **`systems.toml` carries the DynaRes policy per system.** The `video` field
  becomes a preset: `super` (default), `native`, or a pinned frame such as
  `512x224` for SNES, and the launcher writes the matching CRT SwitchRes keys
  into the per launch config, the way the RGB-Pi frontend does.
- **Dreamcast gets 480i.** With the patched kernel and a real DAC the
  Dreamcast, Naomi, PS2 and the 480i PlayStation menus run interlaced as on
  the original hardware.
- **Test content.** `rtomasa/avtest` is a small libretro core that shows a
  grid at 50 and 60 Hz for geometry checks; the 240p Test Suite covers the
  rest. Both are free.

## Sources

- <https://github.com/rtomasa/rgb-pi-frontend> (GPL-3.0), `launcher.py` and
  `config.ini`
- <https://github.com/rtomasa/rpi-dpidac> (GPL-3.0), `rpi-dpidac.c`
- <https://github.com/rtomasa/RetroArch> (plain upstream fork, no DynaRes)
- <https://github.com/forkymcforkface/RGUI-Pi> (Pi 5 interlace limitation)
- <https://www.replayos.com/faq/> and <https://www.replayos.com/changelog/>
  (DynaRes 2.0 claims)
- <https://docs.libretro.com/guides/crtswitchres/> (upstream CRT SwitchRes)
