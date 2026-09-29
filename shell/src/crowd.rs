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

/// A person's height in the hall's units; a cabinet is 1.4.
const PERSON: f32 = 1.32;
/// Seconds of an empty hall after the home is up.
const FIRST: f64 = 8.0;
/// One scene every this many seconds, with the hall quiet between them:
/// all four come round in two and a half minutes, well inside the five a
/// screensaver waits.
const CYCLE: f64 = 36.0;
const OUTLINE: Color = 0x08060e;

#[derive(Clone, Copy)]
struct Look {
    tall: f32,
    hair: Color,
    skin: Color,
    top: Color,
    legs: Color,
    shoes: Color,
    mullet: bool,
    cap: Option<Color>,
    stripe: Option<Color>,
}

const DENIM: Look = Look {
    tall: 1.0,
    hair: 0x5c3820,
    skin: 0xe8b28c,
    top: 0x3a5cbe,
    legs: 0x22284e,
    shoes: 0xececf0,
    mullet: false,
    cap: None,
    stripe: None,
};
const MULLET: Look = Look {
    tall: 0.97,
    hair: 0xe6c46e,
    skin: 0xf0be96,
    top: 0xc42c3c,
    legs: 0x1e1c24,
    shoes: 0x464650,
    mullet: true,
    cap: None,
    stripe: Some(0xf5f5f5),
};
const JANITOR: Look = Look {
    tall: 1.0,
    hair: 0x3c322c,
    skin: 0xd2a07c,
    top: 0x54707a,
    legs: 0x465e68,
    shoes: 0x1e1c22,
    mullet: false,
    cap: Some(0x284638),
    stripe: None,
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
}

#[derive(Clone, Copy)]
enum Leg {
    Walk {
        t0: f32,
        t1: f32,
        from: (f32, f32),
        to: (f32, f32),
        mop: bool,
    },
    Stay {
        t0: f32,
        t1: f32,
        at: (f32, f32),
        facing: Facing,
        act: Act,
        cab: usize,
    },
}

// Where people go: in front of the back left cabinet, at the right row's far
// cabinet, and the places behind the back cabinets they come from.
const BACK_L: (f32, f32) = (-0.44, 2.72);
const SIDE_R: (f32, f32) = (0.46, 2.77);
const HIDE_R: (f32, f32) = (0.44, 3.25);
const HIDE_L: (f32, f32) = (-0.44, 3.25);
const OUT_R: (f32, f32) = (0.70, 3.18);
const OUT_L: (f32, f32) = (-0.72, 3.18);
const CAB_BACK_L: usize = 8;
const CAB_SIDE_R: usize = 7;

fn walk(t0: f32, t1: f32, from: (f32, f32), to: (f32, f32)) -> Leg {
    Leg::Walk {
        t0,
        t1,
        from,
        to,
        mop: false,
    }
}

fn stay(t0: f32, t1: f32, at: (f32, f32), facing: Facing, act: Act, cab: usize) -> Leg {
    Leg::Stay {
        t0,
        t1,
        at,
        facing,
        act,
        cab,
    }
}

/// The scenes, one per cycle in turn: a game lost; two people playing; a
/// record; the mop.
fn scene(n: u64) -> Vec<(Look, Vec<Leg>)> {
    match n % 4 {
        0 => vec![(
            DENIM,
            vec![
                walk(0.0, 1.0, HIDE_R, OUT_R),
                walk(1.0, 3.4, OUT_R, BACK_L),
                stay(3.4, 12.2, BACK_L, Facing::Away, Act::Play, CAB_BACK_L),
                stay(12.2, 13.6, BACK_L, Facing::Away, Act::Over, CAB_BACK_L),
                walk(13.6, 16.0, BACK_L, OUT_R),
                walk(16.0, 17.0, OUT_R, HIDE_R),
            ],
        )],
        1 => vec![
            (
                DENIM,
                vec![
                    walk(0.0, 1.0, HIDE_R, OUT_R),
                    walk(1.0, 3.4, OUT_R, BACK_L),
                    stay(3.4, 14.0, BACK_L, Facing::Away, Act::Play, CAB_BACK_L),
                    walk(14.0, 16.4, BACK_L, OUT_R),
                    walk(16.4, 17.4, OUT_R, HIDE_R),
                ],
            ),
            (
                MULLET,
                vec![
                    walk(4.0, 5.0, HIDE_L, OUT_L),
                    walk(5.0, 7.6, OUT_L, SIDE_R),
                    stay(7.6, 16.0, SIDE_R, Facing::Right, Act::Play, CAB_SIDE_R),
                    walk(16.0, 18.4, SIDE_R, OUT_L),
                    walk(18.4, 19.4, OUT_L, HIDE_L),
                ],
            ),
        ],
        2 => vec![(
            MULLET,
            vec![
                walk(0.0, 1.0, HIDE_L, OUT_L),
                walk(1.0, 3.6, OUT_L, SIDE_R),
                stay(3.6, 13.0, SIDE_R, Facing::Right, Act::Play, CAB_SIDE_R),
                stay(13.0, 14.8, SIDE_R, Facing::Toward, Act::Cheer, CAB_SIDE_R),
                walk(14.8, 17.2, SIDE_R, OUT_L),
                walk(17.2, 18.2, OUT_L, HIDE_L),
            ],
        )],
        _ => vec![(
            JANITOR,
            vec![
                walk(0.0, 1.2, HIDE_R, (0.70, 3.1)),
                Leg::Walk {
                    t0: 1.2,
                    t1: 8.1,
                    from: (0.70, 3.1),
                    to: (-0.70, 3.02),
                    mop: true,
                },
                walk(8.1, 9.3, (-0.70, 3.02), HIDE_L),
            ],
        )],
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
    mop: bool,
}

fn pose_at(legs: &[Leg], t: f32) -> Option<Pose> {
    for leg in legs {
        match *leg {
            Leg::Stay {
                t0,
                t1,
                at,
                facing,
                act,
                cab,
            } if (t0..t1).contains(&t) => {
                return Some(Pose {
                    x: at.0,
                    d: at.1,
                    facing,
                    walk: None,
                    amount: 0.0,
                    act: Some(act),
                    since: t - t0,
                    cab: Some(cab),
                    mop: false,
                });
            }
            Leg::Walk {
                t0,
                t1,
                from,
                to,
                mop,
            } if (t0..t1).contains(&t) => {
                let f = (t - t0) / (t1 - t0);
                let e = f * f * (3.0 - 2.0 * f);
                let (dx, dd) = (to.0 - from.0, to.1 - from.1);
                let length = (dx * dx + dd * dd * 0.64).sqrt();
                let stride = if mop { 0.5 } else { 0.95 };
                let facing = if dx.abs() > dd.abs() * 0.8 {
                    if dx > 0.0 {
                        Facing::Right
                    } else {
                        Facing::Left
                    }
                } else if dd > 0.0 {
                    Facing::Away
                } else {
                    Facing::Toward
                };
                return Some(Pose {
                    x: from.0 + dx * e,
                    d: from.1 + dd * e,
                    facing,
                    walk: Some((length * e / stride).fract()),
                    amount: (f.min(1.0 - f) * 5.0).min(1.0) * if mop { 0.6 } else { 1.0 },
                    act: None,
                    since: t - t0,
                    cab: None,
                    mop,
                });
            }
            _ => {}
        }
    }
    None
}

/// A small canvas a figure is built on, feet at its origin.
struct Canvas {
    px: Vec<Option<Color>>,
    z: Vec<f32>,
}

const CW: i32 = 128;
const CH: i32 = 170;
const OX: i32 = 64;
const OY: i32 = 160;

impl Canvas {
    fn new() -> Self {
        Self {
            px: vec![None; (CW * CH) as usize],
            z: vec![f32::MIN; (CW * CH) as usize],
        }
    }

    fn set(&mut self, x: i32, y: i32, c: Color, z: f32) {
        let (gx, gy) = (x + OX, y + OY);
        if gx < 0 || gy < 0 || gx >= CW || gy >= CH {
            return;
        }
        let i = (gy * CW + gx) as usize;
        if self.z[i] <= z {
            self.px[i] = Some(c);
            self.z[i] = z;
        }
    }

    fn get(&self, x: i32, y: i32) -> Option<Color> {
        let (gx, gy) = (x + OX, y + OY);
        if gx < 0 || gy < 0 || gx >= CW || gy >= CH {
            return None;
        }
        self.px[(gy * CW + gx) as usize]
    }

    /// A limb from a to b; the half away from the light (on the right) in
    /// `shade` when there is one.
    fn capsule(
        &mut self,
        a: (f32, f32),
        b: (f32, f32),
        r: f32,
        c: Color,
        z: f32,
        shade: Option<Color>,
    ) {
        let (x0, x1) = (
            (a.0.min(b.0) - r - 1.0) as i32,
            (a.0.max(b.0) + r + 2.0) as i32,
        );
        let (y0, y1) = (
            (a.1.min(b.1) - r - 1.0) as i32,
            (a.1.max(b.1) + r + 2.0) as i32,
        );
        let (vx, vy) = (b.0 - a.0, b.1 - a.1);
        let ll = (vx * vx + vy * vy).max(1e-6);
        for y in y0..y1 {
            for x in x0..x1 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let t = (((px - a.0) * vx + (py - a.1) * vy) / ll).clamp(0.0, 1.0);
                let (dx, dy) = (px - (a.0 + vx * t), py - (a.1 + vy * t));
                if dx * dx + dy * dy <= r * r {
                    let col = match shade {
                        Some(s) if dx > r * 0.35 => s,
                        _ => c,
                    };
                    self.set(x, y, col, z);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn ellipse(
        &mut self,
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
        c: Color,
        z: f32,
        keep: impl Fn(f32, f32) -> bool,
    ) {
        for y in (cy - ry) as i32 - 1..(cy + ry) as i32 + 2 {
            for x in (cx - rx) as i32 - 1..(cx + rx) as i32 + 2 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                if ((px - cx) / rx).powi(2) + ((py - cy) / ry).powi(2) <= 1.0 && keep(px, py) {
                    self.set(x, y, c, z);
                }
            }
        }
    }
}

fn dark(c: Color, k: f32) -> Color {
    lerp_color(c, 0x0a0614, k)
}

/// A figure posed and drawn at `hp` pixels tall. Returns the canvas and,
/// for the man with the mop, where its head touches the floor.
fn figure(hp: f32, look: &Look, pose: &Pose, t: f32) -> (Canvas, Option<(f32, f32)>) {
    let u = hp / 100.0;
    let mut cv = Canvas::new();
    let facing = if pose.act == Some(Act::Cheer) {
        Facing::Toward
    } else {
        pose.facing
    };
    let side = matches!(facing, Facing::Left | Facing::Right);
    let sgn = if facing == Facing::Right { 1.0 } else { -1.0 };
    let (thigh, shin, leg_r) = (25.0 * u, 24.0 * u, 4.4 * u);
    let amount = pose.amount;
    let p = pose.walk.unwrap_or(0.0) * std::f32::consts::TAU;
    let playing = pose.act == Some(Act::Play);
    let play_t = if playing { pose.since } else { 0.0 };

    // Legs, and where that puts the hips so a foot stays on the floor.
    let mut legs = [((0.0, 0.0), (0.0, 0.0)); 2];
    for (k, leg) in legs.iter_mut().enumerate() {
        let q = p + k as f32 * std::f32::consts::PI;
        if side {
            let a1 = 0.46 * q.sin() * amount;
            let a2 = 0.75 * (q + 1.3).sin().max(0.0) * amount;
            let knee = (sgn * thigh * a1.sin(), thigh * a1.cos());
            let ank = (
                knee.0 + sgn * shin * (a1 - a2).sin(),
                knee.1 + shin * (a1 - a2).cos(),
            );
            *leg = (knee, ank);
        } else {
            let sx = if k == 0 { -1.0 } else { 1.0 };
            let lift = q.sin().max(0.0) * 7.0 * u * amount;
            *leg = (
                (sx * 5.0 * u + sx * lift * 0.15, thigh - lift * 0.35),
                (sx * 5.0 * u, thigh + shin - lift),
            );
        }
    }
    let reach = legs[0].1.1.max(legs[1].1.1);
    let jump = if pose.act == Some(Act::Cheer) {
        (t * 8.0).sin().abs() * 6.0 * u
    } else {
        0.0
    };
    let shift = if playing && !side {
        u * if ((t / 3.1) as i32) % 2 != 0 {
            1.0
        } else {
            -1.0
        }
    } else {
        0.0
    };
    let slump = if pose.act == Some(Act::Over) { u } else { 0.0 };
    let hip = (
        shift
            + if side && pose.walk.is_some() {
                sgn * 1.5 * u
            } else {
                0.0
            },
        -(reach + 3.0 * u) - jump + slump,
    );
    let mut torso_top = (hip.0, hip.1 - 33.0 * u);
    let mut lean = if pose.walk.is_some() && side {
        sgn * 2.0 * u * amount
    } else {
        0.0
    };
    if playing {
        let k = play_t % 6.5;
        let into = (1.0 - (k - 5.2).abs() / 0.6).max(0.0);
        if side {
            lean = sgn * (3.0 + 2.0 * into) * u;
        } else {
            torso_top.1 += 1.5 * into * u;
        }
    }
    let shoulder = (torso_top.0 + lean, torso_top.1 + 4.0 * u);

    for (i, (knee, ank)) in legs.iter().enumerate() {
        let far = side && i == 1;
        let c = if far {
            dark(look.legs, 0.35)
        } else {
            look.legs
        };
        let z = if far { -1.0 } else { 0.0 };
        let hk = (hip.0 + knee.0, hip.1 + knee.1);
        let ha = (hip.0 + ank.0, hip.1 + ank.1);
        let top = (
            hip.0
                + if side {
                    0.0
                } else if i == 0 {
                    -3.0 * u
                } else {
                    3.0 * u
                },
            hip.1,
        );
        cv.capsule(top, hk, leg_r, c, z, Some(dark(c, 0.25)));
        cv.capsule(hk, ha, leg_r * 0.92, c, z, Some(dark(c, 0.25)));
        if side {
            let shoe = if far {
                dark(look.shoes, 0.2)
            } else {
                look.shoes
            };
            cv.capsule(
                (ha.0 - sgn * u, ha.1 + u),
                (ha.0 + sgn * 6.0 * u, ha.1 + 1.5 * u),
                2.6 * u,
                shoe,
                z + 0.5,
                None,
            );
        } else {
            cv.capsule(
                (ha.0 - 2.0 * u, ha.1 + u),
                (ha.0 + 2.0 * u, ha.1 + u),
                2.8 * u,
                look.shoes,
                0.5,
                None,
            );
        }
    }

    let arm_r = 3.4 * u;
    let (up, fore) = (16.0 * u, 15.0 * u);
    let top_c = look.top;
    let hi_c = lerp_color(look.top, 0xffffff, 0.25);
    let sh_c = dark(look.top, 0.35);
    let arm = |cv: &mut Canvas, sx: f32, a_up: f32, a_fore: f32, z: f32, c: Color| {
        let s0 = (shoulder.0 + sx, shoulder.1);
        let el = (s0.0 + up * a_up.sin(), s0.1 + up * a_up.cos());
        let wr = (el.0 + fore * a_fore.sin(), el.1 + fore * a_fore.cos());
        cv.capsule(s0, el, arm_r, c, z, Some(dark(c, 0.25)));
        cv.capsule(el, wr, arm_r * 0.9, c, z, Some(dark(c, 0.25)));
        cv.ellipse(
            wr.0,
            wr.1 + u,
            2.6 * u,
            2.8 * u,
            look.skin,
            z + 0.1,
            |_, _| true,
        );
    };

    let mut mop = None;
    if pose.act == Some(Act::Cheer) {
        for sx in [-1.0f32, 1.0] {
            let s0 = (shoulder.0 + sx * 13.0 * u, shoulder.1 + u);
            let wave = (t * 8.0 + sx).sin() * 1.5 * u;
            let el = (s0.0 + sx * 5.0 * u, s0.1 - 12.0 * u + wave);
            let wr = (el.0 - sx * u, el.1 - 13.0 * u + wave);
            cv.capsule(s0, el, arm_r, top_c, 1.0, Some(dark(top_c, 0.25)));
            cv.capsule(el, wr, arm_r * 0.9, top_c, 1.0, Some(dark(top_c, 0.25)));
            cv.ellipse(wr.0, wr.1 - u, 3.0 * u, 3.0 * u, look.skin, 1.1, |_, _| {
                true
            });
        }
    } else if pose.mop {
        let sweep = (t * 3.2).sin() * 7.0 * u;
        let h1 = (shoulder.0 + sgn * 9.0 * u, shoulder.1 + 16.0 * u);
        let h2 = (shoulder.0 + sgn * 13.0 * u, shoulder.1 + 25.0 * u);
        let foot = (sgn * (26.0 * u + sweep), 0.0);
        let top_end = (h1.0 - sgn * 3.0 * u, h1.1 - 7.0 * u);
        cv.capsule(top_end, foot, 1.1 * u + 0.4, 0xaa8c5a, 1.5, None);
        cv.capsule(
            (foot.0 - 5.0 * u, -1.5 * u),
            (foot.0 + 5.0 * u, -1.5 * u),
            2.4 * u,
            0xc8c8be,
            1.6,
            None,
        );
        arm(&mut cv, -sgn * 2.0 * u, sgn * 0.9, sgn * 1.2, -2.0, sh_c);
        cv.capsule(
            (shoulder.0 + sgn * u, shoulder.1),
            h2,
            arm_r,
            hi_c,
            2.0,
            Some(dark(hi_c, 0.25)),
        );
        cv.ellipse(h1.0, h1.1, 2.6 * u, 2.8 * u, look.skin, 2.1, |_, _| true);
        cv.ellipse(h2.0, h2.1, 2.6 * u, 2.8 * u, look.skin, 2.1, |_, _| true);
        mop = Some(foot);
    } else if side {
        if playing {
            let stick = 0.10 * (t * 5.3).sin();
            arm(
                &mut cv,
                -sgn * 2.0 * u,
                sgn * (0.95 + stick),
                sgn * 1.75,
                -2.0,
                sh_c,
            );
        } else {
            let sw = -0.42 * p.sin() * amount;
            arm(
                &mut cv,
                0.0,
                sgn * sw,
                sgn * (sw + 0.25 + 0.25 * amount),
                -2.0,
                sh_c,
            );
        }
    } else if playing && facing == Facing::Away {
        // From behind the forearms are in front of the body: only the upper
        // arms and the elbows show, working.
        let wig = (t * 11.0).sin() * u;
        let tap = (t * 7.0).sin().max(0.0) * 1.2 * u;
        for sx in [-1.0f32, 1.0] {
            let s0 = (shoulder.0 + sx * 13.0 * u, shoulder.1 + u);
            let el = (
                s0.0 + sx * 3.0 * u + if sx < 0.0 { wig } else { 0.0 },
                s0.1 + 14.0 * u + if sx > 0.0 { tap } else { 0.0 },
            );
            cv.capsule(
                s0,
                el,
                arm_r,
                if sx < 0.0 { top_c } else { sh_c },
                1.0,
                Some(dark(top_c, 0.25)),
            );
        }
    } else {
        let sw = p.sin() * amount;
        for (sx, k) in [(-1.0f32, 1.0f32), (1.0, -1.0)] {
            let s0 = (shoulder.0 + sx * 13.0 * u, shoulder.1 + u);
            let fwd = (sw * k).max(0.0);
            let el = (
                s0.0 + sx * 1.5 * u,
                s0.1 + up
                    - if sw * k > 0.0 {
                        sw.abs() * 1.5 * u
                    } else {
                        0.0
                    },
            );
            let wr = (el.0 + sx * 0.5 * u, el.1 + fore - fwd * 4.0 * u);
            cv.capsule(s0, el, arm_r, top_c, 1.0, Some(dark(top_c, 0.25)));
            cv.capsule(el, wr, arm_r * 0.9, top_c, 1.0, Some(dark(top_c, 0.25)));
            cv.ellipse(wr.0, wr.1 + u, 2.6 * u, 2.8 * u, look.skin, 1.1, |_, _| {
                true
            });
        }
    }

    // The torso, round at the shoulders, lit from the left.
    let half_w = if side { 8.5 } else { 13.5 } * u;
    let (y0, y1) = (torso_top.1 as i32, hip.1 as i32 + 2);
    for y in y0..y1 {
        let f = (y as f32 - torso_top.1) / (hip.1 - torso_top.1).max(1.0);
        let cx = torso_top.0 + lean * (1.0 - f) + (hip.0 - torso_top.0) * f;
        let dy = y as f32 - torso_top.1;
        let mut hw = half_w * (1.0 - 0.18 * f);
        if dy < 4.0 * u {
            hw -= (4.0 * u - dy) * 0.9;
        }
        for x in (cx - hw) as i32..=(cx + hw) as i32 {
            let mut g = (x as f32 + 0.5 - (cx - hw)) / (2.0 * hw).max(1.0);
            if side && sgn < 0.0 {
                g = 1.0 - g;
            }
            let mut c = if g < 0.26 {
                hi_c
            } else if g > 0.78 {
                sh_c
            } else {
                top_c
            };
            if let Some(s) = look.stripe
                && (f - 0.34).abs() < 0.05
            {
                c = s;
            }
            if facing == Facing::Away && (x as f32 + 0.5 - cx).abs() < 0.6 && f > 0.15 {
                c = sh_c;
            }
            if f > 0.92 {
                c = dark(c, 0.3);
            }
            cv.set(x, y, c, 0.2);
        }
    }

    // The near arm, in front of the body.
    if side && pose.act != Some(Act::Cheer) && !pose.mop {
        if playing {
            let tap = 0.12 * (t * 9.0).sin().max(0.0);
            arm(
                &mut cv,
                sgn * u,
                sgn * (0.85 - tap),
                sgn * (1.6 + tap),
                2.0,
                hi_c,
            );
        } else {
            let sw = 0.42 * p.sin() * amount;
            arm(
                &mut cv,
                0.0,
                sgn * sw,
                sgn * (sw + 0.25 + 0.25 * amount),
                2.0,
                top_c,
            );
        }
    }

    // The head: nodding to the game, a look away now and then, bowed at a
    // game lost.
    let mut nod = 0.0;
    let mut glance = false;
    if playing {
        nod = (t * 4.2).sin().max(0.0) * 0.9 * u;
        glance = play_t % 9.0 > 7.6;
    }
    if pose.act == Some(Act::Over) {
        nod = 2.5 * u;
    }
    let hx = shoulder.0
        + if side {
            sgn * 2.5 * u + lean * 0.4
        } else {
            0.0
        };
    let hy = torso_top.1 - 9.0 * u + nod;
    let hr = 8.0 * u;
    cv.capsule(
        (torso_top.0, torso_top.1 + 1.0),
        (hx, hy + hr * 0.6),
        2.6 * u,
        dark(look.skin, 0.15),
        2.5,
        None,
    );
    cv.ellipse(hx, hy, hr * 0.88, hr, look.skin, 3.0, |_, _| true);
    match facing {
        Facing::Away if !glance => {
            cv.ellipse(hx, hy, hr * 0.9, hr * 1.02, look.hair, 3.1, |_, _| true);
            if look.mullet {
                cv.capsule(
                    (hx, hy + hr * 0.4),
                    (hx, hy + hr * 1.5),
                    hr * 0.8,
                    look.hair,
                    3.1,
                    None,
                );
            }
        }
        Facing::Away => {
            cv.ellipse(hx, hy, hr * 0.9, hr * 1.02, look.hair, 3.1, |x, _| {
                x < hx + hr * 0.25
            });
            cv.ellipse(
                hx + hr * 0.55,
                hy + hr * 0.1,
                hr * 0.3,
                hr * 0.4,
                dark(look.skin, 0.1),
                3.2,
                |_, _| true,
            );
        }
        Facing::Toward => {
            cv.ellipse(
                hx,
                hy - hr * 0.35,
                hr * 0.95,
                hr * 0.72,
                look.hair,
                3.1,
                |_, y| y < hy - hr * 0.15,
            );
            if hp > 34.0 {
                cv.set((hx - 3.0 * u) as i32, hy as i32, 0x1e141e, 3.5);
                cv.set((hx + 2.5 * u) as i32, hy as i32, 0x1e141e, 3.5);
            }
        }
        _ => {
            cv.ellipse(
                hx - sgn * 1.2 * u,
                hy - 1.8 * u,
                hr * 0.86,
                hr * 0.8,
                look.hair,
                3.1,
                |x, y| (x - hx) * sgn < 1.8 * u || y < hy - 2.5 * u,
            );
            if look.mullet {
                cv.capsule(
                    (hx - sgn * hr * 0.4, hy),
                    (hx - sgn * hr * 0.5, hy + hr * 1.5),
                    hr * 0.45,
                    look.hair,
                    3.1,
                    None,
                );
            }
            if hp > 34.0 {
                cv.set(
                    (hx + sgn * 4.0 * u) as i32,
                    (hy - 0.5 * u) as i32,
                    0x1e141e,
                    3.5,
                );
            }
        }
    }
    if let Some(cap) = look.cap {
        cv.ellipse(
            hx,
            hy - hr * 0.45,
            hr * 0.98,
            hr * 0.62,
            cap,
            3.3,
            |_, y| y < hy - hr * 0.1,
        );
        if side {
            cv.capsule(
                (hx + sgn * hr * 0.3, hy - hr * 0.2),
                (hx + sgn * hr * 1.3, hy - hr * 0.2),
                1.2 * u,
                cap,
                3.3,
                None,
            );
        }
    }
    (cv, mop)
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
        for (_, legs) in scene(n) {
            if let Some(p) = pose_at(&legs, t)
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
        let mut here: Vec<(Look, Pose)> = scene(n)
            .iter()
            .filter_map(|(look, legs)| pose_at(legs, t).map(|p| (*look, p)))
            .collect();
        here.sort_by(|a, b| b.1.d.total_cmp(&a.1.d));
        for (look, pose) in &here {
            let hp = PERSON * look.tall * unit / pose.d;
            let (fig, mop) = figure(hp, look, pose, now as f32);
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
            for gy in 0..CH {
                for gx in 0..CW {
                    let (x, y) = (gx - OX, gy - OY);
                    let here_c = fig.get(x, y);
                    let edge = here_c.is_none()
                        && [(1, 0), (-1, 0), (0, 1), (0, -1)]
                            .iter()
                            .any(|(dx, dy)| fig.get(x + dx, y + dy).is_some());
                    let c = match (here_c, edge) {
                        (Some(c), _) => {
                            let open = fig.get(x + 1, y).is_none()
                                || fig.get(x - 1, y).is_none()
                                || fig.get(x, y - 1).is_none();
                            match rim {
                                Some(r) if open => lerp_color(c, r, 0.45),
                                _ => c,
                            }
                        }
                        (None, true) => OUTLINE,
                        _ => continue,
                    };
                    let (sx, sy) = (fx + x, fy + y);
                    if hall.depth_at(sx, sy) > pose.d {
                        fb.put(sx, sy, lerp_color(c, fog, haze));
                    }
                }
            }
            if let Some(m) = mop
                && pose.walk.is_some()
            {
                self.wet
                    .push((pose.x + m.0 * pose.d / unit, pose.d - 0.02, now));
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
                assert!(pose_at(&legs, CYCLE as f32 - 0.5).is_none());
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

    #[test]
    fn a_figure_has_a_body() {
        let pose = Pose {
            x: 0.0,
            d: 2.8,
            facing: Facing::Right,
            walk: Some(0.3),
            amount: 1.0,
            act: None,
            since: 0.0,
            cab: None,
            mop: false,
        };
        let (cv, _) = figure(70.0, &DENIM, &pose, 0.0);
        let n = cv.px.iter().filter(|p| p.is_some()).count();
        assert!(n > 800, "a figure of {n} pixels");
    }
}
