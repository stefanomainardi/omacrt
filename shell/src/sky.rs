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

/// How much fog stands between the viewer and a point of the picture.
///
/// The fog layer decides this per pixel with a dither; the walker needs it as
/// a number, so this is the same five bands measured rather than drawn.
fn fog_veil(x: i32, y: i32, air: &Air) -> f32 {
    let mut veil = 0.0f32;
    for band in 0..5 {
        let speed = 3.0 + air.wind * 0.25 + band as f32 * 1.5;
        let off = (air.now as f32 * speed) as i32;
        let y0 = air.horizon - 96 + band * 20;
        let tall = 22;
        if y < y0 || y >= y0 + tall {
            continue;
        }
        let across = 1.0 - ((y - y0) as f32 / tall as f32 - 0.5).abs() * 2.0;
        // The band's own drift makes it thicker here and thinner there.
        let along = 0.75 + 0.25 * (((x + off) as f32) * 0.06).sin();
        veil = veil.max(0.85 * across * along);
    }
    veil.clamp(0.0, 1.0)
}

/// Weather arrives rather than switching.
///
/// The server answers every half hour and the answer used to land in one
/// frame: a sunny sky became a downpour between two sixtieths of a second.
/// This holds the sky the picture is showing and the one coming over it, and
/// hands out the blend between them. The colours of the sky, the number of
/// clouds, how much is falling and the fog all cross on that one number, so
/// the weather comes in the way weather does.
struct Arrival {
    showing: Kind,
    arriving: Option<(Kind, f32)>,
}

/// How long a change takes. Long enough to be a change of weather rather
/// than a cut, short enough that somebody watching sees it finish.
const ARRIVES: f32 = 25.0;

impl Default for Arrival {
    fn default() -> Self {
        Self {
            showing: Kind::Clear,
            arriving: None,
        }
    }
}

impl Arrival {
    /// Tell it what the server says and what time it is; it answers with the
    /// sky that is going, the sky that is coming, and how far through.
    fn update(&mut self, kind: Kind, t: f32) -> (Kind, Kind, f32) {
        let done = |at: f32| ((t - at) / ARRIVES).clamp(0.0, 1.0);
        match self.arriving {
            // The change finished: what was arriving is what is showing.
            Some((to, at)) if done(at) >= 1.0 => {
                self.showing = to;
                self.arriving = None;
                if kind != self.showing {
                    self.arriving = Some((kind, t));
                }
            }
            // It changed again while the last change was still coming in.
            // Whatever is on the screen now is what the new one comes from,
            // and the nearest of the two is close enough at this size.
            Some((to, at)) if to != kind => {
                if done(at) > 0.5 {
                    self.showing = to;
                }
                self.arriving = Some((kind, t));
            }
            None if kind != self.showing => self.arriving = Some((kind, t)),
            _ => {}
        }
        match self.arriving {
            Some((to, at)) => (self.showing, to, done(at)),
            None => (self.showing, self.showing, 1.0),
        }
    }
}

/// How many clouds a sky has, and how big they run.
fn cloud_count(kind: Kind) -> (usize, f32) {
    match kind {
        Kind::Clear => (2, 0.7),
        Kind::Partly => (4, 1.0),
        Kind::Cloudy => (6, 1.15),
        Kind::Overcast | Kind::Fog => (8, 1.35),
        Kind::Rain | Kind::Snow => (7, 1.25),
        Kind::Heavy | Kind::Thunder => (9, 1.45),
    }
}

/// How much is falling out of a sky, and how fast and long each of it is.
fn fall_of(kind: Kind) -> (usize, f32, f32) {
    match kind {
        Kind::Rain => (90, 110.0, 5.0),
        Kind::Heavy | Kind::Thunder => (170, 150.0, 7.0),
        Kind::Snow => (70, 14.0, 1.0),
        _ => (0, 110.0, 5.0),
    }
}

/// Where the street lamps stand, as a fraction of the width. Always the
/// same four, because a lamp that moves is a car.
const LAMPS: [f32; 4] = [0.14, 0.38, 0.66, 0.9];

/// Where the sun or the moon sits on the screen at this point of its arc:
/// a half circle from one side of the sky to the other, flattened to fit.
fn sun_at(w: usize, horizon: i32, arc: f32) -> (f32, f32) {
    let cx = (0.12 + arc * 0.76) * w as f32;
    let top = 22.0;
    let cy = horizon as f32 - (horizon as f32 - top) * (std::f32::consts::PI * arc).sin();
    (cx, cy)
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
    /// The sky that is going and the one that is coming, and how far through
    /// the change the picture is: 1 means `to` alone.
    from: Kind,
    to: Kind,
    blend: f32,
    /// How far through its month the moon is, and how much light that gives:
    /// 0 at new, 1 at full.
    moon: f32,
    moonlight: f32,
    /// Where the sun (or the moon) is on the screen, for anything that has
    /// to catch the light or hide from it.
    sun: (f32, f32),
    /// How much of the sun a cloud is covering, 0 to 1, and the middle and
    /// half width of the shadow that cloud throws on the town.
    cover: f32,
    shadow: (f32, f32),
    /// How long ago a bolt landed on the Atomium: what the blackout and the
    /// lights coming back are measured from. Enormous when nothing has been
    /// struck, which is nearly always.
    struck_since: f32,
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

/// Somebody walking home in the rain, which is most of what Brussels is.
struct Walker {
    x: f32,
    /// Left to right or right to left, as -1 or 1.
    dir: f32,
    /// How far into the stride, so the legs and the bob agree.
    step: f32,
    /// While this is in the future the umbrella is inside out and he is
    /// fighting it rather than walking.
    fighting: f32,
    /// When he next stops to look up at the sky. He stands there for
    /// `DWELL` and then walks on.
    pause_at: f32,
    /// One walk in four has the dog out with him.
    dog: bool,
    /// While this is in the future he is standing still with his head up,
    /// because something is happening above him worth looking at.
    looking: f32,
    /// Where his hat is while the wind has it, and until when he is chasing
    /// it rather than walking home.
    hat_x: f32,
    hat_until: f32,
}

/// An aeroplane crossing, with the trail it leaves behind it.
///
/// Over Brussels a clear sky always has one: the airport is ten kilometres
/// from the Atomium and the approach passes over the city. It takes twenty
/// odd seconds to cross, which is a long time in a picture and the reason it
/// is worth having: something is happening, slowly.
struct Plane {
    /// Where it entered, where it is, how fast, and how high.
    from: f32,
    x: f32,
    speed: f32,
    y: f32,
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
    built_for: Option<(Kind, Kind, usize, usize)>,
    /// When the next flash of lightning is due, and how far into it we are.
    flash_at: f32,
    flash: f32,
    bolt: Vec<(i32, i32)>,
    /// The sky the picture is showing and the one arriving over it. Weather
    /// does not change in a frame.
    arrival: Arrival,
    /// Drops on the window this whole page pretends to be: where each one
    /// is, how big, and how fast it is sliding.
    pane: Vec<(f32, f32, f32, f32)>,
    pane_at: f32,
    /// A light going up one of the Atomium's tubes: which tube, and how far
    /// along it is. Somebody on the escalator.
    bead: Option<(usize, f32)>,
    bead_at: f32,
    /// The tram, and when the next one is due along.
    tram: Option<f32>,
    tram_at: f32,
    /// The birds on the wire: where each one sits, and when it took off. A
    /// bird that is up comes back to the same place, because that is its
    /// place.
    birds: Vec<(f32, f32)>,
    /// Somebody out in it, and when the next one gives it a try.
    walker: Option<Walker>,
    walker_at: f32,
    /// Footprints in the snow, as (x, how old): the snow fills them in.
    prints: Vec<(f32, f32)>,
    /// Rings on the puddles, as (x, y, how old), and when the next drop
    /// lands in one.
    ripples: Vec<(f32, f32, f32)>,
    ripple_at: f32,
    /// The aeroplane crossing the sky, and when the next one is due.
    plane: Option<Plane>,
    plane_at: f32,
    /// When a bolt last landed on the Atomium, in the same seconds
    /// everything else here moves in. A long way in the past to begin with,
    /// because nothing has been struck yet.
    struck: f32,
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
            arrival: Arrival::default(),
            pane: Vec::new(),
            pane_at: 3.0,
            bead: None,
            bead_at: 9.0,
            tram: None,
            tram_at: 12.0,
            birds: Vec::new(),
            walker: None,
            walker_at: 7.0,
            prints: Vec::new(),
            ripples: Vec::new(),
            ripple_at: 0.0,
            plane: None,
            // The first one comes soon enough to be caught, and the rest are
            // a minute or two apart.
            plane_at: 5.0,
            struck: -1000.0,
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

    /// Build the moving parts for the two skies a change is between, so
    /// there is always enough of everything for whichever of them is winning.
    fn build(&mut self, from: Kind, to: Kind, w: usize, h: usize) {
        if self.built_for == Some((from, to, w, h)) {
            return;
        }
        self.built_for = Some((from, to, w, h));
        let width = w as f32;
        let horizon = Self::horizon(h) as f32;

        // Clouds: how many and how big is the whole difference between a
        // clear sky and an overcast one.
        let (from_n, from_size) = cloud_count(from);
        let (to_n, to_size) = cloud_count(to);
        let count = from_n.max(to_n);
        let size = from_size.max(to_size);
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

        // What is falling, and how much of it: enough for the heavier of
        // the two, and `falling` draws as many as the blend asks for.
        let (from_motes, from_speed, from_len) = fall_of(from);
        let (to_motes, to_speed, to_len) = fall_of(to);
        let motes = from_motes.max(to_motes);
        let speed = if to_motes >= from_motes {
            to_speed
        } else {
            from_speed
        };
        let len = if to_motes >= from_motes {
            to_len
        } else {
            from_len
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
        let landmark = is_brussels(&reading.place);
        let t = now as f32;
        let (from, to, blend) = self.arrival.update(reading.kind, t);
        // The kind everything that does not fade reads: the one that has more
        // than half of the picture.
        let kind = if blend > 0.5 { to } else { from };
        self.build(from, to, fb.w, fb.h);
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

        // Where the sun is on the screen, which three layers need: the body
        // itself, the reflection on the Atomium, and whatever a cloud in
        // front of it does to the light.
        let sun = sun_at(fb.w, horizon, arc);
        let (cover, shadow) = self.sun_cover(sun);
        let air = Air {
            theme,
            kind,
            day,
            dusk,
            wind,
            horizon,
            now,
            darkness: reading.darkness(minutes),
            from,
            to,
            blend,
            moon: reading.moon,
            // A full moon gives a whole moon's light and a new one none, and
            // the middle is the fraction of the disc that is lit.
            moonlight: (1.0 - (std::f32::consts::TAU * reading.moon).cos()) * 0.5,
            sun,
            cover,
            shadow,
            struck_since: now as f32 - self.struck,
        };

        self.sky(fb, &air);
        if air.darkness > 0.02 {
            self.draw_stars(fb, &air);
        }
        self.body(fb, &air, arc);
        self.draw_clouds(fb, &air);
        self.draw_plane(fb, &air);
        if from == Kind::Fog || to == Kind::Fog {
            // Over the clouds: fog is the thing between you and them.
            self.fog(fb, &air);
        }
        // Brussels only, and behind the roofline: the town is drawn after it
        // so its foot stands among the buildings.
        if landmark {
            self.atomium(fb, &air, arc);
        }
        self.draw_town(fb, &air);
        self.wire(fb, &air);
        self.tram(fb, &air);
        self.ground(fb, &air);
        self.lamps(fb, &air);
        self.walker(fb, &air, landmark);
        self.wind_streaks(fb, &air);
        self.falling(fb, &air);
        self.lightning(fb, &air, landmark);
        self.pane(fb, &air);
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
        let pair = |day: bool, kind: Kind| -> (Color, Color) {
            match (day, kind) {
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
        // Two skies for the weather that is going and two for the one that is
        // coming, mixed by how far through the change we are.
        let mix = |day: bool| -> (Color, Color) {
            let (at, ab) = pair(day, air.from);
            let (bt, bb) = pair(day, air.to);
            (lerp_color(at, bt, air.blend), lerp_color(ab, bb, air.blend))
        };
        let (day_top, day_bottom) = mix(true);
        let (night_top, night_bottom) = mix(false);
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

    /// How much of the sun the clouds are covering, and where the shadow of
    /// the cloud doing it falls.
    ///
    /// The rays used to be switched off by the *kind* of weather, so a cloud
    /// could drift across the sun and the sun would not notice. This is the
    /// geometry instead: how near a cloud's blobs are to the sun, and which
    /// cloud it is, so the town can be put in its shade.
    fn sun_cover(&self, sun: (f32, f32)) -> (f32, (f32, f32)) {
        let mut cover = 0.0f32;
        let mut shadow = (0.0f32, 0.0f32);
        for cloud in &self.clouds {
            let mut mine = 0.0f32;
            for (dx, dy, r) in &cloud.blobs {
                let bx = cloud.x + dx;
                let by = cloud.y + dy;
                let d = ((bx - sun.0).powi(2) + (by - sun.1).powi(2)).sqrt();
                // Inside the blob is covered, and it thins out over the last
                // few pixels of the edge rather than ending on a line.
                mine = mine.max((1.0 - (d - r * 0.6) / (r + 6.0)).clamp(0.0, 1.0));
            }
            if mine > cover {
                cover = mine;
                shadow = (cloud.x + cloud.width * 0.5, cloud.width * 0.6);
            }
        }
        (cover.clamp(0.0, 1.0), shadow)
    }

    /// The sun or the moon, on the arc between the two times the server
    /// gives, which is the part that makes the page feel like a window.
    fn body(&self, fb: &mut Framebuffer, air: &Air, arc: f32) {
        let (cx, cy) = (air.sun.0 as i32, air.sun.1 as i32);
        let _ = arc;
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
                    let strength = t * 0.45 * (1.0 - air.cover);
                    if BAYER[(py & 3) as usize][(px & 3) as usize] as f32 / 16.0 < strength {
                        fb.put(px, py, air.theme.yellow);
                    }
                }
            }
            // Rays: eight spokes turning, breathing in and out.
            let hidden = matches!(
                air.kind,
                Kind::Overcast | Kind::Heavy | Kind::Thunder | Kind::Fog
            );
            if !hidden && air.cover < 0.9 {
                let turn = air.now as f32 * 0.22;
                // The rays pull in as a cloud crosses the sun, and come back
                // out the other side.
                let breath = (1.0 + 0.16 * (air.now as f32 * 1.1).sin()) * (1.0 - air.cover);
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
            // The moon, in the phase the server says it is in rather than a
            // crescent forever. `phase` runs 0 to 1 through the month, and
            // the terminator is where the sunlit half of a sphere ends: at
            // each row it is an ellipse of half width `k * sqrt(r2 - y2)`,
            // which is the whole of the geometry.
            let phase = air.moon.rem_euclid(1.0);
            let waxing = phase < 0.5;
            let k = (std::f32::consts::TAU * phase).cos();
            let face = lerp_color(air.theme.paper, air.theme.dim, 0.15);
            // The dark limb is not black: earthshine, and it keeps the disc
            // a disc rather than a shape.
            let ash = lerp_color(air.theme.bg, air.theme.paper, 0.12);
            for y in -r..=r {
                let across = ((r * r - y * y) as f32).sqrt();
                let edge = k * across;
                for x in -r..=r {
                    if x * x + y * y > r * r {
                        continue;
                    }
                    let xf = x as f32;
                    let lit = if waxing { xf > edge } else { xf < -edge };
                    fb.put(cx + x, cy + y, if lit { face } else { ash });
                }
            }
            for (ox, oy, cr) in [(-6, -2, 2), (-3, 5, 1), (-8, 4, 1), (5, -4, 1)] {
                // A crater on the dark side of the terminator is not a
                // crater, it is a mistake.
                let across = ((r * r - oy * oy) as f32).sqrt();
                let visible = if air.moon.rem_euclid(1.0) < 0.5 {
                    ox as f32 > k * across
                } else {
                    (ox as f32) < -k * across
                };
                if !visible {
                    continue;
                }
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

    /// The aeroplane, and the trail it leaves.
    ///
    /// Only on a sky you could see one through, and only now and then: it
    /// takes twenty odd seconds to cross, and a minute or two passes before
    /// the next. By day it is a speck of metal with a contrail spreading
    /// behind it; at night it is the navigation lights, blinking, with
    /// nothing else to see.
    fn draw_plane(&mut self, fb: &mut Framebuffer, air: &Air) {
        let w = fb.w as f32;
        if !matches!(air.kind, Kind::Clear | Kind::Partly | Kind::Cloudy) {
            self.plane = None;
            return;
        }
        let t = air.now as f32;
        if self.plane.is_none() {
            if t < self.plane_at {
                return;
            }
            // Left to right or the other way, high up, and at a speed that
            // crosses the sky in twenty odd seconds.
            let right = self.rng.upto(2) == 0;
            let speed = 12.0 + self.rng.unit() * 6.0;
            self.plane = Some(Plane {
                from: if right { -14.0 } else { w + 14.0 },
                x: if right { -14.0 } else { w + 14.0 },
                speed: if right { speed } else { -speed },
                y: 26.0 + self.rng.unit() * 26.0,
            });
            self.plane_at = t + 45.0 + self.rng.unit() * 60.0;
        }
        let Some(plane) = self.plane.as_mut() else {
            return;
        };
        plane.x += plane.speed / 60.0;
        let (x, y, from, speed) = (plane.x, plane.y, plane.from, plane.speed);
        if (speed > 0.0 && x > w + 20.0) || (speed < 0.0 && x < -20.0) {
            self.plane = None;
            return;
        }
        let py = y as i32;
        if air.day {
            // The trail: it spreads and thins with age, so it is a wedge
            // rather than a line, and the oldest of it has gone.
            let life = 9.0 * speed.abs();
            let mut back = 1.0f32;
            while back < life {
                let tx = x - speed.signum() * back;
                if tx < -2.0 || tx > w + 2.0 {
                    back += 1.0;
                    continue;
                }
                let age = back / life;
                let fade = (1.0 - age).powf(1.6) * 0.75;
                let spread = 1.0 + age * 2.4;
                let mut dy = -(spread as i32);
                while dy <= spread as i32 {
                    let ty = py + dy;
                    let across = 1.0 - (dy as f32).abs() / (spread + 0.6);
                    let strength = fade * across;
                    if BAYER[(ty & 3) as usize][(tx as i32 & 3) as usize] as f32 / 16.0 < strength {
                        fb.put(
                            tx as i32,
                            ty,
                            lerp_color(fb.at(tx as i32, ty), air.theme.paper, 0.7),
                        );
                    }
                    dy += 1;
                }
                back += 1.0;
            }
            let _ = from;
            // The aircraft: three pixels of metal, with a wing.
            let nose = x as i32;
            let tail = nose - speed.signum() as i32 * 2;
            fb.put(nose, py, air.theme.paper);
            fb.put(tail, py, lerp_color(air.theme.paper, air.theme.dim, 0.4));
            fb.put(
                (nose + tail) / 2,
                py + 1,
                lerp_color(air.theme.paper, air.theme.dim, 0.5),
            );
        } else {
            // At night there is nothing to see but the lights: the white
            // strobe on top and the red one under a wing, on their own
            // rhythms, which is how you tell a plane from a star.
            let strobe = (t * 1.2).fract() < 0.09;
            let beacon = (t * 0.7).fract() < 0.22;
            if strobe {
                fb.put(x as i32, py, air.theme.paper);
            }
            if beacon {
                fb.put(x as i32 - speed.signum() as i32, py + 1, air.theme.red);
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
        // How many clouds each sky wants. The ones only the arriving sky
        // wants fade in, and the ones only the leaving sky wanted fade out,
        // so a sky fills and clears rather than switching.
        let want_from = cloud_count(air.from).0;
        let want_to = cloud_count(air.to).0;
        for (i, cloud) in self.clouds.iter_mut().enumerate() {
            let here = if i < want_from.min(want_to) {
                1.0
            } else if i < want_to {
                air.blend
            } else if i < want_from {
                1.0 - air.blend
            } else {
                0.0
            };
            cloud.x += base * cloud.layer * (1.0 / 60.0);
            if here <= 0.02 {
                continue;
            }
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
            // At night the tops catch the moon, and how much depends on the
            // phase: a full moon silvers them, a new one leaves them flat.
            let lit = lerp_color(
                body,
                air.theme.paper,
                if air.day {
                    0.35
                } else {
                    0.10 + 0.30 * air.moonlight
                },
            );
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
                        // A cloud that is arriving or leaving is dithered
                        // into the sky rather than switched on: at this size
                        // that reads as thinning out.
                        if here < 0.98
                            && (BAYER[(py & 3) as usize][((bx + x) & 3) as usize] as f32 + 0.5)
                                / 16.0
                                > here
                        {
                            continue;
                        }
                        fb.put(bx + x, py, c);
                    }
                }
            }
        }
    }

    /// Fog: bands of dithered white drifting across each other.
    fn fog(&self, fb: &mut Framebuffer, air: &Air) {
        let w = fb.w as i32;
        // Fog rolls in and clears rather than appearing: this is how much of
        // it there is, from the change the picture is in the middle of.
        let amount = match (air.from == Kind::Fog, air.to == Kind::Fog) {
            (true, true) => 1.0,
            (false, true) => air.blend,
            (true, false) => 1.0 - air.blend,
            (false, false) => return,
        };
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
                    let t = 0.85 * across * amount;
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
    fn atomium(&mut self, fb: &mut Framebuffer, air: &Air, arc: f32) {
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

        // A bolt that lands on it puts the lights out: half a second of
        // nothing, and then the nine come back from the ground up, the way a
        // substation brings a street back. `lit(i)` is whether sphere `i` has
        // its light yet, and nothing has been struck for a long time in the
        // ordinary case, so it is true for all of them.
        const OUT: f32 = 0.55;
        const RETURN: f32 = 0.16;
        let has_light = |i: usize| air.struck_since > OUT + i as f32 * RETURN;
        // The moment of contact, on the top sphere.
        let contact = air.struck_since < 0.12;

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
            // stroke of sunset. A sphere with no light yet is metal whatever
            // the hour.
            let colour = if has_light(i) {
                lerp_color(steel, show(i), air.darkness)
            } else {
                lerp_color(steel, air.theme.bg, 0.55)
            };
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
            if air.darkness > 0.05 && !flash && depth < 0.6 && has_light(i) {
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

            if air.darkness > 0.05 && !flash && has_light(i) {
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
            // The sun lining up with a polished sphere: a four pointed star
            // that grows as it comes into line and goes as it leaves, which
            // takes the best part of an hour of real sun.
            if air.day && air.darkness < 0.3 && !flash && has_light(i) {
                let near = 1.0 - ((sun_x - cx as f32).abs() / 30.0).clamp(0.0, 1.0);
                // The ones at the back do not catch it: a sphere in shadow
                // of the frame has nothing to reflect with.
                if near > 0.05 && depth < 0.4 {
                    let twinkle = 0.78 + 0.22 * (air.now as f32 * 2.6 + i as f32).sin();
                    let arm = (1.5 + 5.5 * near * twinkle * (1.0 - depth)) as i32;
                    for k in 1..=arm {
                        let fade = 1.0 - k as f32 / (arm + 1) as f32;
                        let c = lerp_color(air.theme.paper, air.theme.yellow, 0.35);
                        let c = scale(c, fade * near);
                        fb.put(cx + k, cy, c);
                        fb.put(cx - k, cy, c);
                        fb.put(cx, cy + k, c);
                        fb.put(cx, cy - k, c);
                    }
                    fb.put(cx, cy, air.theme.paper);
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

        let (tx, ty, tr, _) = nodes[nodes.len() - 1];
        if contact {
            // Where the bolt lands: white, and a ring of it thrown outward.
            for dy in -tr - 2..=tr + 2 {
                for dx in -tr - 2..=tr + 2 {
                    if dx * dx + dy * dy <= (tr + 2) * (tr + 2) {
                        fb.put(tx + dx, ty + dy, air.theme.paper);
                    }
                }
            }
        }

        // Somebody on the escalator. The real one has lit tubes with
        // escalators in them, so every so often a bead of light travels up
        // one of the twenty, which turns a monument into a place where
        // somebody is.
        let t = air.now as f32;
        if self.bead.is_none() && t > self.bead_at {
            // Not the diagonals to the middle: those are the ones without an
            // escalator in them.
            let tube = self.rng.upto(12) as usize;
            self.bead = Some((tube, 0.0));
            self.bead_at = t + 7.0 + self.rng.unit() * 9.0;
        }
        if let Some((tube, along)) = self.bead.as_mut() {
            *along += 1.0 / 60.0 / 3.4;
            if *along > 1.0 {
                self.bead = None;
            } else {
                let (a, b) = ATOMIUM_TUBES[*tube];
                // Always upward: an escalator that runs down is a different
                // escalator.
                let (from, to) = if nodes[a].1 > nodes[b].1 {
                    (a, b)
                } else {
                    (b, a)
                };
                let k = *along;
                let x = nodes[from].0 as f32 + (nodes[to].0 - nodes[from].0) as f32 * k;
                let y = nodes[from].1 as f32 + (nodes[to].1 - nodes[from].1) as f32 * k;
                let warm = lerp_color(air.theme.yellow, air.theme.paper, 0.35);
                fb.put(x as i32, y as i32, warm);
                fb.put(x as i32, y as i32 - 1, scale(warm, 0.55));
                fb.put(x as i32 + 1, y as i32, scale(warm, 0.4));
            }
        }

        // The red lamp on the top sphere, for aircraft. On for a moment every
        // four seconds, which is what the real one does.
        if (air.now % 4.0) < 0.18 && has_light(nodes.len() - 1) {
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
        let base = if air.day {
            lerp_color(air.theme.bg, rgb(40, 44, 62), 0.55)
        } else {
            lerp_color(air.theme.bg, rgb(18, 20, 34), 0.7)
        };
        for b in &self.town {
            // A cloud over the sun throws its shade across the roofs, and the
            // shade travels with the cloud. This is the whole reason the sun
            // knows about clouds at all.
            let mid = (b.x + b.w / 2) as f32;
            let inside = 1.0 - ((mid - air.shadow.0).abs() / air.shadow.1.max(1.0)).clamp(0.0, 1.0);
            let shade = if air.day {
                air.cover * inside * 0.45
            } else {
                0.0
            };
            let body = lerp_color(base, air.theme.bg, shade);
            let edge = lerp_color(body, air.theme.paper, 0.12);
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
            // The bolt takes the neighbourhood with it. Each window has its
            // own moment to come back, decided by where it is rather than by
            // chance, so the street fills in the same order every time.
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
                let wait = 0.55 + ((b.x as usize * 7 + i * 13) % 22) as f32 * 0.075;
                if air.struck_since < wait {
                    continue;
                }
                let lamp = lerp_color(air.theme.yellow, air.theme.orange, 0.35);
                fb.rect(cx, cy, 2, 3, lerp_color(body, lamp, air.darkness));
            }
        }
    }

    /// The band under the horizon: dark, textured, and wet when it rains.
    fn ground(&mut self, fb: &mut Framebuffer, air: &Air) {
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
        // Puddles. Standing water is the only thing in this picture that
        // shows what is above it, which is worth having: at night the town's
        // windows and the Atomium's own colours end up in the ground.
        if matches!(air.kind, Kind::Rain | Kind::Heavy | Kind::Thunder) {
            self.puddles(fb, air);
        }
        // And the moon on the wet street: a band of it right under the
        // horizon, brightest where the moon is and only when there is a moon
        // to speak of.
        if matches!(
            air.kind,
            Kind::Rain | Kind::Heavy | Kind::Thunder | Kind::Snow
        ) && air.darkness > 0.4
            && air.moonlight > 0.15
        {
            let mx = air.sun.0 as i32;
            for y in air.horizon + 1..(air.horizon + 7).min(h) {
                let down = (y - air.horizon) as f32;
                for x in (mx - 40).max(0)..(mx + 40).min(w) {
                    let across = 1.0 - ((x - mx).abs() as f32 / 40.0);
                    let t = across * (1.0 - down / 7.0) * air.moonlight * air.darkness;
                    if BAYER[(y & 3) as usize][(x & 3) as usize] as f32 / 16.0 < t * 0.5 {
                        fb.put(x, y, lerp_color(fb.at(x, y), air.theme.paper, 0.25 * t));
                    }
                }
            }
        }
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

    /// The tram's overhead wire, and whoever is sitting on it.
    ///
    /// One line across the sky at the height of the roofs, which is a real
    /// thing in a tram city and the reason the birds have somewhere to be.
    /// Five of them, always the same five places, and they scatter when the
    /// lightning goes or when the tram passes under them and come back one
    /// at a time.
    fn wire(&mut self, fb: &mut Framebuffer, air: &Air) {
        let w = fb.w as i32;
        // High enough that the birds sit against the sky and not against a
        // roof, which is the difference between a bird and a smudge.
        let y = air.horizon - 32;
        let t = air.now as f32;
        if self.birds.is_empty() {
            for i in 0..5 {
                let at = 0.12 + i as f32 * 0.17;
                self.birds.push((at * w as f32, -1000.0));
            }
        }
        // The wire itself: it sags a little between the poles, because a wire
        // that does not sag is a ruler.
        let wire = lerp_color(air.theme.bg, air.theme.paper, 0.30);
        for x in 0..w {
            let span = (x % 106) as f32 / 106.0;
            let sag = ((span - 0.5) * 2.0).powi(2);
            fb.put(x, y + 2 - (sag * 2.0) as i32, wire);
        }
        // The poles, on the same spacing.
        for k in 0..=(w / 106) {
            let px = k * 106;
            fb.rect(px, y, 1, air.horizon - y, wire);
        }
        // Something to scatter for: a bolt of lightning, or the tram going by.
        let scare = self.flash > 0.0
            || self
                .tram
                .map(|tx| self.birds.iter().any(|(bx, _)| (tx - bx).abs() < 40.0))
                .unwrap_or(false);
        let body = lerp_color(
            air.theme.bg,
            air.theme.paper,
            if air.day { 0.34 } else { 0.20 },
        );
        for (i, (bx, up_at)) in self.birds.iter_mut().enumerate() {
            if scare && *up_at < t - 12.0 {
                // They do not all go at once, and they do not all come back
                // at once either.
                *up_at = t + i as f32 * 0.12;
            }
            let flown = t - *up_at;
            if flown > 0.0 && flown < 6.0 {
                // Up in an arc and back down onto the same spot.
                let k = flown / 6.0;
                let lift = ((k * std::f32::consts::PI).sin() * 26.0) as i32;
                let drift = (k * 18.0) as i32;
                let wing = if (t * 9.0 + i as f32).fract() < 0.5 {
                    1
                } else {
                    -1
                };
                let (px, py) = (*bx as i32 + drift, y - lift);
                fb.put(px, py, body);
                fb.put(px - 1, py - wing, body);
                fb.put(px + 1, py - wing, body);
            } else {
                // Sitting: a body, a head and a tail that flicks.
                // Four pixels: the body on the wire, the head up, and a tail
                // that flicks.
                let flick = i32::from(((t * 0.7 + i as f32 * 1.9).fract()) < 0.06);
                fb.put(*bx as i32, y, body);
                fb.put(*bx as i32 + 1, y, body);
                fb.put(*bx as i32 + 1, y - 1, body);
                fb.put(*bx as i32 - 1, y - flick, body);
            }
        }
    }

    /// The tram, along the street at the foot of the town.
    ///
    /// Brussels is a tram city, so one goes by every couple of minutes: a
    /// silhouette with its windows lit, a pantograph up on the wire, and at
    /// night the windows throw their light along the pavement as it passes.
    /// It takes ten seconds to cross, which is the pace of a thing that
    /// stops at every corner.
    fn tram(&mut self, fb: &mut Framebuffer, air: &Air) {
        let w = fb.w as f32;
        let t = air.now as f32;
        if self.tram.is_none() {
            if t < self.tram_at {
                return;
            }
            self.tram = Some(-40.0);
            self.tram_at = t + 70.0 + self.rng.unit() * 70.0;
        }
        let Some(x) = self.tram.as_mut() else {
            return;
        };
        *x += 32.0 / 60.0;
        let x = *x;
        if x > w + 40.0 {
            self.tram = None;
            return;
        }
        let gx = x as i32;
        let base = air.horizon - 1;
        let top = base - 9;
        let body = lerp_color(
            air.theme.bg,
            air.theme.paper,
            if air.day { 0.26 } else { 0.14 },
        );
        let roof = lerp_color(body, air.theme.paper, 0.18);
        // Thirty four pixels of tram: a body, a roof, the pantograph up to
        // the wire, two wheels and six windows.
        fb.rect(gx, top, 34, 9, body);
        fb.rect(gx, top, 34, 1, roof);
        fb.rect(gx + 2, base, 3, 1, roof);
        fb.rect(gx + 28, base, 3, 1, roof);
        // The pantograph, folded like a Z, up to the wire above.
        let wire_y = air.horizon - 30;
        fb.line(gx + 12, top, gx + 18, wire_y + 2, roof);
        fb.line(gx + 18, wire_y + 2, gx + 24, top, roof);
        let glow = lerp_color(air.theme.yellow, air.theme.orange, 0.25);
        // A spark at the wire, now and then, which is the one thing everybody
        // remembers about trams.
        if (t * 0.9 + x * 0.02).fract() < 0.02 {
            fb.put(gx + 18, wire_y + 1, air.theme.paper);
            fb.put(gx + 17, wire_y, glow);
            fb.put(gx + 19, wire_y, glow);
        }
        let lit = air.darkness > 0.2;
        for k in 0..6 {
            let wx = gx + 3 + k * 5;
            let c = if lit {
                lerp_color(glow, air.theme.paper, 0.25)
            } else {
                lerp_color(body, air.theme.paper, 0.22)
            };
            fb.rect(wx, top + 3, 3, 3, c);
        }
        // At night the windows lay their light on the pavement as it goes.
        if lit {
            let h = fb.h as i32;
            for y in air.horizon..(air.horizon + 5).min(h) {
                let down = (y - air.horizon) as f32;
                for dx in -6i32..40 {
                    let px = gx + dx;
                    let across = 1.0 - ((dx as f32 - 17.0).abs() / 24.0).clamp(0.0, 1.0);
                    let strength = across * (1.0 - down / 5.0) * air.darkness;
                    if BAYER[(y & 3) as usize][(px & 3) as usize] as f32 / 16.0 < strength * 0.6 {
                        fb.put(px, y, lerp_color(fb.at(px, y), glow, 0.3 * strength));
                    }
                }
            }
        }
    }

    /// The rain on the glass, in front of everything else.
    ///
    /// The page is a window, and this is the pane. A dozen drops cling to it,
    /// slide when they get heavy enough, and carry a lens with them: each one
    /// shows the picture from a little further down, magnified, the way a
    /// bead of water on glass does. They are the one thing here drawn in
    /// front of the weather rather than in it, and they never touch the band
    /// under the horizon, because the clock has to stay readable.
    fn pane(&mut self, fb: &mut Framebuffer, air: &Air) {
        if !matches!(air.kind, Kind::Rain | Kind::Heavy | Kind::Thunder) {
            self.pane.clear();
            return;
        }
        let w = fb.w as f32;
        let t = air.now as f32;
        let heavy = matches!(air.kind, Kind::Heavy | Kind::Thunder);
        if t > self.pane_at && self.pane.len() < if heavy { 16 } else { 9 } {
            self.pane_at = t + if heavy { 0.5 } else { 1.4 };
            let x = 6.0 + self.rng.unit() * (w - 12.0);
            let y = 12.0 + self.rng.unit() * (air.horizon as f32 - 40.0);
            let r = 2.0 + self.rng.unit() * 2.0;
            self.pane.push((x, y, r, 0.0));
        }
        let limit = air.horizon as f32 - 4.0;
        for (x, y, r, speed) in self.pane.iter_mut() {
            // A drop hangs until it has gathered enough of itself to go, and
            // then it accelerates and wanders a pixel as it goes.
            *speed += (0.06 + *r * 0.05) / 60.0;
            *y += *speed;
            *x += ((t * 1.3 + *y * 0.3).sin()) * 0.06;
            let (cx, cy, rr) = (*x, *y, *r);
            // The lens: what is a little below, brought up and magnified.
            let ri = rr as i32;
            for dy in -ri..=ri {
                for dx in -ri..=ri {
                    let d2 = (dx * dx + dy * dy) as f32;
                    if d2 > rr * rr {
                        continue;
                    }
                    let px = cx as i32 + dx;
                    let py = cy as i32 + dy;
                    if py < 2 || py > limit as i32 {
                        continue;
                    }
                    // Sampled from below and pulled in: a fisheye in four
                    // pixels of water.
                    let k = 1.0 - (d2.sqrt() / rr) * 0.45;
                    let sx = cx + dx as f32 * k;
                    let sy = cy + rr * 1.6 + dy as f32 * k;
                    let c = fb.at(sx as i32, sy as i32);
                    // Water on glass catches a little light of its own, and
                    // holds the dark at its rim: without those two a lens
                    // over a plain sky is invisible, which is true and no
                    // use.
                    let edge = d2.sqrt() / rr;
                    let c = if edge > 0.72 {
                        lerp_color(c, air.theme.bg, 0.30)
                    } else {
                        lerp_color(c, air.theme.paper, 0.12)
                    };
                    fb.put(px, py, c);
                }
            }
            // The highlight and the shadow that make it a bead and not a
            // hole: light comes from above, so the top is bright and the
            // bottom edge holds the dark.
            let hx = cx as i32 - (rr * 0.4) as i32;
            let hy = cy as i32 - (rr * 0.5) as i32;
            let spec = lerp_color(fb.at(cx as i32, cy as i32), air.theme.paper, 0.75);
            fb.put(hx, hy, spec);
            fb.put(hx + 1, hy, lerp_color(spec, air.theme.paper, 0.2));
            fb.put(
                cx as i32,
                cy as i32 + ri,
                lerp_color(fb.at(cx as i32, cy as i32 + ri), air.theme.bg, 0.35),
            );
            // The trail it leaves, which the next drop down the same track
            // will follow.
            if *speed > 0.02 {
                let mut back = 1.0;
                while back < 9.0 {
                    let ty = cy - back;
                    if ty > 2.0 {
                        let fade = 1.0 - back / 9.0;
                        if BAYER[(ty as i32 & 3) as usize][(cx as i32 & 3) as usize] as f32 / 16.0
                            < fade * 0.5
                        {
                            let c = fb.at(cx as i32, ty as i32);
                            fb.put(
                                cx as i32,
                                ty as i32,
                                lerp_color(c, air.theme.paper, 0.14 * fade),
                            );
                        }
                    }
                    back += 1.0;
                }
            }
        }
        self.pane.retain(|(_, y, _, _)| *y < limit);
    }

    /// The street lamps along the pavement, and the light they put on it.
    ///
    /// Four of them, always in the same places. They come on with the town's
    /// windows and go off with them, and they are the reason anybody can see
    /// the man walking home at two in the morning: he brightens as he passes
    /// through each pool of light and goes back to a shadow between them.
    fn lamps(&self, fb: &mut Framebuffer, air: &Air) {
        if air.darkness < 0.05 {
            return;
        }
        let w = fb.w as i32;
        let h = fb.h as i32;
        let glow = lerp_color(air.theme.yellow, air.theme.orange, 0.4);
        for at in LAMPS {
            let x = (at * w as f32) as i32;
            let top = air.horizon - 15;
            // The post, the arm and the lamp itself.
            fb.rect(
                x,
                top,
                1,
                15,
                lerp_color(air.theme.bg, air.theme.paper, 0.16),
            );
            fb.put(x + 1, top, lerp_color(air.theme.bg, air.theme.paper, 0.16));
            fb.put(x + 2, top, scale(glow, air.darkness));
            fb.put(x + 2, top + 1, scale(glow, 0.6 * air.darkness));
            // The pool of light on the pavement under it, dithered so it has
            // no edge, and a hint of it back up the post.
            let reach = 13.0;
            for y in air.horizon..(air.horizon + 7).min(h) {
                for dx in -13i32..=13 {
                    let px = x + 2 + dx;
                    let down = (y - air.horizon) as f32;
                    let d = ((dx * dx) as f32 + down * down * 5.0).sqrt();
                    if d > reach {
                        continue;
                    }
                    let t = (1.0 - d / reach) * air.darkness;
                    if BAYER[(y & 3) as usize][(px & 3) as usize] as f32 / 16.0 < t * 0.7 {
                        fb.put(px, y, lerp_color(fb.at(px, y), glow, 0.35 * t));
                    }
                }
            }
        }
    }

    /// Somebody walking home at the foot of the town, in whatever the sky is
    /// doing.
    ///
    /// Thirteen pixels of him, and he carries the picture because everything
    /// else in it is weather and he is somebody it is happening to. He is out
    /// in every sky but the overcast one, where nothing happens on purpose,
    /// and each sky gives him exactly one thing to do rather than a routine:
    /// the rain leans his umbrella and turns it inside out, the wind takes
    /// his hat when it is dry, the fog swallows him band by band, the snow
    /// keeps his footprints, the lightning stops him where he stands, and on
    /// a clear night he stops under the Atomium to watch the lights.
    ///
    /// He never faces the front and never acknowledges anybody watching. One
    /// crossing takes about a minute and the next is a couple of minutes off,
    /// so he stays a coincidence rather than a mascot.
    fn walker(&mut self, fb: &mut Framebuffer, air: &Air, landmark: bool) {
        let w = fb.w as f32;
        if air.kind == Kind::Overcast {
            self.walker = None;
            self.prints.clear();
            return;
        }
        let t = air.now as f32;
        if self.walker.is_none() {
            if t < self.walker_at {
                return;
            }
            let right = self.rng.upto(2) == 0;
            self.walker = Some(Walker {
                x: if right { -12.0 } else { w + 12.0 },
                dir: if right { 1.0 } else { -1.0 },
                step: 0.0,
                fighting: 0.0,
                pause_at: t + 5.0 + self.rng.unit() * 9.0,
                // One walk in four has the dog out.
                dog: self.rng.upto(4) == 0,
                looking: 0.0,
                hat_x: 0.0,
                hat_until: 0.0,
            });
            self.walker_at = t + 50.0 + self.rng.unit() * 70.0;
        }

        let wet = matches!(air.kind, Kind::Rain | Kind::Heavy | Kind::Thunder);
        let open = wet || air.kind == Kind::Snow;
        let dry = !wet && air.kind != Kind::Snow;
        // The same gust the rain leans on and the clouds drift with.
        let gust = 1.0
            + 0.30 * (t * std::f32::consts::TAU * 0.09).sin()
            + 0.14 * (t * std::f32::consts::TAU * 0.23).sin();
        let squall = matches!(air.kind, Kind::Heavy | Kind::Thunder);
        let flip_now = gust > 1.36 && squall && self.rng.upto(360) == 0;
        // The wind takes a hat on a dry day, which is the same joke as the
        // umbrella and never happens in the same weather as it.
        let hat_now = dry && air.wind > 16.0 && gust > 1.30 && self.rng.upto(300) == 0;
        // What there is to look up at: a bolt that just landed, the plane
        // going over, or the Atomium putting on its show.
        let struck = air.struck_since < 2.6;
        let plane_over = self
            .plane
            .as_ref()
            .filter(|_| air.day)
            .map(|pl| pl.x)
            .unwrap_or(-999.0);
        let show_x = if landmark && air.darkness > 0.5 {
            atomium_nodes(fb.w as i32, air.horizon)[0].0 as f32
        } else {
            -999.0
        };

        let mut splash: Option<f32> = None;
        let mut print_here: Option<f32> = None;
        let Some(one) = self.walker.as_mut() else {
            return;
        };
        if flip_now && one.fighting < t {
            one.fighting = t + 2.4;
        }
        if hat_now && one.hat_until < t {
            one.hat_until = t + 2.8;
            one.hat_x = one.x;
        }
        // Reasons to stand still and look up, in the order they matter.
        if struck {
            one.looking = t + 1.2;
        } else if (plane_over - one.x).abs() < 26.0 && self.rng.upto(30) == 0 {
            one.looking = t + 1.6;
        } else if (show_x - one.x).abs() < 14.0 && one.looking < t - 8.0 {
            one.looking = t + 3.2;
        }

        const DWELL: f32 = 1.6;
        let dwelling = t >= one.pause_at && t < one.pause_at + DWELL;
        if t > one.pause_at + DWELL {
            one.pause_at = t + 9.0 + (t * 3.0).fract() * 11.0;
        }
        let chasing = t < one.hat_until;
        let looking = t < one.looking;
        let fighting = t < one.fighting;
        let stopped = (dwelling || looking || fighting) && !chasing;

        if !stopped {
            // Chasing a hat is quicker than walking home; snow is slower
            // than either.
            let mut pace = 7.0 * (1.0 + 0.12 * (t * 1.7).sin());
            if chasing {
                pace *= 1.8;
            }
            if air.kind == Kind::Snow {
                pace *= 0.72;
            }
            one.x += one.dir * pace / 60.0;
            let before = one.step;
            one.step += pace / 60.0 / 7.0;
            if before.fract() > 0.5 && one.step.fract() <= 0.5 {
                if wet {
                    splash = Some(one.x);
                }
                if air.kind == Kind::Snow {
                    print_here = Some(one.x);
                }
            }
        }

        let (x, dir, step) = (one.x, one.dir, one.step);
        let dog = one.dog;
        let hat_x = one.hat_x;
        let hat_until = one.hat_until;
        if (dir > 0.0 && x > w + 26.0) || (dir < 0.0 && x < -26.0) {
            self.walker = None;
            return;
        }
        if let Some(sx) = splash {
            self.ripples.push((sx, air.horizon as f32 + 1.0, 0.0));
        }
        if let Some(px) = print_here {
            self.prints.push((px, 0.0));
        }
        // Footprints: the snow closes them in about ten seconds.
        if air.kind == Kind::Snow {
            for (_, age) in self.prints.iter_mut() {
                *age += 1.0 / 60.0;
            }
            self.prints.retain(|(_, age)| *age < 10.0);
            for (px, age) in &self.prints {
                let fade = 1.0 - age / 10.0;
                let c = lerp_color(
                    air.theme.paper,
                    lerp_color(air.theme.bg, air.theme.paper, 0.18),
                    1.0 - fade,
                );
                fb.put(*px as i32, air.horizon + 1, scale(c, 0.45 + fade * 0.3));
                fb.put(*px as i32 + 1, air.horizon + 1, scale(c, 0.3 + fade * 0.3));
            }
        } else {
            self.prints.clear();
        }

        // --------------------------------------------------------- the man
        let gx = x as i32;
        let bob = i32::from(!stopped && step.fract() >= 0.5);
        let feet = air.horizon - bob;
        // How close he is to a street lamp: under one he is lit, between two
        // he is a shadow, which is the whole of what a night street looks
        // like.
        let lamp_near = LAMPS
            .iter()
            .map(|at| (at * w - x).abs())
            .fold(f32::MAX, f32::min);
        let lit = (1.0 - lamp_near / 22.0).clamp(0.0, 1.0) * air.darkness;
        let coat = lerp_color(
            air.theme.bg,
            air.theme.paper,
            if air.day { 0.30 } else { 0.16 + 0.34 * lit },
        );
        let dark = lerp_color(coat, air.theme.bg, 0.45);
        let d = dir as i32;
        let lean = if chasing { d } else { 0 };

        let spread = if stopped {
            0
        } else if step.fract() < 0.5 {
            1
        } else {
            -1
        };
        fb.rect(gx - 1, feet - 3, 1, 3, dark);
        fb.rect(gx + spread, feet - 3, 1, 3, dark);
        fb.rect(gx - 1, feet - 8, 3, 5, coat);
        // The head: down when he is walking, up when there is something to
        // look at, and forward when he is chasing a hat.
        let head_up = looking;
        let head_x = gx - 1 + if head_up { 0 } else { d } + lean;
        let head_y = if head_up { feet - 11 } else { feet - 10 };
        let mut face = lerp_color(coat, air.theme.paper, 0.22);
        if head_up && show_x > -900.0 && (show_x - x).abs() < 20.0 {
            // The colour of the show, on the one man watching it.
            face = lerp_color(face, air.theme.magenta, 0.35);
        }
        fb.rect(head_x, head_y, 2, 2, face);

        // ----------------------------------------------------- the umbrella
        let canopy = lerp_color(air.theme.yellow, air.theme.orange, 0.25);
        if open {
            let hand = (gx + d, feet - 9);
            fb.put(hand.0, hand.1, coat);
            let tilt = (-dir * (1.4 + (gust - 1.0) * 3.4)) as i32;
            let cap_y = feet - 14;
            let cap_x = gx + tilt;
            fb.rect(hand.0, cap_y + 2, 1, feet - 9 - (cap_y + 2), dark);
            if fighting {
                for k in -4i32..=4 {
                    let lift = 2 - (4 - k.abs()) / 2;
                    fb.put(cap_x + k, cap_y + lift, canopy);
                }
                fb.put(cap_x - 4, cap_y - 1, canopy);
                fb.put(cap_x + 4, cap_y - 1, canopy);
            } else {
                const DOME: [i32; 9] = [2, 1, 1, 0, 0, 0, 1, 1, 2];
                for (n, drop) in DOME.iter().enumerate() {
                    let k = n as i32 - 4;
                    fb.put(cap_x + k, cap_y + drop, canopy);
                }
                fb.put(cap_x - 5, cap_y + 3, lerp_color(canopy, air.theme.bg, 0.25));
                fb.put(cap_x + 5, cap_y + 3, lerp_color(canopy, air.theme.bg, 0.25));
                if wet {
                    // A pixel of rain bouncing off the edge, one side then
                    // the other.
                    if (t * 9.0).fract() < 0.5 {
                        fb.put(cap_x - 6, cap_y + 4, air.theme.paper);
                    } else {
                        fb.put(cap_x + 6, cap_y + 4, air.theme.paper);
                    }
                } else {
                    // Snow settles on it, and he shakes it off every so
                    // often.
                    let settled = (t * 0.12).fract();
                    if settled > 0.12 {
                        for k in -3i32..=3 {
                            fb.put(cap_x + k, cap_y - 1 + (k.abs() + 1) / 3, air.theme.paper);
                        }
                    }
                }
            }
        } else {
            // Closed, under the arm, because he does not trust the sky.
            fb.rect(gx + d, feet - 7, 1, 4, canopy);
            fb.put(gx + d, feet - 8, dark);
        }

        // --------------------------------------------------------- the hat
        if dry {
            if chasing {
                // Off downwind, turning over as it goes, and he is after it.
                // How long it has been gone: it left 2.8 seconds before it
                // is due back.
                let away = (t - (hat_until - 2.8)).max(0.0);
                let hx = hat_x + dir * away * 26.0;
                let hy = feet as f32 - 12.0 + (away * 6.0).sin() * 3.0;
                fb.rect(hx as i32 - 1, hy as i32, 3, 1, dark);
                fb.put(hx as i32, hy as i32 - 1, dark);
            } else {
                fb.rect(head_x, head_y - 1, 2, 1, dark);
                fb.put(head_x - 1 + d, head_y - 1, dark);
            }
        }

        // --------------------------------------------------------- the dog
        if dog {
            let lead = gx + d * 7;
            let trot = if stopped {
                0
            } else {
                i32::from(step.fract() >= 0.5)
            };
            // Four pixels of dog: body, head, and legs that alternate.
            fb.rect(lead - 1, feet - 3, 3, 2, dark);
            fb.put(lead + d * 2, feet - 4, dark);
            fb.put(lead - 1, feet - 1 + trot, dark);
            fb.put(lead + 1, feet - trot, dark);
            // The lead, from his hand down to the collar.
            fb.line(gx + d, feet - 7, lead + d, feet - 4, scale(coat, 0.7));
        }

        // ------------------------------------------------ the fog swallows him
        if air.kind == Kind::Fog {
            let veil = fog_veil(gx, feet - 14, air);
            if veil > 0.02 {
                for y in feet - 15..=feet {
                    for dx in -7i32..=7 {
                        let px = gx + dx;
                        let c = fb.at(px, y);
                        if c == air.theme.bg {
                            continue;
                        }
                        fb.put(px, y, lerp_color(c, air.theme.paper, veil * 0.62));
                    }
                }
            }
        }

        // ----------------------------------------- and the wet pavement of him
        if wet {
            for dy in 1..5 {
                let src = feet - dy * 3;
                let wob = ((t * 2.3 + dy as f32).sin() * 1.3) as i32;
                for dx in -2i32..=2 {
                    let c = fb.at(gx + dx + wob, src);
                    if c == air.theme.bg {
                        continue;
                    }
                    let y = air.horizon + dy;
                    if BAYER[(y & 3) as usize][((gx + dx) & 3) as usize] as f32 / 16.0 < 0.55 {
                        fb.put(gx + dx, y, lerp_color(fb.at(gx + dx, y), c, 0.5));
                    }
                }
            }
        }
    }

    /// The puddles: what is above the horizon, upside down and squashed, with
    /// rings where the rain lands in them.
    ///
    /// The reflection is compressed two to one because the water is a floor
    /// and not a mirror on a wall, and it wobbles by a pixel or so, which is
    /// what stops it reading as a second picture.
    fn puddles(&mut self, fb: &mut Framebuffer, air: &Air) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let t = air.now as f32;
        // Where the water stands. Fixed, because a puddle that moves is a
        // river, and these are the same four every time it rains.
        // Only a few pixels of ground show before the clock is written over
        // it, so the water is a thin strip: five pixels deep at the most.
        let pools: [(f32, i32, i32); 4] =
            [(0.05, 52, 8), (0.30, 38, 7), (0.55, 58, 8), (0.80, 34, 7)];
        for (at, width, depth) in pools {
            let x0 = (at * w as f32) as i32;
            for y in air.horizon + 1..(air.horizon + depth).min(h) {
                let down = y - air.horizon;
                for x in x0..(x0 + width).min(w) {
                    // The edge of a puddle is shallow, so it reflects less.
                    let across = 1.0 - ((x - x0) as f32 / width as f32 * 2.0 - 1.0).abs().powf(2.5);
                    if across <= 0.05 {
                        continue;
                    }
                    let wobble = ((t * 1.7 + y as f32 * 0.9 + x as f32 * 0.12).sin() * 1.2) as i32;
                    // Squashed hard, because this is a puddle at your feet
                    // looking at a city a mile off: the whole skyline, the
                    // Atomium included, ends up in four pixels of water.
                    let src = air.horizon - down * 8;
                    if src < 0 {
                        continue;
                    }
                    let mirror = fb.at(x + wobble, src);
                    let strength = across * 0.8 * (1.0 - down as f32 / depth as f32 * 0.4);
                    if BAYER[(y & 3) as usize][(x & 3) as usize] as f32 / 16.0 < strength {
                        fb.put(x, y, lerp_color(fb.at(x, y), mirror, 0.75));
                    }
                }
            }
        }
        // A drop lands in one of them every so often, and rings out.
        let heavy = matches!(air.kind, Kind::Heavy | Kind::Thunder);
        if t > self.ripple_at {
            self.ripple_at = t + if heavy { 0.09 } else { 0.22 };
            let (at, width, depth) = pools[self.rng.upto(pools.len() as u32) as usize];
            let x = (at * w as f32) as i32 + self.rng.upto(width as u32) as i32;
            let y = air.horizon + 1 + self.rng.upto(depth as u32) as i32;
            self.ripples.push((x as f32, y as f32, 0.0));
        }
        self.ripples.retain(|(_, _, age)| *age < 0.7);
        for (x, y, age) in self.ripples.iter_mut() {
            *age += 1.0 / 60.0;
            let r = 1.0 + *age * 9.0;
            let fade = 1.0 - *age / 0.7;
            let c = lerp_color(fb.at(*x as i32, *y as i32), air.theme.paper, fade * 0.5);
            for k in 0..12 {
                let a = std::f32::consts::TAU * k as f32 / 12.0;
                // Flatter than it is wide: a ring seen from this angle.
                let px = *x + a.cos() * r;
                let py = *y + a.sin() * r * 0.35;
                if py > air.horizon as f32 && py < h as f32 {
                    fb.put(px as i32, py as i32, c);
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
        // How much is falling right now: the two skies' amounts, mixed. A
        // shower starts with a few drops and thins out the same way.
        let want = {
            let a = fall_of(air.from).0 as f32;
            let b = fall_of(air.to).0 as f32;
            (a + (b - a) * air.blend) as usize
        };
        if want == 0 {
            return;
        }
        let dt = 1.0 / 60.0;
        let w = fb.w as f32;
        let snow = air.kind == Kind::Snow;
        // Rain leans with the wind, and the wind is not a constant: the same
        // two swells the breeze is made of pass through the rain as squalls,
        // so it leans further and then eases off instead of falling at one
        // angle for ever.
        let t = air.now as f32;
        let gust = 1.0
            + 0.30 * (t * std::f32::consts::TAU * 0.09).sin()
            + 0.14 * (t * std::f32::consts::TAU * 0.23).sin();
        let slant = (air.wind * gust / 12.0).clamp(0.0, 3.6);
        let colour = if snow {
            air.theme.paper
        } else {
            lerp_color(air.theme.cyan, air.theme.paper, 0.45)
        };
        let mut landed: Vec<(f32, f32)> = Vec::new();
        for m in self.motes.iter_mut().take(want) {
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
    /// The storm. `landmark` says whether the Atomium is in the picture,
    /// because one bolt in four goes for it when it is.
    fn lightning(&mut self, fb: &mut Framebuffer, air: &Air, landmark: bool) {
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
            self.bolt.clear();
            // The tallest thing for miles, with a lightning rod in every
            // tube: one bolt in four goes for the Atomium rather than for
            // the ground, and takes its lights out when it lands.
            let hit = landmark && self.rng.upto(4) == 0;
            if hit {
                let nodes = atomium_nodes(w, air.horizon);
                let (tx, ty, r, _) = nodes[nodes.len() - 1];
                let sx = tx + self.rng.upto(41) as i32 - 20;
                let sy = 14 + self.rng.upto(10) as i32;
                // Straight enough to be aimed, crooked enough to be
                // lightning, and it ends on the sphere rather than near it.
                let steps = 9;
                for k in 0..=steps {
                    let f = k as f32 / steps as f32;
                    let x = sx as f32 + (tx - sx) as f32 * f;
                    let y = sy as f32 + (ty - r - sy) as f32 * f;
                    let wander = if k == steps {
                        0
                    } else {
                        self.rng.upto(9) as i32 - 4
                    };
                    self.bolt.push((x as i32 + wander, y as i32));
                }
                self.struck = t;
            } else {
                // A bolt down from a cloud, wandering as it goes.
                let mut x = 30 + self.rng.upto((w as u32).saturating_sub(60)) as i32;
                let mut y = 16 + self.rng.upto(24) as i32;
                while y < air.horizon {
                    self.bolt.push((x, y));
                    y += 2 + self.rng.upto(4) as i32;
                    x += self.rng.upto(9) as i32 - 4;
                }
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
    fn weather_arrives_over_twenty_five_seconds() {
        let mut a = Arrival::default();
        // Nothing to do: the picture already shows what the server says.
        assert_eq!(a.update(Kind::Clear, 0.0), (Kind::Clear, Kind::Clear, 1.0));
        // Rain is announced. It comes in rather than landing.
        let (from, to, blend) = a.update(Kind::Rain, 10.0);
        assert_eq!((from, to), (Kind::Clear, Kind::Rain));
        assert_eq!(blend, 0.0);
        let (_, _, blend) = a.update(Kind::Rain, 10.0 + ARRIVES * 0.5);
        assert!((blend - 0.5).abs() < 0.01, "{blend}");
        // And when it has arrived it is simply the weather.
        let done = a.update(Kind::Rain, 10.0 + ARRIVES + 1.0);
        assert_eq!(done, (Kind::Rain, Kind::Rain, 1.0));
    }

    #[test]
    fn a_change_part_way_through_a_change_starts_from_the_picture() {
        let mut a = Arrival::default();
        a.update(Kind::Heavy, 0.0);
        // A third of the way into the downpour, the server says fog. The
        // downpour is not what the picture is showing yet, so the fog comes
        // from where it actually is.
        let (from, to, blend) = a.update(Kind::Fog, ARRIVES / 3.0);
        assert_eq!((from, to), (Kind::Clear, Kind::Fog));
        assert_eq!(blend, 0.0);
        // Past halfway it is the other way round: the downpour is what is on
        // the screen, so that is what the next change leaves behind.
        let mut b = Arrival::default();
        b.update(Kind::Heavy, 0.0);
        let (from, to, _) = b.update(Kind::Fog, ARRIVES * 0.8);
        assert_eq!((from, to), (Kind::Heavy, Kind::Fog));
    }

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
