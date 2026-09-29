//! The arcade hall the home stands in: an aisle between two rows of
//! cabinets, a neon sign with the wordmark on the far wall, a cosmic carpet.
//!
//! The camera does not move. The room is a set of flat patches, each a
//! rectangle on a plane of constant X, Y or depth, drawn by casting the pixel
//! back onto its plane and keeping the nearest; that gives the right
//! occlusion without a polygon rasteriser. What never changes is kept as a
//! picture. What moves (the cabinets' screens and marquees) is remembered per
//! pixel as a place on its surface and shaded again every frame.
//!
//! The room takes its colours from the theme: its darks from the background,
//! the neon on the walls from the cyan, the sign from the wordmark. The
//! cabinets keep colours of their own, the way the consoles on the stage do.
//!
//! Nothing is smooth. Every blend goes in quarters through the ordered
//! dither, and the pictures on the screens step at twelve and a half frames a
//! second, which is about what the games in them ran their attract modes at.

use crate::fb::{Color, Framebuffer, lerp_color, scale};
use omacrt_shell::theme::Theme;

/// Frames of the loop every screen and marquee comes back to.
pub const LOOP: u32 = 48;
/// The loop's frame rate.
pub const TICK_HZ: f64 = 12.5;

pub const FLOOR: f32 = 1.0;
const CEIL: f32 = -0.78;
const WALL: f32 = 1.10;
pub const DBACK: f32 = 3.35;
// The cabinet, top to bottom; Y grows downwards and the eye is at 0.
const TOP: f32 = -0.40;
const MARQ: f32 = -0.25;
const SCR0: f32 = -0.17;
const SCR1: f32 = 0.16;
const PNL: f32 = 0.22;
const PNLB: f32 = 0.31;
const KICK: f32 = 0.86;
/// The aisle face of a row, as |X|.
const FRONT: f32 = 0.72;
const RECESS: f32 = 0.10;
const LEDGE: f32 = 0.10;
const DEEP: f32 = WALL - FRONT;
const SLOTS: [f32; 4] = [1.52, 1.88, 2.24, 2.60];
const CW: f32 = 0.34;
/// The wordmark's column where CRT starts.
const SPLIT: usize = 38;

/// How lit a cabinet is: off, the white flash of a tube striking, or a
/// picture coming up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cab {
    Off,
    Flash,
    On(f32),
}

impl Cab {
    fn level(self) -> f32 {
        match self {
            Cab::Off | Cab::Flash => 0.0,
            Cab::On(v) => v,
        }
    }
}

/// Everything that can be on or off in the hall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lights {
    /// The room's own light, 0 dark to 1 lit.
    pub amb: f32,
    /// Left row near to far, right row near to far, the two at the back.
    pub cabs: [Cab; 10],
    /// The sign's two halves, OMA and CRT: 0 off, 0.5 striking, 1 on.
    pub sign: (f32, f32),
    /// How far down the aisle the wall tubes are lit, in depth.
    pub strip: f32,
    /// The star and the arrow beside the sign.
    pub small: bool,
    /// The sign's C giving out now and then, once the hall is up.
    pub flicker: bool,
}

impl Lights {
    #[cfg(test)]
    pub fn all_on() -> Self {
        Self {
            amb: 1.0,
            cabs: [Cab::On(1.0); 10],
            sign: (1.0, 1.0),
            strip: 99.0,
            small: true,
            flicker: true,
        }
    }

    fn static_key(&self) -> [u32; 13] {
        let mut k = [0u32; 13];
        k[0] = (self.amb * 64.0) as u32;
        for (i, c) in self.cabs.iter().enumerate() {
            k[1 + i] = match c {
                Cab::Off => 0,
                Cab::Flash => 1,
                Cab::On(v) => 2 + (v * 32.0) as u32,
            };
        }
        k[11] = (self.strip.min(9.0) * 64.0) as u32;
        k[12] = self.sign.0 as u32 * 2 + self.sign.1 as u32;
        k
    }
}

/// What a cabinet's screen is showing besides its game: the red of a game
/// lost, the colours of a record.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Fx {
    #[default]
    None,
    Over,
    Record,
}

#[derive(Clone, Copy)]
enum Game {
    Invaders,
    Maze,
    Racer,
    Bars,
}

#[derive(Clone, Copy)]
enum Art {
    Bolt,
    Stars,
    Stripes,
}

#[derive(Clone, Copy)]
struct Scheme {
    body: Color,
    art: Color,
    art2: Color,
    marq: Color,
    mold: Color,
    glow: Color,
    game: Game,
    style: Art,
}

// The cabinets' own colours, a fixed set as an arcade's were.
const VOID0: Color = 0x07040d;
const WHITE: Color = 0xf4f0ff;
const RED: Color = 0xe83848;
const REDLO: Color = 0x7a1c2a;
const AMBER: Color = 0xffb040;
const AMBERHI: Color = 0xffe8a0;
const AMBERLO: Color = 0x9a5418;
const YELLOW: Color = 0xffe04a;
const PINK: Color = 0xff3fa4;
const PINKLO: Color = 0x8a1f5e;
const ROSE: Color = 0xc22c7c;
const CYAN: Color = 0x38e8ff;
const CYANLO: Color = 0x1a6488;
const LIME: Color = 0x70ff60;
const LIMELO: Color = 0x2a7a30;
const BLUE: Color = 0x3a64ff;
const BLUELO: Color = 0x1a2a78;
const STEEL: Color = 0x3a3448;
const STEELHI: Color = 0x6a6080;
const INK0: Color = 0x0d0818;
const GRAPE0: Color = 0x2e1a4c;

const SCHEMES: [Scheme; 6] = [
    Scheme {
        body: INK0,
        art: RED,
        art2: AMBER,
        marq: AMBER,
        mold: RED,
        glow: CYAN,
        game: Game::Invaders,
        style: Art::Bolt,
    },
    Scheme {
        body: BLUELO,
        art: CYAN,
        art2: WHITE,
        marq: PINK,
        mold: CYAN,
        glow: LIME,
        game: Game::Maze,
        style: Art::Stars,
    },
    Scheme {
        body: INK0,
        art: YELLOW,
        art2: AMBER,
        marq: YELLOW,
        mold: AMBER,
        glow: BLUE,
        game: Game::Racer,
        style: Art::Stripes,
    },
    Scheme {
        body: REDLO,
        art: AMBER,
        art2: YELLOW,
        marq: CYAN,
        mold: PINK,
        glow: PINK,
        game: Game::Bars,
        style: Art::Stripes,
    },
    Scheme {
        body: INK0,
        art: LIME,
        art2: CYAN,
        marq: LIME,
        mold: LIME,
        glow: CYAN,
        game: Game::Invaders,
        style: Art::Stars,
    },
    Scheme {
        body: GRAPE0,
        art: PINK,
        art2: AMBER,
        marq: AMBER,
        mold: PINK,
        glow: LIME,
        game: Game::Maze,
        style: Art::Bolt,
    },
];

/// Cabinet `i` of the ten: its scheme, its side (-1 left, 1 right, 0 back).
fn cabinet(i: usize) -> (Scheme, i32) {
    match i {
        0..=3 => (SCHEMES[(i * 2) % 6], -1),
        4..=7 => (SCHEMES[((i - 4) * 2 + 3) % 6], 1),
        _ => (SCHEMES[((i - 8) * 3 + 1) % 6], 0),
    }
}

/// The room's own colours, from the theme. A light theme still gets a dark
/// hall: an arcade with the lights up is a warehouse.
#[derive(Clone, Copy)]
struct Room {
    void: Color,
    ink: Color,
    night: Color,
    plum: Color,
    mauve: Color,
    fog: Color,
    tube: Color,
    tube_hi: Color,
    tube_lo: Color,
}

fn luma(c: Color) -> f32 {
    let (r, g, b) = ((c >> 16) & 255, (c >> 8) & 255, c & 255);
    (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.0
}

impl Room {
    fn of(th: &Theme) -> Self {
        let ink = if luma(th.bg) > 0.45 {
            lerp_color(th.fg, 0x000000, 0.82)
        } else {
            th.bg
        };
        let tint = th.magenta;
        Self {
            void: lerp_color(ink, 0x000000, 0.45),
            ink,
            night: lerp_color(ink, tint, 0.10),
            plum: lerp_color(ink, tint, 0.18),
            mauve: lerp_color(ink, tint, 0.42),
            fog: lerp_color(ink, tint, 0.14),
            tube: th.cyan,
            tube_hi: lerp_color(th.cyan, 0xffffff, 0.6),
            tube_lo: lerp_color(ink, th.cyan, 0.35),
        }
    }
}

/// The sign's colours for one half: its tube, the white-hot middle of a
/// stroke, the dim of a tube striking, the halo on the board.
#[derive(Clone, Copy)]
pub struct Tube {
    pub fill: Color,
    pub hot: Color,
    pub dim: Color,
    pub halo: Color,
}

impl Tube {
    pub fn of(c: Color, bg: Color) -> Self {
        Self {
            fill: c,
            hot: lerp_color(c, 0xffffff, 0.7),
            dim: lerp_color(bg, c, 0.55),
            halo: lerp_color(bg, c, 0.45),
        }
    }
}

#[inline]
fn ordered(x: i32, y: i32) -> f32 {
    (crate::paint::BAYER[(y & 3) as usize][(x & 3) as usize] as f32 + 0.5) / 16.0
}

/// A blend in quarters, dithered: a fixed palette cannot fade.
#[inline]
fn steps(a: Color, b: Color, t: f32, x: i32, y: i32) -> Color {
    if t <= 0.0 {
        return a;
    }
    let q = ((t * 4.0 + ordered(x, y)).floor() / 4.0).clamp(0.0, 1.0);
    lerp_color(a, b, q)
}

#[inline]
fn h2(a: i32, b: i32) -> f32 {
    let n = (a as u32)
        .wrapping_mul(374_761_393)
        .wrapping_add((b as u32).wrapping_mul(668_265_263));
    let n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    (n ^ (n >> 16)) as f32 / 4_294_967_296.0
}

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The camera: where the aisle's vanishing point is, and how wide a unit of
/// the room is at depth one. A PAL picture is taller, not wider.
#[derive(Clone, Copy)]
struct Cam {
    vx: f32,
    vy: f32,
    kx: f32,
    ky: f32,
    w: i32,
    h: i32,
}

impl Cam {
    fn of(w: usize, h: usize) -> Self {
        let k = h as f32 / 240.0;
        Self {
            vx: w as f32 / 2.0,
            // The foot of the back wall sits on the boot's horizon.
            vy: 68.0 * k,
            kx: 150.0,
            ky: 150.0 * k,
            w: w as i32,
            h: h as i32,
        }
    }

    fn proj(&self, x: f32, y: f32, d: f32) -> (f32, f32) {
        (self.vx + x * self.kx / d, self.vy + y * self.ky / d)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Plane {
    /// X constant; a runs in depth, b in Y.
    X,
    /// Y constant; a runs in X, b in depth.
    Y,
    /// Depth constant; a runs in X, b in Y.
    D,
}

#[derive(Clone, Copy)]
enum Dyn {
    Screen,
    Marquee,
}

#[derive(Clone, Copy)]
struct DynPx {
    at: u32,
    kind: Dyn,
    cab: u8,
    u: f32,
    v: f32,
    d: f32,
}

/// A moving surface's pixel: what it is, whose it is, where on it, how far.
type Mark = (Dyn, u8, f32, f32, f32);

struct Raster {
    cam: Cam,
    col: Vec<Color>,
    z: Vec<f32>,
    dynamic: Vec<Option<Mark>>,
}

impl Raster {
    fn new(cam: Cam, void: Color) -> Self {
        let n = (cam.w * cam.h) as usize;
        Self {
            cam,
            col: vec![void; n],
            z: vec![f32::MAX; n],
            dynamic: vec![None; n],
        }
    }

    /// Draw a rectangle on a plane: the shader gets the place on the plane
    /// (a, b), the depth and the pixel, and answers a colour or a hole.
    #[allow(clippy::too_many_arguments)]
    fn patch(
        &mut self,
        plane: Plane,
        c: f32,
        (a0, a1): (f32, f32),
        (b0, b1): (f32, f32),
        dynamic: Option<(Dyn, u8)>,
        mut shade: impl FnMut(f32, f32, f32, i32, i32) -> Option<Color>,
    ) {
        let cam = self.cam;
        let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for a in [a0, a1] {
            for b in [b0, b1] {
                let (px, py) = match plane {
                    Plane::X => cam.proj(c, b, a),
                    Plane::Y => cam.proj(a, c, b),
                    Plane::D => cam.proj(a, b, c),
                };
                x0 = x0.min(px);
                x1 = x1.max(px);
                y0 = y0.min(py);
                y1 = y1.max(py);
            }
        }
        let (alo, ahi) = (a0.min(a1), a0.max(a1));
        let (blo, bhi) = (b0.min(b1), b0.max(b1));
        let ys = (y0 as i32 - 1).max(0)..(y1 as i32 + 2).min(cam.h);
        for y in ys {
            let dy = y as f32 + 0.5 - cam.vy;
            for x in (x0 as i32 - 1).max(0)..(x1 as i32 + 2).min(cam.w) {
                let dx = x as f32 + 0.5 - cam.vx;
                let (d, a, b) = match plane {
                    Plane::X => {
                        if dx.abs() < 1e-4 || dx * c <= 0.0 {
                            continue;
                        }
                        let d = cam.kx * c / dx;
                        (d, d, dy * d / cam.ky)
                    }
                    Plane::Y => {
                        if dy.abs() < 1e-4 || dy * c <= 0.0 {
                            continue;
                        }
                        let d = cam.ky * c / dy;
                        (d, dx * d / cam.kx, d)
                    }
                    Plane::D => (c, dx * c / cam.kx, dy * c / cam.ky),
                };
                if a < alo || a > ahi || b < blo || b > bhi {
                    continue;
                }
                let i = (y * cam.w + x) as usize;
                if d >= self.z[i] {
                    continue;
                }
                match dynamic {
                    Some((kind, cab)) => {
                        // A moving surface keeps where on itself the pixel
                        // is; the shader here turns (a, b) into (u, v).
                        let Some(uv) = shade(a, b, d, x, y) else {
                            continue;
                        };
                        let (u, v) = (
                            ((uv >> 16) & 0xffff) as f32 / 65535.0,
                            (uv & 0xffff) as f32 / 65535.0,
                        );
                        self.z[i] = d;
                        self.dynamic[i] = Some((kind, cab, u, v, d));
                    }
                    None => {
                        let Some(col) = shade(a, b, d, x, y) else {
                            continue;
                        };
                        self.z[i] = d;
                        self.col[i] = col;
                        self.dynamic[i] = None;
                    }
                }
            }
        }
    }
}

/// Pack a place on a surface (0..1 each way) the way `patch` reads it back.
fn uv(u: f32, v: f32) -> Option<Color> {
    let q = |t: f32| (t.clamp(0.0, 1.0) * 65535.0) as u32;
    Some((q(u) << 16) | q(v))
}

struct Builder<'a> {
    room: Room,
    lights: &'a Lights,
    sign: Color,
    fx: [Fx; 10],
}

impl Builder<'_> {
    fn emissive(&self, c: Color, d: f32, x: i32, y: i32, amount: f32) -> Color {
        let t = ((d - 1.2) / (DBACK - 1.0)).clamp(0.0, 1.0) * amount;
        steps(c, self.room.fog, t, x, y)
    }

    /// A surface that only shows by the room's light.
    fn lit(&self, c: Color, d: f32, x: i32, y: i32, amount: f32) -> Color {
        let c = self.emissive(c, d, x, y, amount);
        let a = self.lights.amb;
        if a < 1.0 {
            steps(c, self.room.void, 1.0 - a, x, y)
        } else {
            c
        }
    }

    fn cab_level(&self, i: usize) -> f32 {
        match self.lights.cabs[i] {
            Cab::Flash => 1.0,
            c => c.level(),
        }
    }
}

/// The places screens throw their light on the carpet, and marquees theirs
/// on the ceiling: (X, depth, colour, cabinet).
fn pools() -> Vec<(f32, f32, Color, usize, bool)> {
    let mut out = Vec::new();
    for i in 0..10 {
        let (sch, side) = cabinet(i);
        if side == 0 {
            let k = i - 8;
            let x = if k == 0 { -0.44 } else { 0.44 };
            out.push((x, DBACK - 0.5, sch.glow, i, true));
        } else {
            let slot = SLOTS[i % 4];
            let mid = slot + CW / 2.0;
            out.push((side as f32 * (FRONT - 0.2), mid, sch.glow, i, true));
            out.push((side as f32 * (FRONT + 0.1), mid, sch.marq, i, false));
        }
    }
    out
}

fn carpet(
    b: &Builder,
    x_: f32,
    d: f32,
    x: i32,
    y: i32,
    pools: &[(f32, f32, Color, usize, bool)],
) -> Color {
    let (u, v) = (x_ * 4.6, d * 4.6);
    let (cu, cv) = (u.floor(), v.floor());
    let (fu, fv) = (u - cu, v - cv);
    let r = h2(cu as i32 + 91, cv as i32 + 17);
    let ink = b.room.ink;
    let mut c = ink;
    if r < 0.14 {
        let (dx, dy) = (fu - 0.5, (fv - 0.5) * 1.3);
        if dx * dx + dy * dy < 0.05 {
            c = if dx + dy < 0.0 {
                b.room.mauve
            } else {
                b.room.plum
            };
        } else if (dy + dx * 0.4).abs() < 0.05 && dx.abs() < 0.44 {
            c = CYANLO;
        }
    } else if r < 0.34 {
        let z = 0.5 + 0.22 * (((fu * 3.0) % 2.0 - 1.0).abs() * 2.0 - 1.0);
        if (fv - z).abs() < 0.075 && fu > 0.08 && fu < 0.92 {
            c = if r < 0.24 { ROSE } else { PINKLO };
        }
    } else if r < 0.48 {
        let (a, bb) = (fu - 0.5, fv - 0.28);
        if (bb > 0.0 && bb < 0.5 && (a.abs() - bb * 0.6).abs() < 0.06)
            || ((bb - 0.5).abs() < 0.05 && a.abs() < 0.3)
        {
            c = AMBERLO;
        }
    } else if r < 0.60 && (fu - 0.5).abs() + (fv - 0.5).abs() < 0.1 {
        c = YELLOW;
    }
    if c != ink && d > 2.4 && ordered(x, y) < (d - 2.4) / 1.0 {
        c = ink;
    }
    let mut c = b.lit(c, d, x, y, 0.8);
    // Light from the screens, in each screen's colour, and the sign's wash
    // where the aisle ends.
    let (mut best, mut tint) = (0.0f32, 0);
    for &(px, pd, pc, cab, floor) in pools {
        if !floor {
            continue;
        }
        let (pc, lv) = match b.lights.cabs[cab] {
            Cab::Flash => (WHITE, 1.0),
            k => (pc, k.level()),
        };
        let e = lv * (-((x_ - px) * (x_ - px)) / 0.05 - ((d - pd) * (d - pd)) / 0.03).exp();
        if e > best {
            best = e;
            tint = pc;
        }
    }
    if best > 0.1 {
        c = steps(c, lerp_color(b.room.void, tint, 0.45), best * 0.9, x, y);
    }
    let lit = b.lights.sign.0.max(b.lights.sign.1);
    let wash = (1.0 - (DBACK - d) / 0.8).max(0.0) * (1.0 - x_.abs() / 0.7).max(0.0) * lit;
    if wash > 0.0 {
        c = steps(c, lerp_color(b.room.void, b.sign, 0.35), wash * 0.75, x, y);
    }
    c
}

fn profile_inside(q: f32, y: f32) -> bool {
    let front = DEEP;
    if !(TOP..=FLOOR).contains(&y) || q < 0.0 {
        return false;
    }
    if y < MARQ {
        return q <= front;
    }
    if y < SCR1 {
        let t = (y - MARQ) / (SCR1 - MARQ);
        return q <= front - RECESS * (1.0 - t * 0.4);
    }
    if y < PNL {
        let t = (y - SCR1) / (PNL - SCR1);
        return q <= front - RECESS * 0.6 + t * (LEDGE + RECESS * 0.6);
    }
    if y < PNLB {
        return q <= front + LEDGE;
    }
    if y < PNLB + 0.07 {
        let t = (y - PNLB) / 0.07;
        return q <= front + LEDGE * (1.0 - t);
    }
    if y > KICK {
        return q <= front - 0.03;
    }
    q <= front
}

fn side_art(sch: &Scheme, x_: f32, y_: f32) -> Color {
    // Out from the wall, which is where the art is laid out from.
    let q = WALL - x_.abs();
    match sch.style {
        Art::Bolt => {
            let z = (y_ - TOP) / (FLOOR - TOP);
            let seg = ((z * 3.0) as usize).min(2);
            let zz = z * 3.0 - seg as f32;
            let lean = [0.20, 0.10, 0.22][seg];
            let to = [0.10, 0.24, 0.08][seg];
            let xc = lean + (to - lean) * zz;
            let w = 0.05 - 0.012 * seg as f32;
            if (q - xc).abs() < w * 0.35 {
                sch.art2
            } else if (q - xc).abs() < w {
                sch.art
            } else {
                sch.body
            }
        }
        Art::Stars => {
            let g = h2((q * 40.0) as i32, (y_ * 40.0) as i32);
            if g < 0.05 {
                sch.art2
            } else if g < 0.09 || ((q * 2.2 + (y_ - TOP)) % 0.5 - 0.25).abs() < 0.03 {
                sch.art
            } else {
                sch.body
            }
        }
        Art::Stripes => {
            let w = q * 1.4 + (y_ - TOP) * 0.8;
            if (0.52..0.66).contains(&w) {
                sch.art
            } else if (0.70..0.75).contains(&w) {
                sch.art2
            } else {
                sch.body
            }
        }
    }
}

fn build(cam: Cam, room: Room, lights: &Lights, sign: Color) -> Raster {
    let b = Builder {
        room,
        lights,
        sign,
        fx: [Fx::None; 10],
    };
    let mut r = Raster::new(cam, room.void);
    let pools = pools();

    for i in 0..8 {
        build_side_cabinet(&mut r, &b, i);
    }
    for i in 8..10 {
        build_back_cabinet(&mut r, &b, i);
    }

    // Floor and ceiling.
    r.patch(
        Plane::Y,
        FLOOR,
        (-WALL, WALL),
        (0.9, DBACK),
        None,
        |x_, d, _, x, y| Some(carpet(&b, x_, d, x, y, &pools)),
    );
    r.patch(
        Plane::Y,
        CEIL,
        (-WALL, WALL),
        (0.9, DBACK),
        None,
        |x_, d, dd, x, y| {
            let (tu, tv) = ((x_ * 3.6).rem_euclid(1.0), (d * 2.6).rem_euclid(1.0));
            let mut c = if tu < 0.05 || tv < 0.07 {
                room.night
            } else {
                room.ink
            };
            for &(mx, md, mc, cab, floor) in &pools {
                if floor {
                    continue;
                }
                let lv = b.lights.cabs[cab].level();
                let e =
                    lv * (-((x_ - mx) * (x_ - mx)) / 0.008 - ((d - md) * (d - md)) / 0.012).exp();
                if e > 0.5 {
                    c = steps(c, lerp_color(room.void, mc, 0.25), (e - 0.5) * 0.8, x, y);
                }
            }
            Some(b.lit(c, dd, x, y, 0.9))
        },
    );

    // Side walls: panelling, posters above the cabinets, a neon tube.
    for side in [-1.0f32, 1.0] {
        r.patch(
            Plane::X,
            side * WALL,
            (0.9, DBACK),
            (CEIL, FLOOR),
            None,
            |d, y_, dd, x, y| {
                let mut c = if (d * 9.0) as i32 % 4 != 0 {
                    room.plum
                } else {
                    room.night
                };
                let k = ((d - 1.0) / 0.42).floor();
                let fd = (d - 1.0) / 0.42 - k;
                if d > 1.6
                    && y_ > -0.68
                    && y_ < -0.46
                    && fd > 0.15
                    && fd < 0.85
                    && h2(k as i32, side as i32) < 0.75
                {
                    let sch = [AMBER, CYAN, PINK, LIME]
                        [(h2(k as i32, side as i32 + 7) * 4.0) as usize % 4];
                    let t = (y_ + 0.68) / 0.22;
                    c = if t < 0.12 || t > 0.88 || !(0.2..=0.8).contains(&fd) {
                        VOID0
                    } else if (fd - 0.5).abs() < 0.18 && t > 0.25 && t < 0.7 {
                        lerp_color(sch, VOID0, 0.2)
                    } else {
                        lerp_color(VOID0, sch, 0.35)
                    };
                }
                let tube = -0.74;
                let th = 0.012 + 0.01 / d;
                let on = d <= b.lights.strip;
                if (y_ - tube).abs() < th {
                    return Some(if on {
                        b.emissive(
                            if d < 2.2 { room.tube_hi } else { room.tube },
                            dd,
                            x,
                            y,
                            0.7,
                        )
                    } else {
                        b.lit(room.tube_lo, dd, x, y, 0.7)
                    });
                }
                if on && (y_ - tube).abs() < th * 3.2 && ordered(x, y) < 0.5 {
                    return Some(lerp_color(b.lit(c, dd, x, y, 0.7), room.tube, 0.4));
                }
                Some(b.lit(c, dd, x, y, 0.7))
            },
        );
    }

    // The back wall, brick.
    r.patch(
        Plane::D,
        DBACK,
        (-WALL, WALL),
        (CEIL, FLOOR),
        None,
        |x_, y_, dd, x, y| {
            let by = ((y_ - CEIL) * 42.0) as i32;
            let off = if by % 2 != 0 { 0.5 } else { 0.0 };
            let bx = ((x_ + WALL) * 16.0 + off) as i32;
            let mortar = ((y_ - CEIL) * 42.0).rem_euclid(1.0) < 0.2
                || ((x_ + WALL) * 16.0 + off).rem_euclid(1.0) < 0.09;
            let c = if mortar {
                room.ink
            } else if h2(bx, by) < 0.75 {
                room.night
            } else {
                room.plum
            };
            Some(b.lit(c, dd, x, y, 0.0))
        },
    );
    r
}

fn build_side_cabinet(r: &mut Raster, b: &Builder, i: usize) {
    let (sch, side) = cabinet(i);
    let s = side as f32;
    let d0 = SLOTS[i % 4];
    let d1 = d0 + CW;
    let xf = s * FRONT;
    let mold = sch.mold;
    let body = sch.body;
    let room = b.room;
    let ky = r.cam.ky;

    // The near side, with its outline and art. The t-molding runs round the
    // whole outline on the aisle side, a pixel or two wide up close.
    r.patch(
        Plane::D,
        d0,
        (s * (WALL - DEEP - LEDGE - 0.02), s * WALL),
        (TOP, FLOOR),
        None,
        |x_, y_, dd, x, y| {
            let q = WALL - x_.abs();
            if !profile_inside(q, y_) {
                return None;
            }
            let px = dd / ky;
            let edge_out = !profile_inside(q + 1.6 * px, y_);
            let molding = edge_out
                || (q > DEEP * 0.5
                    && (!profile_inside(q, y_ - 1.2 * px) || !profile_inside(q, y_ + 1.2 * px)));
            let dark = !profile_inside(q, y_ - px) || !profile_inside(q, y_ + px) || q < px;
            let mut c = side_art(&sch, x_, y_);
            if c == body {
                c = lerp_color(body, room.mauve, 0.45);
            }
            if molding {
                c = if !profile_inside(q + 0.8 * px, y_) {
                    mold
                } else {
                    lerp_color(mold, WHITE, 0.35)
                };
            } else if dark {
                c = VOID0;
            }
            Some(b.lit(c, dd, x, y, 0.6))
        },
    );
    let edge = |d: f32| {
        let t = (d - d0) / CW;
        !(0.05..=0.95).contains(&t)
    };
    // The marquee's frame, and the marquee itself a hair in front of it.
    r.patch(
        Plane::X,
        xf,
        (d0, d1),
        (TOP, MARQ),
        None,
        |_, _, dd, x, y| Some(b.lit(mold, dd, x, y, 0.6)),
    );
    r.patch(
        Plane::X,
        xf - s * 0.002,
        (d0 + CW * 0.05, d1 - CW * 0.05),
        (TOP, MARQ),
        Some((Dyn::Marquee, i as u8)),
        |d, y_, _, _, _| {
            let mut u = (d - d0 - CW * 0.05) / (CW * 0.9);
            if side > 0 {
                u = 1.0 - u;
            }
            uv(u, (y_ - TOP) / (MARQ - TOP))
        },
    );
    // The bezel set back under it, the screen in the bezel.
    let xr = s * (FRONT + RECESS);
    r.patch(
        Plane::X,
        xr,
        (d0, d1),
        (MARQ, SCR1 + 0.02),
        None,
        |d, y_, dd, x, y| {
            let c = if edge(d) {
                mold
            } else if !(SCR0 - 0.01..=SCR1 + 0.01).contains(&y_) {
                VOID0
            } else {
                room.ink
            };
            Some(b.lit(c, dd, x, y, 0.6))
        },
    );
    let (s0, s1) = (d0 + 0.04, d1 - 0.04);
    r.patch(
        Plane::X,
        xr - s * 0.002,
        (s0, s1),
        (SCR0, SCR1),
        Some((Dyn::Screen, i as u8)),
        |d, y_, _, _, _| {
            let mut u = (d - s0) / (s1 - s0);
            if side > 0 {
                u = 1.0 - u;
            }
            uv(u, (y_ - SCR0) / (SCR1 - SCR0))
        },
    );
    // The control panel: its top catches the screen's light.
    let xl = s * (FRONT - LEDGE);
    let lv = b.cab_level(i);
    r.patch(
        Plane::Y,
        PNL,
        (xl.min(xf), xl.max(xf)),
        (d0, d1),
        None,
        |x_, d, dd, x, y| {
            let t = (d - d0) / CW;
            let q = x_.abs() - (FRONT - LEDGE);
            let mut c = lerp_color(body, sch.glow, 0.3 * lv);
            if edge(d) {
                c = mold;
            }
            if t > 0.25 && t < 0.40 && (q - 0.06).abs() < 0.02 {
                c = RED;
            }
            for (bs, bc) in [(0.55, YELLOW), (0.68, BLUE), (0.81, RED)] {
                if (t - bs).abs() < 0.04 && (q - 0.05).abs() < 0.022 {
                    c = bc;
                }
            }
            Some(b.lit(c, dd, x, y, 0.6))
        },
    );
    r.patch(
        Plane::X,
        xl,
        (d0, d1),
        (PNL, PNLB),
        None,
        |d, _, dd, x, y| {
            Some(b.lit(
                if edge(d) {
                    mold
                } else {
                    lerp_color(body, STEELHI, 0.25)
                },
                dd,
                x,
                y,
                0.6,
            ))
        },
    );
    // The body, with its coin door lit.
    r.patch(
        Plane::X,
        xf,
        (d0, d1),
        (PNLB, FLOOR),
        None,
        |d, y_, dd, x, y| {
            let t = (d - d0) / CW;
            if edge(d) {
                return Some(b.lit(lerp_color(mold, body, 0.4), dd, x, y, 0.6));
            }
            if y_ > KICK {
                return Some(b.lit(VOID0, dd, x, y, 0.6));
            }
            if t > 0.34 && t < 0.66 && y_ > 0.46 && y_ < 0.68 {
                if y_ > 0.53
                    && y_ < 0.57
                    && ((0.39..0.46).contains(&t) || (0.54..0.61).contains(&t))
                {
                    return Some(b.emissive(AMBER, dd, x, y, 0.6));
                }
                return Some(b.lit(VOID0, dd, x, y, 0.6));
            }
            Some(b.lit(body, dd, x, y, 0.6))
        },
    );
}

fn build_back_cabinet(r: &mut Raster, b: &Builder, i: usize) {
    let (sch, _) = cabinet(i);
    let (x0, x1) = if i == 8 { (-0.62, -0.26) } else { (0.26, 0.62) };
    let d = DBACK - 0.30;
    let w = x1 - x0;
    r.patch(
        Plane::D,
        d,
        (x0, x1),
        (TOP, FLOOR),
        None,
        |x_, y_, dd, x, y| {
            let s = (x_ - x0) / w;
            let c = if !(0.05..=0.95).contains(&s) {
                sch.mold
            } else if y_ < SCR1 + 0.02 {
                VOID0
            } else if y_ < PNLB {
                lerp_color(sch.body, sch.glow, 0.3 * b.cab_level(i))
            } else if y_ > KICK {
                VOID0
            } else if s > 0.36 && s < 0.64 && y_ > 0.46 && y_ < 0.66 {
                if y_ > 0.52
                    && y_ < 0.56
                    && ((0.40..0.47).contains(&s) || (0.53..0.60).contains(&s))
                {
                    return Some(b.emissive(AMBER, dd, x, y, 0.5));
                }
                VOID0
            } else {
                lerp_color(sch.body, VOID0, 0.4)
            };
            Some(b.lit(c, dd, x, y, 0.5))
        },
    );
    r.patch(
        Plane::D,
        d - 0.002,
        (x0 + w * 0.05, x1 - w * 0.05),
        (TOP, MARQ),
        Some((Dyn::Marquee, i as u8)),
        |x_, y_, _, _, _| uv((x_ - x0 - w * 0.05) / (w * 0.9), (y_ - TOP) / (MARQ - TOP)),
    );
    let (s0, s1) = (x0 + w * 0.14, x1 - w * 0.14);
    r.patch(
        Plane::D,
        d - 0.002,
        (s0, s1),
        (SCR0, SCR1),
        Some((Dyn::Screen, i as u8)),
        |x_, y_, _, _, _| uv((x_ - s0) / (s1 - s0), (y_ - SCR0) / (SCR1 - SCR0)),
    );
    for xx in [x0, x1] {
        r.patch(
            Plane::X,
            xx,
            (d, DBACK),
            (TOP, FLOOR),
            None,
            |_, _, dd, x, y| Some(b.lit(sch.body, dd, x, y, 0.6)),
        );
    }
}

fn shade_screen(b: &Builder, p: &DynPx, x: i32, y: i32, tick: u32) -> Color {
    let (cab, u, v, d) = (p.cab as usize, p.u, p.v, p.d);
    let (sch, _) = cabinet(cab);
    let glow = sch.glow;
    match b.lights.cabs[cab] {
        Cab::Flash => return b.emissive(WHITE, d, x, y, 0.3),
        Cab::Off => return b.lit(lerp_color(VOID0, glow, 0.06), d, x, y, 0.5),
        Cab::On(_) => {}
    }
    match b.fx[cab] {
        Fx::Over => {
            let on = (tick / 3).is_multiple_of(2);
            let c = if on {
                RED
            } else {
                lerp_color(VOID0, RED, 0.25)
            };
            return b.emissive(c, d, x, y, 0.3);
        }
        Fx::Record => {
            let k = (tick + (v * 20.0) as u32) % 4;
            return b.emissive([YELLOW, PINK, CYAN, WHITE][k as usize], d, x, y, 0.3);
        }
        Fx::None => {}
    }
    let lv = b.lights.cabs[cab].level();
    let ph = ((tick + cab as u32 * 7) % LOOP) as f32 / LOOP as f32;
    let base = lerp_color(VOID0, glow, 0.16);
    let tau = std::f32::consts::TAU;
    let mut c = base;
    match sch.game {
        Game::Invaders => {
            let off = (ph * 2.0 - 1.0).abs() * 0.3;
            let col = (u - off) * 7.0;
            let row = v * 9.0;
            if (1.0..5.0).contains(&row)
                && (0.0..5.0).contains(&col)
                && col.fract() < 0.62
                && row.fract() < 0.6
            {
                c = if (row as i32) % 2 != 0 {
                    glow
                } else {
                    lerp_color(glow, WHITE, 0.3)
                };
            }
            let ship = 0.5 + 0.25 * (ph * tau).sin();
            if row >= 8.0 && (u - ship).abs() < 0.08 {
                c = LIME;
            }
            if (u - ship).abs() < 0.02 && row > 5.5 && row < 8.0 && (row + ph * 20.0).fract() < 0.4
            {
                c = WHITE;
            }
        }
        Game::Maze => {
            let (gu, gv) = ((u * 7.0) as i32, (v * 6.0) as i32);
            let wall = (gu % 2 == 0 && gv % 3 != 1) || gv == 0 || gv == 5;
            if wall {
                c = BLUELO;
            }
            if (v - 0.58).abs() < 0.1 && (u - ph).abs() < 0.08 {
                c = YELLOW;
            } else if (v - 0.58).abs() < 0.03 && (u * 14.0).fract() < 0.3 && u > ph + 0.06 {
                c = AMBERHI;
            }
        }
        Game::Racer => {
            let wv = 0.1 + v * 0.8;
            if v < 0.36 {
                c = if v > 0.1 { BLUELO } else { PINKLO };
            } else if (u - 0.5).abs() < wv * 0.5 {
                c = STEEL;
                if (u - 0.5).abs() < 0.025 * (0.6 + v) && (v * 5.0 + ph * 4.0).fract() < 0.5 {
                    c = WHITE;
                }
                if v > 0.78 && v < 0.95 && (u - 0.5 - 0.12 * (ph * tau).sin()).abs() < 0.08 {
                    c = RED;
                }
            } else {
                c = if (v * 5.0 + ph * 4.0).fract() < 0.5 {
                    LIMELO
                } else {
                    lerp_color(LIMELO, VOID0, 0.3)
                };
            }
        }
        Game::Bars => {
            let band = ((u + v * 0.3 + ph) * 6.0) as i32 % 4;
            c = lerp_color([glow, AMBER, PINK, CYAN][band as usize], VOID0, 0.3);
        }
    }
    if y % 2 != 0 {
        c = lerp_color(c, VOID0, 0.22);
    }
    if u < 0.14 && v < 0.16 {
        c = lerp_color(c, WHITE, 0.3);
    }
    if lv < 1.0 {
        c = lerp_color(VOID0, c, lv);
    }
    b.emissive(c, d, x, y, 0.5)
}

fn shade_marquee(b: &Builder, p: &DynPx, x: i32, y: i32, tick: u32) -> Color {
    let (cab, u, v, d) = (p.cab as usize, p.u, p.v, p.d);
    let (sch, _) = cabinet(cab);
    let col = sch.marq;
    let lv = b.lights.cabs[cab].level();
    if lv <= 0.0 {
        return b.lit(lerp_color(VOID0, col, 0.18), d, x, y, 0.5);
    }
    // A tube that stutters now and then, at its own moment.
    let k = (tick + cab as u32 * 13) % LOOP;
    let mut lit = if cab % 3 == 1 && matches!(k, 3 | 4 | 9) {
        0.55
    } else {
        1.0
    };
    lit *= lv;
    let mut c = lerp_color(VOID0, col, 0.25 + 0.75 * lit);
    // The game's name, a dark word across the middle.
    if v > 0.3 && v < 0.72 && u > 0.14 && u < 0.86 {
        let cell = (u * 10.0) as i32;
        if h2(cell, cab as i32 + 1) < 0.72 && (u * 10.0).fract() < 0.78 {
            c = lerp_color(col, VOID0, 0.7);
        }
    }
    if v < 0.14 {
        c = lerp_color(c, WHITE, 0.45 * lit);
    }
    b.emissive(c, d, x, y, 0.5)
}

/// The hall, kept between frames: the part that does not move as a
/// picture, rebuilt only when the lights or the screen change.
#[derive(Default)]
pub struct Hall {
    key: Option<(usize, usize, String, [u32; 13])>,
    base: Vec<Color>,
    moving: Vec<DynPx>,
    cam_w: usize,
    /// How far away each pixel of the room is, for whatever stands in it.
    depth: Vec<f32>,
    /// What each screen shows besides its game, set by whoever plays it.
    pub fx: [Fx; 10],
}

impl Hall {
    /// Draw the hall into the whole framebuffer. `sign` is the colour its
    /// light washes the far end of the carpet with.
    pub fn draw(
        &mut self,
        fb: &mut Framebuffer,
        th: &Theme,
        lights: &Lights,
        sign: Color,
        tick: u32,
    ) {
        let key = (fb.w, fb.h, th.name.clone(), lights.static_key());
        let room = Room::of(th);
        let cam = Cam::of(fb.w, fb.h);
        if self.key.as_ref() != Some(&key) {
            let r = build(cam, room, lights, sign);
            self.base = r.col;
            self.depth = r.z;
            self.moving = r
                .dynamic
                .iter()
                .enumerate()
                .filter_map(|(at, d)| {
                    d.map(|(kind, cab, u, v, d)| DynPx {
                        at: at as u32,
                        kind,
                        cab,
                        u,
                        v,
                        d,
                    })
                })
                .collect();
            self.cam_w = fb.w;
            self.key = Some(key);
        }
        fb.px.copy_from_slice(&self.base);
        let b = Builder {
            room,
            lights,
            sign,
            fx: self.fx,
        };
        for p in &self.moving {
            let (x, y) = (
                (p.at as usize % self.cam_w) as i32,
                (p.at as usize / self.cam_w) as i32,
            );
            let c = match p.kind {
                Dyn::Screen => shade_screen(&b, p, x, y, tick),
                Dyn::Marquee => shade_marquee(&b, p, x, y, tick),
            };
            fb.px[p.at as usize] = c;
        }
    }

    /// How far away the room is at a pixel; nothing there is far.
    pub fn depth_at(&self, x: i32, y: i32) -> f32 {
        if x < 0 || y < 0 || self.cam_w == 0 {
            return f32::MAX;
        }
        let i = y as usize * self.cam_w + x as usize;
        if x as usize >= self.cam_w {
            return f32::MAX;
        }
        self.depth.get(i).copied().unwrap_or(f32::MAX)
    }

    /// Where a point of the room lands on the screen.
    pub fn project(fb: &Framebuffer, x: f32, y: f32, d: f32) -> (f32, f32) {
        Cam::of(fb.w, fb.h).proj(x, y, d)
    }

    /// How many pixels tall one unit of the room is at depth one.
    pub fn unit(fb: &Framebuffer) -> f32 {
        Cam::of(fb.w, fb.h).ky
    }

    /// The glow of a cabinet's screen, for the light on whoever plays it.
    pub fn glow(cab: usize) -> Color {
        cabinet(cab).0.glow
    }

    /// Where the top of a cabinet's marquee is, for a word over it.
    pub fn cabinet_top(fb: &Framebuffer, cab: usize) -> (f32, f32) {
        let cam = Cam::of(fb.w, fb.h);
        match cab {
            8 => cam.proj(-0.44, TOP, DBACK - 0.30),
            9 => cam.proj(0.44, TOP, DBACK - 0.30),
            i => {
                let s = if i < 4 { -1.0 } else { 1.0 };
                cam.proj(s * FRONT, TOP, SLOTS[i % 4] + CW / 2.0)
            }
        }
    }

    /// Where the sign's board goes: its top left, for a wordmark of
    /// `cols` by `rows` cells.
    pub fn sign_at(fb: &Framebuffer, cols: i32) -> (i32, i32) {
        let cam = Cam::of(fb.w, fb.h);
        let (_, y) = cam.proj(0.0, CEIL, DBACK);
        (cam.vx as i32 - cols / 2, y as i32 + 5)
    }
}

/// The wordmark as a mask, a cell per pixel and two rows per line of text.
pub fn sign_mask() -> (Vec<bool>, usize, usize) {
    let lines: Vec<Vec<char>> = omacrt_shell::assets::WORDMARK_TXT
        .lines()
        .map(|l| l.chars().collect())
        .collect();
    let cols = lines.iter().map(|l| l.len()).max().unwrap_or(0);
    let rows = lines.len() * 2;
    let mut m = vec![false; cols * rows];
    for (r, line) in lines.iter().enumerate() {
        for (c, ch) in line.iter().enumerate() {
            if matches!(ch, '█' | '▀') {
                m[(r * 2) * cols + c] = true;
            }
            if matches!(ch, '█' | '▄') {
                m[(r * 2 + 1) * cols + c] = true;
            }
        }
    }
    (m, cols, rows)
}

/// The sign on the far wall: the wordmark in tubes on a dark board, OMA in
/// one colour and CRT in the other, the middle of every stroke white hot and
/// a halo on the board.
#[allow(clippy::too_many_arguments)]
pub fn draw_sign(
    fb: &mut Framebuffer,
    mask: &(Vec<bool>, usize, usize),
    (ox, oy): (i32, i32),
    tubes: (Tube, Tube),
    lights: &Lights,
    board: Color,
    tick: u32,
) {
    let (m, cols, rows) = (&mask.0, mask.1, mask.2);
    let k = tick % LOOP;
    let off = lights.flicker && matches!(k, 29 | 31 | 32 | 33);
    let dim = lights.flicker && k == 30;
    let at = |r: i32, c: i32| -> bool {
        r >= 0
            && c >= 0
            && (r as usize) < rows
            && (c as usize) < cols
            && m[r as usize * cols + c as usize]
    };
    let lit_at = |c: usize| -> f32 {
        let lv = if c < SPLIT {
            lights.sign.0
        } else {
            lights.sign.1
        };
        if lv <= 0.0 {
            0.0
        } else if lv < 1.0 || (dim && c >= SPLIT) {
            0.6
        } else if off && (SPLIT..SPLIT + 10).contains(&c) {
            0.0
        } else {
            1.0
        }
    };
    let (cw, rh) = (cols as i32, rows as i32);
    fb.rect(ox - 5, oy - 4, cw + 10, rh + 8, board);
    fb.rect(
        ox - 5,
        oy - 4,
        cw + 10,
        1,
        lerp_color(board, 0xffffff, 0.08),
    );
    for bx in [ox - 3, ox + cw + 2] {
        for by in [oy - 2, oy + rh + 2] {
            fb.put(bx, by, STEEL);
        }
    }
    for r in 0..rh {
        for c in 0..cw {
            if !at(r, c) || lit_at(c as usize) <= 0.0 {
                continue;
            }
            let halo = if (c as usize) < SPLIT {
                tubes.0.halo
            } else {
                tubes.1.halo
            };
            for dy in -3i32..=3 {
                for dx in -3i32..=3 {
                    let dd = dx.abs() + dy.abs();
                    if dd == 0 || dd > 4 || at(r + dy, c + dx) {
                        continue;
                    }
                    let (x, y) = (ox + c + dx, oy + r + dy);
                    let t = match dd {
                        1 => 0.7,
                        2 => 0.35,
                        _ => 0.14,
                    };
                    if ordered(x, y) < t {
                        let under = fb.at(x, y);
                        fb.put(x, y, lerp_color(under, halo, 0.55));
                    }
                }
            }
        }
    }
    for r in 0..rh {
        for c in 0..cw {
            if !at(r, c) {
                continue;
            }
            let t = if (c as usize) < SPLIT {
                tubes.0
            } else {
                tubes.1
            };
            let lv = lit_at(c as usize);
            let core = at(r + 1, c) && at(r - 1, c) && at(r, c + 1) && at(r, c - 1);
            let col = if lv <= 0.0 {
                // A tube with no current: dark glass.
                lerp_color(board, t.fill, if core { 0.14 } else { 0.08 })
            } else if core {
                if lv >= 1.0 { WHITE } else { t.hot }
            } else if lv >= 1.0 {
                t.fill
            } else {
                t.dim
            };
            fb.put(ox + c, oy + r, col);
        }
    }
}

/// The small arrow on the right of the sign, chasing.
pub fn draw_arrow(fb: &mut Framebuffer, x: i32, y: i32, lights: &Lights, tick: u32) {
    for k in 0..3u32 {
        let on = lights.small && (tick / 4) % 3 == k;
        let c = if on { LIME } else { LIMELO };
        let c = if lights.amb < 0.2 && !on {
            scale(c, 0.3)
        } else {
            c
        };
        for j in 0..4 {
            fb.put(x + k as i32 * 4 + j / 2, y + j, c);
            fb.put(x + k as i32 * 4 + j / 2, y + 7 - j, c);
        }
    }
}

/// The lights at time `t` of the hall's power-on, from the moment the word
/// lands on the wall. `None` once everything is on.
pub fn power_on(t: f32) -> Lights {
    let word = if t < 0.0 {
        0.0
    } else if t < 0.32 {
        [0.5, 0.0, 1.0, 0.5][((t / 0.08) as usize).min(3)]
    } else {
        1.0
    };
    let strip = if t >= 0.4 {
        1.0 + (DBACK - 1.0) * ease((t - 0.4) / 0.5)
    } else {
        0.0
    };
    let mut cabs = [Cab::Off; 10];
    let order: [(f32, [usize; 2]); 5] = [
        (CAB_TIMES[0], [8, 9]),
        (CAB_TIMES[1], [3, 7]),
        (CAB_TIMES[2], [2, 6]),
        (CAB_TIMES[3], [1, 5]),
        (CAB_TIMES[4], [0, 4]),
    ];
    let mut on = 0;
    for (t0, ids) in order {
        let c = if t < t0 {
            Cab::Off
        } else if t < t0 + 0.08 {
            Cab::Flash
        } else if t < t0 + 0.16 {
            Cab::Off
        } else {
            Cab::On((0.3 + (t - t0 - 0.16) / 0.25).min(1.0))
        };
        for i in ids {
            cabs[i] = c;
        }
        if t >= t0 + 0.16 {
            on += 1;
        }
    }
    let amb = 0.10
        + if word > 0.9 { 0.16 } else { 0.0 }
        + 0.12 * ease((t - 0.4) / 0.5)
        + 0.12 * on as f32;
    Lights {
        amb: amb.min(1.0),
        cabs,
        sign: (word, word),
        strip,
        small: t >= 1.7,
        flicker: t >= 3.4,
    }
}

/// How long the power-on takes, from the word landing to everything lit.
#[cfg(test)]
pub const POWER_ON_SECS: f32 = 2.1;

/// When each pair of cabinets strikes, from the word landing: the two at
/// the back first, then the rows from far to near.
pub const CAB_TIMES: [f32; 5] = [0.8, 1.02, 1.24, 1.46, 1.68];

/// The dark of a board behind a neon tube, from the theme.
pub fn board(th: &Theme) -> Color {
    Room::of(th).void
}

/// Dark hall, for the moment it stands up behind the laser.
pub fn dark() -> Lights {
    Lights {
        amb: 0.10,
        cabs: [Cab::Off; 10],
        sign: (0.0, 0.0),
        strip: 0.0,
        small: false,
        flicker: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_is_the_wordmark() {
        let (m, cols, rows) = sign_mask();
        assert!(cols > 60 && rows > 16);
        assert!(m.iter().filter(|&&b| b).count() > 400);
    }

    #[test]
    fn a_frame_is_drawn_and_repeats() {
        let th = Theme::tokyo_night();
        let mut hall = Hall::default();
        let mut a = Framebuffer::new(320, 240);
        let mut b = Framebuffer::new(320, 240);
        hall.draw(&mut a, &th, &Lights::all_on(), 0xff00ff, 3);
        hall.draw(&mut b, &th, &Lights::all_on(), 0xff00ff, 3 + LOOP);
        assert_eq!(a.px, b.px);
        let distinct: std::collections::HashSet<_> = a.px.iter().collect();
        assert!(distinct.len() > 40, "a hall of {} colours", distinct.len());
    }

    /// What building the hall costs, which the power-on pays every frame.
    /// `cargo test --release hall::tests::build_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn build_cost() {
        let th = Theme::tokyo_night();
        let mut fb = Framebuffer::new(320, 240);
        let t = std::time::Instant::now();
        let n = 40;
        for k in 0..n {
            let mut hall = Hall::default();
            hall.draw(&mut fb, &th, &power_on(k as f32 * 0.05), 0xff00ff, k);
        }
        println!(
            "{:.2} ms a frame",
            t.elapsed().as_secs_f64() * 1000.0 / n as f64
        );
    }

    #[test]
    fn power_on_ends_lit() {
        let l = power_on(POWER_ON_SECS + 0.5);
        assert!(l.amb > 0.9);
        assert!(l.cabs.iter().all(|c| matches!(c, Cab::On(v) if *v >= 1.0)));
        assert_eq!(power_on(-1.0).sign, (0.0, 0.0));
    }
}
