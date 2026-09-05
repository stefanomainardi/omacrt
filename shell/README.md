# omarchy-crt-shell

Native boot screen and launcher for Omarchy on a 15 kHz CRT. It re-creates the
sequence of [crt.omarchy.org](https://crt.omarchy.org/) (power surge, BIOS
POST, logo reveal, chime, laser-etched wordmark, `ls` menu) as a real
320x240 framebuffer, without any shader that fakes a tube. The CRT provides the
scanlines.

Written in Rust on SDL2. Colors come from the current Omarchy theme
(`~/.config/omarchy/current/colors.toml`). The icon is traced from the Omarchy
favicon and the wordmark is decoded from Omarchy's own `logo.txt`.

## Build and run

```sh
cargo build --release
./target/release/omarchy-crt-shell                # 320x240 in a 3x window
./target/release/omarchy-crt-shell --auto-boot   # skip the PRESS START gate
./target/release/omarchy-crt-shell --fullscreen --stretch --size 320x240 --hz 60
```

Options:

| Flag                                       | Meaning                                                                       |
| ------------------------------------------ | ----------------------------------------------------------------------------- |
| `--size WxH`                               | framebuffer size, default `320x240`                                           |
| `--hz N`                                   | refresh rate shown in the POST, default 60                                    |
| `--scale N`                                | window scale for desktop testing, default 3                                   |
| `--fullscreen`                             | fullscreen on the current output                                              |
| `--stretch`                                | fill the output ignoring aspect ratio, for wide 15 kHz modes such as 1440x240 |
| `--no-audio`                               | disable sound                                                                 |
| `--auto-boot`                              | start the boot sequence immediately                                           |
| `--theme PATH`                             | alternative `colors.toml`                                                     |
| `--menu PATH`                              | alternative `menu.toml`                                                       |
| `--systems PATH`                           | alternative `systems.toml`                                                    |
| `--browse [SYSTEM]`                        | boot straight into the game browser                                           |
| `--idle SECONDS`                           | screensaver after this idle time, default 60, 0 disables                      |
| `--screensaver [NAME]`                     | start in the screensaver, optionally with one effect                          |
| `--headless --dump 1.0,4.5 --dump-dir DIR` | render frames to PPM without a window                                         |

Controls: arrows or `hjkl` move, `Enter` or `Space` select, `Esc` or
`Backspace` go back, `F` stars a game, `q` quits. Game controllers work through
SDL: d-pad moves, `A` or `Start` selects, `B` goes back, `Y` stars a game.

## Menu

`~/.config/omarchy-crt/menu.toml`:

```toml
[[item]]
name = "retroarch/"
command = "retroarch"

[[item]]
name = "mame/"
command = "groovymame"

[[item]]
name = "desktop/"
quit = true

[[item]]
name = "poweroff"
command = "systemctl poweroff"
```

Items without a command show a message; `quit = true` exits the shell;
`confirm = true` asks for a second press within three seconds before running
the command (the default `poweroff` entry uses it).

## Games

`~/.config/omarchy-crt/systems.toml` lists the systems: ROM directory, libretro
core, extensions and a video policy (`super`, `native` or a pinned `WxH`). The
shell writes `retroarch.cfg` once (menu and notifications off, save state on
exit, resume on start) and a `launch.cfg` per game with the policy keys, then
runs `retroarch --config ... --appendconfig ... -L core rom` and waits. Core
options go to `cores.cfg`, the TV profile to `profile.toml` and
`switchres.ini`. See [`../docs/systems.md`](../docs/systems.md) and
[`../docs/video-policy.md`](../docs/video-policy.md).

## Sound

Every sound is synthesized at startup: a switch clunk with a degauss thump, HDD
seek clicks during the POST, a systems-online chord with tape echo, and short
square-wave beeps for navigation.

## Timeline

Seconds after START:

| Time         | Event                                                   |
| ------------ | ------------------------------------------------------- |
| 0.0 to 0.55  | power surge, vertical roll                              |
| 0.45         | POST lines, one every 0.18 s, memory count to 65536K    |
| 1.75 to 2.03 | POST fades, second roll at 1.92                         |
| 2.2 to 3.65  | logo revealed in bands with a scanning beam             |
| 4.0          | chime, logo moves up                                    |
| 4.2 to 6.6   | wordmark etched left to right with sparks               |
| 6.9 to 7.45  | everything settles, listing fades in, menu becomes live |

## Credits

Font: `font8x8` by Daniel Hepper, public domain, based on the IBM VGA fonts.
Wordmark and icon: Omarchy (MIT, Omacom Foundation).
