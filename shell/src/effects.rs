//! Screensaver effects on the wordmark grid, in the spirit of
//! TerminalTextEffects (which Omarchy's own screensaver uses on `logo.txt`).
//!
//! Every effect works on the same cell grid decoded from the block-character
//! wordmark. Most effects are expressed as "when does this cell arrive, from
//! where, and in which color", interpolated toward its final gradient color.

use crate::assets::WORDMARK_TXT;
use crate::etch::LaserEtch;
use crate::fb::{Color, Framebuffer, lerp_color, scale};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Full,
    Upper,
    Lower,
}

#[derive(Clone, Copy)]
pub struct Cell {
    pub col: i32,
    pub row: i32,
    pub shape: Shape,
    pub final_color: Color,
}

pub struct Grid {
    pub cols: i32,
    pub rows: i32,
    pub cells: Vec<Cell>,
}

impl Grid {
    /// `stops` runs bottom to top.
    pub fn wordmark(stops: [Color; 3]) -> Self {
        let lines: Vec<Vec<char>> = WORDMARK_TXT.lines().map(|l| l.chars().collect()).collect();
        let rows = lines.len() as i32;
        let cols = lines.iter().map(|l| l.len()).max().unwrap_or(0) as i32;
        let mut cells = Vec::new();
        for (r, line) in lines.iter().enumerate() {
            for (c, ch) in line.iter().enumerate() {
                let shape = match ch {
                    '█' => Shape::Full,
                    '▀' => Shape::Upper,
                    '▄' => Shape::Lower,
                    _ => continue,
                };
                let f = 1.0 - r as f32 / (rows - 1).max(1) as f32;
                let final_color = if f < 0.5 {
                    lerp_color(stops[0], stops[1], f * 2.0)
                } else {
                    lerp_color(stops[1], stops[2], (f - 0.5) * 2.0)
                };
                cells.push(Cell {
                    col: c as i32,
                    row: r as i32,
                    shape,
                    final_color,
                });
            }
        }
        Self { cols, rows, cells }
    }
}

pub fn draw_cell(fb: &mut Framebuffer, x: i32, y: i32, s: i32, cell: &Cell, color: Color) {
    let ch = 2 * s;
    let cx = x + cell.col * s;
    let cy = y + cell.row * ch;
    match cell.shape {
        Shape::Full => fb.rect(cx, cy, s, ch, color),
        Shape::Upper => fb.rect(cx, cy, s, s, color),
        Shape::Lower => fb.rect(cx, cy + s, s, s, color),
    }
}

fn draw_cell_at(
    fb: &mut Framebuffer,
    x: i32,
    y: i32,
    s: i32,
    cell: &Cell,
    px: f32,
    py: f32,
    color: Color,
) {
    let moved = Cell {
        col: 0,
        row: 0,
        ..*cell
    };
    draw_cell(
        fb,
        x + (px * s as f32).round() as i32,
        y + (py * 2.0 * s as f32).round() as i32,
        s,
        &moved,
        color,
    );
}

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn unit(&mut self) -> f32 {
        (self.next() >> 8) as f32 / (1u32 << 24) as f32
    }
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t) * (1.0 - t)
}
fn ease_in_out(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// One planned motion per cell: appear at `start`, travel `dur` seconds from
/// (`fx`, `fy`) to its place, blending `from` into the final color.
#[derive(Clone, Copy)]
pub struct Plan {
    start: f32,
    dur: f32,
    fx: f32,
    fy: f32,
    from: Color,
    /// Flicker seed for effects that scramble colors before settling.
    seed: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    LaserEtch,
    Rain,
    Beams,
    Burn,
    Slide,
    Decrypt,
    Expand,
    Unstable,
}

pub const ALL: [Kind; 8] = [
    Kind::LaserEtch,
    Kind::Rain,
    Kind::Beams,
    Kind::Burn,
    Kind::Slide,
    Kind::Decrypt,
    Kind::Expand,
    Kind::Unstable,
];

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::LaserEtch => "laseretch",
            Kind::Rain => "rain",
            Kind::Beams => "beams",
            Kind::Burn => "burn",
            Kind::Slide => "slide",
            Kind::Decrypt => "decrypt",
            Kind::Expand => "expand",
            Kind::Unstable => "unstable",
        }
    }
}

pub struct Palette {
    pub green: Color,
    pub cyan: Color,
    pub orange: Color,
    pub yellow: Color,
    pub red: Color,
    pub dim: Color,
}

pub enum Effect {
    Etch(LaserEtch),
    Planned {
        kind: Kind,
        grid: Grid,
        plans: Vec<Plan>,
        length: f32,
        palette: Palette,
    },
}

impl Effect {
    pub fn new(kind: Kind, seed: u32, stops: [Color; 3], palette: Palette) -> Self {
        if kind == Kind::LaserEtch {
            return Effect::Etch(LaserEtch::new(seed, stops));
        }
        let grid = Grid::wordmark(stops);
        let mut rng = Rng(seed | 1);
        let (cols, rows) = (grid.cols as f32, grid.rows as f32);
        let mut plans = Vec::with_capacity(grid.cells.len());
        for cell in &grid.cells {
            let (c, r) = (cell.col as f32, cell.row as f32);
            let u = rng.unit();
            let plan = match kind {
                Kind::Rain => Plan {
                    start: u * 2.2,
                    dur: 0.7 + rng.unit() * 0.5,
                    fx: c,
                    fy: r - rows - 2.0 - rng.unit() * 6.0,
                    from: palette.cyan,
                    seed: rng.next(),
                },
                Kind::Beams => {
                    // Rows lit by a horizontal beam, in a shuffled row order.
                    let row_order = ((cell.row as u32).wrapping_mul(7) % grid.rows as u32) as f32;
                    Plan {
                        start: row_order * 0.22 + c / cols * 0.5,
                        dur: 0.35,
                        fx: c,
                        fy: r,
                        from: 0xffffff,
                        seed: rng.next(),
                    }
                }
                Kind::Burn => Plan {
                    start: (rows - r) / rows * 2.4 + u * 0.35,
                    dur: 0.6,
                    fx: c,
                    fy: r,
                    from: palette.yellow,
                    seed: rng.next(),
                },
                Kind::Slide => {
                    let from_left = cell.row % 2 == 0;
                    Plan {
                        start: r / rows * 0.9,
                        dur: 1.1,
                        fx: if from_left {
                            c - cols - 4.0
                        } else {
                            c + cols + 4.0
                        },
                        fy: r,
                        from: cell.final_color,
                        seed: rng.next(),
                    }
                }
                Kind::Decrypt => Plan {
                    start: 0.0,
                    dur: 1.0 + u * 2.5,
                    fx: c,
                    fy: r,
                    from: palette.green,
                    seed: rng.next(),
                },
                Kind::Expand => Plan {
                    start: u * 0.3,
                    dur: 1.4,
                    fx: cols / 2.0,
                    fy: rows / 2.0,
                    from: 0xffffff,
                    seed: rng.next(),
                },
                Kind::Unstable => Plan {
                    start: 0.4 + u * 0.3,
                    dur: 1.6,
                    fx: c + (rng.unit() - 0.5) * 40.0,
                    fy: r + (rng.unit() - 0.5) * 16.0,
                    from: palette.red,
                    seed: rng.next(),
                },
                Kind::LaserEtch => unreachable!(),
            };
            plans.push(plan);
        }
        let length = plans.iter().map(|p| p.start + p.dur).fold(0.0, f32::max);
        Effect::Planned {
            kind,
            grid,
            plans,
            length,
            palette,
        }
    }

    pub fn advance_to(&mut self, t: f32) {
        if let Effect::Etch(e) = self {
            e.advance_to(t);
        }
    }

    /// Seconds after which the picture is complete.
    pub fn length(&self) -> f32 {
        match self {
            Effect::Etch(e) => e.total_cells() as f32 / 60.0 + 0.6,
            Effect::Planned { length, .. } => *length,
        }
    }

    pub fn draw(&self, fb: &mut Framebuffer, x: i32, y: i32, s: i32, t: f32, fade: f32) {
        match self {
            Effect::Etch(e) => e.draw(fb, x, y, s, fade),
            Effect::Planned {
                kind,
                grid,
                plans,
                palette,
                ..
            } => {
                if *kind == Kind::Burn {
                    // Fire front: a jagged line climbing the text, cells glow before they settle.
                    let front_row = grid.rows as f32 - t / 2.4 * grid.rows as f32;
                    for c in 0..grid.cols {
                        let jitter = ((c as f32 * 1.7 + t * 9.0).sin() * 0.8) as i32;
                        let fy = y + ((front_row as i32 + jitter) * 2 * s);
                        if fy > y - 2 * s && fy < y + grid.rows * 2 * s + 2 * s {
                            fb.rect(x + c * s, fy, s, s, scale(palette.red, 0.7 * fade));
                            fb.rect(x + c * s, fy - s, s, s, scale(palette.orange, 0.5 * fade));
                        }
                    }
                }
                for (cell, plan) in grid.cells.iter().zip(plans) {
                    let local = t - plan.start;
                    if local < 0.0 {
                        continue;
                    }
                    let p = (local / plan.dur).min(1.0);
                    match kind {
                        Kind::Decrypt => {
                            if p < 1.0 {
                                let flick = (plan
                                    .seed
                                    .wrapping_add((t * 30.0) as u32)
                                    .wrapping_mul(2654435761))
                                    >> 28;
                                let c = match flick % 4 {
                                    0 => palette.green,
                                    1 => palette.dim,
                                    2 => scale(palette.green, 0.5),
                                    _ => cell.final_color,
                                };
                                if flick % 5 != 0 {
                                    draw_cell(fb, x, y, s, cell, scale(c, fade));
                                }
                            } else {
                                draw_cell(fb, x, y, s, cell, scale(cell.final_color, fade));
                            }
                        }
                        Kind::Beams => {
                            let color = lerp_color(plan.from, cell.final_color, ease_out(p));
                            draw_cell(fb, x, y, s, cell, scale(color, fade));
                        }
                        Kind::Burn => {
                            let color = if p < 0.5 {
                                lerp_color(palette.red, plan.from, p * 2.0)
                            } else {
                                lerp_color(plan.from, cell.final_color, (p - 0.5) * 2.0)
                            };
                            draw_cell(fb, x, y, s, cell, scale(color, fade));
                        }
                        _ => {
                            let e = if *kind == Kind::Unstable {
                                ease_in_out(p)
                            } else {
                                ease_out(p)
                            };
                            let px = plan.fx + (cell.col as f32 - plan.fx) * e;
                            let py = plan.fy + (cell.row as f32 - plan.fy) * e;
                            let color = lerp_color(plan.from, cell.final_color, e);
                            draw_cell_at(fb, x, y, s, cell, px, py, scale(color, fade));
                        }
                    }
                }
            }
        }
    }
}
