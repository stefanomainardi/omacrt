//! The "CRT" tag under the wordmark, introduced SNES title screen style.
//!
//! A Mode 7 checkerboard floor scrolls toward the viewer, the three letters
//! rise from the horizon spinning around their vertical axis, slam into the
//! foreground with a shake, a copper bar and a burst of dust, hold for a beat
//! under a passing light, then shrink and fly to their resting place under
//! the wordmark, landing with a sparkle. Afterwards the tag idles with a faint
//! glint every few seconds. Picture and sound come from the same timeline.

use crate::fb::{Color, Framebuffer, add, lerp_color, scale};

/// Letter glyphs, 5x7.
const C: [(i32, i32); 13] = [
    (4, 1),
    (3, 0),
    (2, 0),
    (1, 0),
    (0, 1),
    (0, 2),
    (0, 3),
    (0, 4),
    (0, 5),
    (1, 6),
    (2, 6),
    (3, 6),
    (4, 5),
];
const R: [(i32, i32); 18] = [
    (0, 6),
    (0, 5),
    (0, 4),
    (0, 3),
    (0, 2),
    (0, 1),
    (0, 0),
    (1, 0),
    (2, 0),
    (3, 0),
    (4, 1),
    (4, 2),
    (3, 3),
    (2, 3),
    (1, 3),
    (2, 4),
    (3, 5),
    (4, 6),
];
const T: [(i32, i32); 11] = [
    (0, 0),
    (1, 0),
    (2, 0),
    (3, 0),
    (4, 0),
    (2, 1),
    (2, 2),
    (2, 3),
    (2, 4),
    (2, 5),
    (2, 6),
];

/// Canvas size in glyph pixels: three letters one column apart, each one row
/// lower than the previous (stair-step slant).
pub const COLS: i32 = 17;
pub const ROWS: i32 = 9;

// Timeline, seconds since the reveal starts.
const FLOOR_IN: f32 = 0.3;
const APPROACH_START: f32 = 0.2;
const SLAM: f32 = 1.4;
const FLY_START: f32 = 2.0;
const LAND: f32 = 2.6;
pub const TOTAL_SECS: f32 = 2.8;
const GLINT_EVERY: f32 = 9.0;
const GLINT_SECS: f32 = 0.35;

const HORIZON: i32 = 118;
const BIG_SCALE: f32 = 7.0;
const BIG_CY: f32 = 175.0;
const SPINS: f32 = 2.5;

/// Colors the show borrows from the theme.
pub struct Look {
    /// Wordmark gradient, bottom stop first.
    pub stops: [Color; 3],
    pub bg: Color,
    pub floor_light: Color,
    pub floor_dark: Color,
    pub orange: Color,
    pub yellow: Color,
}

/// Every lit glyph pixel in canvas coordinates.
pub fn path() -> Vec<(i32, i32)> {
    let mut out = Vec::with_capacity(42);
    for (i, glyph) in [&C[..], &R[..], &T[..]].iter().enumerate() {
        for (x, y) in glyph.iter() {
            out.push((x + i as i32 * 6, y + i as i32));
        }
    }
    out
}

fn final_color(row: i32, stops: [Color; 3]) -> Color {
    let f = 1.0 - row as f32 / (ROWS - 1) as f32;
    if f < 0.5 {
        lerp_color(stops[0], stops[1], f * 2.0)
    } else {
        lerp_color(stops[1], stops[2], (f - 0.5) * 2.0)
    }
}

fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}
fn ease_in(t: f32) -> f32 {
    t * t
}
fn ease_in_out(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}
fn hash(i: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// Rotation angle of the letters around their vertical axis at time `t`.
fn spin_angle(t: f32) -> f32 {
    let p = clamp01((t - APPROACH_START) / (SLAM - APPROACH_START));
    SPINS * (1.0 - p) * (1.0 - p) * std::f32::consts::TAU
}

/// Distance the floor has scrolled toward the viewer at time `t`.
fn floor_scroll(t: f32) -> f32 {
    if t <= SLAM {
        25.0 * t * t
    } else {
        let d = t - SLAM;
        25.0 * SLAM * SLAM + 70.0 * d * (1.0 - 0.3 * d).max(0.2)
    }
}

/// Mode 7 checkerboard from the horizon down, fogged toward the horizon.
fn draw_floor(
    fb: &mut Framebuffer,
    t: f32,
    alpha: f32,
    jolt: (i32, i32),
    look: &Look,
    flash: bool,
) {
    if alpha <= 0.0 {
        return;
    }
    let w = fb.w as i32;
    let h = fb.h as i32;
    let scroll = floor_scroll(t);
    for y in HORIZON..h {
        let dy = (y - HORIZON) as f32 + 1.0;
        let depth = 12000.0 / dy + scroll * 6.0; // world z of this scanline
        let fog = clamp01((dy - 2.0) / 34.0) * alpha;
        let row_parity = (depth / 10.0).floor() as i32;
        for x in 0..w {
            let wx = (x - w / 2) as f32 * 50.0 / dy;
            let parity = (row_parity + (wx / 10.0).floor() as i32) & 1;
            let base = if parity == 0 {
                look.floor_light
            } else {
                look.floor_dark
            };
            let mut c = lerp_color(look.bg, base, fog);
            if flash {
                c = lerp_color(c, 0xffffff, 0.6 * fog);
            }
            fb.put(x + jolt.0, y + jolt.1, c);
        }
    }
}

/// The letters as one sprite: `s` pixels per glyph pixel, rotated around the
/// vertical axis by `angle`, lit by how much it faces the viewer.
#[allow(clippy::too_many_arguments)]
fn draw_letters(
    fb: &mut Framebuffer,
    cx: f32,
    cy: f32,
    s: f32,
    angle: f32,
    jolt: (i32, i32),
    stops: [Color; 3],
    flash: Option<Color>,
) {
    let cos = angle.cos();
    let width = (s * cos.abs()).max(1.0);
    let light = 0.45 + 0.55 * cos.abs();
    let back = cos < 0.0;
    for (col, row) in path() {
        let x = cx + (col as f32 - COLS as f32 / 2.0) * s * cos;
        let y = cy + (row as f32 - ROWS as f32 / 2.0) * s;
        let mut c = final_color(row, stops);
        c = scale(c, if back { light * 0.45 } else { light });
        if let Some(f) = flash {
            c = f;
        }
        let x0 = if back {
            (x - width).round() as i32
        } else {
            x.round() as i32
        };
        fb.rect(
            x0 + jolt.0,
            y.round() as i32 + jolt.1,
            width.round().max(1.0) as i32,
            s.round().max(1.0) as i32,
            c,
        );
    }
}

fn draw_copper_bar(fb: &mut Framebuffer, y: i32, look: &Look, alpha: f32) {
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
pub fn draw(fb: &mut Framebuffer, x: i32, y: i32, s: i32, t: f32, look: &Look) {
    let w = fb.w as f32;
    let tag_cx = x as f32 + COLS as f32 * s as f32 / 2.0;
    let tag_cy = y as f32 + ROWS as f32 * s as f32 / 2.0;

    if t >= TOTAL_SECS {
        draw_settled(fb, x, y, s, t, look);
        return;
    }

    // Shake right after the slam.
    let jolt = if (SLAM..SLAM + 0.2).contains(&t) {
        let k = (1.0 - (t - SLAM) / 0.2) * 3.0;
        let n = (t * 240.0) as u32;
        (
            ((hash(n) - 0.5) * 2.0 * k).round() as i32,
            ((hash(n + 7) - 0.5) * 2.0 * k).round() as i32,
        )
    } else {
        (0, 0)
    };

    // Floor: fades in, fades out during the flight.
    let floor_alpha = if t < FLY_START {
        clamp01(t / FLOOR_IN)
    } else {
        1.0 - clamp01((t - FLY_START) / (LAND - FLY_START))
    };
    let flash = (SLAM..SLAM + 0.035).contains(&t);
    draw_floor(fb, t, floor_alpha, jolt, look, flash);

    // Copper bar racing from the bottom to the horizon after the slam.
    if (SLAM..SLAM + 0.3).contains(&t) {
        let p = (t - SLAM) / 0.3;
        let by = fb.h as i32 - (p * (fb.h as i32 - HORIZON) as f32) as i32;
        draw_copper_bar(fb, by, look, 1.0 - p * 0.5);
    }

    // Letters.
    if t >= APPROACH_START {
        let (cx, cy, sc, angle, flash_color) = if t < SLAM {
            let p = ease_in(clamp01((t - APPROACH_START) / (SLAM - APPROACH_START)));
            (
                w / 2.0,
                HORIZON as f32 + 4.0 + (BIG_CY - HORIZON as f32 - 4.0) * p,
                1.0 + (BIG_SCALE - 1.0) * p,
                spin_angle(t),
                None,
            )
        } else if t < FLY_START {
            let d = t - SLAM;
            let over = if d < 0.15 {
                1.3 * (d / 0.15 * std::f32::consts::PI).sin()
            } else {
                0.0
            };
            (w / 2.0, BIG_CY, BIG_SCALE + over, 0.0, None)
        } else if t < LAND {
            let p = ease_in_out((t - FLY_START) / (LAND - FLY_START));
            let arc = -34.0 * (p * std::f32::consts::PI).sin();
            (
                w / 2.0 + (tag_cx - w / 2.0) * p,
                BIG_CY + (tag_cy - BIG_CY) * p + arc,
                BIG_SCALE + (s as f32 - BIG_SCALE) * p,
                0.0,
                None,
            )
        } else {
            let flash = if t < LAND + 0.05 {
                Some(0xffffff)
            } else {
                None
            };
            (tag_cx, tag_cy, s as f32, 0.0, flash)
        };
        draw_letters(fb, cx, cy, sc, angle, jolt, look.stops, flash_color);

        // Light sweep across the big letters while they hold.
        if (SLAM + 0.2..SLAM + 0.55).contains(&t) {
            let p = (t - SLAM - 0.2) / 0.35;
            let gx = w / 2.0 - COLS as f32 * BIG_SCALE / 2.0 - 10.0
                + p * (COLS as f32 * BIG_SCALE + 20.0);
            for (col, row) in path() {
                let px = w / 2.0 + (col as f32 - COLS as f32 / 2.0) * BIG_SCALE;
                let d = (px - gx).abs();
                if d < 12.0 {
                    let a = 1.0 - d / 12.0;
                    let py = BIG_CY + (row as f32 - ROWS as f32 / 2.0) * BIG_SCALE;
                    fb.rect(
                        px.round() as i32 + jolt.0,
                        py.round() as i32 + jolt.1,
                        BIG_SCALE as i32,
                        BIG_SCALE as i32,
                        add(final_color(row, look.stops), scale(0xffffff, a * 0.8)),
                    );
                }
            }
        }

        // Dust bursting outward along the floor from the letters' base.
        if (SLAM..SLAM + 0.6).contains(&t) {
            let d = t - SLAM;
            let base_y = BIG_CY + ROWS as f32 / 2.0 * BIG_SCALE + 2.0;
            for i in 0..24u32 {
                let a = (i as f32 / 24.0) * std::f32::consts::TAU + hash(i) * 0.3;
                let speed = 90.0 + 80.0 * hash(i + 100);
                let px = w / 2.0 + a.cos() * speed * d;
                let py = base_y + a.sin().abs() * speed * d * 0.35;
                let life = 1.0 - d / 0.6;
                let c = lerp_color(look.yellow, look.orange, hash(i + 200));
                fb.put(px as i32 + jolt.0, py as i32 + jolt.1, scale(c, life));
            }
        }

        // Landing sparkle.
        if (LAND..LAND + 0.15).contains(&t) {
            let d = (t - LAND) / 0.15;
            for i in 0..8u32 {
                let a = i as f32 / 8.0 * std::f32::consts::TAU + 0.4;
                let r = 4.0 + 16.0 * d;
                let px = tag_cx + a.cos() * r;
                let py = tag_cy + a.sin() * r * 0.6;
                fb.put(px as i32, py as i32, scale(0xffffff, 1.0 - d));
            }
        }
    }
}

/// Resting tag with a quiet glint every few seconds.
fn draw_settled(fb: &mut Framebuffer, x: i32, y: i32, s: i32, t: f32, look: &Look) {
    let p = path();
    for &(cx, cy) in &p {
        fb.rect(x + cx * s, y + cy * s, s, s, final_color(cy, look.stops));
    }
    let since = (t - TOTAL_SECS) % GLINT_EVERY;
    if since < GLINT_SECS {
        let u = since / GLINT_SECS;
        let gx = x - 3 * s + (u * (COLS + 6) as f32 * s as f32) as i32;
        for &(cx, cy) in &p {
            let px = x + cx * s;
            let d = (px - gx).abs();
            if d < 3 * s {
                let a = (1.0 - d as f32 / (3 * s) as f32) * 0.55;
                fb.rect(
                    px,
                    y + cy * s,
                    s,
                    s,
                    add(final_color(cy, look.stops), scale(0xffffff, a * 0.9)),
                );
            }
        }
    }
}

/// The soundtrack of the show, on the same timeline as the picture: a riser
/// with a rumble while the floor rushes in, a whoosh pulsing with the spin,
/// a slam (kick, crash, metallic ring) and a chord stab, a downward whoosh
/// for the flight and a three note sparkle on landing.
pub fn synth(rate: u32) -> Vec<f32> {
    let n = (TOTAL_SECS * rate as f32) as usize;
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
    let (mut lp_riser, mut lp_rumble, mut lp_whoosh, mut lp_fly_a, mut lp_fly_b, mut lp_crash) =
        (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let onepole = |cut: f32| -> f32 {
        let rc = 1.0 / (tau * cut.max(20.0));
        let dt = 1.0 / sr;
        dt / (rc + dt)
    };
    let chord = [329.63f32, 415.30, 493.88, 659.26];
    let bells = [
        (LAND, 1318.5f32),
        (LAND + 0.06, 1661.2),
        (LAND + 0.12, 1975.5),
    ];

    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / sr;
        let mut v = 0.0;
        let white = rnd();

        // Riser and rumble while the letters approach.
        if t < SLAM {
            let p = t / SLAM;
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
            // Whoosh pulsing with the spin.
            if t >= APPROACH_START {
                let aw = onepole(1800.0);
                lp_whoosh += aw * (white - lp_whoosh);
                let pulse = 0.5 + 0.5 * spin_angle(t).cos();
                v += lp_whoosh * 0.14 * pulse * clamp01((t - APPROACH_START) / 0.2);
            }
        }

        // Slam: kick, crash, metallic ring.
        let d = t - SLAM;
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
        let dc = t - (SLAM + 0.05);
        if (0.0..0.35).contains(&dc) {
            let env = (-dc * 9.0).exp();
            for f in chord {
                let sq = if (dc * f).fract() < 0.5 { 1.0 } else { -1.0 };
                v += sq * 0.05 * env;
            }
        }

        // Flight: band-passed whoosh sweeping down.
        if (FLY_START..LAND).contains(&t) {
            let p = (t - FLY_START) / (LAND - FLY_START);
            let cut = 3000.0 * (1.0 - p) + 300.0;
            let a_hi = onepole(cut);
            let a_lo = onepole(cut * 0.5);
            lp_fly_a += a_hi * (white - lp_fly_a);
            lp_fly_b += a_lo * (white - lp_fly_b);
            v += (lp_fly_a - lp_fly_b) * 0.5 * (p * std::f32::consts::PI).sin();
        }

        // Landing sparkle: three quick bells.
        for (at, f) in bells {
            let db = t - at;
            if (0.0..0.25).contains(&db) {
                v += (tau * f * db).sin() * (-db * 22.0).exp() * 0.12;
            }
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
