#!/usr/bin/env bash
# Install on a machine that is not Omarchy, and check what happens.
#
# The claim this makes true: OmaCRT needs Hyprland and the hardware, not a
# distribution and not Omarchy. It is the kind of claim that rots quietly,
# because the machine it is written on is an Omarchy machine and everything
# works there. So it is checked where it is not: continuous integration runs
# on a plain Ubuntu container with no Omarchy, no Arch, no graphics card and
# no television, which is exactly the machine this has to be honest about.
#
# What it asserts, on that machine:
#   - the installer succeeds and puts the binaries in ~/.local/bin
#   - it writes nothing at all under ~/.config/omarchy
#   - it says which four desktop pieces it did not install, and where the
#     keybindings that replace them are
#   - the self test runs, names the machine's real problems, and offers no
#     bar plugin row, because there is no bar
#   - `omacrt plugin` says there is nothing installed rather than failing
#
# Run it anywhere: it works on the author's Omarchy machine too, because it
# hides Omarchy from the PATH rather than pretending to be somewhere else.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

# A PATH with everything the installer legitimately needs and nothing of
# Omarchy's. Dropping the folders that hold `omarchy` is not enough on a
# machine where it also sits in /usr/bin beside every other tool, so the
# directories are mirrored as links with those two names left out. On a
# runner with no Omarchy this changes nothing and costs a second.
mirror="$scratch/path"
mkdir -p "$mirror"
while IFS= read -r dir; do
  [ -d "$dir" ] || continue
  for f in "$dir"/*; do
    name="${f##*/}"
    case "$name" in omarchy | omarchy-*) continue ;; esac
    [ -e "$mirror/$name" ] || ln -s "$f" "$mirror/$name" 2>/dev/null || true
  done
done < <(printf '%s' "$PATH" | tr ':' '\n')
clean_path="$mirror"
for stray in omarchy omarchy-shell; do
  if PATH="$clean_path" command -v "$stray" >/dev/null 2>&1; then
    echo "refusing to run: $stray is still on the PATH after hiding it" >&2
    exit 1
  fi
done

say() { printf '  %s\n' "$1"; }
fail() { printf 'FAIL: %s\n' "$1" >&2; exit 1; }

out="$scratch/install.log"
echo "installing into $scratch with no Omarchy on the PATH"
if ! HOME="$scratch" PATH="$clean_path" bash "$repo/bin/omacrt-install" --no-build \
    >"$out" 2>&1; then
  cat "$out" >&2
  fail "the installer exited non-zero"
fi

[ -x "$scratch/.local/bin/omacrt" ] || fail "no omacrt in ~/.local/bin"
[ -x "$scratch/.local/bin/omacrt-shell" ] || fail "no omacrt-shell in ~/.local/bin"
[ -x "$scratch/.local/bin/omacrt-display" ] || fail "no omacrt-display in ~/.local/bin"
say "the binaries are in ~/.local/bin"

if [ -e "$scratch/.config/omarchy" ]; then
  find "$scratch/.config/omarchy" >&2
  fail "it wrote into ~/.config/omarchy on a machine with no Omarchy"
fi
say "nothing was written under ~/.config/omarchy"

for phrase in "no Omarchy here" "the bar widget, the panel, the library overlay and" \
              "the Television entry in the desktop menu" "docs/hyprland.md"; do
  grep -qF "$phrase" "$out" || { cat "$out" >&2; fail "the installer never said: $phrase"; }
done
say "it says what it did not install, and where the keybindings are"

# The self test. It will fail on a machine with no graphics card and no
# television, and that is the point: it has to say so rather than break.
doctor="$scratch/doctor.log"
HOME="$scratch" PATH="$clean_path" "$scratch/.local/bin/omacrt" doctor --plain \
  >"$doctor" 2>&1 || true
grep -q "desktop integration" "$doctor" || { cat "$doctor" >&2; fail "no desktop integration row"; }
grep -q "Hyprland: the command line" "$doctor" \
  || { cat "$doctor" >&2; fail "the desktop row does not name the other channel"; }
if grep -q "bar plugin" "$doctor"; then
  cat "$doctor" >&2
  fail "a bar plugin row on a machine with no bar"
fi
grep -q "graphics driver" "$doctor" || fail "no graphics driver row"
say "the self test runs and reports the machine it is on"

plugin="$scratch/plugin.log"
HOME="$scratch" PATH="$clean_path" "$scratch/.local/bin/omacrt" plugin \
  >"$plugin" 2>&1 || true
grep -q "not installed" "$plugin" || { cat "$plugin" >&2; fail "omacrt plugin should say it is not installed"; }
say "omacrt plugin says there is none, rather than failing"

# And the uninstall, which used to report removing things that were never
# there.
un="$scratch/uninstall.log"
HOME="$scratch" PATH="$clean_path" bash "$repo/bin/omacrt-install" --uninstall \
  >"$un" 2>&1 || { cat "$un" >&2; fail "the uninstall exited non-zero"; }
[ -e "$scratch/.local/bin/omacrt" ] && fail "the uninstall left the binaries behind"
grep -q "removing the plugins" "$un" && { cat "$un" >&2; fail "it claims to remove plugins it never installed"; }
say "the uninstall takes back what it put there, and nothing else"

echo "a machine that is not Omarchy: every check passed"
