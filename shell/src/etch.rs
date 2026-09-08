//! Laser etch, a pixel port of the `laseretch` effect from TerminalTextEffects
//! (the same effect crt.omarchy.org runs through its WASM build).
//!
//! The wordmark is a grid of block characters. A depth-first random walk
//! (recursive backtracker) over that grid decides the etch order, so letters
//! grow as branching blobs instead of a sweep. Each etched cell flashes a
//! caret, cools from yellow through orange to its final color, which is a
//! vertical gradient across the text. A short diagonal beam with moving
//! stripes points at the cell being cut, and one spark per cell flies along
//! a Bezier arc to the baseline where it glows and dies.

use crate::assets::WORDMARK_TXT;
use crate::fb::{Color, Framebuffer, lerp_color, rgb};

/// Effect ticks per second (TTE runs at 120 on the site).
pub const TICK_HZ: f32 = 120.0;
const ETCH_SPEED: usize = 2; // cells per etch step (twice the site: a boot screen, not a web page)
const ETCH_DELAY: u32 = 1; // ticks between etch steps
const SPAWN_TICKS: u32 = 3;
const COOL_STEP_TICKS: u32 = 3;
const SPARK_STEP_TICKS: u32 = 7;
const BEAM_STEP_TICKS: u32 = 3;
const SPARK_SPEED: f32 = 0.3; // cells per tick along the path

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Full,
    Upper,
    Lower,
}

struct Cell {
    col: i32,
    row: i32,
    shape: Shape,
    /// Tick at which the cell was etched, `None` while pending.
    etched_at: Option<u32>,
    final_color: Color,
}

struct Spark {
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    born: u32,
    travel_ticks: f32,
    glyph: u8,
}

pub struct LaserEtch {
    pub cols: i32,
    pub rows: i32,
    cells: Vec<Cell>,
    order: Vec<usize>,
    next: usize,
    delay: u32,
    tick: u32,
    started: bool,
    cool: Vec<Color>,
    spark_colors: Vec<Color>,
    beam_colors: Vec<Color>,
    sparks: Vec<Spark>,
    beam_at: Option<(i32, i32)>,
    rng: u32,
}

/// A ramp through `stops`, `steps[i]` colours between each pair and the last
/// stop on the end. The callers pass constants, so an empty list is not a
/// case that happens; it is answered rather than assumed.
fn gradient(stops: &[Color], steps: &[usize]) -> Vec<Color> {
    let Some(last) = stops.last() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, pair) in stops.windows(2).enumerate() {
        let n = steps.get(i).or(steps.last()).copied().unwrap_or(1).max(1);
        for k in 0..n {
            out.push(lerp_color(pair[0], pair[1], k as f32 / n as f32));
        }
    }
    out.push(*last);
    out
}

fn ease_out_sine(t: f32) -> f32 {
    (t * std::f32::consts::FRAC_PI_2).sin()
}

impl LaserEtch {
    /// `final_stops` runs bottom to top, like TTE's vertical gradient.
    pub fn new(seed: u32, final_stops: [Color; 3]) -> Self {
        let lines: Vec<Vec<char>> = WORDMARK_TXT.lines().map(|l| l.chars().collect()).collect();
        let rows = lines.len() as i32;
        let cols = lines.iter().map(|l| l.len()).max().unwrap_or(0) as i32;
        let mut cells = Vec::new();
        let mut index = vec![usize::MAX; (rows * cols) as usize];
        for (r, line) in lines.iter().enumerate() {
            for (c, ch) in line.iter().enumerate() {
                let shape = match ch {
                    '█' => Shape::Full,
                    '▀' => Shape::Upper,
                    '▄' => Shape::Lower,
                    _ => continue,
                };
                // Final color: vertical gradient, bottom row = first stop.
                let f = 1.0 - r as f32 / (rows - 1).max(1) as f32;
                let final_color = if f < 0.5 {
                    lerp_color(final_stops[0], final_stops[1], f * 2.0)
                } else {
                    lerp_color(final_stops[1], final_stops[2], (f - 0.5) * 2.0)
                };
                index[r * cols as usize + c] = cells.len();
                cells.push(Cell {
                    col: c as i32,
                    row: r as i32,
                    shape,
                    etched_at: None,
                    final_color,
                });
            }
        }
        let mut me = Self {
            cols,
            rows,
            cells,
            order: Vec::new(),
            next: 0,
            delay: 0,
            tick: 0,
            started: false,
            cool: Vec::new(),
            spark_colors: gradient(&[0xffffff, 0xffe680, 0xff7b00, 0x1a0900], &[3, 8, 8]),
            beam_colors: {
                let mut g = gradient(&[0xffffff, 0x376cff], &[6]);
                let back: Vec<Color> = g.iter().rev().skip(1).copied().collect();
                g.extend(back);
                g.pop();
                g
            },
            sparks: Vec::new(),
            beam_at: None,
            rng: seed | 1,
        };
        me.cool = gradient(&[0xffe680, 0xff7b00], &[8]);
        me.order = me.backtracker_order(&index);
        me
    }

    fn rand(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    fn rand_range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.rand() % ((hi - lo + 1) as u32)) as i32
    }

    /// Depth-first random walk over the whole text rectangle (spaces included,
    /// exactly like TTE), returning the visit order of the non-space cells.
    fn backtracker_order(&mut self, index: &[usize]) -> Vec<usize> {
        let (cols, rows) = (self.cols, self.rows);
        let n = (cols * rows) as usize;
        let mut visited = vec![false; n];
        let start = self.rand_range(0, n as i32 - 1) as usize;
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
            let pick = candidates[self.rand_range(0, candidates.len() as i32 - 1) as usize];
            visited[pick] = true;
            stack.push(pick);
            if index[pick] != usize::MAX {
                order.push(index[pick]);
            }
        }
        order
    }

    /// A quiet soundtrack for the etch, generated from this run's order so it
    /// follows the picture: a soft high hiss while the laser works, one tiny
    /// crackle per cut cell with a pitch that rises toward the top rows, and a
    /// little more sizzle when the beam jumps far (a new branch of the walk).
    pub fn synth(&self, rate: u32) -> Vec<f32> {
        let step = (ETCH_DELAY + 1) as f32 / TICK_HZ;
        let total = (self.order.len() / ETCH_SPEED) as f32 * step + 0.4;
        let n = (total * rate as f32) as usize;
        let mut out = vec![0.0f32; n];
        let sr = rate as f32;
        let tau = std::f32::consts::TAU;
        let mut noise = 0x1234_5679u32 ^ self.rng;
        let mut rnd = || {
            noise ^= noise << 13;
            noise ^= noise >> 17;
            noise ^= noise << 5;
            (noise >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
        };
        // Hiss: band-passed noise, fading in and out with the work.
        let (mut lo, mut hi) = (0.0f32, 0.0f32);
        let a_hi = 1.0 / (1.0 + sr / (tau * 6000.0));
        let a_lo = 1.0 / (1.0 + sr / (tau * 2500.0));
        let work = (self.order.len() / ETCH_SPEED) as f32 * step;
        for (i, s) in out.iter_mut().enumerate() {
            let t = i as f32 / sr;
            let w = rnd();
            hi += a_hi * (w - hi);
            lo += a_lo * (w - lo);
            let env = (t / 0.3).min(1.0) * (1.0 - ((t - work) / 0.35).clamp(0.0, 1.0));
            *s = (hi - lo) * 0.05 * env;
        }
        // Crackles: one per cell.
        let mut prev: Option<(i32, i32)> = None;
        for (k, &idx) in self.order.iter().enumerate() {
            let cell = &self.cells[idx];
            let at = (k / ETCH_SPEED) as f32 * step;
            let start = (at * sr) as usize;
            let jump = prev
                .map(|(c, r)| ((cell.col - c).abs() + (cell.row - r).abs()) > 2)
                .unwrap_or(false);
            prev = Some((cell.col, cell.row));
            let f = 1800.0 + (self.rows - 1 - cell.row) as f32 / (self.rows - 1) as f32 * 2200.0;
            let len = if jump { 0.03 } else { 0.012 };
            let gain = if jump { 0.06 } else { 0.035 };
            for j in 0..(len * sr) as usize {
                let d = j as f32 / sr;
                let v = (tau * f * d).sin() * (-d * (6.0 / len)).exp() * gain
                    + rnd() * (-d * (8.0 / len)).exp() * gain * 0.5;
                if let Some(o) = out.get_mut(start + j) {
                    *o += v;
                }
            }
        }
        out
    }

    pub fn total_cells(&self) -> usize {
        self.cells.len()
    }

    /// Advance the simulation to absolute effect time `t` seconds (t >= 0 starts it).
    pub fn advance_to(&mut self, t: f32) {
        if t < 0.0 {
            return;
        }
        self.started = true;
        let target = (t * TICK_HZ) as u32;
        while self.tick < target {
            self.step();
            self.tick += 1;
        }
    }

    fn step(&mut self) {
        if self.next < self.order.len() {
            if self.delay == 0 {
                for _ in 0..ETCH_SPEED {
                    if self.next >= self.order.len() {
                        break;
                    }
                    let idx = self.order[self.next];
                    self.next += 1;
                    self.cells[idx].etched_at = Some(self.tick);
                    let (c, r) = (self.cells[idx].col, self.cells[idx].row);
                    self.beam_at = Some((c, r));
                    self.emit_spark(c, r);
                }
                self.delay = ETCH_DELAY;
            } else {
                self.delay -= 1;
            }
        } else {
            self.beam_at = None;
        }
        let spark_life = self.spark_colors.len() as u32 * SPARK_STEP_TICKS;
        let tick = self.tick;
        self.sparks.retain(|s| tick - s.born < spark_life);
    }

    fn emit_spark(&mut self, col: i32, row: i32) {
        let target_col = self.rand_range(col - 20, col + 20);
        let target_row = self.rows - 1;
        // TTE rows grow upward; a positive offset there means "above" here.
        let ctrl_row = row - self.rand_range(-10, 20);
        let p0 = (col as f32, row as f32);
        let p1 = (target_col as f32, ctrl_row as f32);
        let p2 = (target_col as f32, target_row as f32);
        // Approximate path length with a few chords.
        let mut len = 0.0;
        let mut prev = p0;
        for i in 1..=8 {
            let q = bezier(p0, p1, p2, i as f32 / 8.0);
            len += ((q.0 - prev.0).powi(2) + (q.1 - prev.1).powi(2)).sqrt();
            prev = q;
        }
        let glyph = (self.rand() % 3) as u8;
        self.sparks.push(Spark {
            p0,
            p1,
            p2,
            born: self.tick,
            travel_ticks: (len / SPARK_SPEED).max(1.0),
            glyph,
        });
    }

    /// Draw with the text's top-left corner at (x, y); each cell is `s` px wide and `2s` tall.
    pub fn draw(&self, fb: &mut Framebuffer, x: i32, y: i32, s: i32, fade: f32) {
        if !self.started {
            return;
        }
        let ch = 2 * s;
        for cell in &self.cells {
            let Some(at) = cell.etched_at else { continue };
            let age = self.tick - at;
            let cx = x + cell.col * s;
            let cy = y + cell.row * ch;
            if age < SPAWN_TICKS {
                // Caret flash: a small "^" in bright yellow.
                let c = crate::fb::scale(0xffe680, fade);
                fb.put(cx + s / 2, cy + 1, c);
                fb.put(cx, cy + 2, c);
                fb.put(cx + s - 1, cy + 2, c);
                continue;
            }
            let step = ((age - SPAWN_TICKS) / COOL_STEP_TICKS) as usize;
            let color = if step < self.cool.len() {
                self.cool[step]
            } else {
                let k = step - self.cool.len();
                // Second half of the cool-down: orange to the cell's final color, 8 steps.
                if k < 8 {
                    lerp_color(0xff7b00, cell.final_color, k as f32 / 8.0)
                } else {
                    cell.final_color
                }
            };
            let color = crate::fb::scale(color, fade);
            match cell.shape {
                Shape::Full => fb.rect(cx, cy, s, ch, color),
                Shape::Upper => fb.rect(cx, cy, s, s, color),
                Shape::Lower => fb.rect(cx, cy + s, s, s, color),
            }
        }

        // Sparks.
        for sp in &self.sparks {
            let age = (self.tick - sp.born) as f32;
            let t = ease_out_sine((age / sp.travel_ticks).min(1.0));
            let (px, py) = bezier(sp.p0, sp.p1, sp.p2, t);
            let step = ((self.tick - sp.born) / SPARK_STEP_TICKS) as usize;
            let color = crate::fb::scale(
                self.spark_colors[step.min(self.spark_colors.len() - 1)],
                fade,
            );
            let sx = x + (px * s as f32) as i32 + s / 2;
            let sy = y + (py * ch as f32) as i32 + ch / 2;
            match sp.glyph {
                0 => fb.put(sx, sy, color),
                1 => {
                    fb.put(sx, sy, color);
                    fb.put(sx, sy + 1, color);
                }
                _ => {
                    fb.put(sx, sy, color);
                    fb.put(sx - 1, sy, color);
                    fb.put(sx + 1, sy, color);
                    fb.put(sx, sy - 1, color);
                    fb.put(sx, sy + 1, color);
                }
            }
        }

        // Beam: diagonal from the cut cell up and to the right, stripes crawling along it.
        if let Some((bc, br)) = self.beam_at {
            let n = self.beam_colors.len() as u32;
            let phase = self.tick / BEAM_STEP_TICKS;
            let mut i = 0u32;
            let (mut c, mut r) = (bc, br);
            while r >= -8 {
                let color = crate::fb::scale(self.beam_colors[((phase + i) % n) as usize], fade);
                let cx = x + c * s;
                let cy = y + r * ch;
                if i == 0 {
                    // Hot spot on the cell.
                    fb.rect(cx, cy + s / 2, s, s, crate::fb::add(color, rgb(60, 60, 60)));
                } else {
                    // A "/" inside the cell: bottom-left to top-right.
                    fb.line(cx, cy + ch - 1, cx + s - 1, cy, color);
                }
                i += 1;
                c += 1;
                r -= 1;
            }
        }
    }
}

fn bezier(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), t: f32) -> (f32, f32) {
    let u = 1.0 - t;
    (
        u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0,
        u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1,
    )
}
