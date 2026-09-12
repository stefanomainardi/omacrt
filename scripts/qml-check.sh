#!/usr/bin/env bash
# Fail when this project's QML does not parse, and say what was ignored.
#
# The widget, the panel and the overlay are written against Quickshell and
# Omarchy's own QML modules. A CI runner can install neither, so every type
# they define is unknown there and qmllint says so at length: the import
# fails, each type is unresolved, a Loader's `item` is a bare QObject so every
# member on it is missing, and our BarWidget appears to inherit itself because
# Quickshell's type of that name cannot be seen.
#
# For a week the CI step answered that by discarding the result with
# `|| true`, which also discarded a syntax error. This keeps the one class of
# finding that means the same thing with or without those modules: the file
# does not parse. Everything qmllint says about types, members and properties
# is a guess without them, and a guess is not something to fail a build on:
# run this locally, where Quickshell at least is installed, to see the rest.
#
# Usage: scripts/qml-check.sh [files...]   (default: every QML in plugin/)
set -euo pipefail

lint=""
for candidate in /usr/lib/qt6/bin/qmllint qmllint6 qmllint; do
  command -v "$candidate" >/dev/null 2>&1 || continue
  # Qt 5 ships a qmllint of the same name that takes none of the same options
  # and exits 255 with no output at all, which would make this a coin toss.
  if "$candidate" --version 2>&1 | grep -qE 'qmllint 6'; then
    lint="$candidate"
    break
  fi
done
if [ -z "$lint" ]; then
  echo "no Qt 6 qmllint found (Arch: qt6-declarative, Debian: qt6-declarative-dev-tools)" >&2
  exit 1
fi

repo="$(cd "$(dirname "$0")/.." && pwd)"
if [ "$#" -gt 0 ]; then
  files=("$@")
else
  files=()
  for f in "$repo"/plugin/*.qml "$repo"/plugin/*/*.qml; do
    [ -f "$f" ] && files+=("$f")
  done
fi
if [ "${#files[@]}" -eq 0 ]; then
  echo "no QML files to check" >&2
  exit 1
fi

out="$("$lint" "${files[@]}" 2>&1 || true)"
# A finding carries a position. The source excerpt, the carets and the hints
# qmllint attaches underneath ("Did you mean", "parent is a member of a parent
# element") do not, and counting those would bury the line that matters.
found="$(printf '%s\n' "$out" | grep -cE '^(Error|Warning|Info): .*:[0-9]+:[0-9]+: ' || true)"
# A file that does not parse is the one thing that means the same with or
# without the modules. qmllint reports it at warning severity, so the category
# is what to match on, not the word in front of it.
bad="$(printf '%s\n' "$out" | grep -E '^Error: |\[syntax\]$' || true)"

if [ -n "$bad" ]; then
  printf '%s\n' "$bad"
  echo
  echo "QML that does not parse. The other $found findings are not checked here." >&2
  exit 1
fi
echo "qmllint: ${#files[@]} files parse; $found findings about types it cannot resolve, not checked here"
