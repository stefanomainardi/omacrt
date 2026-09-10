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
import time
import urllib.error
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
# The database is asked again before the run is given up on. A check that
# passes when it could not reach the database is not a check.
ATTEMPTS = 3
BACKOFF_SECONDS = 5


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
    last = None
    for attempt in range(1, ATTEMPTS + 1):
        try:
            with urllib.request.urlopen(request, timeout=60) as answer:
                body = json.load(answer)
            break
        except Exception as why:  # network, timeout, or a body that is not JSON
            last = why
            if attempt < ATTEMPTS:
                print(
                    f"advisory database attempt {attempt} failed ({why}); "
                    f"trying again in {BACKOFF_SECONDS}s",
                    file=sys.stderr,
                )
                time.sleep(BACKOFF_SECONDS)
    else:
        raise RuntimeError(f"the advisory database could not be reached: {last}")

    results = body.get("results")
    if not isinstance(results, list):
        raise RuntimeError(f"the advisory database answered without results: {body!r}")
    # One answer per question, or the answers do not line up with the crates
    # they are about and `zip` would quietly drop the rest.
    if len(results) != len(packages):
        raise RuntimeError(
            f"asked about {len(packages)} crates and got {len(results)} answers"
        )
    found = []
    for (name, version), result in zip(packages, results):
        if not isinstance(result, dict):
            raise RuntimeError(f"{name} {version}: the answer is not an object")
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
    except Exception as why:
        # A run that could not ask is not a run that found nothing. Saying so
        # and returning success is how a green tick comes to mean nothing.
        print(f"the advisory check did not run: {why}", file=sys.stderr)
        return 2

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
