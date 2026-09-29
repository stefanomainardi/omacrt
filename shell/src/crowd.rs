//! The people in the arcade hall.
//!
//! A while after the home is up somebody comes in from behind the cabinets
//! at the back, walks to one and plays; now and then something happens (a
//! game lost, a record, the man with the mop). One scene at a time, with
//! the hall quiet in between: it is the room the menu stands in, not a show
//! in front of it.
//!
//! A figure is a skeleton, hips, knees, ankles, shoulders, elbows and a
//! head, posed by a few angles and drawn as capsules at the size its depth
//! gives it, then outlined; a sprite shrunk by a fraction is mush, a figure
//! built at its own size keeps its one pixel edges. The walk is a cycle
//! driven by the distance covered, so the feet do not slide, and the hips
//! ride so the foot in stance stays on the floor. Each pixel is tested
//! against the hall's depth, so a cabinet nearer than someone hides them.

use crate::fb::{Color, Framebuffer, lerp_color};
use crate::hall::{FLOOR, Fx, Hall};
use crate::people;

/// A person's height in the hall's units; a cabinet is 1.4.
const PERSON: f32 = 1.32;
/// Seconds of an empty hall after the home is up.
const FIRST: f64 = 8.0;
/// One scene every this many seconds, with the hall quiet between them:
/// all four come round in two and a half minutes, well inside the five a
/// screensaver waits.
const CYCLE: f64 = 36.0;
/// The dark round the words over a cabinet.
const OUTLINE: Color = 0x08060e;
/// How many poses a second a figure is drawn at. Its place on the floor is
/// worked out at the same instants as its pose, so the foot that carries the
/// weight stays where it was put between them.
const POSES: f32 = 15.0;

#[derive(Clone, Copy)]
struct Look {
    tall: f32,
    hair: Color,
    skin: Color,
    top: Color,
    legs: Color,
    shoes: Color,
    shirt: Color,
    mullet: bool,
    cap: Option<Color>,
    /// Which person this is, for the cache of their sprites.
    id: u8,
}

impl Look {
    fn palette(&self) -> people::Palette {
        people::Palette {
            skin: self.skin,
            hair: self.hair,
            top: self.top,
            shirt: self.shirt,
            legs: self.legs,
            shoes: self.shoes,
            mullet: self.mullet,
            cap: self.cap,
        }
    }
}

// The colours are the bases the sprites shade from, muted and a little warm
// in the skin, as Fate of Atlantis coloured its people.
const DENIM: Look = Look {
    tall: 1.0,
    hair: 0x684026,
    skin: 0xcc8058,
    top: 0x3e64aa,
    legs: 0x42465c,
    shoes: 0xd6d6d4,
    shirt: 0xd2d0c4,
    mullet: false,
    cap: None,
    id: 0,
};
const MULLET: Look = Look {
    tall: 0.97,
    hair: 0xbc9048,
    skin: 0xd28860,
    top: 0xa82c32,
    legs: 0x34343c,
    shoes: 0x463c38,
    shirt: 0x2c2a32,
    mullet: true,
    cap: None,
    id: 1,
};
const JANITOR: Look = Look {
    tall: 1.0,
    hair: 0x463c38,
    skin: 0xc47c56,
    top: 0x5c686c,
    legs: 0x505c60,
    shoes: 0x2c2826,
    shirt: 0x969c96,
    mullet: false,
    cap: Some(0x2c4a3c),
    id: 2,
};

#[derive(Clone, Copy, PartialEq, Debug)]
enum Facing {
    Away,
    Toward,
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Act {
    Play,
    Over,
    Cheer,
    /// Standing with the mop's head on the floor, before and after mopping.
    Rest,
}

#[derive(Clone)]
enum Leg {
    /// Along a path of points on the floor, easing in at the start and out
    /// at the end.
    Walk {
        t0: f32,
        t1: f32,
        path: Vec<(f32, f32)>,
        tool: Tool,
    },
    Stay {
        t0: f32,
        t1: f32,
        at: (f32, f32),
        facing: Facing,
        act: Act,
        cab: Option<usize>,
    },
}

/// What somebody has in their hands as they walk.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Tool {
    None,
    /// The mop carried, its head off the floor.
    Carry,
    /// The mop on the floor, going to and fro.
    Mop,
}

/// How fast people walk, in the hall's units a second, and the man with
/// the mop.
const MOPPING: f32 = 0.16;
/// Mopping, a step to the next patch of floor every so many seconds, and
/// how long the step takes.
const MOP_EVERY: f32 = 1.6;
const MOP_STEP: f32 = 0.5;

// The floor plan. A door at the far end of each side wall; beside it the
// gap between the end of the row and the cabinets at the back, which is the
// way in; the lane along the front of the back cabinets, in front of whoever
// is playing them; and where people stand to play.
const DOOR_R: [(f32, f32); 3] = [(1.34, 3.13), (0.86, 3.13), (0.62, 2.965)];
const LANE: f32 = 2.76;
const AT_BACK_L: (f32, f32) = (-0.44, 2.92);
const AT_BACK_R: (f32, f32) = (0.44, 2.92);
const AT_SIDE_R: (f32, f32) = (0.47, 2.71);
const CAB_BACK_L: usize = 8;
const CAB_BACK_R: usize = 9;
const CAB_SIDE_R: usize = 7;

fn door(right: bool) -> Vec<(f32, f32)> {
    let s = if right { 1.0 } else { -1.0 };
    DOOR_R.iter().map(|&(x, d)| (s * x, d)).collect()
}

/// A scene as it is written: one step after another, each starting where
/// the last one ended, a walk taking as long as its length asks.
struct Plan {
    t: f32,
    legs: Vec<Leg>,
}

impl Plan {
    fn at(t: f32) -> Self {
        Self {
            t,
            legs: Vec::new(),
        }
    }

    fn walk(self, path: Vec<(f32, f32)>) -> Self {
        self.go(path, Tool::None)
    }

    fn carry(self, path: Vec<(f32, f32)>) -> Self {
        self.go(path, Tool::Carry)
    }

    fn mop(self, path: Vec<(f32, f32)>) -> Self {
        self.go(path, Tool::Mop)
    }

    fn go(mut self, path: Vec<(f32, f32)>, tool: Tool) -> Self {
        let length: f32 = path.windows(2).map(|w| dist(w[0], w[1])).sum();
        let speed = if tool == Tool::Mop {
            MOPPING
        } else {
            stride(1.0) * CADENCE
        };
        let secs = length / speed + 0.5;
        self.legs.push(Leg::Walk {
            t0: self.t,
            t1: self.t + secs,
            path,
            tool,
        });
        self.t += secs;
        self
    }

    fn stay(
        mut self,
        secs: f32,
        at: (f32, f32),
        facing: Facing,
        act: Act,
        cab: Option<usize>,
    ) -> Self {
        self.legs.push(Leg::Stay {
            t0: self.t,
            t1: self.t + secs,
            at,
            facing,
            act,
            cab,
        });
        self.t += secs;
        self
    }
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt()
}

fn joined(mut a: Vec<(f32, f32)>, b: &[(f32, f32)]) -> Vec<(f32, f32)> {
    a.extend_from_slice(b);
    a
}

fn back(mut a: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    a.reverse();
    a
}

/// In through a door, to a place to play, and the way back out: nobody cuts
/// across a cabinet or walks between a player and their game.
fn to_back(right_door: bool, at: (f32, f32)) -> Vec<(f32, f32)> {
    let s = if right_door { 1.0 } else { -1.0 };
    joined(door(right_door), &[(s * 0.40, LANE), (at.0, LANE), at])
}

fn to_side_r(right_door: bool) -> Vec<(f32, f32)> {
    let s = if right_door { 1.0 } else { -1.0 };
    joined(
        door(right_door),
        &[(s * 0.40, LANE), (0.28, AT_SIDE_R.1), AT_SIDE_R],
    )
}

/// The scenes, one per cycle in turn: a game lost; two people playing side
/// by side; a record; the mop.
fn scene(n: u64) -> Vec<(Look, Vec<Leg>)> {
    match n % 4 {
        0 => {
            let path = to_back(true, AT_BACK_L);
            let p = Plan::at(0.0)
                .walk(path.clone())
                .stay(8.8, AT_BACK_L, Facing::Away, Act::Play, Some(CAB_BACK_L))
                .stay(1.4, AT_BACK_L, Facing::Away, Act::Over, Some(CAB_BACK_L))
                .walk(back(path));
            vec![(DENIM, p.legs)]
        }
        1 => {
            let a = to_back(true, AT_BACK_R);
            let b = to_back(false, AT_BACK_L);
            let one = Plan::at(0.0)
                .walk(a.clone())
                .stay(10.0, AT_BACK_R, Facing::Away, Act::Play, Some(CAB_BACK_R))
                .walk(back(a));
            let two = Plan::at(3.0)
                .walk(b.clone())
                .stay(9.0, AT_BACK_L, Facing::Away, Act::Play, Some(CAB_BACK_L))
                .walk(back(b));
            vec![(DENIM, one.legs), (MULLET, two.legs)]
        }
        2 => {
            let path = to_side_r(false);
            let p = Plan::at(0.0)
                .walk(path.clone())
                .stay(9.4, AT_SIDE_R, Facing::Right, Act::Play, Some(CAB_SIDE_R))
                .stay(1.8, AT_SIDE_R, Facing::Toward, Act::Cheer, Some(CAB_SIDE_R))
                .walk(back(path));
            vec![(MULLET, p.legs)]
        }
        _ => {
            // He comes in with the mop in his hand, walks along the back to
            // where he starts, puts it down, and mops his way across the
            // aisle; then he lifts it and carries it out by the other door.
            // The last step in ends across the aisle rather than toward the
            // camera, so he arrives in profile, the way he mops.
            let (from, to) = ((0.30, LANE), (-0.45, LANE));
            let p = Plan::at(0.0)
                .carry(joined(door(true), &[from]))
                .stay(0.9, from, Facing::Left, Act::Rest, None)
                .mop(vec![from, to])
                .stay(0.7, to, Facing::Left, Act::Rest, None)
                .carry(joined(vec![to], &back(door(false))));
            vec![(JANITOR, p.legs)]
        }
    }
}

/// Where somebody is and what they are doing, at a moment of their scene.
#[derive(Clone, Copy, Debug)]
struct Pose {
    x: f32,
    d: f32,
    facing: Facing,
    walk: Option<f32>,
    amount: f32,
    act: Option<Act>,
    since: f32,
    cab: Option<usize>,
    tool: Tool,
}

/// Strides a second. Two steps a second is an ordinary walk; the hall's
/// people used to take one, and looked as if they were wading.
const CADENCE: f32 = 0.95;

/// The ground a figure covers in one stride, in the hall's units, from the
/// joint curves the sprites are posed by: the foot on the floor moves back
/// under the body by exactly as much as the body moves on, or it slides.
fn stride(tall: f32) -> f32 {
    people::stride() * PERSON * tall
}

fn facing_of(dx: f32, dd: f32) -> Facing {
    if dx.abs() > dd.abs() * 0.8 {
        if dx > 0.0 {
            Facing::Right
        } else {
            Facing::Left
        }
    } else if dd > 0.0 {
        Facing::Away
    } else {
        Facing::Toward
    }
}

/// How long a figure that has stopped walking stands facing the way it
/// walked before it turns to what it came for.
const SETTLE: f32 = 0.25;

fn pose_at(legs: &[Leg], t: f32, tall: f32) -> Option<Pose> {
    for (n, leg) in legs.iter().enumerate() {
        match leg {
            Leg::Stay {
                t0,
                t1,
                at,
                facing,
                act,
                cab,
            } if (*t0..*t1).contains(&t) => {
                // Arriving, an actor in those games stops as he walked, feet
                // together, and only then turns: the walk does not end on a
                // pose facing somewhere else.
                let arrived = n
                    .checked_sub(1)
                    .and_then(|m| match &legs[m] {
                        Leg::Walk { path, .. } if path.len() > 1 => {
                            let (a, b) = (path[path.len() - 2], path[path.len() - 1]);
                            Some(facing_of(b.0 - a.0, b.1 - a.1))
                        }
                        _ => None,
                    })
                    .filter(|f| f != facing && t - t0 < SETTLE);
                return Some(Pose {
                    x: at.0,
                    d: at.1,
                    facing: arrived.unwrap_or(*facing),
                    walk: None,
                    amount: 0.0,
                    act: if arrived.is_some() { None } else { Some(*act) },
                    since: t - t0,
                    cab: *cab,
                    tool: if *act == Act::Rest {
                        Tool::Mop
                    } else {
                        Tool::None
                    },
                });
            }
            Leg::Walk { t0, t1, path, tool } if (*t0..*t1).contains(&t) => {
                let mop = &(*tool == Tool::Mop);
                let total: f32 = path.windows(2).map(|w| dist(w[0], w[1])).sum();
                let secs = t1 - t0;
                // Speed up over the first quarter second, slow down over
                // the last: in between, steady.
                let ramp = 0.25f32.min(secs / 2.0);
                let v = total / (secs - ramp);
                let e = t - t0;
                let s = if e < ramp {
                    v * e * e / (2.0 * ramp)
                } else if e > secs - ramp {
                    let r = secs - e;
                    total - v * r * r / (2.0 * ramp)
                } else {
                    v * ramp / 2.0 + v * (e - ramp)
                };
                let mut s = s.clamp(0.0, total);
                let stride = stride(tall) * if *mop { 0.5 } else { 1.0 };
                // Somebody mopping does not walk slowly: he stands and works
                // the mop to and fro, and every so often takes one ordinary
                // step to the next patch of floor. The ground is covered in
                // those steps, one every MOP_EVERY seconds, each taking
                // MOP_STEP of them, the legs making half a stride.
                let mut mop_leg = None;
                if *mop {
                    let n = (e / MOP_EVERY).floor();
                    let k = ((e - n * MOP_EVERY) / MOP_STEP).clamp(0.0, 1.0);
                    let eased = k * k * (3.0 - 2.0 * k);
                    let step = MOPPING * MOP_EVERY;
                    s = ((n + eased) * step).min(total);
                    let half = (n as i64).rem_euclid(2) as f32 * 0.5;
                    mop_leg = Some((half + 0.5 * k, (std::f32::consts::PI * k).sin()));
                }
                // Where on the path that is.
                let mut left = s;
                let (mut x, mut d, mut dir) = (path[0].0, path[0].1, (0.0, 0.0));
                for w in path.windows(2) {
                    let l = dist(w[0], w[1]);
                    dir = (w[1].0 - w[0].0, w[1].1 - w[0].1);
                    if left <= l || l == 0.0 {
                        let f = if l > 0.0 { left / l } else { 0.0 };
                        x = w[0].0 + dir.0 * f;
                        d = w[0].1 + dir.1 * f;
                        break;
                    }
                    left -= l;
                    x = w[1].0;
                    d = w[1].1;
                }
                let speed = if e < ramp {
                    e / ramp
                } else if e > secs - ramp {
                    (secs - e) / ramp
                } else {
                    1.0
                };
                return Some(Pose {
                    x,
                    d,
                    facing: facing_of(dir.0, dir.1),
                    walk: Some(mop_leg.map_or((s / stride).fract(), |m| m.0)),
                    amount: mop_leg.map_or(speed.clamp(0.0, 1.0), |m| m.1),
                    act: None,
                    since: e,
                    cab: None,
                    tool: *tool,
                });
            }
            _ => {}
        }
    }
    None
}

/// A 3 by 5 face for the words over a cabinet.
fn glyph(ch: char) -> u16 {
    match ch {
        'G' => 0b111_100_101_101_111,
        'A' => 0b010_101_111_101_101,
        'M' => 0b101_111_111_101_101,
        'E' => 0b111_100_110_100_111,
        'O' => 0b111_101_101_101_111,
        'V' => 0b101_101_101_101_010,
        'R' => 0b110_101_110_101_101,
        'N' => 0b110_101_101_101_101,
        'W' => 0b101_101_111_111_101,
        'C' => 0b111_100_100_100_111,
        'D' => 0b110_101_101_101_110,
        '!' => 0b010_010_010_000_010,
        _ => 0,
    }
}

fn label(fb: &mut Framebuffer, cx: i32, y: i32, text: &str, c: Color) {
    let w = text.chars().count() as i32 * 4 - 1;
    let x0 = cx - w / 2;
    let mut on = Vec::new();
    for (i, ch) in text.chars().enumerate() {
        let g = glyph(ch);
        for r in 0..5 {
            for k in 0..3 {
                if g & (1 << (14 - (r * 3 + k))) != 0 {
                    on.push((x0 + i as i32 * 4 + k, y + r));
                }
            }
        }
    }
    for &(x, y) in &on {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1)] {
            if !on.contains(&(x + dx, y + dy)) {
                fb.put(x + dx, y + dy, OUTLINE);
            }
        }
    }
    for &(x, y) in &on {
        fb.put(x, y, c);
    }
}

/// The people, kept between frames: when the home came up, and where the
/// mop has been.
#[derive(Default)]
pub struct Crowd {
    since: Option<f64>,
    wet: Vec<(f32, f32, f64)>,
    /// Sprites already drawn, by who and in what pose. A pose is held for
    /// four frames and a walk comes round every stride, so most are drawn
    /// once and read many times.
    sprites: std::collections::HashMap<SpriteKey, std::rc::Rc<people::Sprite>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct SpriteKey {
    id: u8,
    view: u8,
    act: u8,
    phase: u16,
    amount: u8,
    t: u16,
}

impl Crowd {
    /// The home is up from `now`: the hall's clock for its people starts.
    pub fn start(&mut self, now: f64) {
        if self.since.is_none() {
            self.since = Some(now);
        }
    }

    fn at(&self, now: f64) -> Option<(u64, f32)> {
        let c = now - self.since? - FIRST;
        (c >= 0.0).then(|| ((c / CYCLE) as u64, (c % CYCLE) as f32))
    }

    /// What the screens of the cabinets being played show besides their
    /// game, for the hall to draw before the people stand in front of it.
    pub fn fx(&self, now: f64) -> [Fx; 10] {
        let mut fx = [Fx::None; 10];
        let Some((n, t)) = self.at(now) else {
            return fx;
        };
        for (look, legs) in scene(n) {
            if let Some(p) = pose_at(&legs, t, look.tall)
                && let Some(cab) = p.cab
            {
                fx[cab] = match p.act {
                    Some(Act::Over) => Fx::Over,
                    Some(Act::Cheer) => Fx::Record,
                    _ => Fx::None,
                };
            }
        }
        fx
    }

    /// Draw whoever is in the hall, over the hall and under the menu.
    /// The sprite for a person in a pose, drawn once and kept.
    fn sprite(&mut self, look: &Look, pose: &Pose, now: f32) -> std::rc::Rc<people::Sprite> {
        let act = match (pose.act, pose.tool) {
            (Some(Act::Play), _) => people::Act::Play,
            (Some(Act::Cheer), _) => people::Act::Cheer,
            (Some(Act::Over), _) => people::Act::Over,
            (_, Tool::Mop) => people::Act::Mop,
            (_, Tool::Carry) => people::Act::Carry,
            _ => people::Act::Walk,
        };
        let view = match (act, pose.facing) {
            (people::Act::Cheer, _) => people::View::Toward,
            (_, Facing::Right) => people::View::Side(true),
            (_, Facing::Left) => people::View::Side(false),
            (_, Facing::Toward) => people::View::Toward,
            (_, Facing::Away) => people::View::Away,
        };
        // The small motions of playing, cheering and mopping run from when
        // the act began; a mop's sweep from the clock, since it walks.
        let t = match (act, pose.act) {
            // at rest the mop is still
            (people::Act::Mop, Some(Act::Rest)) => 0.0,
            (people::Act::Mop, _) => now,
            _ => pose.since,
        };
        let p = people::Pose {
            view,
            act,
            phase: pose.walk.unwrap_or(0.0),
            amount: pose.amount,
            t,
        };
        let key = SpriteKey {
            id: look.id,
            view: match view {
                people::View::Side(true) => 0,
                people::View::Side(false) => 1,
                people::View::Toward => 2,
                people::View::Away => 3,
            },
            act: act as u8,
            phase: (p.phase.rem_euclid(1.0) * 960.0) as u16,
            amount: (p.amount * 50.0) as u8,
            t: if matches!(act, people::Act::Walk | people::Act::Carry) {
                0
            } else {
                ((t * POSES) as u32 % 4096) as u16
            },
        };
        if self.sprites.len() > 600 {
            self.sprites.clear();
        }
        self.sprites
            .entry(key)
            .or_insert_with(|| std::rc::Rc::new(people::sprite(&look.palette(), &p)))
            .clone()
    }

    pub fn draw(&mut self, fb: &mut Framebuffer, hall: &Hall, fog: Color, now: f64) {
        let Some((n, t)) = self.at(now) else {
            return;
        };
        let unit = Hall::unit(fb);
        let tick = (now * crate::hall::TICK_HZ) as u32;
        // The mop's trail dries in five seconds.
        self.wet.retain(|w| now - w.2 < 5.0 && w.2 <= now);
        for &(x, d, at) in &self.wet {
            let (px, py) = Hall::project(fb, x, FLOOR, d);
            let age = ((now - at) / 5.0) as f32;
            for dx in -4..=4 {
                for dy in 0..2 {
                    let (xx, yy) = (px as i32 + dx, py as i32 + dy);
                    if hall.depth_at(xx, yy) >= d - 0.05 && dither(xx, yy) < 0.55 * (1.0 - age) {
                        let under = fb.at(xx, yy);
                        fb.put(xx, yy, lerp_color(under, 0x5a82aa, 0.35));
                    }
                }
            }
        }
        // People move at fifteen poses a second, as sprites in a game of the
        // period did, and hold each pose for four frames. Drawn afresh every
        // frame their edges crawl a fraction of a pixel at a time, which on
        // a figure forty pixels tall reads as a puppet rather than a drawing.
        let t = (t * POSES).floor() / POSES;
        let now = (now * POSES as f64).floor() / POSES as f64;
        let mut here: Vec<(Look, Pose)> = scene(n)
            .iter()
            .filter_map(|(look, legs)| pose_at(legs, t, look.tall).map(|p| (*look, p)))
            .collect();
        here.sort_by(|a, b| b.1.d.total_cmp(&a.1.d));
        for (look, pose) in &here {
            let hp = PERSON * look.tall * unit / pose.d;
            let sprite = self.sprite(look, pose, now as f32);
            let (fx, fy) = Hall::project(fb, pose.x, FLOOR, pose.d);
            let (fx, fy) = (fx.round() as i32, fy.round() as i32);
            // A shadow on the carpet.
            let sw = (hp * 0.15) as i32;
            for x in fx - sw..=fx + sw {
                for y in fy..fy + 2 {
                    if hall.depth_at(x, y) > pose.d && dither(x, y) < 0.6 {
                        let under = fb.at(x, y);
                        fb.put(x, y, lerp_color(under, 0x000000, 0.5));
                    }
                }
            }
            let rim = (pose.act == Some(Act::Play))
                .then(|| pose.cab.map(Hall::glow))
                .flatten();
            let haze = ((pose.d - 2.4) * 0.35).clamp(0.0, 0.3);
            // The sprite is drawn at fifty pixels and scaled to the height its
            // depth gives it, a pixel taken for each one it covers, as SCUMM
            // scaled its actors: the drawing stays the same drawing at every
            // distance.
            let scale = hp / people::HEIGHT as f32;
            let (tw, th) = (
                (people::W as f32 * scale).round() as i32,
                (people::H as f32 * scale).round() as i32,
            );
            let (ax, ay) = (
                (people::FOOT.0 as f32 * scale).round() as i32,
                (people::FOOT.1 as f32 * scale).round() as i32,
            );
            for ty in 0..th {
                let sy = ((ty as f32 + 0.5) / scale) as i32;
                for tx in 0..tw {
                    let sx = ((tx as f32 + 0.5) / scale) as i32;
                    let Some(mut c) = sprite.at(sx, sy) else {
                        continue;
                    };
                    // The screen he is playing lights the edges it can reach.
                    if let Some(r) = rim
                        && (sprite.at(sx + 1, sy).is_none()
                            || sprite.at(sx - 1, sy).is_none()
                            || sprite.at(sx, sy - 1).is_none())
                    {
                        c = lerp_color(c, r, 0.45);
                    }
                    let (x, y) = (fx + tx - ax, fy + ty - ay);
                    if hall.depth_at(x, y) > pose.d {
                        fb.put(x, y, lerp_color(c, fog, haze));
                    }
                }
            }
            if let Some(m) = sprite.mop
                && pose.walk.is_some()
            {
                self.wet
                    .push((pose.x + m.0 * scale * pose.d / unit, pose.d - 0.02, now));
            }
            // The rare moments' words, over the cabinet.
            if let Some(cab) = pose.cab {
                let (cx, cy) = Hall::cabinet_top(fb, cab);
                match pose.act {
                    Some(Act::Over) if (tick / 3).is_multiple_of(2) => {
                        label(fb, cx as i32, cy as i32 - 9, "GAME OVER", 0xe83848);
                    }
                    Some(Act::Cheer) => {
                        let c = [0xffe04a, 0xf4f0ff, 0xffb040][(tick / 2 % 3) as usize];
                        label(fb, cx as i32 - 8, cy as i32 - 9, "NEW RECORD!", c);
                        for j in 0..6 {
                            let a = tick as f32 * 0.35 + j as f32 * 1.05;
                            let rr = 6.0 + (tick % 6) as f32;
                            let (sx, sy) = (cx - 8.0 + a.cos() * rr * 2.0, cy - 7.0 + a.sin() * rr);
                            fb.put(
                                sx as i32,
                                sy as i32,
                                if j % 2 != 0 { 0xf4f0ff } else { 0xffe04a },
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn dither(x: i32, y: i32) -> f32 {
    (crate::paint::BAYER[(y & 3) as usize][(x & 3) as usize] as f32 + 0.5) / 16.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scene_starts_hidden_and_ends_hidden() {
        for n in 0..4 {
            for (_, legs) in scene(n) {
                // Nobody is in the hall once their scene is over.
                assert!(pose_at(&legs, CYCLE as f32 - 0.5, 1.0).is_none());
                // Every walk joins the next leg where it left off.
                for w in legs.windows(2) {
                    let end = match w[0] {
                        Leg::Walk { t1, .. } | Leg::Stay { t1, .. } => t1,
                    };
                    let start = match w[1] {
                        Leg::Walk { t0, .. } | Leg::Stay { t0, .. } => t0,
                    };
                    assert!((end - start).abs() < 1e-6, "scene {n} has a gap");
                }
            }
        }
    }
}
