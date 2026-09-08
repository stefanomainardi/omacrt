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

/// Brussels, and only Brussels: the one place this picture keeps a landmark
/// for. The city has three spellings depending on who is answering, and the
/// place has to be the city itself rather than merely contain its name, so
/// "Brussels Airport" gets nothing.
pub fn is_brussels(place: &str) -> bool {
    let city = place
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    matches!(
        city.as_str(),
        "brussels" | "bruxelles" | "brussel" | "bruxelles-capitale" | "brussels-capital"
    )
}

/// The nine spheres, as (x, y, radius, how far back it stands).
///
/// A cube standing on one corner: the foot, a ring of three, the middle, a
/// ring of three turned sixty degrees from the first, and the top. The rings
/// alternate so the frame reads as woven rather than as a ladder: the lower
/// one has a sphere at the front and two behind, the upper one two at the
/// front and one behind.
fn atomium_nodes(w: i32, horizon: i32) -> [(i32, i32, i32, f32); 9] {
    // Right of centre, so the afternoon sun reaches it, and a little way
    // into the roofline so it stands in the town rather than on it.
    let cx = w * 60 / 100;
    // High enough that the roofline hides its foot and not its shoulders.
    let base = horizon - 16;
    let level = 16;
    let spread = 18;
    [
        (cx, base, 6, 0.35),
        (cx, base - level, 6, 0.0),
        (cx - spread, base - level, 5, 0.9),
        (cx + spread, base - level, 5, 0.9),
        (cx, base - 2 * level, 7, 0.15),
        (cx - spread, base - 3 * level, 6, 0.2),
        (cx + spread, base - 3 * level, 6, 0.2),
        (cx, base - 3 * level, 5, 0.9),
        (cx, base - 4 * level, 6, 0.35),
    ]
}

/// The twenty tubes: twelve edges of the cube and eight diagonals to the
/// sphere in the middle, by index into `atomium_nodes`.
const ATOMIUM_TUBES: [(usize, usize); 20] = [
    // the foot to the lower ring
    (0, 1),
    (0, 2),
    (0, 3),
    // the lower ring to the upper one, two each
    (1, 5),
    (1, 6),
    (2, 5),
    (2, 7),
    (3, 6),
    (3, 7),
    // the upper ring to the top
    (5, 8),
    (6, 8),
    (7, 8),
    // and every corner to the middle
    (0, 4),
    (1, 4),
    (2, 4),
    (3, 4),
    (5, 4),
    (6, 4),
    (7, 4),
    (8, 4),
];

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

/// What the sky is doing this frame.
///
/// Every layer needs most of this and none of it changes while a frame is
/// drawn, so it travels as one thing rather than as seven arguments each.
struct Air<'a> {
    theme: &'a Theme,
    kind: Kind,
    /// Is the sun up?
    day: bool,
    /// How much orange a low sun is putting into the sky, 0 to 1.
    dusk: f32,
    /// Kilometres an hour, which sets the drift, the lean and the streaks.
    wind: f32,
    /// The line the sky ends on.
    horizon: i32,
    /// Seconds, for everything that moves.
    now: f64,
    /// How dark the sky is, 1 in the night and 0 in daylight, with twilight
    /// in between: what the stars fade on rather than blink out on.
    darkness: f32,
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

        let air = Air {
            theme,
            kind,
            day,
            dusk,
            wind,
            horizon,
            now,
            darkness: reading.darkness(minutes),
        };

        self.sky(fb, &air);
        if air.darkness > 0.02 {
            self.draw_stars(fb, &air);
        }
        self.body(fb, &air, arc);
        self.draw_clouds(fb, &air);
        if kind == Kind::Fog {
            // Over the clouds: fog is the thing between you and them.
            self.fog(fb, &air);
        }
        // Brussels only, and behind the roofline: the town is drawn after it
        // so its foot stands among the buildings.
        if is_brussels(&reading.place) {
            self.atomium(fb, &air, arc);
        }
        self.draw_town(fb, &air);
        self.ground(fb, &air);
        self.wind_streaks(fb, &air);
        self.falling(fb, &air);
        self.lightning(fb, &air);
    }

    /// The sky itself: two colours and a dither between them, chosen by the
    /// kind of weather and by how low the sun is.
    fn sky(&self, fb: &mut Framebuffer, air: &Air) {
        // Fixed colours, because a sunset has to look like a sunset, tinted
        // a quarter of the way toward the air.theme so a green desktop still
        // feels like the same machine.
        // A little way toward the air.theme's background, so a green desktop
        // still feels like the same machine, and no further: a sky that has
        // lost its own colour is a grey rectangle.
        let tint = |c: Color| lerp_color(c, air.theme.bg, 0.14);
        let pair = |day: bool| -> (Color, Color) {
            match (day, air.kind) {
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
            }
        };
        let (day_top, day_bottom) = pair(true);
        let (night_top, night_bottom) = pair(false);
        // A low sun pushes orange into the bottom of the sky, and a sun below
        // the horizon is as low as one gets, which is what the twilight blend
        // below fades in and out of.
        let dusk = if air.day { air.dusk } else { 1.0 };
        let day_bottom = lerp_color(day_bottom, rgb(230, 128, 62), dusk * 0.75);
        // Twilight: the sky does not change colour the instant the sun clears
        // the horizon, so the two skies are mixed by how dark it is. At noon
        // and at midnight this is one sky or the other exactly.
        let top = lerp_color(day_top, night_top, air.darkness);
        let bottom = lerp_color(day_bottom, night_bottom, air.darkness);
        let (top, bottom) = (tint(top), tint(bottom));
        for y in 0..air.horizon.min(fb.h as i32) {
            let t = y as f32 / air.horizon as f32;
            for x in 0..fb.w as i32 {
                fb.put(x, y, dither(x, y, t, top, bottom));
            }
        }
    }

    fn draw_stars(&self, fb: &mut Framebuffer, air: &Air) {
        for (x, y, phase) in &self.stars {
            // Every star has its own rhythm, so the sky does not blink.
            let tw = 0.55 + 0.45 * ((air.now as f32 * 1.7 + phase).sin());
            let c = lerp_color(
                air.theme.bg,
                air.theme.paper,
                tw.clamp(0.15, 1.0) * 0.9 * air.darkness,
            );
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
    fn body(&self, fb: &mut Framebuffer, air: &Air, arc: f32) {
        let w = fb.w as f32;
        let cx = (0.12 + arc * 0.76) * w;
        // A half circle, flattened to fit the sky.
        let top = 22.0;
        let cy =
            air.horizon as f32 - (air.horizon as f32 - top) * (std::f32::consts::PI * arc).sin();
        let (cx, cy) = (cx as i32, cy as i32);
        let r = if air.day { 13 } else { 11 };

        if air.day {
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
                        fb.put(px, py, air.theme.yellow);
                    }
                }
            }
            // Rays: eight spokes turning, breathing in and out.
            let hidden = matches!(
                air.kind,
                Kind::Overcast | Kind::Heavy | Kind::Thunder | Kind::Fog
            );
            if !hidden {
                let turn = air.now as f32 * 0.22;
                let breath = 1.0 + 0.16 * (air.now as f32 * 1.1).sin();
                let core = lerp_color(air.theme.paper, air.theme.yellow, 0.4);
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
                        fb.put(x, y, lerp_color(core, air.theme.orange, fade));
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
                        lerp_color(air.theme.yellow, air.theme.paper, 0.55)
                    } else {
                        air.theme.yellow
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
                    let c = lerp_color(air.theme.paper, air.theme.dim, 0.15);
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
                            lerp_color(air.theme.paper, air.theme.dim, 0.5),
                        );
                    }
                }
            }
        }
    }

    /// The clouds, drifting at the speed of the real wind.
    fn draw_clouds(&mut self, fb: &mut Framebuffer, air: &Air) {
        let w = fb.w as f32;
        // Even a still air.day moves the clouds a little, or the sky is a
        // photograph.
        let base = 2.0 + air.wind * 0.55;
        let dark = matches!(air.kind, Kind::Heavy | Kind::Thunder | Kind::Overcast);
        for cloud in self.clouds.iter_mut() {
            cloud.x += base * cloud.layer * (1.0 / 60.0);
            if cloud.x - cloud.width > w {
                cloud.x = -cloud.width;
            }
            let body = if air.day {
                if dark {
                    lerp_color(rgb(74, 78, 92), air.theme.bg, 0.25)
                } else {
                    lerp_color(rgb(226, 230, 238), rgb(230, 150, 90), air.dusk * 0.5)
                }
            } else {
                lerp_color(rgb(46, 50, 70), air.theme.bg, 0.35)
            };
            let lit = lerp_color(body, air.theme.paper, if air.day { 0.35 } else { 0.18 });
            let shade = lerp_color(body, air.theme.bg, 0.45);
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
                        if py >= air.horizon {
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
    fn fog(&self, fb: &mut Framebuffer, air: &Air) {
        let w = fb.w as i32;
        // Five bands, thick, slow and overlapping, so the sky behind them
        // comes and goes rather than sitting behind a screen door.
        for band in 0..5 {
            let speed = 3.0 + air.wind * 0.25 + band as f32 * 1.5;
            let off = (air.now as f32 * speed) as i32;
            let y0 = air.horizon - 96 + band * 20;
            let tall = 22;
            for y in y0.max(0)..(y0 + tall).min(air.horizon) {
                // Thickest through the middle of the band, nothing at its edges.
                let across = 1.0 - ((y - y0) as f32 / tall as f32 - 0.5).abs() * 2.0;
                for x in 0..w {
                    // The pattern travels with the band, so the fog drifts
                    // rather than the sky flickering behind a fixed screen.
                    let t = 0.85 * across;
                    if (BAYER[(y & 3) as usize][((x + off) & 3) as usize] as f32 + 0.5) / 16.0 < t {
                        let i = (y * w + x) as usize;
                        fb.px[i] = lerp_color(fb.px[i], air.theme.paper, 0.55);
                    }
                }
            }
        }
    }

    /// The town along the horizon, in silhouette, with the windows coming on
    /// after dark and an aerial here and there.
    /// The Atomium, and only over Brussels.
    ///
    /// It is a body centred cubic cell of iron standing on one corner, so
    /// that is what this draws: eight spheres at the corners of a cube, one
    /// in the middle, the twelve edges and the eight diagonals. Nine spheres
    /// and twenty tubes, the way it was built for 1958.
    ///
    /// By day it is metal, with the reflection sitting where the real sun is
    /// in the real sky, and the sun passes behind it because this is drawn
    /// after it. At night it does what the real one does on a good evening: a
    /// wave of colour travels through the spheres, a lamp chases round each
    /// one, and the red light on the top sphere answers to aircraft.
    fn atomium(&self, fb: &mut Framebuffer, air: &Air, arc: f32) {
        let nodes = atomium_nodes(fb.w as i32, air.horizon);
        // Where the light comes from, as a direction on the screen: the real
        // sun's place in its arc by day, and the moon's the same way at
        // night, which is what puts the highlight on the correct side.
        let sun_x = (0.12 + arc * 0.76) * fb.w as f32;
        let sun_y =
            air.horizon as f32 - (air.horizon as f32 - 22.0) * (std::f32::consts::PI * arc).sin();
        // Fog eats the bottom of it first, the way distance does.
        let fog = if air.kind == Kind::Fog { 0.55 } else { 0.0 };
        let wet = matches!(air.kind, Kind::Rain | Kind::Heavy | Kind::Thunder);
        let flash = self.flash > 0.0;

        // The metal, and the show. Both are a colour per sphere: by day the
        // same steel for all nine, by night a wave that walks through them.
        let steel = lerp_color(rgb(150, 162, 184), air.theme.paper, 0.14);
        let steel = lerp_color(steel, air.theme.orange, air.dusk * 0.5);
        let palette = [
            air.theme.magenta,
            air.theme.blue,
            air.theme.cyan,
            air.theme.green,
            air.theme.yellow,
            air.theme.orange,
            air.theme.red,
        ];
        let show = |i: usize| -> Color {
            // A quarter of a colour a second, and each sphere a step behind
            // the one before it: a wave rather than nine lamps in unison.
            let phase = air.now as f32 * 0.25 + i as f32 * 0.45;
            let n = palette.len() as f32;
            let k = phase.rem_euclid(n);
            let a = palette[k as usize % palette.len()];
            let b = palette[(k as usize + 1) % palette.len()];
            lerp_color(a, b, k.fract())
        };

        // Tubes first, the ones at the back before the ones at the front, so
        // the front of the frame reads as nearer.
        let dim_first =
            |edge: &(usize, usize)| -> bool { nodes[edge.0].3.max(nodes[edge.1].3) > 0.6 };
        let mut edges: Vec<(usize, usize)> = ATOMIUM_TUBES.to_vec();
        edges.sort_by_key(|e| !dim_first(e));
        for (a, b) in edges {
            let (ax, ay, _, ad) = nodes[a];
            let (bx, by, _, bd) = nodes[b];
            let depth = ad.max(bd);
            let body = if flash {
                lerp_color(air.theme.bg, air.theme.paper, 0.12)
            } else if air.darkness < 0.5 {
                scale(steel, 0.55 - depth * 0.2)
            } else {
                // At night the tubes carry a little of the colour of the two
                // spheres they join, which is what makes the whole frame glow
                // rather than nine separate balls.
                let mix = lerp_color(show(a), show(b), 0.5);
                lerp_color(air.theme.bg, mix, 0.35 - depth * 0.15)
            };
            let lit = lerp_color(body, air.theme.paper, if flash { 0.5 } else { 0.35 });
            fb.line(ax, ay, bx, by, body);
            fb.line(ax + 1, ay, bx + 1, by, body);
            fb.line(ax, ay - 1, bx, by - 1, lit);
        }

        // Then the spheres, back to front.
        let mut order: Vec<usize> = (0..nodes.len()).collect();
        order.sort_by(|a, b| nodes[*b].3.total_cmp(&nodes[*a].3));
        for i in order {
            let (cx, cy, r, depth) = nodes[i];
            // Metal by day, the show by night, and mixed through twilight:
            // the lights come up as the sky goes down rather than at the
            // stroke of sunset.
            let colour = lerp_color(steel, show(i), air.darkness);
            let far = 1.0 - depth * 0.35;
            let deep = fog * (1.0 - (air.horizon - cy) as f32 / 70.0).clamp(0.0, 1.0);
            let base = if flash {
                air.theme.bg
            } else {
                lerp_color(scale(colour, far), air.theme.bg, deep)
            };
            let dark = lerp_color(base, air.theme.bg, if air.day { 0.72 } else { 0.55 });
            let lit = lerp_color(base, air.theme.paper, if wet { 0.62 } else { 0.5 });
            // At night a lit sphere throws a little light into the air around
            // it, the same dithered halo the sun gets, in its own colour.
            // Without it the show reads as paint rather than as lamps.
            if air.darkness > 0.05 && !flash && depth < 0.6 {
                let reach = 4.0;
                for dy in -r - 5..=r + 5 {
                    for dx in -r - 5..=r + 5 {
                        let d = ((dx * dx + dy * dy) as f32).sqrt();
                        if d <= r as f32 || d > r as f32 + reach {
                            continue;
                        }
                        let t = 1.0 - (d - r as f32) / reach;
                        let px = cx + dx;
                        let py = cy + dy;
                        if py >= air.horizon {
                            continue;
                        }
                        if BAYER[(py & 3) as usize][(px & 3) as usize] as f32 / 16.0
                            < t * 0.2 * air.darkness
                        {
                            fb.put(px, py, lerp_color(fb.at(px, py), base, 0.3));
                        }
                    }
                }
            }
            self.sphere(fb, cx, cy, r, (sun_x, sun_y), dark, base, lit, flash);

            if air.darkness > 0.05 && !flash {
                // The lamps round the equator of each sphere, one of them
                // running ahead of the others.
                let lamps = 8;
                let lead = (air.now as f32 * 1.6 + i as f32 * 0.7).rem_euclid(lamps as f32);
                for l in 0..lamps {
                    let a = std::f32::consts::TAU * l as f32 / lamps as f32;
                    // On the sphere rather than outside it: lamps that stand
                    // off the edge turn nine spheres into nine sea urchins.
                    let lx = cx + (a.cos() * (r as f32 - 1.0)) as i32;
                    let ly = cy + (a.sin() * (r as f32 - 1.0) * 0.65) as i32;
                    let ahead =
                        ((l as f32 - lead).abs()).min(lamps as f32 - (l as f32 - lead).abs());
                    let bright = (1.0 - ahead / 2.0).clamp(0.15, 1.0);
                    fb.put(
                        lx,
                        ly,
                        lerp_color(base, air.theme.paper, bright * 0.65 * air.darkness),
                    );
                }
            }
            if air.kind == Kind::Snow {
                // A cap, because snow sits on a sphere the same way it sits
                // on a roof.
                fb.rect(cx - r / 2, cy - r, r, 1, air.theme.paper);
                fb.put(cx - r / 2 - 1, cy - r + 1, air.theme.paper);
                fb.put(cx + r / 2, cy - r + 1, air.theme.paper);
            }
        }

        // The red lamp on the top sphere, for aircraft. On for a moment every
        // four seconds, which is what the real one does.
        let (tx, ty, tr, _) = nodes[nodes.len() - 1];
        if (air.now % 4.0) < 0.18 {
            fb.put(tx, ty - tr - 2, air.theme.red);
            fb.put(
                tx,
                ty - tr - 1,
                lerp_color(air.theme.red, air.theme.paper, 0.4),
            );
        }
    }

    /// One sphere of the Atomium: a filled circle lit from where the sun is,
    /// its terminator dithered because a fixed palette had no other way of
    /// bending light.
    #[allow(clippy::too_many_arguments)]
    fn sphere(
        &self,
        fb: &mut Framebuffer,
        cx: i32,
        cy: i32,
        r: i32,
        light: (f32, f32),
        dark: Color,
        mid: Color,
        lit: Color,
        flash: bool,
    ) {
        // The direction the light comes from, as a unit vector on the screen.
        let (lx, ly) = (light.0 - cx as f32, light.1 - cy as f32);
        let len = (lx * lx + ly * ly).sqrt().max(1.0);
        let (lx, ly) = (lx / len, ly / len);
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy > r * r {
                    continue;
                }
                let (nx, ny) = (dx as f32 / r as f32, dy as f32 / r as f32);
                // How much this part of the ball faces the light, softened so
                // the ball reads round rather than as two halves.
                let facing = ((nx * lx + ny * ly) * 0.5 + 0.5).clamp(0.0, 1.0);
                let t = facing.powf(1.3);
                let x = cx + dx;
                let y = cy + dy;
                let c = if t > 0.6 {
                    dither(x, y, (t - 0.6) / 0.4, mid, lit)
                } else {
                    dither(x, y, t / 0.6, dark, mid)
                };
                fb.put(x, y, c);
            }
        }
        if flash {
            // A lightning frame: the ball is a silhouette with its edge lit.
            for a in 0..48 {
                let a = std::f32::consts::TAU * a as f32 / 48.0;
                let px = cx + (a.cos() * r as f32) as i32;
                let py = cy + (a.sin() * r as f32) as i32;
                fb.put(px, py, lit);
            }
            return;
        }
        // The reflection: two pixels where the ball faces the light most,
        // which is the only part of this that says "polished".
        let sx = cx + (lx * r as f32 * 0.55) as i32;
        let sy = cy + (ly * r as f32 * 0.55) as i32;
        fb.put(sx, sy, lit);
        fb.put(sx + 1, sy, lerp_color(lit, mid, 0.4));
    }

    fn draw_town(&self, fb: &mut Framebuffer, air: &Air) {
        let body = if air.day {
            lerp_color(air.theme.bg, rgb(40, 44, 62), 0.55)
        } else {
            lerp_color(air.theme.bg, rgb(18, 20, 34), 0.7)
        };
        let edge = lerp_color(body, air.theme.paper, 0.12);
        for b in &self.town {
            let top = air.horizon - b.h;
            fb.rect(b.x, top, b.w, b.h, body);
            fb.rect(b.x, top, b.w, 1, edge);
            if b.aerial {
                let ax = b.x + b.w / 2;
                fb.rect(ax, top - 7, 1, 7, body);
                fb.rect(ax - 3, top - 7, 7, 1, body);
                fb.rect(ax - 2, top - 5, 5, 1, body);
            }
            if air.darkness <= 0.02 {
                continue;
            }
            // Windows. The pattern was decided once, so they do not flicker,
            // and they go out over the same twilight the stars fade on.
            let cols = ((b.w - 6) / 6).max(1);
            for (i, on) in b.windows.iter().enumerate() {
                if !on {
                    continue;
                }
                let cx = b.x + 3 + (i as i32 % cols) * 6;
                let cy = top + 3 + (i as i32 / cols) * 6;
                if cy + 2 >= air.horizon {
                    continue;
                }
                let lamp = lerp_color(air.theme.yellow, air.theme.orange, 0.35);
                fb.rect(cx, cy, 2, 3, lerp_color(body, lamp, air.darkness));
            }
        }
    }

    /// The band under the horizon: dark, textured, and wet when it rains.
    fn ground(&self, fb: &mut Framebuffer, air: &Air) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let near = lerp_color(
            air.theme.bg,
            air.theme.paper,
            if air.day { 0.10 } else { 0.05 },
        );
        let far = lerp_color(
            air.theme.bg,
            air.theme.paper,
            if air.day { 0.02 } else { 0.01 },
        );
        for y in air.horizon..h {
            let t = (y - air.horizon) as f32 / (h - air.horizon) as f32;
            for x in 0..w {
                fb.put(x, y, dither(x, y, t, near, far));
            }
        }
        fb.rect(
            0,
            air.horizon,
            w,
            1,
            lerp_color(air.theme.bg, air.theme.paper, 0.22),
        );
        // Wet ground: the town's lights smeared down into it.
        if matches!(air.kind, Kind::Rain | Kind::Heavy | Kind::Thunder) && !air.day {
            for b in &self.town {
                if !b.aerial {
                    continue;
                }
                let x = b.x + b.w / 2;
                let wobble = ((air.now as f32 * 2.0 + x as f32).sin() * 1.5) as i32;
                for y in air.horizon + 1..(air.horizon + 9).min(h) {
                    let fade = 1.0 - (y - air.horizon) as f32 / 9.0;
                    fb.put(
                        x + wobble * (y - air.horizon) / 8,
                        y,
                        lerp_color(near, air.theme.yellow, fade * 0.35),
                    );
                }
            }
        }
    }

    /// Streaks of moving air. Only worth drawing when there is enough of it.
    fn wind_streaks(&mut self, fb: &mut Framebuffer, air: &Air) {
        if air.wind < 12.0 {
            return;
        }
        let w = fb.w as f32;
        let n = ((air.wind - 10.0) / 4.0) as i32;
        for i in 0..n.min(12) {
            // Each streak has its own height and speed, from its own index,
            // so no two of them line up.
            let seed = i as f32 * 37.0;
            let y = 12.0 + ((seed * 1.7).sin().abs() * (air.horizon as f32 - 40.0));
            let speed = 60.0 + air.wind * 4.0 + (seed * 3.1).sin() * 20.0;
            let x = ((air.now as f32 * speed + seed * 53.0) % (w + 60.0)) - 30.0;
            let len = 5.0 + (seed * 0.7).sin().abs() * 7.0;
            for d in 0..len as i32 {
                let t = d as f32 / len;
                let c = lerp_color(
                    fb.px[((y as i32).clamp(0, fb.h as i32 - 1) * fb.w as i32
                        + (x as i32 + d).clamp(0, fb.w as i32 - 1))
                        as usize],
                    air.theme.paper,
                    0.30 * (1.0 - (t - 0.5).abs() * 2.0),
                );
                fb.put(x as i32 + d, y as i32, c);
            }
        }
    }

    /// Rain, or snow, and what it does when it lands.
    fn falling(&mut self, fb: &mut Framebuffer, air: &Air) {
        if self.motes.is_empty() {
            return;
        }
        let dt = 1.0 / 60.0;
        let w = fb.w as f32;
        let snow = air.kind == Kind::Snow;
        // Rain leans with the air.wind; snow is pushed sideways and wanders.
        let slant = (air.wind / 12.0).clamp(0.0, 3.2);
        let colour = if snow {
            air.theme.paper
        } else {
            lerp_color(air.theme.cyan, air.theme.paper, 0.45)
        };
        let mut landed: Vec<(f32, f32)> = Vec::new();
        for m in self.motes.iter_mut() {
            m.y += m.speed * dt;
            // The lean is a ratio of the fall: a drop moving two pixels down
            // and three across is a drop in a strong air.wind, and anything more
            // than that is a drop going sideways.
            m.x += if snow {
                ((air.now as f32 * 1.3 + m.phase).sin() * 7.0 + air.wind * 0.4) * dt
            } else {
                slant * m.speed * dt
            };
            if m.x > w {
                m.x -= w;
            }
            if m.x < 0.0 {
                m.x += w;
            }
            if m.y > air.horizon as f32 {
                if !snow {
                    landed.push((m.x, air.horizon as f32));
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
    fn lightning(&mut self, fb: &mut Framebuffer, air: &Air) {
        if air.kind != Kind::Thunder {
            return;
        }
        let (w, h) = (fb.w as i32, fb.h as i32);
        let t = air.now as f32;
        if self.flash <= 0.0 && t > self.flash_at {
            // Somewhere between three and eleven seconds, so it is never a
            // metronome.
            self.flash_at = t + 3.0 + self.rng.unit() * 8.0;
            self.flash = 0.35;
            // A bolt down from a cloud, wandering as it goes.
            let mut x = 30 + self.rng.upto((w as u32).saturating_sub(60)) as i32;
            let mut y = 16 + self.rng.upto(24) as i32;
            self.bolt.clear();
            while y < air.horizon {
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
                fb.px[i] = lerp_color(fb.px[i], air.theme.paper, lit * 0.55);
            }
        }
        let mut last: Option<(i32, i32)> = None;
        for (x, y) in &self.bolt {
            if let Some((px, py)) = last {
                fb.line(px, py, *x, *y, air.theme.paper);
                fb.line(px + 1, py, *x + 1, *y, scale(air.theme.paper, 0.5));
            }
            last = Some((*x, *y));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_brussels_gets_the_atomium() {
        for name in [
            "Brussels",
            "bruxelles",
            "Brussel",
            "BRUSSELS",
            "Brussels, Belgium",
        ] {
            assert!(is_brussels(name), "{name}");
        }
        for name in [
            "",
            "Milano",
            "Brussels Airport Hotel",
            "New Brussels",
            "Bruges",
        ] {
            assert!(!is_brussels(name), "{name}");
        }
    }

    #[test]
    fn the_atomium_stands_in_the_sky_at_both_heights() {
        // 240 lines is a television and 288 is a PAL one; on both, the whole
        // of it has to be above the horizon and below the top of the frame,
        // and its foot has to be low enough for the roofline to hide it.
        for h in [240usize, 288] {
            let horizon = Sky::horizon(h);
            let nodes = atomium_nodes(320, horizon);
            let top = nodes.iter().map(|(_, y, r, _)| y - r).min().unwrap();
            let foot = nodes.iter().map(|(_, y, r, _)| y + r).max().unwrap();
            assert!(top > 24, "at {h} lines it reaches {top}");
            assert!(foot < horizon, "at {h} lines its foot is at {foot}");
            // And it stands in the right half, clear of the clock under the
            // horizon on the left.
            let x = nodes[0].0;
            assert!(x > 160 && x < 300, "at {h} lines it stands at {x}");
        }
    }

    #[test]
    fn every_sphere_is_joined_to_the_others() {
        // Twenty tubes, and not one of them joins a sphere to itself or
        // names a sphere that is not there.
        assert_eq!(ATOMIUM_TUBES.len(), 20);
        let mut touched = [0usize; 9];
        for (a, b) in ATOMIUM_TUBES {
            assert_ne!(a, b);
            assert!(a < 9 && b < 9);
            touched[a] += 1;
            touched[b] += 1;
        }
        // The middle sphere answers to all eight corners; every corner has
        // three edges and the one diagonal.
        assert_eq!(touched[4], 8);
        for (i, n) in touched.iter().enumerate() {
            if i != 4 {
                assert_eq!(*n, 4, "sphere {i} has {n} tubes");
            }
        }
    }
}
