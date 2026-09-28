//! A small lit stage for the thing that is selected: a console, a box.
//!
//! A dark room, a lamp above that throws a cone of dithered light, and a
//! floor drawn in perspective in the colour of the brand, with its lines
//! running to one point the way a Mode 7 floor did. What stands on it is
//! either a sprite built here from rects and pixel runs, lit from the lamp
//! with a one pixel rim, or a picture with a shadow under it.
//!
//! Colours come from ramps: five steps around a colour, the shadows leaning
//! violet and the lights leaning warm, which is how a pixel artist shades
//! instead of only darkening.

use crate::fb::{Color, Framebuffer, lerp_color, rgb};
use crate::paint::{BAYER, gradient_v, mix};
use omacrt_shell::colour::ch;
use omacrt_shell::theme::Theme;

fn to_hls(c: Color) -> (f32, f32, f32) {
    let (r, g, b) = (ch(c, 16) / 255.0, ch(c, 8) / 255.0, ch(c, 0) / 255.0);
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let l = (mx + mn) / 2.0;
    if (mx - mn).abs() < 1e-6 {
        return (0.0, l, 0.0);
    }
    let d = mx - mn;
    let s = if l > 0.5 {
        d / (2.0 - mx - mn)
    } else {
        d / (mx + mn)
    };
    let h = if mx == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if mx == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } / 6.0;
    (h, l, s)
}

fn from_hls(h: f32, l: f32, s: f32) -> Color {
    let l = l.clamp(0.0, 1.0);
    let s = s.clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h.rem_euclid(1.0) * 6.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let q = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    rgb(q(r), q(g), q(b))
}

fn toward(h: f32, target: f32, amount: f32) -> f32 {
    let d = (target - h + 0.5).rem_euclid(1.0) - 0.5;
    (h + d * amount).rem_euclid(1.0)
}

/// Five steps around `c`: two of shadow, the colour, two of light.
pub fn ramp(c: Color) -> [Color; 5] {
    const COOL: f32 = 250.0 / 360.0;
    const WARM: f32 = 48.0 / 360.0;
    let (h, l, s) = to_hls(c);
    let mut out = [0; 5];
    for (k, slot) in out.iter_mut().enumerate() {
        let t = (k as f32 - 2.0) / 2.0;
        *slot = if t < 0.0 {
            let a = -t;
            from_hls(
                toward(h, COOL, 0.18 * a),
                l + (l * 0.30 - l) * a,
                s * (1.0 + 0.15 * a),
            )
        } else {
            let top = (l + (1.0 - l) * 0.62).min(0.95);
            from_hls(
                toward(h, WARM, 0.14 * t),
                l + (top - l) * t,
                s * (1.0 - 0.25 * t),
            )
        };
    }
    out
}

/// The room, the lamp and the floor. Returns where the floor starts, and
/// the ramp it was drawn from so what stands on it can be lit to match.
pub fn draw(
    fb: &mut Framebuffer,
    th: &Theme,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    brand: Color,
) -> (i32, [Color; 5]) {
    let br = ramp(lerp_color(th.bg, brand, 0.55));
    let deep = 46.min(h / 3);
    let fy = y + h - deep;
    gradient_v(
        fb,
        x,
        y,
        w,
        h - deep,
        &[th.bg, lerp_color(th.bg, br[0], 0.6), br[0]],
    );
    // The floor: rows drawing together towards the horizon, and lines that
    // all run to the one point under the lamp.
    fb.rect(x, fy, w, deep, lerp_color(th.bg, br[0], 0.7));
    let mut step = 1.0f32;
    let mut row = fy as f32;
    while (row as i32) < y + h {
        fb.rect(x, row as i32, w, 1, br[1]);
        step *= 1.35;
        row += step;
    }
    let vx = x + w / 2;
    for i in -10..=10 {
        for j in 0..deep {
            let px = vx + i * 9 * (j + 4) / 12;
            if px >= x && px < x + w {
                fb.put(px, fy + j, br[1]);
            }
        }
    }
    fb.rect(x, fy, w, 1, br[2]);
    // The lamp: a cone of light from above, denser in its heart, laid over
    // what is there through the dither.
    for j in 0..h {
        let half = 10.0 + j as f32 * 0.55;
        let row_cov = 0.30 * (1.0 - j as f32 / h as f32) + 0.10;
        let py = y + j;
        for i in -(half as i32)..=(half as i32) {
            let px = vx + i;
            if px < x || px >= x + w {
                continue;
            }
            let edge = 1.0 - i.abs() as f32 / half;
            let cov = row_cov * (edge * 2.2).min(1.0);
            if cov * 16.0 > BAYER[(py & 3) as usize][(px & 3) as usize] as f32 {
                let p = fb.at(px, py);
                fb.put(px, py, lerp_color(p, br[4], 0.28));
            }
        }
    }
    // A lit top edge and a dark bottom one, so the stage is a place.
    fb.rect(x, y, w, 1, lerp_color(br[3], th.paper, 0.3));
    fb.rect(x, y + h - 1, w, 1, crate::paint::Tones::of(th).lo);
    (fy, br)
}

/// A soft shadow on the floor, an ellipse laid in through the dither.
pub fn shadow(fb: &mut Framebuffer, th: &Theme, cx: i32, y: i32, half_w: i32, rows: i32) {
    for j in 0..rows {
        let f = j as f32 / rows as f32;
        let half = (half_w as f32 * (1.0 - f * f).sqrt()) as i32;
        mix(fb, cx - half, y + j, 2 * half, 1, th.bg, 0.7 - f * 0.4);
    }
}

/// Whether a console has a sprite of its own rather than a picture.
pub fn has_sprite(name: &str) -> bool {
    matches!(name, "snes" | "sfc")
}

/// A sprite for a console, when there is one. Returns its size if it drew.
pub fn console(
    fb: &mut Framebuffer,
    name: &str,
    x: i32,
    y: i32,
    light: Color,
) -> Option<(i32, i32)> {
    match name {
        "snes" | "sfc" => {
            snes(fb, x, y, light);
            Some((SNES_W, SNES_H))
        }
        _ => None,
    }
}

pub const SNES_W: i32 = 99;
pub const SNES_H: i32 = 50;

/// A PAL Super Nintendo from the front and above, lit from a lamp above and
/// to the left. Designed on a 66 by 33 grid and drawn half as large again,
/// every line kept one pixel.
fn snes(fb: &mut Framebuffer, x: i32, y: i32, light: Color) {
    let k = 1.5f32;
    let at = |v: i32| (v as f32 * k).round() as i32;
    let sz = |v: i32| ((v as f32 * k).round() as i32).max(1);
    let rect = |fb: &mut Framebuffer, a: i32, b: i32, w: i32, h: i32, c: Color| {
        fb.rect(x + at(a), y + at(b), sz(w), sz(h), c)
    };
    let g = ramp(rgb(178, 178, 186));
    let dg = ramp(rgb(96, 96, 104));
    let (w, top, front) = (66, 24, 9);
    // The top face, its back corners rounded off.
    for j in 0..top {
        let inset = if j < 3 { 3 - j } else { 0 };
        rect(fb, inset, j, w - 2 * inset, 1, g[3]);
    }
    // The front face, darker, its lower edge rounded too.
    for j in 0..front {
        let inset = if j == front - 1 { 1 } else { 0 };
        rect(fb, inset, top + j, w - 2 * inset, 1, g[1]);
    }
    rect(fb, 0, top, w, 1, g[2]);
    // The raised deck around the slot.
    rect(fb, 14, 3, 38, 14, g[3]);
    rect(fb, 14, 3, 38, 1, g[4]);
    rect(fb, 14, 3, 1, 14, g[4]);
    rect(fb, 14, 16, 38, 1, g[2]);
    rect(fb, 51, 4, 1, 13, g[2]);
    // The cartridge slot: a lit lip, then the dark mouth.
    rect(fb, 21, 6, 24, 1, g[4]);
    rect(fb, 21, 7, 24, 3, g[0]);
    rect(fb, 22, 8, 22, 1, lerp_color(g[0], 0, 0.5));
    // Power and reset: dark sliders in their tracks.
    for sx in [4, 55] {
        rect(fb, sx, 9, 7, 6, g[2]);
        rect(fb, sx + 1, 10, 5, 4, dg[1]);
        rect(fb, sx + 1, 10, 3, 3, dg[3]);
        rect(fb, sx + 1, 10, 3, 1, dg[4]);
    }
    // Eject, below the slot.
    rect(fb, 28, 17, 10, 3, dg[2]);
    rect(fb, 28, 17, 10, 1, dg[4]);
    // The seam around the case.
    rect(fb, 2, 21, w - 4, 1, g[2]);
    // Front: two controller ports, four coloured dots and the power lamp.
    for px in [20, 38] {
        rect(fb, px, top + 2, 9, 5, g[0]);
        rect(fb, px + 1, top + 3, 7, 1, dg[2]);
        rect(fb, px, top + 1, 9, 1, g[2]);
    }
    for (i, c) in [
        rgb(220, 50, 60),
        rgb(240, 200, 40),
        rgb(60, 170, 80),
        rgb(60, 110, 220),
    ]
    .into_iter()
    .enumerate()
    {
        fb.put(x + at(6 + i as i32 * 2), y + at(top + 4), c);
    }
    fb.put(x + at(60), y + at(top + 4), rgb(230, 40, 40));
    // The rim of light from the lamp: the top edge and the left edge.
    for i in 3..w - 3 {
        fb.put(x + at(i), y, light);
        fb.put(x + at(i) + 1, y, light);
    }
    let side = lerp_color(light, g[3], 0.4);
    for j in 3..top {
        fb.put(x, y + at(j), side);
        fb.put(x, y + at(j) + 1, side);
    }
    fb.put(x + at(1), y + at(2), light);
    fb.put(x + at(2), y + at(1), light);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ramp_runs_from_dark_to_light_through_its_colour() {
        let c = rgb(0x8d, 0x6b, 0xd9);
        let r = ramp(c);
        let l = |c: Color| to_hls(c).1;
        assert!(l(r[0]) < l(r[1]) && l(r[1]) < l(r[2]) && l(r[2]) < l(r[3]) && l(r[3]) < l(r[4]));
        let (h, _, _) = to_hls(c);
        let (h2, _, _) = to_hls(r[2]);
        assert!(
            (h - h2).abs() < 0.01,
            "the middle step is the colour itself"
        );
    }

    #[test]
    fn hls_goes_there_and_back() {
        for c in [0x7aa2f7, 0xe0af68, 0x9ece6a, 0x0b0d14, 0xffffff] {
            let (h, l, s) = to_hls(c);
            let back = from_hls(h, l, s);
            for sh in [16, 8, 0] {
                assert!(
                    (ch(c, sh) - ch(back, sh)).abs() <= 1.0,
                    "{c:06x} came back {back:06x}"
                );
            }
        }
    }
}
