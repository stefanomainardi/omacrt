#!/usr/bin/env python3
"""Check every locked crate against the advisory database.

The same source `cargo audit` reads, asked directly over OSV's batch API, so
this needs nothing installed but Python and works the same on this machine
and in CI.

    python3 scripts/audit.py            report, and fail on anything new
    python3 scripts/audit.py --list     report everything, including the known

An advisory that cannot be fixed here belongs in ACCEPTED below, with the
reason written next to it. Anything else fails the run.
"""

import json
import re
import sys
import urllib.request
from pathlib import Path

# Advisories that stay, and why. Each one is a transitive dependency this
# project does not choose and does not use.
ACCEPTED = {
    "RUSTSEC-2026-0196": (
        "cgmath is unmaintained. It arrives through smithay, which the display "
        "process needs for its own compositor. Informational; nothing here "
        "calls cgmath."
    ),
    "RUSTSEC-2026-0197": (
        "cgmath's Matrix::swap_columns is unsound when both indices are the "
        "same. Same path: smithay's dependency, never called from this code, "
        "and unreachable without calling it."
    ),
}

OSV = "https://api.osv.dev/v1/querybatch"


def locked(lock: Path) -> list[tuple[str, str]]:
    """Every crate and version in a Cargo.lock."""
    text = lock.read_text()
    out = []
    for block in text.split("[[package]]")[1:]:
        name = re.search(r'^name = "([^"]+)"', block, re.M)
        version = re.search(r'^version = "([^"]+)"', block, re.M)
        if name and version:
            out.append((name.group(1), version.group(1)))
    return out


def advisories(packages: list[tuple[str, str]]) -> list[tuple[str, str, str]]:
    """Ask OSV about all of them at once."""
    queries = [
        {"package": {"name": n, "ecosystem": "crates.io"}, "version": v}
        for n, v in packages
    ]
    request = urllib.request.Request(
        OSV,
        data=json.dumps({"queries": queries}).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=60) as answer:
        results = json.load(answer).get("results", [])
    found = []
    for (name, version), result in zip(packages, results):
        for vuln in result.get("vulns") or []:
            found.append((vuln.get("id", "?"), name, version))
    return found


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    lock = root / "shell" / "Cargo.lock"
    if not lock.exists():
        print(f"no lock file at {lock}", file=sys.stderr)
        return 2
    packages = locked(lock)
    print(f"{len(packages)} crates locked")
    try:
        found = advisories(packages)
    except Exception as why:  # a network that is not there is not a failure
        print(f"could not reach the advisory database: {why}", file=sys.stderr)
        return 0

    unexpected = []
    for advisory, name, version in sorted(found):
        known = advisory in ACCEPTED
        mark = "known" if known else "NEW"
        print(f"{mark:5} {advisory}  {name} {version}")
        if known and "--list" in sys.argv:
            print(f"      {ACCEPTED[advisory]}")
        if not known:
            unexpected.append((advisory, name, version))

    if not found:
        print("no advisory against any version in the tree")
    if unexpected:
        print(
            f"\n{len(unexpected)} advisory(ies) with nowhere to go. Fix them, or "
            "add them to ACCEPTED in this file with the reason.",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
