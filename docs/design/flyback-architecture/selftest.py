#!/usr/bin/env python3
"""Break the drawing on purpose, four ways, and require check.py to catch it.

A check that has only ever been run against a drawing that is correct is a
check nobody has tested. Two of the four cases below were genuinely missed by
earlier versions of check.py: the label long enough to cross a panel, which
was reported as having four hundred units of room, and the right-anchored
label, which was measured as though it started where it ends.

The breakages are derived from the current drawing rather than stored beside
it, so they cannot go stale when the drawing changes. `--write DIR` emits
them as files for somebody else's checker to be run against, which is the
point: a check that only works on the drawing it was written against is the
same trap one layer up.

    python3 selftest.py
    python3 selftest.py --write /tmp/broken

To check the suite itself, break the checker rather than the drawing: set the
ground-recognising condition in check.py to False, which is the state before
the fix that occasioned it, and run this. Six cases stay green and one goes
red, "a drawing with a ground rectangle no longer has its label clearances
checked", and the run exits one. A suite that cannot be made to fail by
removing the thing it guards is not testing that thing.
"""
import re
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).parent
GOOD = HERE / "Main.dc.html"
# The exported figure, which carries a ground rectangle covering the whole
# page. It is here because a ground counted as a container silently switches
# the clearance test off: every free-standing label finds it, is measured
# against the edges of the page, and fits. The same breakage has to be caught
# on a drawing with a ground as on one without.
EXPORTED = HERE / "../../media/architecture.svg"


def push_baseline_down(svg):
    """A line added to a box that was never told its height changed."""
    return re.sub(r'(<text[^>]*x="330" y=")(\d+)(")',
                  lambda m: f"{m.group(1)}{int(m.group(2)) + 40}{m.group(3)}", svg, count=1)


def label_crosses_panel(svg):
    """An arrow label long enough to sit on top of a panel rather than beside
    it, which "measure to the nearest panel on each side" skips in silence."""
    return svg.replace(">wp_drm_lease_v1<", ">wp_drm_lease_v1_and_then_some<", 1)


def label_eats_its_clearance(svg):
    """The same label a little longer: still in the gap, no longer clear of it."""
    return svg.replace(">wp_drm_lease_v1<", ">wp_drm_lease_v1xxxx<", 1)


def line_past_its_box(svg):
    """Text widened past the right edge of the box it is in."""
    return svg.replace(">0600 in the user's own state folder<",
                       ">0600 in the user's own state folder and then a great deal more<", 1)


CASES = {
    "baseline-below-its-rule": (push_baseline_down, "below its own rule"),
    "label-crosses-a-panel": (label_crosses_panel, "runs over a panel"),
    "label-without-clearance": (label_eats_its_clearance, "clear on the"),
    "line-past-its-box": (line_past_its_box, "past its right edge"),
}


def run(path):
    return subprocess.run([sys.executable, str(HERE / "check.py"), str(path)],
                          capture_output=True, text=True)


def main():
    good = GOOD.read_text()
    out_dir = None
    if "--write" in sys.argv:
        out_dir = Path(sys.argv[sys.argv.index("--write") + 1])
        out_dir.mkdir(parents=True, exist_ok=True)
        (out_dir / "good.svg").write_text(re.search(r"<svg .*?</svg>", good, re.S).group(0))

    failed = []
    # Both real drawings have to pass, and the second one is not redundant:
    # Frame.dc.html carries a right-anchored label, which an earlier checker
    # measured as though it started where it ends and reported as thirty-nine
    # units past its box. A false positive is only visible on a drawing that
    # is correct, so the guard against one is a correct drawing that passes.
    for real in (GOOD, HERE / "Frame.dc.html"):
        r = run(real)
        if r.returncode:
            failed.append(f"{real.name} does not pass: {r.stderr.strip()}")
        elif real.name not in r.stdout or not r.stdout.startswith("ok:"):
            # A declined drawing also exits zero. Requiring the pass to be a
            # judgement rather than an absence of complaint is the same rule
            # as everywhere else here: an instrument that says nothing and an
            # instrument that says fine are not the same answer.
            failed.append(f"{real.name} was not judged: {r.stdout.strip()}")
        else:
            print(f"ok   {real.name} judged and passes")

    if EXPORTED.exists():
        exported = EXPORTED.read_text()
        broken = label_eats_its_clearance(exported)
        if broken == exported:
            failed.append("the exported figure: the clearance breakage did not apply")
        else:
            with tempfile.NamedTemporaryFile("w", suffix=".svg", delete=False) as f:
                f.write(broken)
                path = f.name
            r = run(path)
            if not r.returncode or "clear on the" not in r.stderr:
                failed.append("a drawing with a ground rectangle no longer has its "
                              "label clearances checked")
            else:
                print("ok   clearance is still checked on a drawing with a ground")

    for name, (break_it, expect) in CASES.items():
        broken = break_it(good)
        if broken == good:
            failed.append(f"{name}: the breakage did not apply, the drawing has changed under it")
            continue
        if out_dir:
            (out_dir / f"{name}.svg").write_text(re.search(r"<svg .*?</svg>", broken, re.S).group(0))
        with tempfile.NamedTemporaryFile("w", suffix=".svg", delete=False) as f:
            f.write(broken)
            path = f.name
        r = run(path)
        if not r.returncode:
            failed.append(f"{name}: NOT CAUGHT")
        elif expect not in r.stderr:
            failed.append(f"{name}: caught, but not for the right reason: {r.stderr.strip()}")
        else:
            first = r.stderr.strip().splitlines()[0].split(": ", 1)[-1]
            print(f"ok   {name}: {first}")

    if out_dir:
        print(f"\nwrote {len(CASES) + 1} files to {out_dir}")
    if failed:
        for f in failed:
            print(f"FAIL {f}", file=sys.stderr)
        sys.exit(f"{len(failed)} of {len(CASES) + 3} checks failed")
    print(f"\nall {len(CASES) + 3} pass")


if __name__ == "__main__":
    main()
