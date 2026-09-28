//! The drawing idioms of a 16-bit menu, on top of the framebuffer.
//!
//! A console with a fixed palette had no alpha and no smooth gradients: a
//! soft edge was two colours mixed through an ordered dither, and a window
//! looked raised because one edge was lit and the other was not. Everything
//! here follows that, so the menus sit in the same world as the sky and the
//! boot, which were drawn that way from the start.
//!
//! Every colour is derived from the theme. A light theme turns the shading
//! round, because a shadow on a pale ground is the foreground, not black.

use crate::fb::{Color, Framebuffer, lerp_color, scale};
use omacrt_shell::colour::ch;
use omacrt_shell::theme::Theme;

/// The ordered dither of every home computer that had to fake a gradient.
pub const BAYER: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/// A pixel of `a` or of `b` depending on where it is, so that a fraction
/// between the two colours reads as a mix from a distance.
#[inline]
pub fn dither(x: i32, y: i32, t: f32, a: Color, b: Color) -> Color {
    let level = (t.clamp(0.0, 1.0) * 16.0) as u8;
    if level > BAYER[(y & 3) as usize][(x & 3) as usize] {
        b
    } else {
        a
    }
}

/// How many shades a gradient is quantised to between two stops. A palette
/// of the period had a handful of entries free for a sky or a window, not
/// two hundred and fifty-six, and the steps are part of the look.
const LEVELS: f32 = 6.0;

/// The colour of one position `t` along a quantised, dithered blend.
#[inline]
fn stepped(x: i32, y: i32, t: f32, a: Color, b: Color) -> Color {
    let q = t.clamp(0.0, 1.0) * LEVELS;
    let lo = q.floor();
    let ca = lerp_color(a, b, lo / LEVELS);
    let cb = lerp_color(a, b, ((lo + 1.0) / LEVELS).min(1.0));
    dither(x, y, q - lo, ca, cb)
}

/// Top to bottom through the stops, in steps, mixed by the dither.
pub fn gradient_v(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, stops: &[Color]) {
    if stops.is_empty() || w <= 0 || h <= 0 {
        return;
    }
    if stops.len() == 1 {
        fb.rect(x, y, w, h, stops[0]);
        return;
    }
    let n = (stops.len() - 1) as f32;
    for j in 0..h {
        let f = j as f32 / (h - 1).max(1) as f32 * n;
        let k = (f.floor() as usize).min(stops.len() - 2);
        let (a, b) = (stops[k], stops[k + 1]);
        let t = f - k as f32;
        for i in 0..w {
            fb.put(x + i, y + j, stepped(x + i, y + j, t, a, b));
        }
    }
}

/// Left to right from `a` to `b`, in steps, mixed by the dither.
pub fn gradient_h(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, a: Color, b: Color) {
    for i in 0..w.max(0) {
        let t = i as f32 / (w - 1).max(1) as f32;
        for j in 0..h.max(0) {
            fb.put(x + i, y + j, stepped(x + i, y + j, t, a, b));
        }
    }
}

fn luma(c: Color) -> f32 {
    0.2126 * ch(c, 16) + 0.7152 * ch(c, 8) + 0.0722 * ch(c, 0)
}

/// The shades a raised surface needs, worked out once from the theme.
#[derive(Clone, Copy, Debug)]
pub struct Tones {
    /// The lit edge of anything raised.
    pub hi: Color,
    /// The dark rim around it, and the shaded edge.
    pub lo: Color,
    /// A panel's fill, top and bottom.
    pub panel: Color,
    pub panel2: Color,
    /// A drop shadow under text and under the wordmark.
    pub shadow: Color,
}

impl Tones {
    pub fn of(th: &Theme) -> Self {
        let light = luma(th.bg) > 128.0;
        let ink = if light { th.fg } else { 0x000000 };
        Tones {
            hi: lerp_color(th.selection, th.paper, 0.55),
            lo: lerp_color(th.bg, ink, if light { 0.35 } else { 0.6 }),
            panel: lerp_color(th.bg, th.selection, 0.55),
            panel2: lerp_color(th.bg, th.selection, 0.25),
            shadow: lerp_color(th.bg, ink, if light { 0.25 } else { 0.7 }),
        }
    }
}

/// A window edge: a dark line outside, a lit edge and a shaded edge inside,
/// corners taken off so it reads as a bevel and not as a box. Sunken swaps
/// the light and the shade, which is how a slot or a readout is drawn.
#[allow(clippy::too_many_arguments)]
pub fn bevel(
    fb: &mut Framebuffer,
    t: &Tones,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    top: Color,
    bottom: Color,
    raised: bool,
) {
    if w < 4 || h < 4 {
        return;
    }
    let (hi, lo) = if raised { (t.hi, t.lo) } else { (t.lo, t.hi) };
    gradient_v(fb, x + 1, y + 1, w - 2, h - 2, &[top, bottom]);
    fb.rect(x + 1, y, w - 2, 1, t.lo);
    fb.rect(x + 1, y + h - 1, w - 2, 1, t.lo);
    fb.rect(x, y + 1, 1, h - 2, t.lo);
    fb.rect(x + w - 1, y + 1, 1, h - 2, t.lo);
    fb.rect(x + 1, y + 1, w - 2, 1, hi);
    fb.rect(x + 1, y + 1, 1, h - 2, hi);
    let shade = lerp_color(bottom, lo, 0.6);
    fb.rect(x + 2, y + h - 2, w - 3, 1, shade);
    fb.rect(x + w - 2, y + 2, 1, h - 3, shade);
}

/// Text standing on a one pixel shadow, down and to the right.
pub fn text_shadow(fb: &mut Framebuffer, x: i32, y: i32, s: &str, c: Color, shadow: Color) {
    fb.text(x + 1, y + 1, s, shadow, 1);
    fb.text(x, y, s, c, 1);
}

/// The selection as a lit key: an accent bar with light on its top edge, a
/// shade on its bottom, a notch at the left the eye finds before it reads a
/// word, and a soft sheen at `shine` (0 to 1 along the bar, outside it off).
#[allow(clippy::too_many_arguments)]
pub fn select_bar(
    fb: &mut Framebuffer,
    th: &Theme,
    t: &Tones,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    shine: f32,
) {
    if w < 6 || h < 4 {
        return;
    }
    let top = lerp_color(th.selection, th.accent, 0.42);
    let mid = lerp_color(th.selection, th.accent, 0.18);
    gradient_v(fb, x + 1, y, w - 2, h, &[top, mid, th.selection]);
    fb.rect(x, y + 1, 1, h - 2, mid);
    fb.rect(x + w - 1, y + 1, 1, h - 2, th.selection);
    fb.rect(x + 1, y, w - 2, 1, lerp_color(th.accent, th.paper, 0.35));
    fb.rect(
        x + 1,
        y + h - 1,
        w - 2,
        1,
        lerp_color(th.selection, t.lo, 0.5),
    );
    fb.rect(x, y + 2, 2, h - 4, th.accent);
    if (0.0..=1.0).contains(&shine) {
        let gx = x + (w as f32 * shine) as i32;
        for j in 1..h - 1 {
            for i in -6..=6 {
                let px = gx + i - j / 2;
                if px > x + 2 && px < x + w - 2 {
                    let k = (1.0 - i.abs() as f32 / 7.0) * 0.2;
                    let p = fb.at(px, y + j);
                    fb.put(px, y + j, lerp_color(p, th.paper, k));
                }
            }
        }
    }
}

/// A pad button as a button: a round cap in its own colour with the letter
/// cut out of it. Returns the width it took.
pub fn keycap(fb: &mut Framebuffer, x: i32, y: i32, key: char, cap: Color, ink: Color) -> i32 {
    fb.rect(x + 1, y, 7, 9, cap);
    fb.rect(x, y + 1, 9, 7, cap);
    fb.rect(x + 2, y, 5, 1, lerp_color(cap, 0xffffff, 0.35));
    fb.rect(x + 2, y + 8, 5, 1, scale(cap, 0.6));
    fb.glyph(x + 1, y + 1, key, ink, 1);
    9
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dither_at_half_covers_half() {
        let lit = (0..4)
            .flat_map(|y| (0..4).map(move |x| (x, y)))
            .filter(|&(x, y)| dither(x, y, 0.5, 0, 1) == 1)
            .count();
        assert_eq!(lit, 8, "half of a 4x4 Bayer cell is eight of its sixteen");
        assert!((0..16).all(|i| dither(i % 4, i / 4, 0.0, 0, 1) == 0));
        assert!((0..16).all(|i| dither(i % 4, i / 4, 1.0, 0, 1) == 1));
    }

    #[test]
    fn a_gradient_starts_and_ends_on_its_stops() {
        let mut fb = Framebuffer::new(8, 16);
        gradient_v(&mut fb, 0, 0, 8, 16, &[0x000000, 0xffffff]);
        assert!((0..8).all(|x| fb.at(x, 0) == 0x000000));
        assert!((0..8).all(|x| fb.at(x, 15) == 0xffffff));
    }

    #[test]
    fn a_light_theme_puts_its_shadows_towards_the_ink() {
        let mut th = Theme::tokyo_night();
        let dark = Tones::of(&th);
        assert!(luma(dark.shadow) < luma(th.bg) + 1.0);
        th.bg = 0xeff1f5;
        th.fg = 0x4c4f69;
        let pale = Tones::of(&th);
        // Darker than the ground, but not black: the foreground, mixed in.
        assert!(luma(pale.shadow) < luma(th.bg));
        assert!(luma(pale.shadow) > 60.0);
    }
}
