//! The floor the boot act stands on, and the act's soundtrack.
//!
//! A Mode 7 checkerboard unrolls from the horizon toward the viewer while a
//! light comes up it, and the same timeline drives the sound: a thump as the
//! seam lights, a riser and a rumble under the approach, a slam, three bells
//! as the phosphor lights, and a whoosh as the floor goes.
//!
//! What used to be here as well was a "CRT" tag: three letters that rose
//! from the horizon spinning, slammed into the foreground and flew to a
//! resting place under the wordmark. The wordmark says CRT itself now, so the
//! tag would have said it twice, and the letters, their spin, their flight
//! and their idle glint went with it.

use crate::fb::{Color, Framebuffer, lerp_color, scale};

/// Letter glyphs, 5x7.
// The act's beats, in seconds from TAG_START, so the sound and the picture
// are read off the same sheet.
const A_RUN: f32 = 0.7; // the light starts coming up the floor
const A_SLAM: f32 = 2.0; // it reaches the letters
const A_BELLS: f32 = 2.1; // the phosphor lights
const A_LEAVE: f32 = 2.2; // the floor goes
const A_END: f32 = 3.05;

pub const HORIZON: i32 = 118;

/// The horizon at this framebuffer's height. The constant is the value
/// for the 320x240 tube; on a taller framebuffer the ground has to move with
/// everything else that is placed as a fraction of the height, or the floor
/// ends up drawn over the wordmark.
pub fn horizon(h: i32) -> i32 {
    HORIZON * h / 240
}
pub struct Look {
    pub bg: Color,
    pub floor_light: Color,
    pub floor_dark: Color,
    pub orange: Color,
    pub yellow: Color,
}

/// Every lit glyph pixel in canvas coordinates.
fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}
fn ease_in_out(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// A cheap deterministic hash, for the shake and for the dust.
pub fn hash(i: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// How far the floor has scrolled at time `t`. It accelerates while the
/// light is coming up it and then eases off, which is what makes the
/// approach read as an approach rather than as a conveyor belt.
pub fn floor_scroll(t: f32) -> f32 {
    // Where the acceleration stops. Tuned by eye against the light's own
    // travel, and left where it was when the letters went.
    const KNEE: f32 = 1.4;
    if t <= KNEE {
        25.0 * t * t
    } else {
        let d = t - KNEE;
        25.0 * KNEE * KNEE + 70.0 * d * (1.0 - 0.3 * d).max(0.2)
    }
}

/// Mode 7 checkerboard from the horizon down, fogged toward the horizon.
#[allow(clippy::too_many_arguments)]
pub fn draw_floor(
    fb: &mut Framebuffer,
    horizon: i32,
    t: f32,
    alpha: f32,
    jolt: (i32, i32),
    look: &Look,
    flash: bool,
    // 0 while the floor is still rolled up at the horizon, 1 when it
    // has reached the viewer. A slab that fades up out of nothing has no
    // cause; a floor that unrolls from the light on the horizon does.
    open: f32,
) {
    if alpha <= 0.0 {
        return;
    }
    let w = fb.w as i32;
    let h = fb.h as i32;
    let scroll = floor_scroll(t);
    // After F-Zero and Super Mario Kart: a haze band sitting ON the
    // horizon, brightest at the seam and gone four rows up, so the floor
    // arrives out of light instead of out of a hard line.
    let haze = lerp_color(look.bg, look.floor_light, 0.5);
    // The seam lights before anything unrolls out of it, and hardest then.
    let seam = clamp01(open * 5.0) * (1.0 + 1.6 * (1.0 - clamp01(open * 2.2)));
    for i in 0..5 {
        let k = ((1.0 - i as f32 / 5.0) * 0.55 * alpha * seam).min(1.0);
        fb.rect(
            jolt.0,
            horizon - 1 - i + jolt.1,
            w,
            1,
            lerp_color(look.bg, haze, k),
        );
    }
    // The floor is never cut off: what travels across it is light, not an
    // edge of geometry. Ahead of the front the checker is only just there.
    let front = horizon as f32 + ease_in_out(clamp01(open)) * (h - horizon) as f32;
    let edge = front.round() as i32;
    for y in horizon..h {
        let dy = (y - horizon) as f32 + 1.0;
        let depth = 12000.0 / dy + scroll * 6.0; // world z of this scanline
        let lit = clamp01((front - y as f32) / 6.0);
        let fog = clamp01((dy - 2.0) / 34.0)
            * alpha
            * (0.12 * clamp01(open * 3.0) + 0.88 * lit);
        let row = (depth / 10.0).floor() as i32;
        let row_parity = row;
        for x in 0..w {
            let wx = (x - w / 2) as f32 * 50.0 / dy;
            let col = (wx / 10.0).floor() as i32;
            let parity = (row_parity + col) & 1;
            let base = if parity == 0 {
                look.floor_light
            } else {
                look.floor_dark
            };
            let mut c = lerp_color(look.bg, base, fog);
            // Pattern thinning: the far rows cannot hold a checker without
            // shimmering, so the light squares lose every other pixel there
            // and the two colours dither into one another instead.
            if dy < 9.0 && parity == 0 && ((x + y) & 1) == 0 {
                c = lerp_color(look.bg, look.floor_dark, fog);
            }
            if flash {
                c = lerp_color(c, 0xffffff, 0.6 * fog);
            }
            // The front carries the light it is opening the floor with.
            let from_edge = (edge - y).abs();
            if from_edge <= 2 && open > 0.0 && open < 1.0 {
                let k = (1.0 - from_edge as f32 / 3.0) * 0.8;
                c = lerp_color(c, look.yellow, k);
            }
            fb.put(x + jolt.0, y + jolt.1, c);
        }
    }
}

/// The floor's own projection inverted, so a light can be an object in
/// that world rather than a rectangle over it.
pub struct OnFloor {
    pub y: i32,
    pub half: i32,
    pub rows: i32,
    pub fog: f32,
}

pub fn on_floor(horizon: i32, dy: f32, world_half: f32, world_deep: f32) -> OnFloor {
    let dy = dy.max(0.6);
    let far = 12000.0 / (12000.0 / dy + world_deep);
    OnFloor {
        y: horizon + dy.round() as i32 - 1,
        half: (world_half * dy / 50.0).round() as i32,
        rows: (dy - far).round().max(1.0) as i32,
        fog: clamp01((dy - 2.0) / 34.0),
    }
}

pub fn draw_copper_bar(fb: &mut Framebuffer, y: i32, look: &Look, alpha: f32) {
    let rows = [
        look.orange,
        look.yellow,
        0xffffff,
        0xffffff,
        look.yellow,
        look.orange,
    ];
    for (i, c) in rows.iter().enumerate() {
        fb.rect(0, y + i as i32, fb.w as i32, 1, scale(*c, alpha));
    }
}

/// Draw the tag at local time `t`; (`x`, `y`, `s`) is the resting place.
/// The soundtrack of the show, on the same timeline as the picture: a riser
/// with a rumble while the floor rushes in, a whoosh pulsing with the spin,
/// a slam (kick, crash, metallic ring) and a chord stab, a downward whoosh
/// for the flight and a three note sparkle on landing.
pub fn synth(rate: u32) -> Vec<f32> {
    let n = (A_END * rate as f32) as usize;
    let mut out = vec![0.0f32; n];
    let tau = std::f32::consts::TAU;
    let sr = rate as f32;
    let mut noise = 0x9e37_79b9u32;
    let mut rnd = || {
        noise ^= noise << 13;
        noise ^= noise >> 17;
        noise ^= noise << 5;
        (noise >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    };
    let (mut ph1, mut ph2) = (0.0f32, 0.0f32);
    let (mut lp_riser, mut lp_rumble, mut lp_open, mut lp_run_a, mut lp_run_b, mut lp_crash) =
        (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let (mut lp_go_a, mut lp_go_b) = (0.0f32, 0.0f32);
    let onepole = |cut: f32| -> f32 {
        let rc = 1.0 / (tau * cut.max(20.0));
        let dt = 1.0 / sr;
        dt / (rc + dt)
    };
    let chord = [329.63f32, 415.30, 493.88, 659.26];
    let bells = [
        (A_BELLS, 1318.5f32),
        (A_BELLS + 0.06, 1661.2),
        (A_BELLS + 0.12, 1975.5),
    ];

    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / sr;
        let mut v = 0.0;
        let white = rnd();

        // The seam lighting, on the frame it lights: a low thump with a
        // bright edge on it. The floor has to be heard arriving.
        if t < 0.4 {
            let f = 70.0 * (-t * 6.0).exp() + 34.0;
            v += (tau * f * t).sin() * (-t * 9.0).exp() * 0.34;
            let ao = onepole(2600.0);
            lp_open += ao * (white - lp_open);
            v += lp_open * (-t * 10.0).exp() * 0.22;
        }

        // Riser and rumble under the whole approach, tightening into the slam.
        if t < A_SLAM {
            let p = t / A_SLAM;
            let f = 80.0 * (600.0f32 / 80.0).powf(p);
            ph1 = (ph1 + f * 1.006 / sr).fract();
            ph2 = (ph2 + f * 0.994 / sr).fract();
            let saw = (ph1 * 2.0 - 1.0) + (ph2 * 2.0 - 1.0);
            let a = onepole(250.0 + 3800.0 * p * p);
            lp_riser += a * (saw - lp_riser);
            v += lp_riser * 0.11 * (0.3 + 0.7 * p);
            let ar = onepole(120.0);
            lp_rumble += ar * (white - lp_rumble);
            v += lp_rumble * 0.6 * (0.4 + 0.6 * p);
        }

        // The light coming up the floor: a noise band whose cutoff and level
        // climb as it nears, which is what an approach sounds like.
        if (A_RUN..A_SLAM).contains(&t) {
            let p = (t - A_RUN) / (A_SLAM - A_RUN);
            let cut = 320.0 + 5200.0 * p * p;
            let a_hi = onepole(cut);
            let a_lo = onepole(cut * 0.45);
            lp_run_a += a_hi * (white - lp_run_a);
            lp_run_b += a_lo * (white - lp_run_b);
            v += (lp_run_a - lp_run_b) * 0.5 * (0.12 + 0.88 * p);
        }

        // Slam: kick, crash, metallic ring.
        let d = t - A_SLAM;
        if d >= 0.0 {
            let f = 160.0 * (-d * 8.0).exp() + 40.0;
            v += (tau * f * d).sin() * (-d * 7.0).exp() * 0.5;
            let ac = onepole(6000.0);
            lp_crash += ac * (white - lp_crash);
            v += lp_crash * (-d * 5.0).exp() * 0.28;
            v += (tau * 1300.0 * d).sin() * (-d * 12.0).exp() * 0.1;
            v += (tau * 2100.0 * d).sin() * (-d * 16.0).exp() * 0.07;
        }

        // Chord stab just after the impact.
        let dc = t - (A_SLAM + 0.05);
        if (0.0..0.35).contains(&dc) {
            let env = (-dc * 9.0).exp();
            for f in chord {
                let sq = if (dc * f).fract() < 0.5 { 1.0 } else { -1.0 };
                v += sq * 0.05 * env;
            }
        }

        // Three bells as the phosphor lights.
        for (at, f) in bells {
            let db = t - at;
            if (0.0..0.25).contains(&db) {
                v += (tau * f * db).sin() * (-db * 22.0).exp() * 0.12;
            }
        }

        // The floor going: the same whoosh, downward, under the fade.
        if (A_LEAVE..A_END).contains(&t) {
            let p = (t - A_LEAVE) / (A_END - A_LEAVE);
            let cut = 2600.0 * (1.0 - p) + 260.0;
            let a_hi = onepole(cut);
            let a_lo = onepole(cut * 0.5);
            lp_go_a += a_hi * (white - lp_go_a);
            lp_go_b += a_lo * (white - lp_go_b);
            v += (lp_go_a - lp_go_b) * 0.42 * (p * std::f32::consts::PI).sin();
        }

        *s = v;
    }
    // Normalize to a safe peak.
    let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    if peak > 0.0 {
        let g = 0.55 / peak;
        for s in out.iter_mut() {
            *s *= g;
        }
    }
    out
}

