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

/// A pen that draws in design units: sprites are designed on a small grid
/// and drawn half as large again, every line still one pixel wide.
struct Pen<'a> {
    fb: &'a mut Framebuffer,
    x: i32,
    y: i32,
    k: f32,
}

impl Pen<'_> {
    fn at(&self, v: i32) -> i32 {
        (v as f32 * self.k).round() as i32
    }
    fn rect(&mut self, a: i32, b: i32, w: i32, h: i32, c: Color) {
        let (x0, y0) = (self.x + self.at(a), self.y + self.at(b));
        let (x1, y1) = (self.x + self.at(a + w), self.y + self.at(b + h));
        self.fb.rect(x0, y0, (x1 - x0).max(1), (y1 - y0).max(1), c);
    }
    /// A line across, one real pixel tall whatever the scale.
    fn hline(&mut self, a: i32, b: i32, w: i32, c: Color) {
        let (x0, x1) = (self.x + self.at(a), self.x + self.at(a + w));
        self.fb
            .rect(x0, self.y + self.at(b), (x1 - x0).max(1), 1, c);
    }
    /// A line down, one real pixel wide.
    fn vline(&mut self, a: i32, b: i32, h: i32, c: Color) {
        let (y0, y1) = (self.y + self.at(b), self.y + self.at(b + h));
        self.fb
            .rect(self.x + self.at(a), y0, 1, (y1 - y0).max(1), c);
    }
    fn dot(&mut self, a: i32, b: i32, c: Color) {
        let (x, y) = (self.x + self.at(a), self.y + self.at(b));
        self.fb.put(x, y, c);
    }
    fn ellipse(&mut self, cx: i32, cy: i32, rx: i32, ry: i32, c: Color) {
        let (px, py) = (self.x + self.at(cx), self.y + self.at(cy));
        let (rx, ry) = (rx as f32 * self.k, ry as f32 * self.k);
        for dy in -(ry as i32)..=(ry as i32) {
            let f = dy as f32 / ry;
            let half = (rx * (1.0 - f * f).max(0.0).sqrt()).round() as i32;
            self.fb.rect(px - half, py + dy, 2 * half + 1, 1, c);
        }
    }
    /// The edge of an ellipse between two angles (0 is to the right, a
    /// quarter turn is down), one pixel wide.
    #[allow(clippy::too_many_arguments)]
    fn arc(&mut self, cx: i32, cy: i32, rx: i32, ry: i32, from: f32, to: f32, c: Color) {
        let (px, py) = (self.x + self.at(cx), self.y + self.at(cy));
        let (rx, ry) = (rx as f32 * self.k, ry as f32 * self.k);
        let steps = ((rx + ry) * 4.0) as i32;
        for i in 0..=steps {
            let t = from + (to - from) * i as f32 / steps as f32;
            let a = t * std::f32::consts::TAU;
            self.fb.put(
                px + (rx * a.cos()).round() as i32,
                py + (ry * a.sin()).round() as i32,
                c,
            );
        }
    }
    /// The rim of light from the lamp, along the top and the left edge of a
    /// box whose back corners are rounded by `round`.
    fn rim(&mut self, w: i32, top: i32, round: i32, light: Color, side: Color) {
        let (x0, x1) = (self.x + self.at(round), self.x + self.at(w - round));
        self.fb.rect(x0, self.y, x1 - x0, 1, light);
        let (y0, y1) = (self.y + self.at(round), self.y + self.at(top));
        self.fb.rect(self.x, y0, 1, y1 - y0, side);
        for i in 0..round {
            self.dot(round - 1 - i, i + 1, light);
        }
    }
    /// A box seen from the front and above: a top face with its back corners
    /// rounded, a darker front face, and the edge between them lit.
    #[allow(clippy::too_many_arguments)]
    fn case(
        &mut self,
        w: i32,
        top: i32,
        front: i32,
        round: i32,
        top_c: Color,
        front_c: Color,
        edge: Color,
    ) {
        for j in 0..top {
            let inset = (round - j).max(0);
            self.rect(inset, j, w - 2 * inset, 1, top_c);
        }
        for j in 0..front {
            let inset = if j == front - 1 { 1 } else { 0 };
            self.rect(inset, top + j, w - 2 * inset, 1, front_c);
        }
        self.hline(0, top, w, edge);
    }
    /// A controller port: a dark mouth with its pins.
    #[allow(clippy::too_many_arguments)]
    fn port(&mut self, a: i32, b: i32, w: i32, h: i32, mouth: Color, pins: Color, lip: Color) {
        self.hline(a, b - 1, w, lip);
        self.rect(a, b, w, h, mouth);
        self.hline(a + 1, b + 1, w - 2, pins);
    }
}

struct Sprite {
    /// Size on the design grid.
    w: i32,
    h: i32,
    draw: fn(&mut Pen, Color),
}

const SCALE: f32 = 1.5;

fn sprite(name: &str) -> Option<Sprite> {
    Some(match name {
        "snes" | "sfc" => Sprite {
            w: 66,
            h: 33,
            draw: snes,
        },
        "megadrive" | "genesis" | "md" => Sprite {
            w: 66,
            h: 30,
            draw: megadrive,
        },
        "nes" | "famicom" => Sprite {
            w: 66,
            h: 34,
            draw: nes,
        },
        "psx" | "playstation" => Sprite {
            w: 66,
            h: 29,
            draw: psx,
        },
        "n64" => Sprite {
            w: 66,
            h: 31,
            draw: n64,
        },
        _ => return None,
    })
}

/// Stand a console's sprite on the floor at `cx`, with its shadow. False
/// when the console has no sprite and a picture has to do instead.
pub fn stand(
    fb: &mut Framebuffer,
    th: &Theme,
    name: &str,
    cx: i32,
    floor: i32,
    light: Color,
) -> bool {
    let Some(s) = sprite(name) else {
        return false;
    };
    let (w, h) = ((s.w as f32 * SCALE) as i32, (s.h as f32 * SCALE) as i32);
    let (x, y) = (cx - w / 2, floor + 14 - h);
    shadow(fb, th, cx, floor + 12, w / 2 + 6, 6);
    let mut pen = Pen { fb, x, y, k: SCALE };
    (s.draw)(&mut pen, light);
    true
}

const RED_LAMP: Color = 0xe62828;

/// A PAL Super Nintendo: grey, the raised deck around the slot, two dark
/// sliders, the eject button, and four coloured dots on the front.
fn snes(p: &mut Pen, light: Color) {
    let g = ramp(rgb(178, 178, 186));
    let dg = ramp(rgb(96, 96, 104));
    let (w, top, front) = (66, 24, 9);
    p.case(w, top, front, 3, g[3], g[1], g[2]);
    p.rect(14, 3, 38, 14, g[3]);
    p.hline(14, 3, 38, g[4]);
    p.vline(14, 3, 14, g[4]);
    p.hline(14, 16, 38, g[2]);
    p.vline(51, 4, 13, g[2]);
    p.hline(21, 6, 24, g[4]);
    p.rect(21, 7, 24, 3, g[0]);
    p.hline(22, 8, 22, lerp_color(g[0], 0, 0.5));
    for sx in [4, 55] {
        p.rect(sx, 9, 7, 6, g[2]);
        p.rect(sx + 1, 10, 5, 4, dg[1]);
        p.rect(sx + 1, 10, 3, 3, dg[3]);
        p.hline(sx + 1, 10, 3, dg[4]);
    }
    p.rect(28, 17, 10, 3, dg[2]);
    p.hline(28, 17, 10, dg[4]);
    p.hline(2, 21, w - 4, g[2]);
    for px in [20, 38] {
        p.port(px, top + 2, 9, 5, g[0], dg[2], g[2]);
    }
    for (i, c) in [0xdc323c, 0xf0c828, 0x3caa50, 0x3c6edc]
        .into_iter()
        .enumerate()
    {
        p.dot(6 + i as i32 * 2, top + 4, c);
    }
    p.dot(60, top + 4, RED_LAMP);
    p.rim(w, top, 3, light, lerp_color(light, g[3], 0.4));
}

/// The first Mega Drive: black, the raised disc with the gold ring around
/// the slot, the volume slider and the headphone socket at the back left.
fn megadrive(p: &mut Pen, light: Color) {
    let b = ramp(rgb(52, 52, 60));
    let gold = ramp(rgb(200, 160, 64));
    let (w, top, front) = (66, 21, 9);
    p.case(w, top, front, 4, b[2], b[1], b[3]);
    // The disc, raised, lit on its top left and shaded on its bottom right.
    p.ellipse(40, 10, 18, 9, b[3]);
    p.arc(40, 10, 18, 9, 0.5, 0.75, b[4]);
    p.arc(40, 10, 18, 9, 0.0, 0.25, b[1]);
    p.ellipse(40, 10, 13, 6, b[2]);
    // The gold ring on its front half, broken where the letters would be.
    let (cx, cy) = (40, 10);
    let steps = 40;
    for i in 0..steps {
        if i % 4 == 3 {
            continue;
        }
        let t = 0.05 + 0.4 * i as f32 / steps as f32;
        let a = t * std::f32::consts::TAU;
        let (px, py) = (cx as f32 + 15.5 * a.cos(), cy as f32 + 7.5 * a.sin());
        p.dot(px.round() as i32, py.round() as i32, gold[3]);
    }
    // The slot across the disc.
    p.hline(29, 7, 22, b[4]);
    p.rect(29, 8, 22, 3, b[0]);
    // Volume slider and headphone socket.
    p.rect(5, 5, 12, 2, b[0]);
    p.rect(9, 4, 3, 4, b[3]);
    p.hline(9, 4, 3, b[4]);
    p.ellipse(8, 13, 2, 1, b[0]);
    p.dot(8, 12, b[4]);
    // Front: power switch, reset, the red lamp, two ports.
    p.rect(5, top + 2, 7, 4, b[3]);
    p.hline(5, top + 2, 7, b[4]);
    p.rect(15, top + 2, 4, 4, b[3]);
    p.hline(15, top + 2, 4, b[4]);
    p.dot(22, top + 3, RED_LAMP);
    for px in [38, 50] {
        p.port(px, top + 2, 9, 5, b[0], b[2], b[2]);
    }
    p.rim(w, top, 4, light, lerp_color(light, b[2], 0.5));
}

/// The European NES: a grey box with a ribbed back, the dark grey lid on the
/// front, and the dark band with the buttons and the ports.
fn nes(p: &mut Pen, light: Color) {
    let g = ramp(rgb(190, 188, 184));
    let dg = ramp(rgb(74, 74, 80));
    let (w, top, front) = (66, 16, 18);
    p.case(w, top, front, 2, g[3], g[2], g[4]);
    // Ribs across the back of the top.
    for r in [2, 4, 6, 8] {
        p.hline(4, r, w - 8, g[2]);
    }
    // The lid, set into the upper front.
    p.rect(11, top + 2, 44, 7, dg[2]);
    p.hline(11, top + 2, 44, dg[4]);
    p.hline(11, top + 8, 44, dg[1]);
    p.hline(28, top + 5, 10, dg[1]);
    // The dark band along the bottom.
    p.rect(1, top + 10, w - 2, 8, dg[1]);
    p.hline(1, top + 10, w - 2, dg[3]);
    // Power and reset, with the lamp between them.
    p.rect(6, top + 12, 6, 3, g[3]);
    p.hline(6, top + 12, 6, g[4]);
    p.rect(16, top + 12, 6, 3, g[3]);
    p.hline(16, top + 12, 6, g[4]);
    p.dot(13, top + 13, RED_LAMP);
    for px in [38, 50] {
        p.port(px, top + 12, 9, 4, dg[0], g[1], dg[2]);
    }
    p.rim(w, top, 2, light, lerp_color(light, g[3], 0.4));
}

/// The first PlayStation: grey, the round lid, three buttons to its right,
/// and the ports and memory card slots on the front.
fn psx(p: &mut Pen, light: Color) {
    let g = ramp(rgb(186, 186, 192));
    let dg = ramp(rgb(92, 92, 100));
    let (w, top, front) = (66, 20, 9);
    p.case(w, top, front, 3, g[3], g[1], g[2]);
    // The lid: a disc raised over the top, a groove around its middle.
    p.ellipse(25, 10, 20, 9, g[3]);
    p.arc(25, 10, 20, 9, 0.5, 0.75, g[4]);
    p.arc(25, 10, 20, 9, 0.0, 0.25, g[1]);
    p.arc(25, 10, 15, 6, 0.0, 1.0, g[2]);
    // The mark in the middle of the lid, in its four colours.
    for (i, c) in [0xe03c3c, 0xf0c028, 0x3cb45a, 0x3c78dc]
        .into_iter()
        .enumerate()
    {
        p.dot(23 + (i as i32 % 2), 9 + (i as i32 / 2), c);
    }
    // Open, reset, power.
    p.ellipse(54, 6, 5, 2, g[2]);
    p.arc(54, 6, 5, 2, 0.5, 0.75, g[4]);
    p.ellipse(50, 13, 2, 1, g[2]);
    p.ellipse(58, 13, 2, 1, g[2]);
    p.dot(58, 16, 0x3cdc5a);
    // Front: two ports, and the memory card slots over them.
    for px in [11, 44] {
        p.hline(px + 1, top + 1, 9, dg[0]);
        p.port(px, top + 3, 11, 4, dg[0], dg[2], g[2]);
    }
    p.rim(w, top, 3, light, lerp_color(light, g[3], 0.4));
}

/// The Nintendo 64: charcoal, the slot on its raised back, the sliders, the
/// coloured mark and four ports along the front.
fn n64(p: &mut Pen, light: Color) {
    let n = ramp(rgb(62, 62, 70));
    let (w, top, front) = (66, 20, 11);
    p.case(w, top, front, 5, n[2], n[1], n[3]);
    // The raised back with the cartridge slot.
    p.rect(17, 2, 32, 11, n[3]);
    p.hline(17, 2, 32, n[4]);
    p.vline(17, 2, 11, n[4]);
    p.hline(17, 12, 32, n[1]);
    p.hline(21, 5, 24, n[4]);
    p.rect(21, 6, 24, 2, n[0]);
    // Power and reset sliders either side.
    for sx in [6, 53] {
        p.rect(sx, 12, 7, 4, n[0]);
        p.rect(sx + 1, 12, 3, 3, rgb(150, 150, 158));
    }
    // The mark: four coloured blocks.
    p.rect(31, 15, 2, 2, 0xdc3232);
    p.rect(33, 15, 2, 2, 0x32a050);
    p.rect(31, 17, 2, 2, 0x3264dc);
    p.rect(33, 17, 2, 2, 0xf0c828);
    // Four ports along the front.
    for (i, px) in [9, 22, 36, 49].into_iter().enumerate() {
        let _ = i;
        p.port(px, top + 3, 8, 5, n[0], n[2], n[3]);
    }
    p.dot(4, top + 4, RED_LAMP);
    p.rim(w, top, 5, light, lerp_color(light, n[2], 0.5));
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
