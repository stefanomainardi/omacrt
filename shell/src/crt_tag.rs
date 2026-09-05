//! The "CRT" tag under the wordmark, revealed the way a vector monitor draws:
//! a hot electron dot traces the strokes of C, R and T in one continuous
//! pass, the phosphor trail cools behind it, each finished letter flashes and
//! rings a note of a rising arpeggio, and a bass stamp locks the word in place
//! while a glint sweeps across it. Afterwards the tag idles with a faint glint
//! every few seconds, like polished metal catching the light.
//!
//! Picture and sound share the same stroke path, so the whine of the beam
//! follows the dot on screen exactly.

use crate::fb::{Color, Framebuffer, add, lerp_color, scale};

/// Letter glyphs, 5x7, stroke order matters: the dot follows these lists.
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

/// Canvas size in glyph pixels: three letters, one column apart, each one row
/// lower than the previous (stair-step slant).
pub const COLS: i32 = 17;
pub const ROWS: i32 = 9;
pub const TRACE_SECS: f32 = 0.9;
pub const TOTAL_SECS: f32 = 1.45;
const GLINT_EVERY: f32 = 9.0;
const GLINT_SECS: f32 = 0.35;

/// The whole stroke path in canvas coordinates, with the letter index.
pub fn path() -> Vec<(i32, i32, u8)> {
    let mut out = Vec::with_capacity(42);
    for (i, glyph) in [&C[..], &R[..], &T[..]].iter().enumerate() {
        for (x, y) in glyph.iter() {
            out.push((x + i as i32 * 6, y + i as i32, i as u8));
        }
    }
    out
}

/// Seconds at which each letter is complete, derived from the path.
pub fn letter_done_times() -> [f32; 3] {
    let p = path();
    let n = p.len() as f32;
    let mut out = [0.0; 3];
    for letter in 0..3u8 {
        let last = p.iter().rposition(|&(_, _, l)| l == letter).unwrap_or(0) as f32;
        out[letter as usize] = (last + 1.0) / n * TRACE_SECS;
    }
    out
}

/// Vertical gradient like the wordmark: bottom stop first.
fn final_color(row: i32, stops: [Color; 3]) -> Color {
    let f = 1.0 - row as f32 / (ROWS - 1) as f32;
    if f < 0.5 {
        lerp_color(stops[0], stops[1], f * 2.0)
    } else {
        lerp_color(stops[1], stops[2], (f - 0.5) * 2.0)
    }
}

/// Draw the tag at local time `t` (seconds since the reveal started); `s` is
/// the pixel size of one glyph pixel. `accent` colors the beam and glints.
pub fn draw(
    fb: &mut Framebuffer,
    x: i32,
    y: i32,
    s: i32,
    t: f32,
    stops: [Color; 3],
    accent: Color,
) {
    let p = path();
    let n = p.len();
    let done = letter_done_times();
    let white = 0xffffff;

    if t < TRACE_SECS {
        // How far along the path the dot is (fractional for smooth timing).
        let pos = t / TRACE_SECS * n as f32;
        let idx = pos.floor() as usize;
        let step = TRACE_SECS / n as f32;
        for (k, &(cx, cy, letter)) in p.iter().enumerate() {
            if k > idx {
                break;
            }
            let age = (idx - k) as f32 * step;
            let flash = done[letter as usize] - t < 0.07 && done[letter as usize] > t;
            let base = final_color(cy, stops);
            let color = if flash {
                white
            } else if k == idx {
                white
            } else {
                // Phosphor cool-down: white, then the accent, then the final color.
                let a = (age / 0.28).min(1.0);
                if a < 0.4 {
                    lerp_color(white, accent, a / 0.4)
                } else {
                    lerp_color(accent, base, (a - 0.4) / 0.6)
                }
            };
            fb.rect(x + cx * s, y + cy * s, s, s, color);
        }
        // Beam halo around the dot.
        if let Some(&(cx, cy, _)) = p.get(idx.min(n - 1)) {
            let hx = x + cx * s;
            let hy = y + cy * s;
            fb.rect_add(hx - s, hy - s, 3 * s, 3 * s, scale(accent, 0.35));
            fb.rect_add(hx - 2 * s, hy, 5 * s, s, scale(accent, 0.15));
            fb.rect_add(hx, hy - 2 * s, s, 5 * s, scale(accent, 0.15));
        }
        return;
    }

    // Settled letters, with a one frame impact jolt right after the trace.
    let jolt = if t < TRACE_SECS + 0.05 { 1 } else { 0 };
    for &(cx, cy, _) in &p {
        fb.rect(x + cx * s + jolt, y + cy * s, s, s, final_color(cy, stops));
    }

    // Lock-in glint sweeping left to right, then a quiet one every few seconds.
    let glint_u = if t < TOTAL_SECS {
        Some((t - TRACE_SECS) / (TOTAL_SECS - TRACE_SECS))
    } else {
        let since = (t - TOTAL_SECS) % GLINT_EVERY;
        (since < GLINT_SECS).then(|| since / GLINT_SECS)
    };
    if let Some(u) = glint_u {
        let strength = if t < TOTAL_SECS { 1.0 } else { 0.55 };
        let gx = x - 3 * s + (u * (COLS + 6) as f32 * s as f32) as i32;
        for &(cx, cy, _) in &p {
            let px = x + cx * s;
            let d = (px - gx).abs();
            if d < 3 * s {
                let a = (1.0 - d as f32 / (3 * s) as f32) * strength;
                let c = add(final_color(cy, stops), scale(white, a * 0.9));
                fb.rect(px, y + cy * s, s, s, c);
            }
        }
    }
}

/// The sound of the reveal, sharing the path timing with the picture:
/// a beam whine whose pitch follows the dot's height, a rising square wave
/// arpeggio as each letter completes, and a bass stamp with a noise crack
/// when the word locks.
pub fn synth(rate: u32) -> Vec<f32> {
    let n = (TOTAL_SECS * rate as f32) as usize;
    let mut out = vec![0.0f32; n];
    let p = path();
    let done = letter_done_times();
    let tau = 2.0 * std::f32::consts::PI;
    let mut phase = 0.0f32;
    let mut noise = 0x9e37_79b9u32;
    let mut rnd = || {
        noise ^= noise << 13;
        noise ^= noise >> 17;
        noise ^= noise << 5;
        (noise >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    };
    // E major arpeggio, sixteen-bit style: E5, G#5, B5.
    let notes = [659.26f32, 830.61, 987.77];
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / rate as f32;
        let mut v = 0.0;

        // Beam whine while tracing.
        if t < TRACE_SECS {
            let k = ((t / TRACE_SECS) * p.len() as f32) as usize;
            let row = p[k.min(p.len() - 1)].1 as f32;
            let vib = 1.0 + 0.012 * (tau * 6.0 * t).sin();
            let f = (520.0 + (ROWS as f32 - 1.0 - row) / (ROWS as f32 - 1.0) * 620.0) * vib;
            phase = (phase + f / rate as f32).fract();
            // Triangle wave, thin like an oscilloscope.
            let tri = 4.0 * (phase - 0.5).abs() - 1.0;
            let env =
                (t / 0.02).min(1.0) * (1.0 - ((t - (TRACE_SECS - 0.05)) / 0.05).clamp(0.0, 1.0));
            v += tri * 0.045 * env;
        }

        // Arpeggio: one note per finished letter.
        for (li, &at) in done.iter().enumerate() {
            let dt = t - at;
            if (0.0..0.22).contains(&dt) {
                let f = notes[li];
                let sq = if (dt * f).fract() < 0.5 { 1.0 } else { -1.0 };
                let sub = (tau * f * 0.5 * dt).sin();
                let env = (-dt * 14.0).exp();
                v += (sq * 0.6 + sub * 0.4) * 0.11 * env;
            }
        }

        // Stamp: pitch drop plus a crack when the trace ends.
        let dt = t - TRACE_SECS;
        if dt >= 0.0 {
            let f = 150.0 * (-dt * 6.0).exp() + 42.0;
            let body = (tau * f * dt).sin() * (-dt * 9.0).exp() * 0.5;
            let crack = rnd() * (-dt * 110.0).exp() * 0.25;
            v += body + crack;
        }
        *s = v.clamp(-1.0, 1.0);
    }
    out
}
