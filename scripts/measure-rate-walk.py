#!/usr/bin/env python3
"""The two dashed rules, and nothing else crosses the picture.

Each carries 44 dashes, which is what tells it from anything the camera or
the tube breaks up. Separation is 3/4 of the picture height; it is reported
over the rule's own width, measured in the same frame, so a hand-held
camera's drift divides out. The width cannot change: the line rate never
moves.
"""
import glob, statistics
from PIL import Image
THR = 140

def rowruns(px, w, y):
    n, inr, a, b = 0, False, None, None
    for x in range(w):
        lit = px[x, y] > THR
        if lit:
            if not inr: n += 1
            if a is None: a = x
            b = x
        inr = lit
    return n, a, b

def measure(f):
    im = Image.open(f).convert("L"); w, h = im.size; px = im.load()
    cand = []
    for y in range(h):
        n, a, b = rowruns(px, w, y)
        if n >= 12 and a is not None and b - a > 300:
            cand.append((y, n, a, b))
    if len(cand) < 2: return None
    groups, cur = [], [cand[0]]
    for c in cand[1:]:
        if c[0] - cur[-1][0] <= 6: cur.append(c)
        else: groups.append(cur); cur = [c]
    groups.append(cur)
    if len(groups) != 2: return None, len(groups)
    t, b = groups
    ty = sum(x[0] for x in t)/len(t); by = sum(x[0] for x in b)/len(b)
    wid = max(max(x[3]-x[2] for x in t), max(x[3]-x[2] for x in b))
    if by - ty < 80 or wid < 350: return None
    return by - ty, wid

if __name__ == "__main__":
    rows = []
    for f in sorted(glob.glob("f*.jpg")):
        m = measure(f)
        if isinstance(m, tuple) and len(m) == 2 and m[0]:
            rows.append((f, m[0], m[1], m[0]/m[1]))
    print(f"{len(rows)} of {len(glob.glob('f*.jpg'))} frames measured")
    # steps are plateaus of the ratio; 30 frames each at 5 fps
    runs, cur = [], [rows[0]]
    for r in rows[1:]:
        if abs(r[3]-statistics.median(x[3] for x in cur))/statistics.median(x[3] for x in cur) < 0.02:
            cur.append(r)
        else:
            runs.append(cur); cur=[r]
    runs.append(cur)
    print("\n  frames   h/w      spread    first frame")
    for g in runs:
        if len(g) >= 10:
            rs=[x[3] for x in g]
            print(f"   {len(g):>4}   {statistics.median(rs):.4f}   {max(rs)-min(rs):.4f}   {g[0][0]}")
