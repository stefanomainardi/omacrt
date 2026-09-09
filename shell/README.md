# omacrt-shell

Native boot screen and launcher for Omarchy on a 15 kHz CRT. It re-creates the
sequence of [crt.omarchy.org](https://crt.omarchy.org/) (power surge, BIOS
POST, logo reveal, chime, laser-etched wordmark, `ls` menu) as a real
320x240 framebuffer, without any shader that fakes a tube. The CRT provides the
scanlines.

Written in Rust on SDL2. Colors come from the current Omarchy theme
(`~/.config/omarchy/current/colors.toml`). The mark is the project's own, four
bars crossed by the dark cut of the beam's return, drawn as geometry; the
wordmark is decoded from a block drawing that takes its letterforms from
Omarchy's own `logo.txt`.

## Build and run

```sh
cargo build --release
./target/release/omacrt-shell                # 320x240 in a 3x window
./target/release/omacrt-shell --auto-boot   # skip the PRESS START gate
./target/release/omacrt-shell --fullscreen --stretch --size 320x240 --hz 60
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
| `--systems PATH`                           | alternative `systems.toml`                                                    |
| `--config-dir DIR`                         | settings, profile, recents and the RetroArch configs somewhere else           |
| `--browse [SYSTEM]`                        | boot straight into a screen: a system name, or `settings`, `frame`, `monitor`, `ambient`, `saversettings`, `diag`, `about`, `power`, `profile`, `pair`, `style`, `fit` |
| `--idle SECONDS`                           | screensaver after this idle time, default 60, 0 disables                      |
| `--screensaver [NAME]`                     | start in the screensaver, optionally with one effect                          |
| `--pads`                                   | what SDL makes of every connected pad, then exit                              |
| `--headless --dump 1.0,4.5 --dump-dir DIR` | render frames to PPM without a window                                         |
| `--realtime`                               | with `--headless`, hold the loop to the wall clock: anything drawn from live data needs it |
| `--record DIR --record-secs N --script F`  | offline render: every frame as PPM plus `audio.wav`, inputs replayed from a script |
| `--dump-audio DIR`                         | write every synthesized sound as a WAV and exit                               |

Controls: arrows or `hjkl` move, `Enter` or `Space` select (and skip the boot
sequence while it plays), `Esc` or `Backspace` go back, `F` stars a game, `q`
quits. Game controllers work through
SDL: d-pad or left stick moves, `A` or `Start` selects, `B` goes back, `Y`
stars a game. Extra pad mappings load from
`~/.config/omacrt/gamecontrollerdb.txt`. See
[`../docs/input.md`](../docs/input.md).

## Menu

Home: Games, Favorites, Recent, Settings, About, Power. Settings opens the TV
profile, pad pairing, screensaver options and a diagnostics page (kernel, GPU,
connectors, RetroArch version, cores, switching flag, pad). Power off needs a
second press within three seconds. Screensaver options live in
`~/.config/omacrt/settings.toml`:

```toml
[screensaver]
enabled = true
idle_secs = 60
effect = "random" # or laseretch, rain, beams, burn, slide, decrypt, expand, unstable, vhstape

# Theme directory name from ~/.local/share/omarchy/themes, or "system".
theme = "system"
```

## Games

`~/.config/omacrt/systems.toml` lists the systems: ROM directory, libretro
core, extensions and a video policy (`super`, `native` or a pinned `WxH`). The
shell writes `retroarch.cfg` once (menu and notifications off, save state on
exit, resume on start) and a `launch.cfg` per game with the policy keys, then
runs `retroarch --config ... --appendconfig ... -L core rom` and waits. Core
options go to `cores.cfg`, the TV profile to `profile.toml` and
`switchres.ini`. See [`../docs/systems.md`](../docs/systems.md) and
[`../docs/video-policy.md`](../docs/video-policy.md).

## Sound

Every sound is synthesized at startup: a switch clunk with a degauss thump, HDD
seek clicks during the POST, a systems-online chord with tape echo, the CRT tag
reveal (beam whine, arpeggio, stamp) and short square-wave beeps for
navigation. `--dump-audio DIR` writes them all as WAV files.

## Timeline

Seconds after START:

| Time         | Event                                                                                               |
| ------------ | --------------------------------------------------------------------------------------------------- |
| 0.0 to 0.55  | power surge, vertical roll                                                                          |
| 0.45         | POST lines, one every 0.18 s, memory count to 65536K                                                |
| 1.75 to 2.03 | POST fades, second roll at 1.92                                                                     |
| 2.2 to 3.65  | logo revealed in bands with a scanning beam                                                         |
| 4.0          | chime, logo moves up                                                                                |
| 4.2 to 6.6   | wordmark etched left to right with sparks                                                           |
| 6.9 to 9.7   | icon and wordmark settle; Mode 7 show: floor, spinning letters, slam at 8.3, flight, landing at 9.5 |
| 9.6 to 10.1  | listing fades in, prompt is typed, menu becomes live                                                |

## Credits

Font: `font8x8` by Daniel Hepper, public domain, based on the IBM VGA fonts.
Wordmark letterforms: Omarchy (MIT, Omacom Foundation). See
[`../THIRD-PARTY.md`](../THIRD-PARTY.md).
