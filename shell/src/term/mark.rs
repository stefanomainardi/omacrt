//! The mark, in characters.
//!
//! The launcher draws four bars and the dark cut of the beam's return with
//! [`crate::assets::RETRACE`], in framebuffer pixels. A terminal has cells
//! rather than pixels, and a cell is about twice as tall as it is wide, so
//! two pixel rows are packed into one row of `▀`, `▄` and `█` and the
//! geometry comes out square on the screen. Nothing here decides what the
//! mark looks like: it asks the same `segments` the tube is drawn from.
//!
//! The cut moves and the bars stand still. On the tube the return happens
//! every nine seconds because nothing else is going on; here it happens
//! when a check answers, so the one thing that moves is the one thing that
//! means something.

use crate::assets::{RETRACE, Retrace};

/// The mark's size in grid units: six character rows by twelve columns, the
/// smallest that keeps four bars, three gaps and a cut of the right width
/// apart from each other. It stands beside the wordmark drawn at half its
/// size, which is five rows, so the two are the same height.
pub const SIZE: f32 = 12.0;

/// How long a return takes, in seconds. `Scene::RETRACE_LASTS`.
pub const LASTS: f32 = 0.46;

/// One cell of the mark: the character, and whether it is the edge the cut
/// has just left, which stays a shade brighter for a frame or two.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Cell {
    pub ch: char,
    pub hot: bool,
}

/// The mark with the cut at `base`, as rows of cells.
pub fn rows(size: f32, base: i32) -> Vec<Vec<Cell>> {
    let r = &RETRACE;
    let segs = r.segments(size, base);
    let side = size.round() as i32;
    let width = ((r.grid as f32) * (size / r.grid as f32)).round() as i32;
    // 0 empty, 1 lit, 2 the hot edge.
    let mut px = vec![0u8; (side.max(0) * width.max(0)) as usize];
    let at = |y: i32, x: i32| (y * width + x) as usize;
    for seg in segs {
        for y in seg.y..(seg.y + seg.h).min(side) {
            for x in seg.x..(seg.x + seg.w).min(width) {
                if y >= 0 && x >= 0 {
                    px[at(y, x)] = 1;
                }
            }
        }
        if seg.hot && seg.x < width {
            for y in seg.y..(seg.y + seg.h).min(side) {
                if y >= 0 {
                    px[at(y, seg.x)] = 2;
                }
            }
        }
    }
    let mut out = Vec::new();
    let mut y = 0;
    while y < side {
        let mut row = Vec::with_capacity(width as usize);
        for x in 0..width {
            let top = px[at(y, x)];
            let bottom = if y + 1 < side { px[at(y + 1, x)] } else { 0 };
            let ch = match (top > 0, bottom > 0) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => ' ',
            };
            row.push(Cell {
                ch,
                hot: top == 2 || bottom == 2,
            });
        }
        out.push(row);
        y += 2;
    }
    out
}

/// How wide the mark is, in columns, at the size the report draws it.
pub fn cols(size: f32) -> usize {
    let r = &RETRACE;
    ((r.grid as f32) * (size / r.grid as f32)).round().max(0.0) as usize
}

/// Where the cut is, `k` of the way through a return. `Scene::retrace_base`,
/// which is the whole animation: one integer.
pub fn base_at(k: f32) -> i32 {
    let r: &Retrace = &RETRACE;
    if !(0.0..1.0).contains(&k) {
        return r.rest;
    }
    let jump = 0.09;
    let travel = if k < jump {
        r.thick as f32 * (k / jump)
    } else {
        let p = ((k - jump) / (1.0 - jump)).clamp(0.0, 1.0);
        let p = p * p * (3.0 - 2.0 * p);
        r.thick as f32 + (r.span() - r.thick) as f32 * p
    };
    let (span, lo) = (r.span() as f32, r.low() as f32);
    (((r.rest as f32 + travel - lo) % span + span) % span + lo).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(rows: &[Vec<Cell>]) -> Vec<String> {
        rows.iter()
            .map(|r| r.iter().map(|c| c.ch).collect())
            .collect()
    }

    #[test]
    fn the_mark_is_six_rows_of_twelve_columns() {
        let m = rows(SIZE, RETRACE.rest);
        assert_eq!(m.len(), 6);
        assert!(m.iter().all(|r| r.len() == 12));
        assert_eq!(cols(SIZE), 12);
    }

    #[test]
    fn it_stands_the_same_height_as_the_half_wordmark() {
        // Six rows of mark against five of word: the pair reads level.
        assert_eq!(crate::term::wordmark_half().len(), 5);
        assert_eq!(rows(SIZE, RETRACE.rest).len(), 6);
    }

    #[test]
    fn four_bars_and_a_cut_that_steps() {
        // At rest the cut sits near the left of the top bar and one pitch
        // further right on each bar below it, which is the 45 degree edge.
        let m = text(&rows(SIZE, RETRACE.rest));
        let gap: Vec<usize> = m
            .iter()
            .map(|r| r.chars().filter(|c| *c == ' ').count())
            .collect();
        assert!(gap.iter().any(|g| *g > 0), "{m:?}");
        // No row is empty: every bar is drawn whatever the cut is doing.
        assert!(m.iter().all(|r| r.chars().any(|c| c != ' ')), "{m:?}");
    }

    #[test]
    fn the_cut_leaves_and_the_mark_fills_in() {
        // Far enough along the return, the cut has left the grid and every
        // bar is whole again.
        let m = text(&rows(SIZE, 60));
        assert!(
            m.iter().all(|r| !r.contains(' ')),
            "the cut is still on the grid: {m:?}"
        );
    }

    #[test]
    fn one_edge_is_hot() {
        let m = rows(SIZE, 8);
        assert!(m.iter().flatten().any(|c| c.hot), "no hot edge");
    }

    #[test]
    fn the_travel_starts_and_ends_at_rest() {
        assert_eq!(base_at(-0.1), RETRACE.rest);
        assert_eq!(base_at(1.0), RETRACE.rest);
        assert_eq!(base_at(0.0), RETRACE.rest);
        // Half way through, the cut is somewhere else entirely.
        assert_ne!(base_at(0.5), RETRACE.rest);
    }
}
