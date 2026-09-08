//! The weather, drawn the way a 16 bit game drew a sky.
//!
//! One picture per kind of weather, at 320 by 240, with everything moving:
//! the sun crosses its real arc between the times the server gives for
//! sunrise and sunset and the moon takes the same path at night, clouds drift
//! in three layers at the speed of the real wind, rain slants with it and
//! breaks on the ground, snow wanders down, lightning lights the whole
//! frame, and a town sits along the bottom with its windows coming on after
//! dark.
//!
//! Two rules keep the mood. Nothing is smooth: every gradient is a 4x4
//! ordered dither, the way a console with a fixed palette faked one, and
//! nothing is anti-aliased. And nothing is symmetrical: the clouds, the
//! buildings and the stars all come out of a small deterministic generator,
//! so the sky is the same sky every evening but it was never drawn by hand.

use crate::fb::{Color, Framebuffer, lerp_color, rgb, scale};
use crate::theme::Theme;
use omarchy_crt_shell::ambient::{Kind, Reading};

/// The ordered dither of every home computer that had to fake a gradient.
const BAYER: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/// A pixel of `a` or of `b` depending on where it is, so that a fraction
/// between the two colours reads as a mix from a distance.
#[inline]
fn dither(x: i32, y: i32, t: f32, a: Color, b: Color) -> Color {
    let level = (t.clamp(0.0, 1.0) * 16.0) as u8;
    if level > BAYER[(y & 3) as usize][(x & 3) as usize] {
        b
    } else {
        a
    }
}

/// The little generator everything in the picture is shaped by.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        // xorshift, because the sky does not need a good one.
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn upto(&mut self, n: u32) -> u32 {
        if n == 0 { 0 } else { self.next() % n }
    }
    fn unit(&mut self) -> f32 {
        (self.next() % 1000) as f32 / 1000.0
    }
}

/// A cloud: a handful of overlapping blobs, drifting.
struct Cloud {
    x: f32,
    y: f32,
    /// Blobs as (offset x, offset y, radius).
    blobs: Vec<(f32, f32, f32)>,
    /// Fraction of the wind this layer takes, so the sky has depth.
    layer: f32,
    width: f32,
}

/// A drop of rain, a flake of snow, or a speck the wind is carrying.
struct Mote {
    x: f32,
    y: f32,
    speed: f32,
    /// Where in its own wobble this one is, so they do not move as one.
    phase: f32,
    len: f32,
}

/// A splash where a drop landed, for the few frames it lasts.
struct Splash {
    x: f32,
    y: f32,
    age: f32,
}

/// One building of the town along the bottom.
struct Building {
    x: i32,
    w: i32,
    h: i32,
    /// Whether each window is lit, filled once and left alone.
    windows: Vec<bool>,
    /// An aerial on the roof, which is what this project is about.
    aerial: bool,
}

/// Everything the picture needs to keep between frames.
pub struct Sky {
    clouds: Vec<Cloud>,
    motes: Vec<Mote>,
    splashes: Vec<Splash>,
    stars: Vec<(i32, i32, f32)>,
    town: Vec<Building>,
    /// The kind the moving parts were built for, so a change rebuilds them.
    built_for: Option<(Kind, usize, usize)>,
    /// When the next flash of lightning is due, and how far into it we are.
    flash_at: f32,
    flash: f32,
    bolt: Vec<(i32, i32)>,
    rng: Rng,
}

impl Default for Sky {
    fn default() -> Self {
        Self::new()
    }
}

impl Sky {
    pub fn new() -> Self {
        Self {
            clouds: Vec::new(),
            motes: Vec::new(),
            splashes: Vec::new(),
            stars: Vec::new(),
            town: Vec::new(),
            built_for: None,
            flash_at: 4.0,
            flash: 0.0,
            bolt: Vec::new(),
            rng: Rng(0x1234_5678),
        }
    }

    /// Where the ground starts: the picture is sky above this line and a dark
    /// band below it, which is where the writing goes.
    pub fn horizon(h: usize) -> i32 {
        (h as f32 * 0.70) as i32
    }

    // ------------------------------------------------------------- building

    fn build(&mut self, kind: Kind, w: usize, h: usize) {
        if self.built_for == Some((kind, w, h)) {
            return;
        }
        self.built_for = Some((kind, w, h));
        let width = w as f32;
        let horizon = Self::horizon(h) as f32;

        // Clouds: how many and how big is the whole difference between a
        // clear sky and an overcast one.
        let (count, size) = match kind {
            Kind::Clear => (2, 0.7),
            Kind::Partly => (4, 1.0),
            Kind::Cloudy => (6, 1.15),
            Kind::Overcast | Kind::Fog => (8, 1.35),
            Kind::Rain | Kind::Snow => (7, 1.25),
            Kind::Heavy | Kind::Thunder => (9, 1.45),
        };
        self.clouds = (0..count)
            .map(|i| {
                let layer = 0.45 + 0.55 * (i % 3) as f32 / 2.0;
                let r = 7.0 + self.rng.unit() * 9.0 * size;
                let blobs: Vec<(f32, f32, f32)> = (0..3 + self.rng.upto(3))
                    .map(|b| {
                        (
                            b as f32 * r * 0.85,
                            self.rng.unit() * r * 0.35,
                            r * (0.7 + self.rng.unit() * 0.5),
                        )
                    })
                    .collect();
                let span = blobs
                    .iter()
                    .map(|(dx, _, br)| dx + br)
                    .fold(0.0f32, f32::max);
                Cloud {
                    x: self.rng.unit() * width,
                    y: 8.0 + self.rng.unit() * (horizon - 60.0).max(20.0),
                    blobs,
                    layer,
                    width: span,
                }
            })
            .collect();

        // What is falling, and how much of it.
        let motes = match kind {
            Kind::Rain => 90,
            Kind::Heavy | Kind::Thunder => 170,
            Kind::Snow => 70,
            _ => 0,
        };
        let (speed, len) = match kind {
            Kind::Snow => (14.0, 1.0),
            Kind::Heavy | Kind::Thunder => (150.0, 7.0),
            _ => (110.0, 5.0),
        };
        self.motes = (0..motes)
            .map(|_| Mote {
                x: self.rng.unit() * width,
                y: self.rng.unit() * horizon,
                speed: speed * (0.75 + self.rng.unit() * 0.5),
                phase: self.rng.unit() * std::f32::consts::TAU,
                len: len * (0.7 + self.rng.unit() * 0.6),
            })
            .collect();
        self.splashes.clear();

        // Stars, for after dark. They are the same stars every night.
        let mut stars = Rng(0x9e37_79b9);
        self.stars = (0..46)
            .map(|_| {
                (
                    (stars.upto(w as u32)) as i32,
                    (stars.upto((horizon as u32).saturating_sub(20))) as i32,
                    stars.unit() * std::f32::consts::TAU,
                )
            })
            .collect();

        // The town. Its shape is fixed, so it is the same town every evening.
        let mut town = Rng(0x0512_00b1);
        self.town.clear();
        let mut x = -4;
        while x < w as i32 {
            let bw = 14 + town.upto(20) as i32;
            let bh = 10 + town.upto(34) as i32;
            let cols = ((bw - 6) / 6).max(1);
            let rows = ((bh - 6) / 6).max(1);
            let windows = (0..cols * rows).map(|_| town.upto(10) > 3).collect();
            self.town.push(Building {
                x,
                w: bw,
                h: bh,
                windows,
                aerial: town.upto(10) > 6,
            });
            x += bw + 1 + town.upto(3) as i32;
        }
    }

    // -------------------------------------------------------------- drawing

    /// Draw the whole picture. `minutes` is the time of day, which decides
    /// the light; `now` is a clock in seconds, which decides the movement.
    pub fn draw(
        &mut self,
        fb: &mut Framebuffer,
        theme: &Theme,
        reading: &Reading,
        minutes: u32,
        now: f64,
    ) {
        let kind = reading.kind;
        self.build(kind, fb.w, fb.h);
        let w = fb.w as i32;
        let h = fb.h as i32;
        let horizon = Self::horizon(fb.h);
        let day = reading.daylight(minutes);
        let arc = reading.arc(minutes);
        // How high the sun is, and so how much orange the sky takes. A
        // straight distance from noon makes every morning a sunset; the sine
        // of the arc is the sun's own height, and squaring what is left of it
        // keeps the orange to the hour it belongs to.
        let elevation = (std::f32::consts::PI * arc).sin();
        let dusk = if day { (1.0 - elevation).powi(2) } else { 0.0 };
        let wind = reading.wind_kmh.unwrap_or(6.0).clamp(0.0, 60.0);

        self.sky(fb, theme, kind, day, dusk, horizon);
        if !day {
            self.draw_stars(fb, theme, now);
        }
        self.body(fb, theme, day, arc, horizon, now, kind);
        self.draw_clouds(fb, theme, day, dusk, kind, wind, horizon);
        if kind == Kind::Fog {
            // Over the clouds: fog is the thing between you and them.
            self.fog(fb, theme, horizon, wind, now);
        }
        self.draw_town(fb, theme, day, horizon);
        self.ground(fb, theme, kind, day, horizon, now);
        self.wind_streaks(fb, theme, wind, horizon, now);
        self.falling(fb, theme, kind, wind, horizon, now);
        self.lightning(fb, theme, kind, horizon, now, w, h);
    }

    /// The sky itself: two colours and a dither between them, chosen by the
    /// kind of weather and by how low the sun is.
    fn sky(
        &self,
        fb: &mut Framebuffer,
        theme: &Theme,
        kind: Kind,
        day: bool,
        dusk: f32,
        horizon: i32,
    ) {
        // Fixed colours, because a sunset has to look like a sunset, tinted
        // a quarter of the way toward the theme so a green desktop still
        // feels like the same machine.
        // A little way toward the theme's background, so a green desktop
        // still feels like the same machine, and no further: a sky that has
        // lost its own colour is a grey rectangle.
        let tint = |c: Color| lerp_color(c, theme.bg, 0.14);
        let (top, bottom) = match (day, kind) {
            (true, Kind::Clear | Kind::Partly) => (rgb(20, 62, 148), rgb(92, 162, 226)),
            (true, Kind::Cloudy) => (rgb(58, 78, 112), rgb(150, 164, 180)),
            (true, Kind::Overcast | Kind::Fog) => (rgb(70, 76, 88), rgb(148, 150, 156)),
            (true, Kind::Rain) => (rgb(44, 56, 82), rgb(112, 124, 146)),
            (true, Kind::Heavy | Kind::Thunder) => (rgb(30, 36, 56), rgb(84, 92, 116)),
            // Snow needs a sky dark enough for white to show against it.
            (true, Kind::Snow) => (rgb(58, 68, 92), rgb(136, 146, 166)),
            (false, Kind::Clear | Kind::Partly) => (rgb(6, 8, 26), rgb(24, 30, 66)),
            (false, Kind::Snow) => (rgb(14, 18, 38), rgb(52, 60, 88)),
            (false, _) => (rgb(8, 10, 22), rgb(28, 32, 52)),
        };
        // A low sun pushes orange into the bottom of the sky.
        let bottom = if dusk > 0.0 {
            lerp_color(bottom, rgb(230, 128, 62), dusk * 0.75)
        } else {
            bottom
        };
        let (top, bottom) = (tint(top), tint(bottom));
        for y in 0..horizon.min(fb.h as i32) {
            let t = y as f32 / horizon as f32;
            for x in 0..fb.w as i32 {
                fb.put(x, y, dither(x, y, t, top, bottom));
            }
        }
    }

    fn draw_stars(&self, fb: &mut Framebuffer, theme: &Theme, now: f64) {
        for (x, y, phase) in &self.stars {
            // Every star has its own rhythm, so the sky does not blink.
            let tw = 0.55 + 0.45 * ((now as f32 * 1.7 + phase).sin());
            let c = lerp_color(theme.bg, theme.paper, tw.clamp(0.15, 1.0) * 0.9);
            fb.put(*x, *y, c);
            if tw > 0.93 {
                // The brightest few get the four points of a drawn star.
                fb.put(x + 1, *y, scale(c, 0.4));
                fb.put(x - 1, *y, scale(c, 0.4));
                fb.put(*x, y + 1, scale(c, 0.4));
                fb.put(*x, y - 1, scale(c, 0.4));
            }
        }
    }

    /// The sun or the moon, on the arc between the two times the server
    /// gives, which is the part that makes the page feel like a window.
    fn body(
        &self,
        fb: &mut Framebuffer,
        theme: &Theme,
        day: bool,
        arc: f32,
        horizon: i32,
        now: f64,
        kind: Kind,
    ) {
        let w = fb.w as f32;
        let cx = (0.12 + arc * 0.76) * w;
        // A half circle, flattened to fit the sky.
        let top = 22.0;
        let cy = horizon as f32 - (horizon as f32 - top) * (std::f32::consts::PI * arc).sin();
        let (cx, cy) = (cx as i32, cy as i32);
        let r = if day { 13 } else { 11 };

        if day {
            // A halo first, dithered so it has no edge, and the rays over
            // it: the other way round and the halo swallows them, because
            // they are the same yellow.
            let reach = 6.0;
            for y in -r - 6..=r + 6 {
                for x in -r - 6..=r + 6 {
                    let d = ((x * x + y * y) as f32).sqrt();
                    if d <= r as f32 || d > r as f32 + reach {
                        continue;
                    }
                    let t = 1.0 - (d - r as f32) / reach;
                    let px = cx + x;
                    let py = cy + y;
                    if BAYER[(py & 3) as usize][(px & 3) as usize] as f32 / 16.0 < t * 0.45 {
                        fb.put(px, py, theme.yellow);
                    }
                }
            }
            // Rays: eight spokes turning, breathing in and out.
            let hidden = matches!(
                kind,
                Kind::Overcast | Kind::Heavy | Kind::Thunder | Kind::Fog
            );
            if !hidden {
                let turn = now as f32 * 0.22;
                let breath = 1.0 + 0.16 * (now as f32 * 1.1).sin();
                let core = lerp_color(theme.paper, theme.yellow, 0.4);
                for i in 0..8 {
                    let a = turn + i as f32 * std::f32::consts::TAU / 8.0;
                    let (sa, ca) = (a.sin(), a.cos());
                    let from = r as f32 + 5.0;
                    let to = from + 10.0 * breath;
                    let mut t = from;
                    while t < to {
                        let x = cx + (ca * t) as i32;
                        let y = cy + (sa * t) as i32;
                        let fade = (t - from) / (to - from);
                        fb.put(x, y, lerp_color(core, theme.orange, fade));
                        t += 1.0;
                    }
                }
            }
            // The disc: two tones, lighter at the top left, which is all the
            // shading a circle this small can carry.
            for y in -r..=r {
                for x in -r..=r {
                    if x * x + y * y > r * r {
                        continue;
                    }
                    let lit = (x + y) < -3;
                    let c = if lit {
                        lerp_color(theme.yellow, theme.paper, 0.55)
                    } else {
                        theme.yellow
                    };
                    fb.put(cx + x, cy + y, c);
                }
            }
        } else {
            // The moon: a disc with a bite out of it and a few craters.
            let bite = 7;
            for y in -r..=r {
                for x in -r..=r {
                    if x * x + y * y > r * r {
                        continue;
                    }
                    let dx = x - bite;
                    if dx * dx + y * y <= r * r {
                        continue;
                    }
                    let c = lerp_color(theme.paper, theme.dim, 0.15);
                    fb.put(cx + x, cy + y, c);
                }
            }
            for (ox, oy, cr) in [(-6, -2, 2), (-3, 5, 1), (-8, 4, 1)] {
                for y in -cr..=cr {
                    for x in -cr..=cr {
                        if x * x + y * y > cr * cr {
                            continue;
                        }
                        fb.put(
                            cx + ox + x,
                            cy + oy + y,
                            lerp_color(theme.paper, theme.dim, 0.5),
                        );
                    }
                }
            }
        }
    }

    /// The clouds, drifting at the speed of the real wind.
    #[allow(clippy::too_many_arguments)]
    fn draw_clouds(
        &mut self,
        fb: &mut Framebuffer,
        theme: &Theme,
        day: bool,
        dusk: f32,
        kind: Kind,
        wind: f32,
        horizon: i32,
    ) {
        let w = fb.w as f32;
        // Even a still day moves the clouds a little, or the sky is a
        // photograph.
        let base = 2.0 + wind * 0.55;
        let dark = matches!(kind, Kind::Heavy | Kind::Thunder | Kind::Overcast);
        for cloud in self.clouds.iter_mut() {
            cloud.x += base * cloud.layer * (1.0 / 60.0);
            if cloud.x - cloud.width > w {
                cloud.x = -cloud.width;
            }
            let body = if day {
                if dark {
                    lerp_color(rgb(74, 78, 92), theme.bg, 0.25)
                } else {
                    lerp_color(rgb(226, 230, 238), rgb(230, 150, 90), dusk * 0.5)
                }
            } else {
                lerp_color(rgb(46, 50, 70), theme.bg, 0.35)
            };
            let lit = lerp_color(body, theme.paper, if day { 0.35 } else { 0.18 });
            let shade = lerp_color(body, theme.bg, 0.45);
            for (dx, dy, r) in &cloud.blobs {
                let bx = (cloud.x + dx) as i32;
                let by = (cloud.y + dy) as i32;
                let r = *r as i32;
                for y in -r..=r {
                    for x in -r..=r {
                        // Flattened underneath, the way a cloud sits.
                        let yy = if y > 0 { y * 2 } else { y };
                        if x * x + yy * yy > r * r {
                            continue;
                        }
                        let py = by + y;
                        if py >= horizon {
                            continue;
                        }
                        let c = if y < -r / 3 {
                            lit
                        } else if y > r / 3 {
                            shade
                        } else {
                            body
                        };
                        fb.put(bx + x, py, c);
                    }
                }
            }
        }
    }

    /// Fog: bands of dithered white drifting across each other.
    fn fog(&self, fb: &mut Framebuffer, theme: &Theme, horizon: i32, wind: f32, now: f64) {
        let w = fb.w as i32;
        // Five bands, thick, slow and overlapping, so the sky behind them
        // comes and goes rather than sitting behind a screen door.
        for band in 0..5 {
            let speed = 3.0 + wind * 0.25 + band as f32 * 1.5;
            let off = (now as f32 * speed) as i32;
            let y0 = horizon - 96 + band * 20;
            let tall = 22;
            for y in y0.max(0)..(y0 + tall).min(horizon) {
                // Thickest through the middle of the band, nothing at its edges.
                let across = 1.0 - ((y - y0) as f32 / tall as f32 - 0.5).abs() * 2.0;
                for x in 0..w {
                    // The pattern travels with the band, so the fog drifts
                    // rather than the sky flickering behind a fixed screen.
                    let t = 0.85 * across;
                    if (BAYER[(y & 3) as usize][((x + off) & 3) as usize] as f32 + 0.5) / 16.0 < t {
                        let i = (y * w + x) as usize;
                        fb.px[i] = lerp_color(fb.px[i], theme.paper, 0.55);
                    }
                }
            }
        }
    }

    /// The town along the horizon, in silhouette, with the windows coming on
    /// after dark and an aerial here and there.
    fn draw_town(&self, fb: &mut Framebuffer, theme: &Theme, day: bool, horizon: i32) {
        let body = if day {
            lerp_color(theme.bg, rgb(40, 44, 62), 0.55)
        } else {
            lerp_color(theme.bg, rgb(18, 20, 34), 0.7)
        };
        let edge = lerp_color(body, theme.paper, 0.12);
        for b in &self.town {
            let top = horizon - b.h;
            fb.rect(b.x, top, b.w, b.h, body);
            fb.rect(b.x, top, b.w, 1, edge);
            if b.aerial {
                let ax = b.x + b.w / 2;
                fb.rect(ax, top - 7, 1, 7, body);
                fb.rect(ax - 3, top - 7, 7, 1, body);
                fb.rect(ax - 2, top - 5, 5, 1, body);
            }
            if day {
                continue;
            }
            // Windows. The pattern was decided once, so they do not flicker.
            let cols = ((b.w - 6) / 6).max(1);
            for (i, on) in b.windows.iter().enumerate() {
                if !on {
                    continue;
                }
                let cx = b.x + 3 + (i as i32 % cols) * 6;
                let cy = top + 3 + (i as i32 / cols) * 6;
                if cy + 2 >= horizon {
                    continue;
                }
                fb.rect(cx, cy, 2, 3, lerp_color(theme.yellow, theme.orange, 0.35));
            }
        }
    }

    /// The band under the horizon: dark, textured, and wet when it rains.
    fn ground(
        &self,
        fb: &mut Framebuffer,
        theme: &Theme,
        kind: Kind,
        day: bool,
        horizon: i32,
        now: f64,
    ) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let near = lerp_color(theme.bg, theme.paper, if day { 0.10 } else { 0.05 });
        let far = lerp_color(theme.bg, theme.paper, if day { 0.02 } else { 0.01 });
        for y in horizon..h {
            let t = (y - horizon) as f32 / (h - horizon) as f32;
            for x in 0..w {
                fb.put(x, y, dither(x, y, t, near, far));
            }
        }
        fb.rect(0, horizon, w, 1, lerp_color(theme.bg, theme.paper, 0.22));
        // Wet ground: the town's lights smeared down into it.
        if matches!(kind, Kind::Rain | Kind::Heavy | Kind::Thunder) && !day {
            for b in &self.town {
                if !b.aerial {
                    continue;
                }
                let x = b.x + b.w / 2;
                let wobble = ((now as f32 * 2.0 + x as f32).sin() * 1.5) as i32;
                for y in horizon + 1..(horizon + 9).min(h) {
                    let fade = 1.0 - (y - horizon) as f32 / 9.0;
                    fb.put(
                        x + wobble * (y - horizon) / 8,
                        y,
                        lerp_color(near, theme.yellow, fade * 0.35),
                    );
                }
            }
        }
    }

    /// Streaks of moving air. Only worth drawing when there is enough of it.
    fn wind_streaks(
        &mut self,
        fb: &mut Framebuffer,
        theme: &Theme,
        wind: f32,
        horizon: i32,
        now: f64,
    ) {
        if wind < 12.0 {
            return;
        }
        let w = fb.w as f32;
        let n = ((wind - 10.0) / 4.0) as i32;
        for i in 0..n.min(12) {
            // Each streak has its own height and speed, from its own index,
            // so no two of them line up.
            let seed = i as f32 * 37.0;
            let y = 12.0 + ((seed * 1.7).sin().abs() * (horizon as f32 - 40.0));
            let speed = 60.0 + wind * 4.0 + (seed * 3.1).sin() * 20.0;
            let x = ((now as f32 * speed + seed * 53.0) % (w + 60.0)) - 30.0;
            let len = 5.0 + (seed * 0.7).sin().abs() * 7.0;
            for d in 0..len as i32 {
                let t = d as f32 / len;
                let c = lerp_color(
                    fb.px[((y as i32).clamp(0, fb.h as i32 - 1) * fb.w as i32
                        + (x as i32 + d).clamp(0, fb.w as i32 - 1))
                        as usize],
                    theme.paper,
                    0.30 * (1.0 - (t - 0.5).abs() * 2.0),
                );
                fb.put(x as i32 + d, y as i32, c);
            }
        }
    }

    /// Rain, or snow, and what it does when it lands.
    fn falling(
        &mut self,
        fb: &mut Framebuffer,
        theme: &Theme,
        kind: Kind,
        wind: f32,
        horizon: i32,
        now: f64,
    ) {
        if self.motes.is_empty() {
            return;
        }
        let dt = 1.0 / 60.0;
        let w = fb.w as f32;
        let snow = kind == Kind::Snow;
        // Rain leans with the wind; snow is pushed sideways and wanders.
        let slant = (wind / 12.0).clamp(0.0, 3.2);
        let colour = if snow {
            theme.paper
        } else {
            lerp_color(theme.cyan, theme.paper, 0.45)
        };
        let mut landed: Vec<(f32, f32)> = Vec::new();
        for m in self.motes.iter_mut() {
            m.y += m.speed * dt;
            // The lean is a ratio of the fall: a drop moving two pixels down
            // and three across is a drop in a strong wind, and anything more
            // than that is a drop going sideways.
            m.x += if snow {
                ((now as f32 * 1.3 + m.phase).sin() * 7.0 + wind * 0.4) * dt
            } else {
                slant * m.speed * dt
            };
            if m.x > w {
                m.x -= w;
            }
            if m.x < 0.0 {
                m.x += w;
            }
            if m.y > horizon as f32 {
                if !snow {
                    landed.push((m.x, horizon as f32));
                }
                m.y = -m.len - (m.phase * 3.0);
                m.x = (m.x + m.phase * 41.0) % w;
            }
            if snow {
                // A flake is one pixel, and two on the bigger ones.
                fb.put(m.x as i32, m.y as i32, colour);
                if m.len > 1.1 {
                    fb.put(m.x as i32 + 1, m.y as i32, scale(colour, 0.6));
                }
            } else {
                // A drop is a short slanted streak, brighter at the bottom.
                let steps = m.len as i32;
                for d in 0..steps {
                    let t = d as f32 / steps as f32;
                    let x = m.x - slant * (steps - d) as f32 * 0.5;
                    let y = m.y - (steps - d) as f32;
                    fb.put(x as i32, y as i32, scale(colour, 0.35 + t * 0.65));
                }
            }
        }
        for (x, y) in landed {
            self.splashes.push(Splash { x, y, age: 0.0 });
        }
        // The splashes: two pixels going out and up, and gone.
        self.splashes.retain_mut(|s| {
            s.age += dt;
            if s.age > 0.22 {
                return false;
            }
            let t = s.age / 0.22;
            let spread = 1.0 + t * 4.0;
            let lift = (1.0 - (t * 2.0 - 1.0).abs()) * 3.0;
            let c = scale(colour, 1.0 - t);
            fb.put((s.x - spread) as i32, (s.y - lift) as i32, c);
            fb.put((s.x + spread) as i32, (s.y - lift) as i32, c);
            true
        });
    }

    /// Lightning: the whole frame lit for two frames, and a bolt.
    #[allow(clippy::too_many_arguments)]
    fn lightning(
        &mut self,
        fb: &mut Framebuffer,
        theme: &Theme,
        kind: Kind,
        horizon: i32,
        now: f64,
        w: i32,
        h: i32,
    ) {
        if kind != Kind::Thunder {
            return;
        }
        let t = now as f32;
        if self.flash <= 0.0 && t > self.flash_at {
            // Somewhere between three and eleven seconds, so it is never a
            // metronome.
            self.flash_at = t + 3.0 + self.rng.unit() * 8.0;
            self.flash = 0.35;
            // A bolt down from a cloud, wandering as it goes.
            let mut x = 30 + self.rng.upto((w as u32).saturating_sub(60)) as i32;
            let mut y = 16 + self.rng.upto(24) as i32;
            self.bolt.clear();
            while y < horizon {
                self.bolt.push((x, y));
                y += 2 + self.rng.upto(4) as i32;
                x += self.rng.upto(9) as i32 - 4;
            }
        }
        if self.flash <= 0.0 {
            return;
        }
        self.flash -= 1.0 / 60.0;
        let strength = (self.flash / 0.35).clamp(0.0, 1.0);
        // The first moment is the bright one; then it falls away.
        let lit = strength * strength;
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                fb.px[i] = lerp_color(fb.px[i], theme.paper, lit * 0.55);
            }
        }
        let mut last: Option<(i32, i32)> = None;
        for (x, y) in &self.bolt {
            if let Some((px, py)) = last {
                fb.line(px, py, *x, *y, theme.paper);
                fb.line(px + 1, py, *x + 1, *y, scale(theme.paper, 0.5));
            }
            last = Some((*x, *y));
        }
    }
}
