//! Rare moments on the weather page that tip it into a painting by René
//! Magritte for twenty five seconds: men in bowler hats standing in the air
//! instead of the rain, a daylight sky over a street at night, the man with
//! the apple, an eye as big as the sky, a rock with a castle floating by, a
//! dove made of sky in a storm, an apple filling the street, the moon in
//! front of a tree, an easel painting the city it hides, and the caption
//! under the clock that says it is not a clock.
//!
//! One comes every few minutes, and only one that fits the weather and the
//! hour. It comes in and goes out through the ordered dither, the way the
//! weather itself arrives.

use crate::fb::{Color, Framebuffer, lerp_color, rgb};
use crate::gallery::{apple, bowler, od, sky_clouds, steps};
use omacrt_shell::ambient::Kind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Moment {
    Golconda,
    Empire,
    Son,
    Apple,
    Eye,
    Pipe,
    Sept,
    Rock,
    Easel,
    Dove,
}

const ALL: [Moment; 10] = [
    Moment::Golconda,
    Moment::Empire,
    Moment::Son,
    Moment::Apple,
    Moment::Eye,
    Moment::Pipe,
    Moment::Sept,
    Moment::Rock,
    Moment::Easel,
    Moment::Dove,
];

impl Moment {
    pub fn by_name(name: &str) -> Option<Self> {
        Some(match name {
            "golconda" | "golconde" => Moment::Golconda,
            "empire" => Moment::Empire,
            "son" | "apple-man" => Moment::Son,
            "apple" => Moment::Apple,
            "eye" => Moment::Eye,
            "pipe" => Moment::Pipe,
            "sept" | "moon" => Moment::Sept,
            "rock" | "castle" => Moment::Rock,
            "easel" => Moment::Easel,
            "dove" => Moment::Dove,
            _ => return None,
        })
    }

    /// Whether the weather and the hour suit it.
    fn fits(self, c: &Cond) -> bool {
        let fair = matches!(c.kind, Kind::Clear | Kind::Partly);
        let dry = !matches!(
            c.kind,
            Kind::Rain | Kind::Heavy | Kind::Thunder | Kind::Snow
        );
        match self {
            Moment::Golconda => matches!(c.kind, Kind::Rain | Kind::Heavy),
            Moment::Empire => c.darkness > 0.6 && fair,
            Moment::Son => c.day && c.darkness < 0.2 && dry,
            Moment::Apple => c.day && c.darkness < 0.2 && fair,
            Moment::Eye => c.day && c.dusk > 0.35 && dry,
            Moment::Pipe => true,
            Moment::Sept => c.darkness > 0.7 && fair,
            Moment::Rock => c.day && c.darkness < 0.2 && fair,
            Moment::Easel => c.day && c.darkness < 0.2 && dry,
            Moment::Dove => matches!(c.kind, Kind::Thunder | Kind::Heavy),
        }
    }

    /// Whether it belongs in the sky, behind the town, or in front of it.
    pub fn in_sky(self) -> bool {
        matches!(
            self,
            Moment::Empire | Moment::Eye | Moment::Rock | Moment::Dove
        )
    }
}

/// What the page is showing, for choosing a moment that fits.
pub struct Cond {
    pub kind: Kind,
    pub day: bool,
    pub darkness: f32,
    pub dusk: f32,
}

/// How long one lasts, and how long it takes to come and go.
const LASTS: f32 = 25.0;
const FADE: f32 = 1.2;

pub struct Moments {
    on: Option<(Moment, f32)>,
    next_at: Option<f32>,
    forced: Option<Moment>,
    rng: u32,
}

impl Default for Moments {
    fn default() -> Self {
        Self {
            on: None,
            next_at: None,
            forced: None,
            rng: 0x6d61_6772,
        }
    }
}

impl Moments {
    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng % 10_000) as f32 / 10_000.0
    }

    /// Show this one from now on and keep it, for rendering it on purpose.
    pub fn force(&mut self, m: Moment) {
        self.forced = Some(m);
    }

    /// The moment showing at `t`, how far in it has come (0 to 1), and the
    /// seconds since it began.
    pub fn at(&mut self, t: f32, c: &Cond) -> Option<(Moment, f32, f32)> {
        if let Some(m) = self.forced {
            return Some((m, 1.0, t));
        }
        let next = *self.next_at.get_or_insert(t + 70.0);
        if self.on.is_none() && t >= next {
            let fit: Vec<Moment> = ALL.iter().copied().filter(|m| m.fits(c)).collect();
            if !fit.is_empty() {
                let pick = fit[((self.rand() * fit.len() as f32) as usize).min(fit.len() - 1)];
                self.on = Some((pick, t));
            }
            self.next_at = Some(t + 200.0 + self.rand() * 160.0);
        }
        let (m, since) = self.on?;
        let local = t - since;
        if local > LASTS || !m.fits(c) {
            self.on = None;
            return None;
        }
        let alpha = (local / FADE).min((LASTS - local) / FADE).clamp(0.0, 1.0);
        Some((m, alpha, local))
    }

    pub fn is(&self, m: Moment) -> bool {
        self.forced == Some(m) || self.on.map(|o| o.0) == Some(m)
    }
}

/// Draw `m` over what is already there, through the dither at `alpha`.
pub fn draw(fb: &mut Framebuffer, m: Moment, alpha: f32, t: f32, horizon: i32, moon: Color) {
    let before = (alpha < 1.0).then(|| fb.px.clone());
    let w = fb.w as i32;
    match m {
        Moment::Golconda => golconda(fb, w, horizon, t),
        Moment::Empire => empire(fb, w, horizon, t),
        Moment::Son => son(fb, w / 4, horizon, t),
        Moment::Apple => {
            apple(fb, w * 3 / 10, horizon - 38, 40, t, 0.0);
            for x in w * 3 / 10 - 44..w * 3 / 10 + 44 {
                if od(x, horizon) < 0.6 {
                    let c = fb.at(x, horizon);
                    fb.put(x, horizon, lerp_color(c, 0, 0.4));
                }
            }
        }
        Moment::Eye => eye(fb, w / 2, 62, t),
        Moment::Rock => rock(fb, 40.0 + t * 3.0, 70.0 + (t * 0.8).sin() * 2.0),
        Moment::Dove => dove(fb, w as f32 * 0.46, 70.0, 56.0),
        Moment::Sept => sept(fb, 64, horizon, moon),
        Moment::Easel => easel(fb, 104, horizon),
        Moment::Pipe => {}
    }
    if let Some(before) = before {
        let fw = fb.w;
        for (i, px) in fb.px.iter_mut().enumerate() {
            let (x, y) = ((i % fw) as i32, (i / fw) as i32);
            if od(x, y) > alpha {
                *px = before[i];
            }
        }
    }
}

fn golconda(fb: &mut Framebuffer, w: i32, horizon: i32, t: f32) {
    for (size, gap, dy) in [(9, 22, 20), (14, 34, 30), (20, 52, 42)] {
        for row in 0..6 {
            for k in -1..w / gap + 2 {
                let x = k * gap + (row % 2) * gap / 2 + size / 2;
                let phase = k as f32 * 1.7 + row as f32 * 2.3 + size as f32 * 0.37;
                let y = (6.0
                    + (row * dy) as f32
                    + size as f32 * 0.3
                    + (t * 0.8 + phase).sin() * size as f32 / 14.0) as i32;
                if y > horizon - size / 3 {
                    continue;
                }
                bowler(fb, x, y, size, (k + row).rem_euclid(3) != 0, 0);
            }
        }
    }
}

fn empire(fb: &mut Framebuffer, w: i32, horizon: i32, t: f32) {
    let d = t * 2.0;
    sky_clouds(
        fb,
        (0, 0, w, horizon),
        0x4a84cc,
        0xa8cce8,
        &[
            (20.0 + d, 34.0, 62.0, 41),
            (150.0 + d, 22.0, 80.0, 43),
            (250.0 + d, 60.0, 50.0, 47),
        ],
        (0.4, -0.9),
        rgb(246, 247, 248),
        rgb(176, 186, 198),
    );
}

/// The man in the overcoat and the bowler hat, the apple where his face is.
fn son(fb: &mut Framebuffer, x0: i32, base: i32, t: f32) {
    let (coat, coat_hi, coat_lo) = (0x3a3c48, 0x585c6c, 0x22242c);
    fb.rect(x0 - 7, base - 26, 6, 26, 0x1e1e24);
    fb.rect(x0 + 2, base - 26, 6, 26, 0x1e1e24);
    let top = base - 78;
    for y in top..base - 22 {
        let f = (y - top) as f32 / (base - 22 - top) as f32;
        let half = if y > top + 4 {
            (17.0 - 3.0 * f) as i32
        } else {
            12 + (y - top)
        };
        for x in x0 - half..=x0 + half {
            let u = (x - x0) as f32 / half.max(1) as f32;
            let c = if u < -0.55 {
                coat_hi
            } else if u > 0.6 || (u.abs() < 0.08 && f > 0.2) {
                coat_lo
            } else {
                coat
            };
            fb.put(x, y, c);
        }
    }
    for y in top..top + 16 {
        let wv = (5 - (y - top) / 3).max(0);
        fb.rect(x0 - wv, y, 2 * wv + 1, 1, rgb(236, 236, 236));
    }
    fb.rect(x0, top + 2, 1, 14, 0xb41e24);
    fb.rect(x0 - 1, top + 3, 3, 2, 0xc82c2c);
    fb.rect(x0 - 4, top - 4, 9, 5, rgb(220, 176, 150));
    for y in top - 18..top - 2 {
        for x in x0 - 7..x0 + 8 {
            if ((x - x0) as f32 / 7.5).powi(2) + ((y - top + 10) as f32 / 8.5).powi(2) <= 1.0 {
                fb.put(x, y, rgb(224, 180, 152));
            }
        }
    }
    fb.rect(x0 - 11, top - 19, 23, 2, 0x0c0c10);
    for y in top - 29..top - 19 {
        let half = if y > top - 27 { 7 } else { 6 };
        fb.rect(x0 - half, y, 2 * half + 1, 1, 0x101014);
    }
    fb.rect(x0 - 7, top - 21, 15, 1, 0x28282e);
    apple(
        fb,
        x0,
        top - 9 + ((t * 1.3).sin() * 0.6).round() as i32,
        7,
        t,
        0.0,
    );
}

/// An eye as big as the sky, its iris a daylight sky with clouds.
fn eye(fb: &mut Framebuffer, cx: i32, cy: i32, t: f32) {
    let (w, h) = (150.0f32, 64.0f32);
    for y in cy - 34..cy + 35 {
        for x in cx - 77..cx + 78 {
            let (u, v) = ((x - cx) as f32 / (w / 2.0), (y - cy) as f32 / (h / 2.0));
            let lid = 1.0 - u * u;
            if v.abs() > lid {
                if v.abs() < lid + 0.08 {
                    fb.put(x, y, rgb(40, 30, 30));
                }
                continue;
            }
            let mut c = lerp_color(
                rgb(236, 230, 226),
                rgb(200, 190, 190),
                v.abs() / lid.max(0.01),
            );
            let d = ((x - cx) as f32).hypot((y - cy) as f32);
            if d < 27.0 {
                c = steps(0x3f7fd0, 0xbfe0f4, (y - (cy - 27)) as f32 / 54.0, x, y, 4.0);
                if (x as f32 * 0.18 + t * 0.2).sin() + (y as f32 * 0.3 + x as f32 * 0.07).sin()
                    > 1.2
                {
                    c = rgb(246, 248, 252);
                }
                if d > 25.0 {
                    c = rgb(30, 50, 80);
                }
            }
            if d < 9.0 {
                c = rgb(6, 6, 10);
            }
            if ((x - cx + 12) * (x - cx + 12) + (y - cy + 10) * (y - cy + 10)) < 6 {
                c = rgb(255, 255, 255);
            }
            fb.put(x, y, c);
        }
    }
    for k in -8..=8 {
        let u = k as f32 / 9.0;
        let x = cx + (u * w / 2.0) as i32;
        let y = cy - ((1.0 - u * u) * h / 2.0) as i32;
        for i in 0..4 {
            fb.put(x - (u * i as f32) as i32, y - i, rgb(30, 22, 22));
        }
    }
}

/// A boulder hanging in the air with a castle on its flat top.
fn rock(fb: &mut Framebuffer, cx: f32, cy: f32) {
    let (rx, ry) = (30.0f32, 34.0f32);
    let (hi, mid, lo, deep) = (
        rgb(186, 176, 164),
        rgb(138, 128, 120),
        rgb(92, 84, 80),
        rgb(58, 52, 52),
    );
    for y in (cy - 6.0) as i32..(cy + ry) as i32 {
        for x in (cx - rx - 2.0) as i32..(cx + rx + 3.0) as i32 {
            let v = ((y as f32 - cy + 6.0) / (ry + 6.0)).max(0.0);
            let half =
                rx * (1.0 - v.powf(1.6) * 0.92) + 2.0 * (y as f32 * 0.7 + x as f32 * 0.1).sin();
            if (x as f32 - cx).abs() > half {
                continue;
            }
            let u = (x as f32 - cx) / half.max(1.0);
            let lit = -u * 0.7 - v * 0.9 + 0.35 * (y as f32 * 0.45 + u * 3.0).sin();
            let c = if lit > 0.15 {
                hi
            } else if lit > -0.35 {
                mid
            } else if lit > -0.8 {
                lo
            } else {
                deep
            };
            fb.put(x, y, c);
        }
    }
    let top = (cy - 6.0) as i32;
    let cx = cx as i32;
    fb.rect(cx - 26, top - 1, 52, 1, rgb(90, 120, 70));
    let (wall, wall_lo) = (rgb(164, 154, 144), rgb(112, 104, 98));
    fb.rect(cx - 13, top - 12, 26, 12, wall);
    fb.rect(cx + 5, top - 12, 8, 12, wall_lo);
    for (tx, th) in [(cx - 16, 22), (cx + 9, 26), (cx - 3, 16)] {
        fb.rect(tx, top - th, 7, th, wall);
        fb.rect(tx + 5, top - th, 2, th, wall_lo);
        for m in (0..7).step_by(2) {
            fb.rect(tx + m, top - th - 2, 1, 2, wall);
        }
        fb.rect(tx + 3, top - th + 5, 1, 3, rgb(40, 36, 44));
    }
    fb.rect(cx - 2, top - 7, 4, 7, rgb(40, 36, 40));
}

#[allow(dead_code)]
fn in_poly(pts: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut inside = false;
    for i in 0..pts.len() {
        let ((x0, y0), (x1, y1)) = (pts[i], pts[(i + 1) % pts.len()]);
        if (y0 > y) != (y1 > y) && x < x0 + (y - y0) * (x1 - x0) / (y1 - y0) {
            inside = !inside;
        }
    }
    inside
}

/// A dove seen from below as it flies, the way La grande famille has it:
/// the wings spread wide with their feathers fanned at the back edge, the
/// head up, the tail spread under it, and inside the outline a daylight
/// sky with white clouds, against the storm.
fn dove(fb: &mut Framebuffer, cx: f32, cy: f32, s: f32) {
    let wing: [(f32, f32); 14] = [
        (-0.12, -0.22),
        (-0.50, -0.36),
        (-0.90, -0.52),
        (-1.25, -0.60),
        (-1.14, -0.46),
        (-1.20, -0.38),
        (-1.04, -0.30),
        (-1.10, -0.21),
        (-0.91, -0.15),
        (-0.95, -0.07),
        (-0.75, -0.03),
        (-0.72, 0.05),
        (-0.50, 0.03),
        (-0.14, 0.16),
    ];
    let left: Vec<(f32, f32)> = wing.to_vec();
    let right: Vec<(f32, f32)> = wing.iter().map(|&(x, y)| (-x, y)).collect();
    let tail: Vec<(f32, f32)> = vec![
        (-0.12, 0.36),
        (0.12, 0.36),
        (0.30, 0.84),
        (0.18, 0.80),
        (0.10, 0.88),
        (0.0, 0.82),
        (-0.10, 0.88),
        (-0.18, 0.80),
        (-0.30, 0.84),
    ];
    let beak: Vec<(f32, f32)> = vec![(-0.08, -0.50), (-0.22, -0.46), (-0.08, -0.42)];
    let polys = [left, right, tail, beak];
    let inside = |x: f32, y: f32| -> bool {
        let (u, v) = ((x - cx) / s, (y - cy) / s);
        (u / 0.18).powi(2) + ((v - 0.02) / 0.42).powi(2) <= 1.0
            || u * u + (v + 0.47) * (v + 0.47) <= 0.13 * 0.13
            || polys.iter().any(|p| {
                let mut hit = false;
                for i in 0..p.len() {
                    let ((x0, y0), (x1, y1)) = (p[i], p[(i + 1) % p.len()]);
                    if (y0 > v) != (y1 > v) && u < x0 + (v - y0) * (x1 - x0) / (y1 - y0) {
                        hit = !hit;
                    }
                }
                hit
            })
    };
    let (x0, x1, y0, y1) = (
        (cx - 1.3 * s) as i32,
        (cx + 1.3 * s) as i32,
        (cy - 0.7 * s) as i32,
        (cy + 0.95 * s) as i32,
    );
    let at = |x: i32, y: i32| inside(x as f32 + 0.5, y as f32 + 0.5);
    for y in y0..y1 {
        for x in x0..x1 {
            if !at(x, y) {
                if [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .any(|(dx, dy)| at(x + dx, y + dy))
                {
                    fb.put(x, y, rgb(22, 22, 34));
                }
                continue;
            }
            let t = (y - y0) as f32 / (y1 - y0) as f32;
            let mut c = steps(0x4f8fdc, 0xcfe8f6, t, x, y, 4.0);
            if (x as f32 * 0.12).sin() + (y as f32 * 0.2 + x as f32 * 0.05).sin() > 1.3 {
                c = rgb(244, 246, 252);
            }
            fb.put(x, y, c);
        }
    }
}

/// A tree at night, and the crescent moon in front of its leaves.
fn sept(fb: &mut Framebuffer, tx: i32, horizon: i32, moon: Color) {
    let top = horizon - 88;
    fb.rect(tx - 3, top + 40, 6, horizon - top - 40, rgb(30, 24, 26));
    for y in top - 30..top + 46 {
        for x in tx - 36..tx + 37 {
            let d = ((x - tx) as f32 / 34.0).hypot((y - top - 6) as f32 / 40.0);
            if d < 1.0 + 0.07 * (x as f32 * 0.9 + y as f32 * 0.5).sin() {
                let c = if x - tx < -10 && y < top {
                    rgb(40, 60, 56)
                } else {
                    rgb(22, 36, 34)
                };
                fb.put(x, y, c);
            }
        }
    }
    let (mx, my, r) = (tx + 4, top - 2, 9);
    for y in my - r - 1..my + r + 2 {
        for x in mx - r - 1..mx + r + 2 {
            let inner =
                (x - mx - 5) * (x - mx - 5) + (y - my + 2) * (y - my + 2) > (r - 1) * (r - 1);
            if (x - mx) * (x - mx) + (y - my) * (y - my) <= r * r && inner {
                fb.put(x, y, lerp_color(moon, rgb(246, 240, 200), 0.6));
            }
        }
    }
}

/// An easel in the street, and on it a canvas painting exactly the part of
/// the city it hides.
fn easel(fb: &mut Framebuffer, x0: i32, horizon: i32) {
    let (w, h) = (44, 36);
    let y0 = horizon - 58;
    let snap: Vec<Color> = (0..h)
        .flat_map(|j| (0..w).map(move |i| (i, j)))
        .map(|(i, j)| fb.at(x0 + i, y0 + j))
        .collect();
    let (wood, wood_lo) = (rgb(150, 110, 70), rgb(100, 70, 44));
    for k in 0..h + 22 {
        fb.put((x0 as f32 + 6.0 - k as f32 * 0.12) as i32, y0 + k, wood);
        fb.put(
            (x0 as f32 + w as f32 - 6.0 + k as f32 * 0.12) as i32,
            y0 + k,
            wood,
        );
        fb.put(x0 + w / 2, y0 + k, wood_lo);
    }
    for j in 0..h {
        for i in 0..w {
            fb.put(x0 + i, y0 + j, snap[(j * w + i) as usize]);
        }
    }
    for i in -1..=w {
        fb.put(x0 + i, y0 - 1, rgb(236, 232, 220));
        fb.put(x0 + i, y0 + h, rgb(200, 196, 186));
    }
    for j in -1..=h {
        fb.put(x0 - 1, y0 + j, rgb(236, 232, 220));
        fb.put(x0 + w, y0 + j, rgb(180, 176, 166));
    }
    fb.rect(x0 - 3, y0 + h + 1, w + 6, 2, wood);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_moment_has_a_name_and_fits_somewhere() {
        for m in ALL {
            let name = format!("{m:?}").to_lowercase();
            assert!(
                Moment::by_name(&name).is_some() || m == Moment::Son,
                "{name}"
            );
        }
        let rain = Cond {
            kind: Kind::Rain,
            day: true,
            darkness: 0.0,
            dusk: 0.0,
        };
        assert!(Moment::Golconda.fits(&rain));
        assert!(!Moment::Apple.fits(&rain));
    }

    #[test]
    fn a_moment_comes_and_goes() {
        let mut m = Moments::default();
        let c = Cond {
            kind: Kind::Clear,
            day: true,
            darkness: 0.0,
            dusk: 0.0,
        };
        assert!(m.at(0.0, &c).is_none());
        let during = m.at(71.0, &c);
        assert!(during.is_some());
        assert!(m.at(71.0 + LASTS + 1.0, &c).is_none());
    }
}
