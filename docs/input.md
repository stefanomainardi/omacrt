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
| Search                 | `/`, then type     | left trigger, on screen keyboard |
| Next, previous letter  | `PageDown`, `PageUp` | `RB`, `LB`      |
| First, last row        | `Home`, `End`      | none              |
| Quit the shell         | `q`                | none              |

`/` on a game list filters that list as you type: every word must appear in
the title, titles starting with the first word come first, `Backspace` deletes,
`Esc` clears the text and then closes the bar. `/` on the home menu or the
systems list opens the whole collection, every system at once, to search
across it. On a pad the left trigger opens the same bar with an on screen
keyboard: d-pad to move, `A` types, `X` deletes, `Y` is space, `B` puts the
keyboard away with the filter kept. The shoulder buttons jump to the next or
previous initial letter, which is how a list of thousands is read without a
keyboard.

The left stick acts as a d-pad with key repeat: one step when it leaves the
dead zone, then a step every 120 ms while held.

On the music screens `X` pauses or resumes what plays, or plays a whole
playlist when one is selected; `Y` stars a station (they gather under
"Favourite stations"), `/` or the left trigger filters the list as in the game
lists.

On the deck (now playing): `A`, `Enter` or `Space` pause; left and right tune
to the next station or track; up and down move the volume by 3 dB; `X` shows
or hides the visualizer; the shoulder buttons (`PageUp` and `PageDown` on a
keyboard) switch between cassette and turntable, or between visualizer modes
while one shows; `Y` (`F`) sets the sleep timer; `B` leaves the music playing.
Settings, Music chooses when the visualizer starts, how often it changes,
which modes take part, whether it replaces the screensaver, lyrics, the
default deck look, the radio country and pad rumble. Settings, Videos sets
the YouTube quality and the number of search hits.

The Equalizer row at the foot of the Music screen opens the ten bands of the
engine: left and right pick a band, up and down move it a decibel at a time
between -12 and +12, `A` walks the presets it knows (Flat, Rock, Pop, Jazz)
and the row shows which one is in force. Moving a band by hand makes the
preset Custom, as the engine reports it. A change made anywhere else, in
cliamp's own interface for instance, shows up here.

Button hints at the bottom of each screen follow the pad family SDL reports.
A PlayStation pad shows `X run  O back  ^ fav`; Xbox and Nintendo pads show
letters. SDL names buttons by position, so on a Nintendo pad the letters match
the physical labels.

## Pads SDL does not know

A pad without a mapping in SDL's database arrives as a bare joystick. When one
is plugged in (or was already there at start) the launcher opens a short
wizard on the tube once the menu is up: it names a control at a time (A, B,
X, Y, Start, Select, the d-pad, shoulders, triggers, the left stick, the home
button) and records the button, axis or hat that answers. Pressing the button
already given as A skips a control the pad lacks; `Enter` skips too, `Esc`
cancels. The mapping is written in the
[SDL_GameControllerDB](https://github.com/mdqinc/SDL_GameControllerDB) format
(MIT) to `~/.config/omarchy-crt/gamecontrollerdb.txt`, loaded at every start,
and the pad works right away. `X` on the Settings, Pads screen runs the wizard
again for the current pad when a mapping came out wrong. RetroArch keeps its
own autoconfig profiles; the wizard is for the launcher.

## In a game

`Select` plus `Start` on the pad, the home button, or `omarchy-crt shell key
menu` pauses the game and raises the launcher's pause menu over it: resume,
save state, load state, rewind two seconds, fast forward (toggles RetroArch's
speed and resumes), slow motion (toggles it too), reset, back to the
launcher. Rewind needs the system to allow it, since it costs memory and CPU
and is off for the 3D consoles; the menu says so rather than doing nothing.
The menu shows when the game's latest save state was written. `B` or `Esc`
resumes.

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
