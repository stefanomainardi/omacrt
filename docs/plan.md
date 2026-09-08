# State of the project, September 2026

What works, how it got there, and what comes next. The research that led here
is in [`15khz.md`](15khz.md) and
[`rgb-pi-2.md`](rgb-pi-2.md).

## Done

- **First light, 2026-09-06.** The launcher on the BeoCenter 1 through the
  RGB-Pi 2 at 240p and 288p, audio over SCART, from the stock Omarchy kernel.
- **The tube is ours, 2026-09-07.** The DAC's connector is marked non-desktop
  by an EDID override installed at boot (`omarchy-crt-lease.service`); Hyprland
  offers it through the DRM lease protocol and `omarchy-crt-display` takes it,
  programs the timing through DRM and runs its own small Wayland compositor on
  it. The launcher, RetroArch and mpv are its clients. Live line changes per
  system, screenshots (`omarchy-crt shot`) and recordings (`omarchy-crt
  record`) of the tube, a desktop monitor window that carries the keyboard.
  This replaced every earlier workaround (pinned windows, workspace rules,
  parked pointer) and every plan for a patched kernel.
- **Playing well.** Pause menu over the game with resume, save and load state,
  rewind, fast forward, slow motion, reset and back to the launcher, all
  through real hotkeys the compositor presses, held as long as the action
  needs (RetroArch 1.22's network commands crash it). Save
  states in sight on the rows and under the box art. A mapping wizard for
  pads SDL does not know. Per system libretro options, run-ahead and rewind
  whitelists written at every launch.
- **A launcher worth looking at.** Box art and console pictures, the cover
  flow, region and revision tags, last played, breadcrumbs, marquee,
  favourites, search across the whole collection with letter jumps and an on
  screen keyboard, glints on the logos, every installed Omarchy theme.
- **Music.** cliamp as the engine (daemon, Unix socket, v1 protocol): radio by
  country and genre through Radio Browser, favourites, history, every
  configured provider (Spotify works after `cliamp setup`). The hi-fi deck
  (cassette, dial with static, VU meters), seven visualizers with a kick
  detector standing in for the screensaver, synced lyrics, sleep timer, the
  selection band breathing with the beat, optional pad rumble. Album art for
  Spotify tracks through the public oEmbed endpoint and station logos from the
  directory listing, on the cassette label, the record label and the dial. A
  ten band equaliser page over the engine's own bands and presets. What plays
  keeps a line at the foot of the home screen.
- **Video.** Local films through mpv with the fit pipeline; YouTube from the
  tube (search through yt-dlp, watch later, recently watched, the clipboard
  link, `omarchy-crt watch`), fetched at 480p.
- **The bar plugin.** Power, standard, DAC sync, audio and TV volume, a field
  to send a link to the television, the library overlay (sources and disks,
  systems with a core picker and install offers, the folder of the systems
  that read one, BIOS import, unplaced folders). The scan runs inside the
  overlay and shows the folder it is reading.
- **The CLI.** Everything above from a terminal, with `--json` where the
  plugin needs it.
- **Covers by title.** `shell/src/covers.rs`: the per system name index of
  the libretro thumbnails, a normalized match with region preference and a
  word overlap fallback; `omarchy-crt library covers` for the whole
  collection, the same lookup lazily in the launcher. On a PlayStation set
  without region tags, 13 of 14 sampled titles found their art. Arcade
  systems name their files after the emulated set, so the set names are
  turned into titles through the databases RetroArch ships: Neo Geo went
  from 2 covers of 120 to all of them, and the lists read as games rather
  than as file names.
- **Interlaced modes.** `mode 480i` and `mode 576i` program 480 and 576 line
  frames at the same line rates as the progressive standards, and the
  launcher draws at the full height. Whether a film looks better as fields
  than as a deinterlaced 240p picture is still a judgement for the eye.

- **The television when nothing is playing.** A photo frame reading the
  house's own Immich server (memories, an album, the favourites), an ambient
  page with the clock, the weather and the next appointment, and a system
  monitor in the shape of a 16 bit status screen. Either can stand in for the
  screensaver. The prepared pictures are kept, so the frame works with the
  server switched off.
- **A Television entry in the Omarchy menu**, through
  `~/.config/omarchy/extensions/omarchy-menu.jsonc`, which is the extension
  point Omarchy offers. Nothing patched. Every row calls the CLI, reaching the
  launcher with `omarchy-crt shell screen NAME`, and *Play a game...* hands
  the whole collection to Omarchy's own runner and plays what comes back.
- **The weather, drawn.** The ambient page is a window rather than a text
  field: the sun on its real arc between sunrise and sunset, the moon on the
  same path at night, clouds at the speed of the real wind, rain that slants
  with it and breaks on the ground, snow, fog, lightning, and a town along the
  horizon whose windows come on after dark. Every gradient is an ordered
  dither.
- **Four screensaver pages that take turns.** `[screensaver] pages` is which of
  them an idle television shows and `cycle_secs` is how long each keeps the
  screen. Music playing still takes it for the visualizer, and a page opened
  on purpose is never covered by another.
- **Clearing up after itself.** An emulator no longer outlives its launcher.
  `omarchy-crt doctor` surveys what has been left behind and `--fix` clears
  it, and the same sweep runs when the launcher starts or stops and once a
  minute from the watchdog.

## Next

1. **Open the repository.** The checklist is [`release.md`](release.md): the
   security read through and the contribution rules are done, the code review
   and the last of the documentation are not.
2. **Pause menu.** Aspect and shader choices.
3. **Player count.** Not in the file names the collection uses; waits for a
   metadata source.
4. **More screensaver effects** from the TerminalTextEffects catalog.
5. **A kernel with the 15 kHz patches** only if a timing the DAC needs turns
   out unreachable from userspace. Nothing so far has.

## Lessons kept

- Hyprland 0.56 spreads wallpaper, bar and popups over several DRM planes:
  only screencopy (`grim`, `wf-recorder`) captures the composited desktop.
- The Omarchy shell reloads plugin code on any file change in its plugin
  folders; under the lock screen that reload aborts Quickshell. The installer
  refuses to touch the plugin folders while the session is locked.
- A full screen surface cannot live inside a bar widget's panel; it is its
  own plugin of kind `overlay`.
- cliamp 1.63 speaks the v1 socket protocol; the v2 envelope of newer releases
  is not accepted yet.
- Hyprland 0.56 takes dispatchers through its Lua API: `hyprctl dispatch
  workspace 9` is a syntax error that changes nothing, while `hyprctl eval
  'hl.dispatch(hl.dsp.focus({ workspace = "name:x" }))'` works. A take that
  believed the first form recorded the wrong workspace.
- PipeWire remembers a sink per application name, so moving our player's
  audio to the television taught WirePlumber that *every* mpv belongs there,
  desktop video included. Our player names its audio client
  `omarchy-crt-player` for this reason.
- This desktop focuses the window under the pointer (`input:follow_mouse`),
  so parking the cursor after focusing a window hands the keyboard straight
  back: park first, focus second. A video would not answer Escape because of
  this.
- The launcher's own keys reach a running game only through the compositor
  (`omarchy-crt game key`), and RetroArch reads its own bindings: start is
  enter, the arcade coin rshift, A is x.
- An emulator does not die with the launcher that started it: the signal goes
  to the launcher alone and the child is reparented to systemd, where it holds
  the audio and answers "something is playing" for as long as the machine is
  up. One of those blocked every launch from the desktop for eleven hours.
- walker is a single instance application. Started while another one is open,
  the second process hands its arguments over and exits without printing,
  which from a menu row is indistinguishable from a row that does nothing.
- A command that travels down the control pipe has to be understood at both
  ends. Installing only the CLI after adding one leaves the launcher logging
  `control: unknown input <name>`, and the mistake looks like a broken
  feature rather than a stale binary.
