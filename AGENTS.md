# Working on omarchy-crt

This is the guide for anyone, agent or person, changing this repository. It
says what the pieces are, how to check a change without a television in the
room, the conventions the code and the commits follow, and the handful of
rules that each cost a broken session to learn. Read the rules first: they are
the part that does damage.

`README.md` is the project's own description. `docs/plan.md` says what works
and what is next. This file is about the work.

## What this is

An Omarchy edition for retro gaming on 15 kHz CRT televisions. The desktop
hands one connector over with DRM leasing; `omarchy-crt-display` takes it,
programs a modeline no desktop would accept (15.731 kHz, 240 active lines) and
becomes the compositor for that output alone. The launcher, RetroArch, mpv and
cliamp are its clients. Everything else in the repository serves that.

It is not a distribution, a fork or a patch. It lives inside Omarchy through
Omarchy's own extension points: Quickshell plugins for the bar widget and the
library overlay, the menu extension file for the desktop menu, Hyprland's Lua
configuration for window rules. **A change that requires patching Omarchy is
the wrong change.** That constraint is the project's argument, not a
limitation to work around.

## Rules that have already gone wrong

Each of these cost a session. None of them is theoretical.

**Never touch `~/.config/omarchy/plugins` while the session is locked, and
never ask the shell to rescan plugins while it is locked.** The Omarchy shell
watches those folders and reloads plugin code on any file change; under the
lock screen that reload makes its lock plugin re-create surfaces with no
active lock, which Quickshell treats as fatal. The shell crashes and restarts
on top of the lock screen. `bin/omarchy-crt-install` checks
`omarchy-shell lock isLocked` and installs the binaries only. Check the same
thing before writing anything under there by hand.

**Never restart the Omarchy shell to make a change take effect.** Wait for an
unlocked desktop and let the file watcher do it, or tell the user to run the
installer again.

**An emulator does not die with the launcher.** The signal goes to the
launcher alone and the child is reparented to systemd, where it holds the
audio and answers "something is playing" for as long as the machine is up.
`crt::tidy` finds those (ours by our configuration file on their command
line, orphaned by having no launcher above them in the process tree) and
`sweep_orphans` is called from `stop`, `start` and the watchdog. Anything new
that spawns a program on the tube belongs in that survey.

**Never put the launcher's binary name on the same command line as a command
that restarts it.** `pkill -f` matches the shell running the command itself,
so the tool's own shell is killed and the command exits 144 with the work half
done. Write the name into a variable, or use the CLI (`omarchy-crt shell
restart`) on a line that does not also mention `omarchy-crt-shell`.

**`hyprctl keyword` does not work on Hyprland 0.56.** It answers "keyword
can't work with non-legacy parsers. Use eval". Dispatchers and settings go
through `hyprctl eval` with the Lua API:

```
hyprctl eval 'hl.dispatch(hl.dsp.focus({ workspace = "name:crt" }))'
```

**Never set `misc:exit_window_retains_fullscreen` for the session.** A leased
output returns early from the code that used to clear it, so the setting
outlived the game and every desktop window came back fullscreen after an
unlock. Save what a setting was, set it for as long as it is needed, clear it
in both directions and at login.

**Do not invent libretro core option values.** A set of guessed Dolphin
options (`renderer`, `shader_compilation_mode`, `cpu_core`) turned a working
GameCube game into a magenta screen. If an option's exact value string has not
been watched working on this machine, leave it out: the cores have defaults
and the defaults run. What decides the picture is the line count the
television is set to, from `library::default_lines`, not the core.

**A stock kernel cannot scan out interlace.** `amdgpu` without Calamity's
patches will accept a 480i modeline and show a narrow strip. `output.interlace`
exists for that and defaults to false. Do not "fix" a squashed picture by
turning it on.

**Secrets never go on a command line.** `~/.config/omarchy-crt/immich.toml`
holds an API key, mode 600, untracked. It is handed to curl on its standard
input (`-K -`) so it does not appear in the process list. Anything else with a
key follows that pattern. Never commit one, never print one, never pass one as
an argument.

**Never name the DAC's proprietary video feature.** Public wording for what
this project is: "an Omarchy version for retro gaming on CRT". Never "recorded
from a CRT", and no personal library figures in README prose.

**No attribution lines in commit messages or pull request descriptions.**

## Layout

| Path | What |
| --- | --- |
| `shell/` | The whole Rust crate: launcher, CLI, display process, shared library |
| `plugin/` | The Quickshell bar widget and panel; `plugin/library/` the overlay |
| `menu/` | The Television rows the installer writes into Omarchy's menu extension file |
| `bin/omarchy-crt-install` | Build and install; `--system` for the boot time lease; `--uninstall` |
| `scripts/` | EDID override and lease setup, DRM probing, the offline demo renderer |
| `systemd/` | The oneshot unit that hands the tube over at boot |
| `docs/` | The 15 kHz study, hardware, systems, video policy, controllers, CLI, troubleshooting, plan |
| `packaging/` | The Arch `PKGBUILD` |

### The crate

Three binaries out of one crate. `shell/src/lib.rs` is what the CLI and the
launcher share; the modules declared in `main.rs` belong to the launcher
alone.

**Shared (`lib.rs`)**

| Module | What |
| --- | --- |
| `crt/mod.rs` | `Config` (`crt.toml`), `State` (`state.json`), paths, `run` |
| `crt/output.rs` | Modelines, standards, the mode the tube is set to |
| `crt/display.rs` | Starting and talking to the leased display process |
| `crt/launcher.rs` | Starting, stopping and focusing the launcher |
| `crt/control.rs` | The launcher's control pipe: one input or screen name per line |
| `crt/dac.rs` | The DAC over I2C: sync mode, status |
| `crt/audio.rs` | PipeWire routing to the television and back |
| `crt/bios.rs`, `crt/roms.rs` | BIOS report and import, ROM folders |
| `crt/watchdog.rs` | Pidfile, restart budget, putting the display back |
| `library.rs` | Systems, cores, per system options, the launch command |
| `index.rs` | The scan: any folder layout into games with a system and tags |
| `covers.rs` | libretro thumbnails, title matching, arcade set names from RDB |
| `immich.rs` | The photograph server: lists, previews, ffmpeg, the cache |
| `ambient.rs` | Weather from wttr.in, the next event from an `.ics` calendar |
| `settings.rs`, `profile.rs`, `config.rs`, `store.rs` | The user's files, versioned, written durably |
| `videofit.rs`, `player.rs`, `yt.rs` | Fitting modern video to a 4:3 tube, mpv, yt-dlp |
| `music.rs` | The cliamp client (Unix socket, v1 protocol) |
| `padmap.rs`, `rumble.rs`, `states.rs`, `scumm.rs`, `coredata.rs` | Pads, force feedback, save states, ScummVM, core data files |
| `logfile.rs` | Rotation at 8 MiB, `OMARCHY_CRT_LOG=debug` |

**The launcher (`main.rs` and friends)**

| Module | What |
| --- | --- |
| `main.rs` | SDL window, input, the frame loop, the control pipe, headless and offline rendering |
| `scene/mod.rs` | The `Screen` enum, the state, the dispatch, the frame loop and the helpers every screen uses |
| `scene/browse.rs` | Systems, games, folders, search, the cover flow, the launch |
| `scene/boot.rs` | The boot sequence and the home menu |
| `scene/hifi.rs` | The music screens: sources, lists, the deck, the equaliser |
| `scene/video.rs` | Films, YouTube, the player overlay, the fit settings |
| `scene/pause.rs` | A running game and the pause menu over it |
| `scene/settings.rs` | The settings pages, diagnostics, about, the pad wizard |
| `scene/idle.rs` | Which page an idle television shows, and the rules for who wins |
| `scene/frame.rs` | The photo frame and the ambient page |
| `scene/monitor.rs` | The system monitor |
| `fb.rs` | The framebuffer and the drawing primitives |
| `art.rs` | Pictures on a worker thread: fetch, decode, scale |
| `photos.rs` | The photo frame's supply thread, and the weather and calendar with it |
| `sysmon.rs` | The system monitor's sampling, from `/proc` and `/sys` |
| `sky.rs` | The weather drawn: sun, moon, clouds, rain, snow, fog, lightning, a town |
| `deck.rs` | The hi-fi deck and the visualizers |
| `effects.rs`, `etch.rs`, `crt_tag.rs` | The boot sequence and the screensaver effects |
| `theme.rs`, `icons.rs`, `assets.rs`, `font8x8.rs` | Omarchy themes, 8x8 icons, the wordmark, the font |
| `audio.rs` | Every sound, synthesized at startup |
| `pad.rs` | Held direction: first repeat, then acceleration |

## Checking a change without a television

Everything below runs on any machine. What needs the tube is checked in the
living room, and always will be.

```
cd shell
cargo fmt                       # before anything else
cargo clippy --all-targets      # CI denies warnings
cargo test
cargo build --release
```

CI runs those four plus a headless boot whose frames have to come out as
pictures rather than a blank screen.

**Render a screen and look at it.** This is the fastest way to check anything
drawn, and it is how the layouts in this repository were fixed:

```
cargo run --bin omarchy-crt-shell -- --headless \
  --browse monitor --dump 14.0 --dump-dir /tmp/shot
magick /tmp/shot/frame_14.00.ppm -filter point -resize 300% /tmp/shot/a.png
```

`--browse` takes a system name or one of the named screens: `settings`,
`saver`, `diag`, `about`, `power`, `profile`, `pair`, `style`, `fit`,
`monitor`, `processes`, `frame`, `ambient`. `--size WxH` renders at another
shape, and
320x288 is worth checking because a PAL tube is 288 lines, not 240. The dump
times are seconds after boot, so anything past the boot sequence needs about
ten.

Headless runs at sixty times real time and does not wait for threads, so a
screen fed by the network (the frame) shows its waiting state unless the cache
is already warm. `--realtime` makes the loop keep the wall clock, which
anything drawn from live data needs: without it the system monitor asks the
kernel for its counters more often than the kernel moves them and reads zero.

A sequence of frames for an animation is a `--dump` list rather than a loop of
runs:

```
stamps=$(python3 -c "print(','.join(f'{12.0+i*0.4:.2f}' for i in range(26)))")
omarchy-crt-shell --headless --realtime --browse monitor \
  --dump "$stamps" --dump-dir /tmp/seq
magick -delay 10 -loop 0 $(ls /tmp/seq/*.ppm | sort -V) \
  -filter point -resize 200% -colors 96 -layers optimize out.gif
```

**Try a different configuration without touching the user's.** `--config-dir`
points settings, profile and recents somewhere else:

```
cargo run --bin omarchy-crt-shell -- --headless --config-dir /tmp/cfg \
  --browse frame --dump 14.0 --dump-dir /tmp/shot
```

**Tests are for what can be decided on this machine**: parsers, name
matching, modelines, the video fit, durable writes, the shape of an answer
from a server. A test that needs a tube, a pad or a network is not written.
Every test gets a directory of its own under the temporary directory, because
they run at the same time.

## Conventions

**Commits** follow Conventional Commits: `feat(frame): ...`, `fix(crt): ...`.
The subject is lowercase, imperative, under 72 characters. The body says what
was wrong and why the change is right, in plain prose, and it is worth
writing: the history of this repository is the design document. No em dash or
en dash anywhere. No attribution trailers.

**Prose, everywhere it is durable**, means commit messages, code comments,
`docs/`, the README and the CHANGELOG: complete sentences, no filler, no
announcing what is about to be said. A comment explains why the code is the
way it is, not what the next line does. British spelling in prose
(`favourite`, `colour`) where it does not clash with an identifier; identifiers
and file formats keep the spelling they already have (`favorites.txt`,
`theme.colors`).

**The CHANGELOG** follows Keep a Changelog: one bullet per change under
`## [Unreleased]`, grouped Added, Changed, Fixed, Removed. What changed, not
how.

**Branches.** Work lands on `develop` and is merged to `main` when it has run
on the television. The remote has seen both a local merge and a pull request;
either is fine, but check `git log origin/main` before pushing, because the
two have diverged once already.

## Adding the usual things

**A screen.** A variant on `Screen` in `scene/mod.rs`, an arm in the
navigation match and one in the drawing match (both in `scene/browse.rs`), an
arm in `activate_browser` (or `Action::None` if A does nothing), a name in
`open_screen` so the control pipe and the desktop menu can reach it, a name in
`debug_browse` so it can be rendered headlessly, and a row in `HOME` or in the
hub it belongs to. The drawing itself goes in the `scene/` file for its
family, and a method another family calls is `pub(super)`. Watch
the home menu's height: the wordmark and the CRT tag take the top half of a
240 line screen, so eight rows fit and a ninth runs off the bottom. Render it
and look before assuming otherwise; that is how About ended up in Settings.

**Something drawn that has to look like 1994.** Two rules earn most of it.
No gradient is smooth: `sky.rs` has a 4x4 ordered dither and everything
graded goes through it, which is what a console with a fixed palette did.
And nothing is symmetrical or hand placed: the clouds, the buildings and the
stars come out of a small xorshift with a fixed seed, so the picture is the
same every evening and was never drawn by hand. Then render it and look at
it, at 320x240 and at 320x288.

**A page for an idle television.** It goes in the **Ambient** hub
(`AMBIENT_ITEMS`) as well as in `settings::PAGES`: the hub is how somebody
finds it on purpose, the list is how it takes its turn when the set is left
alone.
Putting one anywhere else because the home menu is full is how the photo
frame briefly ended up under Videos.

**A screensaver page.** `PAGES` in `settings.rs` is the list, and
`start_saver_page` puts one up. A page is a screen like any other, so it
needs everything in the paragraph above as well; being in that list is
what makes it take its turn in the mix and what makes the first key press
give the previous screen back. The mix is turned at the top of `draw`,
above every branch, because the effects page returns from the first one.

**A setting.** `settings.rs`: a field with `#[serde(default = "...")]` so an
older file still loads, a row in the matching settings page, an entry in
`docs/cli.md`, a CHANGELOG bullet. Never make a field required. A field that
moves to another section stays behind as `#[serde(default, skip_serializing)]`
and is copied over in `Settings::migrate`, so a file written by an older build
keeps what it asked for.

**A settings page.** `SETTINGS_LEFT` and `SETTINGS_RIGHT` in `scene/mod.rs`
are the two columns of the settings page: headings the cursor skips and rows
that each name a `Page`. Add the variant, add the row under the heading it
belongs to, and answer for it in `activate_settings`. Keep the columns the
same length, because left and right cross between them at the same height.
Nothing counts rows: a screen coming back to Settings calls
`settings_row(Page::Thing)`, because two of them counted to the wrong number
the moment the list grew. Three tests hold the rest: the columns and the rows
agree, a column clears the line a message uses, and a label fits the column
it is in.

**A console.** `library.rs`: an entry in `default_systems` with its core and
extensions, a line in `default_lines` for the height it draws, and options
only where the exact value strings are known to work. `docs/systems.md`
records what was chosen and why.

**A control pipe command.** Both ends have to be installed and the launcher
restarted, or the CLI sends something the launcher does not know and logs
`control: unknown input <name>`. Installing only `omarchy-crt` after adding
one is a whole debugging session on its own; ask for the launcher's log
before believing anything else.

**A CLI verb.** `shell/src/bin/omarchy-crt.rs`: the `HELP` text, an arm in the
dispatch, and a section in `docs/cli.md`. If the launcher has to do something,
it goes through the control pipe rather than a signal.

**A row in the Omarchy menu.** `menu/omarchy-menu.jsonc`, which the
installer writes into Omarchy's extension file. Two things are worth knowing
before designing one. The icons must be code points Omarchy's own menu already
uses, or the row comes out blank in whatever font the bar has. And a
**provider is not available to a third party**: the menu plugin holds its
providers in a fixed table in its own QML (`fonts`, `power-profiles`, and a
native `apps`), so a row cannot enumerate anything of ours at runtime. A list
that changes belongs behind a picker on the desktop, which is what
`bin/omarchy-crt-pick` is.

**Anything that talks to a network service.** Through `curl` as a
subprocess, with `--proto =http,https`, a timeout, a size cap, and a cache on
disk with an age. Nothing in the frame loop is allowed to block on a socket.

## The television, in the few facts that matter

- 15.731 kHz, 240 active lines at 60 Hz for NTSC; 288 at 50 Hz for PAL. The
  launcher's framebuffer is 320 by the line count.
- A game's console decides the line count, from `default_lines`, and the mode
  changes live. The launcher waits for the mode to settle before starting the
  emulator and after leaving it, because an emulator that starts first draws
  into the old mode and lands in a column.
- Centring is a property of the set in the room. It is found once by eye and
  saved; a mode change made for a game passes the saved shift, never zero.
- Interlace needs a patched kernel. See the rule above.
- The tube has no keyboard of its own once the connector is leased. The
  desktop preview window carries it, and `omarchy-crt shell key` and
  `omarchy-crt game key` are how anything else presses a button.

## Where to read more

| Question | File |
| --- | --- |
| Why any of this works at all | `docs/15khz.md`, `docs/rgb-pi-2.md` |
| What every CLI command does | `docs/cli.md` |
| What is set per console and why | `docs/systems.md` |
| Pads, mapping, rumble | `docs/input.md` |
| Fitting modern video to a 4:3 tube | `docs/video.md`, `docs/video-policy.md` |
| Something is broken | `docs/troubleshooting.md` |
| What works and what is next | `docs/plan.md` |
| What runs as root, what is downloaded | `SECURITY.md` |
