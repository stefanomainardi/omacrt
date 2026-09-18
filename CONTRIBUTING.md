# Contributing

OmaCRT is a personal project maintained in spare time, with no fixed roadmap.
Contributions are welcome. Discuss substantial changes before starting work
so their scope and testing requirements are clear.

## Hardware testing

**Changes to display behaviour must be tested on a real CRT.**

Half of this project cannot be tested any other way. A modeline the kernel
accepts can still show a strip; a picture that is centred on one set is off on
another; a core that draws 480 lines looks fine in a screenshot and wrong on
glass. So a change that touches the display path, the timings, the launcher's
drawing or an emulator's configuration has to say, in the pull request, what
it ran on: the card, the DAC, the set, the standard.

If you do not have a CRT, there is still plenty here: the library scanner, the
title matching, the video fit, the CLI, the documentation. Say so, and say
what you did check.

## What gets merged

- **A fix with a reproduction.** What you did, what happened, what should have
  happened.
- **A console that needed its own settings**, with a note on why: the line
  count it drew, the core it needs, what was wrong before.
- **A correction to the documentation.** Anything here that is out of date or
  simply wrong. These are the easiest to merge and the most useful.
- **A page for the launcher** that belongs: another screensaver page, another
  visualiser, another test pattern.

## What probably does not

- **A new dependency** without a reason the existing dependencies cannot meet.
  The launcher deliberately draws its own pixels.
- **A feature nobody asked for**, or one that only makes sense on your
  hardware. Open a discussion first and save yourself the work.
- **A refactor without a concrete benefit.** Explain the maintenance,
  correctness or testing problem it addresses.
- **Anything that patches Omarchy.** Use its extension points without modifying
  upstream files or forking its plugins. Improvements to plain Hyprland support
  are welcome; see [`docs/hyprland.md`](docs/hyprland.md).

## Standards

- `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` and
  `python3 scripts/audit.py` all clean. CI runs the same four.
- **Tests for what can be decided on a machine with no television**: parsers,
  name matching, modeline arithmetic, the video fit, durable writes. Not for
  what needs a tube; that is what the living room is for.
- **Conventional Commits**, with a body explaining the problem and the reason
  for the change.
- **Plain prose** in commits, comments and documentation. A comment explains why the code is the way it is. The next line speaks for itself. No em dashes.
- **Code you can explain.** AI assistance is welcome. Contributors remain
  responsible for understanding and testing their changes, including the
  hardware checks described above.

Read [`AGENTS.md`](AGENTS.md) before making changes. It covers the modules,
headless rendering, common development tasks and operational precautions.

## Bugs, ideas and questions

- **A bug** is an issue, with the template filled in: the hardware, the
  television, and the output of `omacrt doctor`. That command exists to
  make a bug report answerable, so please run it.
- **Ideas and questions** belong in discussions. Support is provided as time
  allows.
- **What your television did** is an issue with its own template, and the most useful thing anybody can send. Every number in this project was
  taken on one set in one room; what a different tube, a different converter
  or a different card does with the same timings is the part this cannot find
  out on its own. A set that refused to lock is as welcome as one that did.
  Reports land in [`docs/sets.md`](docs/sets.md).

## Branches

Work lands on `develop` and is merged to `main` when it has run on the
television. Pull requests go to `develop`.
