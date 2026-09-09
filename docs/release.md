# Opening the repository

What is left before this is public, in the order it de-risks the decision.
Written against what the repository actually holds today, measured rather
than guessed:

| | |
| --- | --- |
| Rust | 32,432 lines over 66 files. `scene.rs` held 7,988 of them and 191 methods; it is ten files now, the largest 2,776 |
| Tests | 84, in five binaries: the parsers, the modelines, the video fit, the durable writes, the title matching |
| Docs | 3,248 lines over 17 files, all English since the 15 kHz study was rewritten |
| Root | one systemd oneshot and one script it runs, already hardened in the unit |
| Network | eight `curl` call sites, all through one function that sets `--proto`, a size cap and a timeout |
| Unsafe | 39 blocks: `libc` signals and ioctls, and the compositor's own bindings |
| Panics | 10 outside the tests, each with its reason beside it |
| Secrets | none in the history, no hard coded home path, no personal path in the tree |

## A. What gates the decision, not the work

1. ~~**The name.**~~ The letter about using *Omarchy* in the name went out on
   2026-09-08. The name stays, and the credits and the footer keep saying what
   the letter promised: Omarchy and its marks belong to the Omacom Foundation,
   this is a fan project, and it speaks for nobody but itself.
2. ~~**Third party review.**~~ Done. Three things are compiled into the
   binary and all three are now named with their terms: `font8x8` (public
   domain), the TerminalTextEffects ports (MIT, behaviour followed rather than
   code copied), and **Omarchy's own wordmark**, which is a committed copy of
   its `logo.txt`. That last one is the finding: MIT asks for the notice to
   travel with the copy, so `shell/assets/LICENSE.omarchy` now does, and the
   README says whose it is. RetroArch's `systematic` pictures, the libretro
   buildbot, wttr.in and Immich were missing from the list and are in it.
3. **A personal data pass.** The screenshots carry a hostname, a city, a live
   clock and the names of running programs. None of it is dangerous and all of
   it is a choice; make the choice on purpose rather than by accident. The one
   that needs a decision is `library-overlay.png`: it shows two source paths,
   `/run/media/stefano/...` and `/home/stefano/Games/roms`, and a count of
   games indexed. It has to be retaken anyway once the session is unlocked, so
   the decision is whether the retake redacts the paths and the number.

## B. Security hardening

Done on 2026-09-08. Two real findings, both in the newest code, both fixed;
everything else was already right and is now written down in `SECURITY.md`
rather than promised.

- [x] **What is executed.** There was one caller and it passed a constant,
      but the shell is gone rather than documented: `Action::Launch` carries an
      argv now and `menu.rs` cannot run a command line at all.
- [x] **What panics.** 51 counted, 18 outside the tests, 10 now. The three
      float sorts behind `--dump` took a value from the command line into
      `partial_cmp().unwrap()`, so `--dump nan` was a panic; they use a total
      order. A lease without a file descriptor is an error rather than a panic
      the watchdog would restart in a loop. The ten that stay are mutex locks
      and one just-pushed vector, each with the reason on the line above.
- [x] **What is downloaded.** All eight leashed. Two real findings: the
      photograph server's asset id went straight into a file name, so an
      answer of `../../.ssh/authorized_keys` would have decided where a file
      was written, and the photo cache had ffmpeg write its final name
      directly. The id is checked against letters, digits and dashes before
      it is used, and the file is written beside its name and moved into
      place. A note beside a picture also survives a name with a newline.
- [x] **What listens.** Both named pipes are created `0600`, the launcher's
      and the display process's. No port is opened anywhere.
- [x] **What runs as root.** Unchanged and still right: the script refuses a
      connector name that is not letters, digits and dashes and one that does
      not exist, and the unit gives it two capabilities and three writable
      paths.
- [x] **The dependency tree.** `scripts/audit.py` asks OSV about all 157
      locked crates with nothing installed but Python, and runs in CI. Two
      advisories stand, both `cgmath` through `smithay`, never called from
      here, listed with their reasons; anything new fails the build.
- [x] **The photograph server's key.** Documented in `SECURITY.md`, which
      also stopped claiming the project holds no credential at all, because
      since the photo frame it can.

## C. Code review

- [x] **Split `scene.rs`.** Ten files where there was one: `mod.rs` keeps the
      state, the dispatch, the frame loop and the shared helpers (1,447
      lines), and the 151 methods that belong to a family moved to it, the
      largest being `browse.rs` at 2,776 because the navigation and drawing
      matches answer for every screen the browser has. Not a line was
      rewritten: the mover proved it could put the file back together
      first, 81 methods became `pub(super)` because a sibling calls them, and
      every screen was rendered before and after. Eight frames byte
      identical, and the seventeen that differ do so only where a clock or a
      live counter is drawn, which two runs of the same binary a minute apart
      also do.
- [x] **The `too_many_arguments` allowances.** The lint was allowed for the
      whole crate, which made the nine attributes dead decoration. The lint is
      on now and the exceptions are five, each with a line saying why: a
      rectangle, a level and a phase are the geometry, and a struct would hide
      it. Two were not geometry and got their struct: the sky's seven
      conditions became `Air`, threaded through ten layers as one thing, and
      the photo supply's seven arguments became `Wanted`, which is comparable,
      so asking for the same thing twice now changes nothing by construction
      rather than by a pair of extra fields.
- [x] **Duplication.** A window hash over `shell/src` found four places
      where ten lines or more existed twice. The photo frame's caption was
      derived twice from the same six fields and the two had already drifted;
      `crt_mode` and `crt_mode_async` built the same command line; and
      `draw_video_fit` was a second copy of `draw_settings_table`, header to
      hints, for the sake of one extra line of text, which the table now
      takes as an optional footer. The fourth was the flag block every
      `curl` call site repeated, which became `net::curl`: eight lists that
      happened to agree, three of which had stopped agreeing, are one
      function that grants the leash. Exercised against the real servers.
- [x] **Every `pub` in `lib.rs`.** Twenty six public functions were called
      from nowhere outside their own file, so the shared library's surface was
      describing itself rather than what is shared. Twenty five are private
      now and one, `crt::applied_standard`, was dead and is gone.
- [x] **Read the whole thing once, out loud.** The comments came out clean,
      by eye and by a scan for a comment whose words are its next line's. Five
      other things did not: `SAVER_PAGES`, named in a doc comment and twice in
      `AGENTS.md`, has not existed since the screensaver settings were
      separated; `blit_trapezoid` was documented against a `blit_scaled` that
      never existed; `sweep_orphans` returned how many messes it had found
      while claiming to return how many went, so an emulator that refused to
      die was logged as cleared; `gradient` in the etcher would have
      underflowed on an empty step list; and the lease's file descriptor was
      unwrapped in a program where every other failure prints a line and
      exits. The ten remaining panics now each say why on the line above,
      which is what `SECURITY.md` had already promised for them.

## D. Documentation

- [x] **The 15 kHz study into English.** Done, as `docs/15khz.md`, and not a
      translation: the study was written before anything worked and its two
      main conclusions, that HDMI was impossible and that a compositor could
      never change modes per game, are both disproved by what shipped. The
      page keeps what is still true (what a SCART set wants on which pin, the
      three walls a modern GPU hits, the kernel patch landscape, the two
      Hyprland findings, the missing EDID) and replaces the rest with what
      actually happened and why, with the real modelines and the numbers
      checked against `crt.toml`.
- [x] **A drift pass.** Done by checking rather than rereading: the
      launcher's flag table against the binary's own usage, the README's CLI
      block against `--help`, the documented systems against
      `default_systems`. Three findings, all fixed: a flag for a file that no
      longer exists, thirteen verbs the README never named, and three systems
      missing from the table.
- [x] **The README's shape.** The three honest notes, the AI one included,
      opened the file: thirty lines of caveats before a single picture. What
      it is, what it looks like, what it does and why now come first, and the
      notes sit where a reader can actually judge them. The mermaid flowchart,
      which said the same thing as the drawn diagram above it, moved to
      `docs/architecture.md`. 484 lines, and every internal link in every
      markdown file resolves.
- [x] **Contribution rules.** `CONTRIBUTING.md`, a bug template that asks for
      the machine, the tube and the output of `omacrt doctor`, blank
      issues disabled with Discussions in their place, a pull request template
      that asks what a change ran on, and `CODEOWNERS`. The README's own
      section became a pointer rather than a second copy.
- [x] **`library-overlay.png`.** Retaken, and the retake found three real
      layout faults rather than just being a picture: the card had a fixed
      height and cut the last system in half, the folder field reserved too
      little room and drew "Rescan sources" off the edge, and two paths
      still carried a home directory. All three fixed, so the picture shows
      `~/Games/roms`, `External HD/roms`, `~/Videos` and
      `~/.config/retroarch/system` with the game count kept.
      Quickshell loads an overlay's QML once, at the first summon, and keeps
      it: `rescanPlugins` is not one of its IPC methods and disable/enable
      does not drop it, so the fixes were verified against a separate
      Quickshell instance rather than the running shell.
- [ ] **`panel.png`.** Stale only in its numbers (21880 games in 14 systems
      against 28852 in 15) and its launcher pid. Nothing personal in it. The
      panel is a bar widget rather than a standalone overlay, so retaking it
      wants either the shell reloaded or the same trick with a host widget.
- [ ] **The hostname in `boot.gif`.** The POST lines read `/etc/hostname`,
      so the boot sequence shows this machine's name. A choice rather than a
      risk, and the only personal thing left in the pictures.

## E. Publishing

- [x] `develop` into `main`, and `main` is what the world sees.
- [x] Tag `v0.4.0` and give the CHANGELOG a release section with a date.
      `v0.3.0` exists and is left where it is: it was cut on 2026-09-08, under
      the old name and before the read through of the next day, so its tree
      says `omarchy-crt` throughout and carries none of those fixes. A tag
      that has been published is not moved; the release the repository opens
      with is 0.4.0. `omacrt version` says which one is installed, and the
      user agent the fetches send is built from the same number.
- [x] Repository settings: the description now says what the project is
      rather than what it boots, twelve topics, Discussions on. **`main`
      cannot be protected while the repository is private**: GitHub answers
      403 and asks for Pro or a public repository, so that rule goes on
      immediately after the flip, requiring the CI check and refusing force
      pushes and deletion.
- [ ] Private to public. Everything else is done and CI is green on `main`
      at `v0.4.0`; what is left is the decision and the two screenshots
      below.
- [ ] Protect `main` the moment it is public: require the CI check, refuse
      force pushes and deletion. GitHub answers 403 to that on a private
      repository, which is why it could not be set before.
