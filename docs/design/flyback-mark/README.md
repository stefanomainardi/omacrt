# The Flyback mark, as it was drawn

The working files behind [`docs/identity.md`](../../identity.md). Every sheet
is a self-contained HTML artboard: open one in a browser and it renders on its
own, or seed them together onto one pan-and-zoom canvas with the `/design`
skill in Claude Code.

| | |
| --- | --- |
| `sketch/DirectionA.dc.html` | the whole raster, with one unbroken staircase return |
| `sketch/DirectionB.dc.html` | four bars, the last one stopping mid-scan |
| `sketch/DirectionC.dc.html` | the ramp and its return stroke, as first drawn |
| `Main.dc.html` | the chosen direction, finished: construction, the numbers, the size ramp |
| `Colour.dc.html` | colour, clear space, the mark as the tube draws it, and four misuses |
| `Lockup.dc.html` | the two lockups, and the family beside Retrace |
| `canvas.json` | where the three finished sheets sit on the canvas |

The rejected directions are kept deliberately. A process nobody can inspect is
a claim rather than a record.

`sketch/directions.png` and `refinement.png` are rendered from those sheets,
which is also how to regenerate them: pull the first `<svg>` out of a sheet and
run it through `rsvg-convert`.

## The geometry, in one place

Everything on these sheets sits on the grid the parent mark already defined in
`shell/src/assets.rs`:

| | |
| --- | --- |
| grid | 44 × 44 |
| bar thickness | 8 |
| gap | 4 |
| pitch | 12 |
| Flyback's return stroke | 8 × 44, four units clear of the bars |
| Flyback's bar widths | 8, 16, 24, 32, flush right at 44 |

Whole units only. The mark has to be drawable by a beam on 240 scan lines.
