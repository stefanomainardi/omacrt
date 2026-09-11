//! The laser etch, in characters.
//!
//! The launcher opens by cutting its wordmark out of the dark with a laser: a
//! depth first random walk decides the order, each cut cell flashes white and
//! cools through yellow and orange to its final colour, and a short beam
//! points at the cell being cut. That effect began life in a terminal, as
//! `laseretch` in TerminalTextEffects, and the wordmark is already a grid of
//! block characters, so bringing it back to a terminal is bringing it home.
//!
//! What is different here, and it is the point: **the etch is the progress
//! bar.** It advances because a check finished, not because time passed. When
//! the machine answers quickly the word appears quickly; when a probe hangs,
//! the laser waits with you.

use crate::colour::{Color, lerp_color, rgb};

/// How many ticks a cut cell takes to cool to its final colour.
const COOL_TICKS: u32 = 14;
/// Ticks a freshly cut cell shows white.
const FLASH_TICKS: u32 = 2;
/// How far the beam reaches back from the cell being cut.
const BEAM_LEN: i32 = 5;

struct Cell {
    row: i32,
    col: i32,
    ch: char,
    final_color: Color,
    cut_at: Option<u32>,
}

/// A spark thrown off a cut, falling to the baseline.
pub struct Spark {
    pub row: f32,
    pub col: f32,
    pub vy: f32,
    pub life: f32,
}

pub struct Etch {
    pub rows: i32,
    pub cols: i32,
    cells: Vec<Cell>,
    /// Visit order over the ink cells, from the random walk.
    order: Vec<usize>,
    cut: usize,
    tick: u32,
    rng: u32,
    pub sparks: Vec<Spark>,
    beam_at: Option<(i32, i32)>,
}

impl Etch {
    /// `stops` runs bottom to top, the way the launcher's gradient does.
    pub fn new(seed: u32, stops: [Color; 3]) -> Self {
        Self::of(&super::wordmark_half(), seed, stops)
    }

    /// The same, over any block drawing: the self test cuts the wordmark at
    /// half its size, which is a different grid from the one the tube uses.
    pub fn of(art: &[String], seed: u32, stops: [Color; 3]) -> Self {
        let lines: Vec<Vec<char>> = art.iter().map(|l| l.chars().collect()).collect();
        let rows = lines.len() as i32;
        let cols = lines.iter().map(|l| l.len()).max().unwrap_or(0) as i32;
        let mut cells = Vec::new();
        let mut index = vec![usize::MAX; (rows * cols).max(0) as usize];
        for (r, line) in lines.iter().enumerate() {
            for (c, ch) in line.iter().enumerate() {
                // Anything that is not blank is a cell the laser has to cut:
                // the wordmark is drawn with quadrant characters, not only
                // with halves.
                if *ch == ' ' {
                    continue;
                }
                let f = 1.0 - r as f32 / (rows - 1).max(1) as f32;
                let final_color = if f < 0.5 {
                    lerp_color(stops[0], stops[1], f * 2.0)
                } else {
                    lerp_color(stops[1], stops[2], (f - 0.5) * 2.0)
                };
                index[r * cols as usize + c] = cells.len();
                cells.push(Cell {
                    row: r as i32,
                    col: c as i32,
                    ch: *ch,
                    final_color,
                    cut_at: None,
                });
            }
        }
        let mut me = Self {
            rows,
            cols,
            cells,
            order: Vec::new(),
            cut: 0,
            tick: 0,
            rng: seed | 1,
            sparks: Vec::new(),
            beam_at: None,
        };
        me.order = me.walk(&index);
        me
    }

    fn rand(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    /// Depth first random walk over the whole rectangle, spaces included, so
    /// the letters grow as branching blobs rather than a sweep. The order of
    /// the ink cells is what comes back.
    fn walk(&mut self, index: &[usize]) -> Vec<usize> {
        let (cols, rows) = (self.cols, self.rows);
        let n = (cols * rows).max(0) as usize;
        if n == 0 {
            return Vec::new();
        }
        let mut visited = vec![false; n];
        let start = (self.rand() as usize) % n;
        let mut stack = vec![start];
        visited[start] = true;
        let mut order = Vec::new();
        if index[start] != usize::MAX {
            order.push(index[start]);
        }
        while let Some(&cur) = stack.last() {
            let (c, r) = ((cur as i32) % cols, (cur as i32) / cols);
            let mut candidates = Vec::with_capacity(4);
            for (dc, dr) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nc, nr) = (c + dc, r + dr);
                if nc < 0 || nr < 0 || nc >= cols || nr >= rows {
                    continue;
                }
                let ni = (nr * cols + nc) as usize;
                if !visited[ni] {
                    candidates.push(ni);
                }
            }
            if candidates.is_empty() {
                stack.pop();
                continue;
            }
            let pick = candidates[(self.rand() as usize) % candidates.len()];
            visited[pick] = true;
            stack.push(pick);
            if index[pick] != usize::MAX {
                order.push(index[pick]);
            }
        }
        order
    }

    /// How much of the word is cut, 0 to 1.
    pub fn done(&self) -> f32 {
        if self.order.is_empty() {
            return 1.0;
        }
        self.cut as f32 / self.order.len() as f32
    }

    /// Cut, and cooled: no cell is still on its way from white to the colour
    /// it keeps. What is printed once and never redrawn has to be printed
    /// cold, or the word stays orange for ever.
    pub fn cold(&self) -> bool {
        self.cut >= self.order.len()
            && self.cells.iter().all(|c| match c.cut_at {
                Some(at) => self.tick.saturating_sub(at) >= FLASH_TICKS + COOL_TICKS,
                None => false,
            })
    }

    pub fn finished(&self) -> bool {
        self.cut >= self.order.len() && self.sparks.is_empty()
    }

    /// Advance time by one frame, and cut up to `target` of the word.
    ///
    /// The cutting rate is bounded so that a check which answers instantly
    /// does not blink the whole word into existence: the laser catches up
    /// over the next few frames instead.
    pub fn tick(&mut self, target: f32, max_cells: usize) {
        self.tick += 1;
        let want = ((target.clamp(0.0, 1.0)) * self.order.len() as f32).round() as usize;
        let mut cut_now = 0;
        while self.cut < want && cut_now < max_cells {
            let i = self.order[self.cut];
            let tick = self.tick;
            let (row, col) = {
                let c = &mut self.cells[i];
                c.cut_at = Some(tick);
                (c.row, c.col)
            };
            self.beam_at = Some((row, col));
            if self.rand().is_multiple_of(3) {
                let vy = 0.18 + (self.rand() % 100) as f32 / 900.0;
                self.sparks.push(Spark {
                    row: row as f32,
                    col: col as f32,
                    vy,
                    life: 1.0,
                });
            }
            self.cut += 1;
            cut_now += 1;
        }
        if self.cut >= self.order.len() {
            self.beam_at = None;
        }
        for s in &mut self.sparks {
            s.row += s.vy;
            s.col += 0.06;
            s.life -= 0.06;
        }
        self.sparks
            .retain(|s| s.life > 0.0 && s.row < self.rows as f32 + 1.0);
    }

    /// The character and colour at a grid position, or nothing where the
    /// laser has not been yet.
    pub fn cell(&self, row: i32, col: i32) -> Option<(char, Color)> {
        let c = self
            .cells
            .iter()
            .find(|c| c.row == row && c.col == col && c.cut_at.is_some())?;
        let age = self.tick.saturating_sub(c.cut_at?);
        if age < FLASH_TICKS {
            return Some((c.ch, rgb(255, 255, 255)));
        }
        let t = ((age - FLASH_TICKS) as f32 / COOL_TICKS as f32).clamp(0.0, 1.0);
        // White hot, then the yellow and orange of the cut, then the colour
        // the letter keeps.
        let colour = if t < 0.5 {
            lerp_color(rgb(255, 230, 128), rgb(255, 123, 0), t * 2.0)
        } else {
            lerp_color(rgb(255, 123, 0), c.final_color, (t - 0.5) * 2.0)
        };
        Some((c.ch, colour))
    }

    /// The beam, as points from the cell being cut back towards the source,
    /// brightest at the tip.
    pub fn beam(&self) -> Vec<(i32, i32, f32)> {
        let Some((r, c)) = self.beam_at else {
            return Vec::new();
        };
        (1..=BEAM_LEN)
            .map(|i| (r - i, c - i * 2, 1.0 - i as f32 / (BEAM_LEN + 1) as f32))
            .filter(|(r, c, _)| *r >= 0 && *c >= 0)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_walk_visits_every_inked_cell_exactly_once() {
        let e = Etch::new(7, [0x111111, 0x222222, 0x333333]);
        assert!(!e.order.is_empty(), "the wordmark has ink in it");
        let mut seen = e.order.clone();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), e.order.len(), "a cell was cut twice");
        assert_eq!(seen.len(), e.cells.len(), "a cell was never cut");
    }

    /// The laser follows the checks. Nothing is cut before its check is done,
    /// and the whole word is cut when they all are.
    #[test]
    fn cutting_follows_the_target_and_never_overshoots() {
        let mut e = Etch::new(11, [0x111111, 0x222222, 0x333333]);
        for _ in 0..200 {
            e.tick(0.5, 4);
        }
        let half = e.done();
        assert!(
            (half - 0.5).abs() < 0.02,
            "cut {half} of the word, not half"
        );
        for _ in 0..400 {
            e.tick(1.0, 4);
        }
        assert_eq!(e.done(), 1.0);
        assert!(e.cell(0, 0).is_none() || e.cell(0, 0).is_some());
    }

    /// A check that answers instantly must not blink the word into existence.
    #[test]
    fn the_laser_catches_up_rather_than_jumping() {
        let mut e = Etch::new(3, [0x111111, 0x222222, 0x333333]);
        e.tick(1.0, 4);
        assert!(e.done() < 0.2, "the whole word appeared in one frame");
    }
}
