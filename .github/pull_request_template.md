## What this changes

<!-- What was wrong, and why this is the right fix. A sentence or two. -->

## What it ran on

<!--
  Required for anything touching the display path, the timings, the launcher's
  drawing or an emulator's configuration: the card, the DAC, the television,
  the standard. "No television, this is scanner/CLI/docs only" is a fine
  answer where it is true.
-->

## Checks

- [ ] `cargo fmt` and `cargo clippy --all-targets -- -D warnings` clean
- [ ] `cargo test` passes
- [ ] `python3 scripts/audit.py` passes
- [ ] Tests added for anything that can be decided without a television
- [ ] `CHANGELOG.md` has a line under Unreleased, if a user would notice
