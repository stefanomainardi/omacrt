# Contributing

This is one person's television, given away because it turned out well. It is
not a product, there is no roadmap anybody is owed, and it is worked on when
it is fun to work on. That is the deal, and it is worth knowing before you
spend an evening on a change.

None of which means the door is shut. The parts of this that other people can
improve are real, and a good change is welcome.

## The rule that is not negotiable

**It has to have run on a real television.**

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

- **A new dependency** without a reason that cannot be met by the thirteen
  already here. This draws its own pixels on purpose.
- **A feature nobody asked for**, or one that only makes sense on your
  hardware. Open a discussion first and save yourself the work.
- **A refactor for taste.** Splitting a file because it is long is a change
  with all the risk of a rewrite and none of the benefit, unless it comes with
  a reason the code itself gives you.
- **Anything that patches Omarchy.** The whole argument of this project is
  that it did not have to. A change that needs a patched desktop, a forked
  plugin or a modified upstream file is the wrong change, and there is nearly
  always an extension point that does the job.

## Standards

Nothing exotic, but they are held to:

- `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` and
  `python3 scripts/audit.py` all clean. CI runs the same four.
- **Tests for what can be decided on a machine with no television**: parsers,
  name matching, modeline arithmetic, the video fit, durable writes. Not for
  what needs a tube; that is what the living room is for.
- **Conventional Commits**, and a body that says what was wrong and why the
  change is right. The history of this repository is its design document, so a
  commit that says "fix stuff" costs a future reader an hour.
- **Plain prose** in commits, comments and documentation. A comment explains
  why the code is the way it is, not what the next line does. No em dashes.
- **Code you can explain.** Use whatever tools you like, including an
  assistant, but the change is yours: it has to run on your television and you
  have to be able to say why every line of it is there. A patch its author
  cannot defend is not a contribution, it is homework for the maintainer.

[`AGENTS.md`](AGENTS.md) is the working guide: what the modules are, how to
render a screen without a television and look at it, how to add a screen, a
console, a setting or a CLI verb, and the handful of rules that do damage when
they are broken. Read it before the first change; it covers most of the ways
to get this wrong.

## Bugs, ideas and questions

- **A bug** is an issue, with the template filled in: the hardware, the
  television, and the output of `omacrt doctor`. That command exists to
  make a bug report answerable, so please run it.
- **An idea** is a discussion, not an issue. It costs nothing to float and it
  will not sit in a list making the project look neglected.
- **A question** is a discussion too. There is no support queue here, and
  nobody is on call.

## Branches

Work lands on `develop` and is merged to `main` when it has run on the
television. Pull requests go to `develop`.
