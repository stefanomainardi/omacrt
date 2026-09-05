# Controllers

The shell and RetroArch read the same pads through two different stacks, so
the physical layout stays identical from the menu into the game.

## In the shell

The shell uses SDL2's game controller API. Every pad in SDL's built-in database
(Xbox, PlayStation, Switch Pro, 8BitDo, most USB clones) arrives with the same
abstract layout, and pads can be plugged in or removed while the shell runs.

| Action                 | Keyboard           | Pad               |
| ---------------------- | ------------------ | ----------------- |
| Move                   | arrows, `hjkl`     | d-pad, left stick |
| Select, start the boot | `Enter`, `Space`   | `A`, `Start`      |
| Back                   | `Esc`, `Backspace` | `B`, `Back`       |
| Star a game            | `F`                | `Y`               |
| Quit the shell         | `q`                | none              |

The left stick acts as a d-pad with key repeat: one step when it leaves the
dead zone, then a step every 120 ms while held.

Button hints at the bottom of each screen follow the pad family SDL reports.
A PlayStation pad shows `X run  O back  ^ fav`; Xbox and Nintendo pads show
letters. SDL names buttons by position, so on a Nintendo pad the letters match
the physical labels.

Pads SDL does not know can be described in
`~/.config/omarchy-crt/gamecontrollerdb.txt`, in the
[SDL_GameControllerDB](https://github.com/mdqinc/SDL_GameControllerDB) format
(MIT). The file is loaded at startup when present.

## In RetroArch

The base `retroarch.cfg` selects the `udev` input and joypad drivers, enables
autodetection and points at the profiles RetroArch ships in
`/usr/share/libretro/autoconfig/udev`, so a recognised pad gets its RetroPad
mapping without any menu. Leaving a game is `Select` plus `Start` on the pad or
`Esc` on the keyboard; the RetroArch menu itself is unreachable.

Per system, `systems.toml` controls two things:

- **`analog_dpad`.** Left stick as d-pad inside the game, `1` by default so
  8 and 16 bit systems play on the stick too. Set to `0` on systems with a real
  analog stick; the defaults do this for PlayStation, Nintendo 64 and
  Dreamcast.
- **`devices`.** RetroArch `--device=PORT:TYPE` pairs to pick the emulated
  controller type (a light gun, a mouse, a six button pad).

## Bluetooth pairing

`pair-pad` in the menu drives `bluetoothctl` without leaving the shell:

1. Opening the screen powers the adapter and scans for eight seconds. Put the
   pad in pairing mode meanwhile.
2. Found devices are listed by name; the address tail helps tell twins apart.
3. Selecting one runs pair, trust and connect. The status line reports the
   result; a failure usually means the pad left pairing mode, so retry.

Paired pads reconnect on their own next time they are switched on, and SDL
picks them up while the shell is running.
