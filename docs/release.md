# Opening the repository

What is left before this is public, in the order it de-risks the decision.
Written against what the repository actually holds today, measured rather
than guessed:

| | |
| --- | --- |
| Rust | 32,366 lines over 45 files. `scene.rs` alone is 7,988 of them, with 191 methods on one struct |
| Tests | 82, in five binaries: the parsers, the modelines, the video fit, the durable writes, the title matching |
| Docs | 2,982 lines over 17 files, all English since the 15 kHz study was rewritten |
| Root | one systemd oneshot and one script it runs, already hardened in the unit |
| Network | eight `curl` call sites, every one with `--proto`, a size cap and a timeout |
| Unsafe | 39 blocks: `libc` signals and ioctls, and the compositor's own bindings |
| Panics | 51 `unwrap()`, 3 `expect`, 2 `panic!` |
| Secrets | none in the history, no hard coded home path, no personal path in the tree |

## A. What gates the decision, not the work

1. ~~**The name.**~~ The letter about using *Omarchy* in the name went out on
   2026-09-08. The name stays, and the credits and the footer keep saying what
   the letter promised: Omarchy and its marks belong to the Omacom Foundation,
   this is a fan project, and it speaks for nobody but itself.
2. **Third party review.** `LICENSE` is MIT and `THIRD-PARTY.md` is 55 lines.
   Every asset that ships in the binary has to be in it: `font8x8`, the
   TerminalTextEffects ports, the Omarchy icon and wordmark decoded from
   Omarchy's own `logo.txt`, RetroArch's `systematic` pictures, and the
   libretro thumbnails fetched at runtime. The wordmark is the one that needs
   the letter above.
3. **A personal data pass.** The screenshots carry a hostname, a city, a live
   clock and the names of running programs. None of it is dangerous and all of
   it is a choice; make the choice on purpose rather than by accident.

## B. Security hardening

Nothing found so far is a hole. The work is to look properly and write down
what was found, so `SECURITY.md` is a statement rather than a promise.

- [ ] **What is executed.** `menu.rs` runs `sh -c` with a string. Confirm
      every caller passes an internal constant and never anything a file or a
      server chose, then say so in the comment and in `SECURITY.md`.
- [ ] **What panics.** 51 `unwrap()`. In the launcher a panic is a black
      television, so each one on a path that can be reached at run time
      becomes a fallback. The ones on a mutex or after a checked `is_some`
      can stay, with the reason written where they are.
- [ ] **What is downloaded.** Eight fetches, all leashed. Check each cache
      write goes through a temporary file and a rename, that a hostile answer
      cannot escape its directory, and that a name from a server never
      becomes a path.
- [ ] **What listens.** The control pipe (`0600`), the mpv socket, cliamp's
      socket. No port is opened. Confirm the modes and write it down.
- [ ] **What runs as root.** One oneshot unit, one script, `CAP_DAC_OVERRIDE`
      and `CAP_SYS_ADMIN`, `ProtectSystem=strict`. Re-read the script's
      argument validation with fresh eyes.
- [ ] **The dependency tree.** `cargo audit` once, and a note about which
      advisories are accepted and why.
- [ ] **The photograph server's key.** `immich.toml`, mode 600, handed to
      curl on its standard input. Document it where somebody setting it up
      will read it.

## C. Code review

- [ ] **Split `scene.rs`.** 7,988 lines and 191 methods in one file is the
      loudest thing in the repository, and the first thing a reader will
      judge. It divides along seams that already exist: boot, browse, music,
      video, the pause menu, the settings pages, the idle pages, the
      screensaver. Same crate, same struct, several files.
- [ ] **The nine `too_many_arguments` allowances.** Each is a drawing
      function that wants a struct. Take the ones where a struct is clearer
      and leave the ones where the geometry is the argument list.
- [ ] **Duplication in the drawing code.** The settings pages, the menu
      screens and the list rows have each been copied once too often.
- [ ] **Every `pub` in `lib.rs`.** Anything the CLI and the launcher do not
      both use should not be shared.
- [ ] **Read the whole thing once, out loud.** Comments that restate their
      line, names that lie, a function that grew a second job.

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
- [ ] **A drift pass.** Every claim in the README and in `docs/` checked
      against the code that is there today. Anything the project no longer
      does comes out.
- [ ] **The README's shape.** 492 lines is a long shop window. What a
      stranger needs in the first minute stays; the rest moves into `docs/`
      with a link.
- [ ] **Contribution rules.** `CONTRIBUTING.md`, an issue template that asks
      for the hardware and the output of `omarchy-crt doctor`, a pull request
      template, `CODEOWNERS`. Issues for bugs, ideas in Discussions.
- [ ] **The screenshots that are stale.** `library-overlay.png` and the
      panel, after the plugin change reaches an unlocked session.

## E. Publishing

- [ ] `develop` into `main`, and `main` is what the world sees.
- [ ] Tag `v0.1.0` and give the CHANGELOG a release section with a date.
- [ ] Repository settings: the description, the topics, Discussions on,
      issues for verified bugs, `main` protected.
- [ ] Private to public, once A1 has an answer.
