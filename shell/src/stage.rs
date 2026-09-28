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
    /// A filled polygon, its corners on the design grid.
    fn poly(&mut self, pts: &[(i32, i32)], c: Color) {
        let pts: Vec<(f32, f32)> = pts
            .iter()
            .map(|&(a, b)| ((self.x + self.at(a)) as f32, (self.y + self.at(b)) as f32))
            .collect();
        let y0 = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min) as i32;
        let y1 = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max) as i32;
        for y in y0..=y1 {
            let yc = y as f32 + 0.5;
            let mut xs = Vec::new();
            for i in 0..pts.len() {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                if (a.1 <= yc && yc < b.1) || (b.1 <= yc && yc < a.1) {
                    xs.push(a.0 + (yc - a.1) * (b.0 - a.0) / (b.1 - a.1));
                }
            }
            xs.sort_by(f32::total_cmp);
            for pair in xs.chunks(2) {
                if let [l, r] = pair {
                    let (l, r) = (l.round() as i32, r.round() as i32);
                    self.fb.rect(l, y, r - l, 1, c);
                }
            }
        }
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
            h: 34,
            draw: n64,
        },
        "dreamcast" | "dc" => Sprite {
            w: 60,
            h: 30,
            draw: dreamcast,
        },
        "saturn" => Sprite {
            w: 66,
            h: 30,
            draw: saturn,
        },
        "neogeo" => Sprite {
            w: 66,
            h: 24,
            draw: neogeo,
        },
        "arcade" | "mame" | "mame2003" | "fbneo" | "naomi" => Sprite {
            w: 36,
            h: 58,
            draw: cabinet,
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

/// The Super Nintendo as Europe and Japan had it: grey, the darker deck with
/// the slot and the four coloured dots in their circle, POWER, eject and
/// RESET in front of it, the name, and two ports and the lamp on the front.
fn snes(p: &mut Pen, light: Color) {
    let g = ramp(rgb(190, 190, 196));
    let (w, top, front) = (66, 24, 9);
    p.case(w, top, front, 3, g[3], g[1], g[2]);
    let deck = rgb(150, 150, 158);
    p.rect(7, 4, 52, 9, deck);
    p.hline(7, 4, 52, g[2]);
    p.hline(7, 12, 52, rgb(118, 118, 126));
    p.hline(12, 8, 34, rgb(112, 112, 120));
    p.ellipse(52, 8, 3, 2, g[3]);
    for (a, b, c) in [
        (51, 7, 0x3c64c8),
        (53, 7, 0xdc3232),
        (51, 9, 0x3caa50),
        (53, 9, 0xf0c828),
    ] {
        p.dot(a, b, c);
    }
    p.rect(11, 14, 9, 5, g[4]);
    p.rect(12, 14, 7, 2, rgb(70, 70, 76));
    p.rect(22, 15, 17, 5, rgb(140, 140, 148));
    p.hline(22, 15, 17, rgb(170, 170, 178));
    p.rect(42, 15, 9, 5, rgb(72, 72, 78));
    p.hline(42, 15, 9, rgb(104, 104, 112));
    p.hline(5, 21, 10, rgb(60, 60, 66));
    p.hline(5, 22, 14, rgb(60, 60, 66));
    p.port(9, top + 3, 11, 4, rgb(170, 170, 176), rgb(70, 70, 76), g[2]);
    p.port(
        33,
        top + 3,
        11,
        4,
        rgb(170, 170, 176),
        rgb(70, 70, 76),
        g[2],
    );
    p.dot(56, top + 3, 0xa02828);
    p.rim(w, top, 3, light, lerp_color(light, g[3], 0.4));
}

/// The first Mega Drive: black, the grille at the back left, the volume,
/// power and reset at the front left, the raised disc with the curved slot
/// and the 16-BIT plate, two ports on the front and the name on the right.
fn megadrive(p: &mut Pen, light: Color) {
    let b = ramp(rgb(56, 56, 64));
    let gold = ramp(rgb(206, 168, 72));
    let (w, top, front) = (66, 21, 9);
    p.case(w, top, front, 4, b[2], b[1], b[3]);
    // The grille.
    p.rect(5, 2, 14, 7, b[1]);
    for r in (3..9).step_by(2) {
        p.hline(5, r, 14, b[0]);
    }
    // The disc: raised, its rim lit and shaded, the slot curving across.
    p.ellipse(42, 10, 19, 9, b[3]);
    p.arc(42, 10, 19, 9, 0.5, 0.75, b[4]);
    p.arc(42, 10, 19, 9, 0.0, 0.25, b[1]);
    p.ellipse(42, 10, 14, 6, b[2]);
    p.arc(42, 13, 13, 6, 0.6, 0.9, b[0]);
    p.arc(42, 12, 13, 6, 0.6, 0.9, b[4]);
    // The 16-BIT plate on the front of the disc: black with gold letters,
    // the white label under it with its red dot.
    p.rect(37, 12, 11, 3, 0x0c0c10);
    p.hline(38, 13, 9, gold[3]);
    p.rect(37, 15, 11, 3, rgb(214, 210, 200));
    p.dot(42, 16, 0xd22828);
    // Volume, power and reset at the front left.
    p.rect(5, 11, 3, 7, b[0]);
    p.rect(5, 13, 3, 2, b[4]);
    p.rect(10, 11, 9, 3, b[0]);
    p.dot(12, 12, 0xd22828);
    p.dot(15, 12, 0xe6e6ea);
    p.rect(10, 16, 6, 2, rgb(170, 170, 176));
    // Front: two ports on the left, the name on the right.
    for px in [6, 17] {
        p.port(px, top + 2, 9, 5, b[0], b[2], b[2]);
    }
    p.hline(42, top + 4, 12, 0xe6e6ea);
    p.hline(56, top + 4, 6, 0xe6e6ea);
    p.rim(w, top, 4, light, lerp_color(light, b[2], 0.5));
}

/// The NES: a light grey box, the ribbed panel at the front right of the top;
/// on the front the lid with the name in red, the dark band with power, reset
/// and the lamp, and the black end with the two ports.
fn nes(p: &mut Pen, light: Color) {
    let g = ramp(rgb(196, 194, 190));
    let (w, top, front) = (66, 16, 18);
    let black = rgb(44, 44, 48);
    p.case(w, top, front, 2, g[3], g[2], g[4]);
    // The ribbed panel, and the dark strip behind it.
    p.rect(45, 3, 18, 12, g[3]);
    for r in (4..15).step_by(2) {
        p.hline(45, r, 18, g[1]);
    }
    p.rect(45, 1, 18, 2, black);
    // The lid on the front, with the name in red.
    p.rect(2, top + 1, 41, 9, g[3]);
    p.hline(2, top + 1, 41, g[4]);
    p.hline(2, top + 9, 41, g[1]);
    p.hline(5, top + 3, 8, 0xd22828);
    p.hline(5, top + 5, 16, 0xd22828);
    // The dark band under it: power, reset, the lamp.
    p.rect(1, top + 10, 43, 8, rgb(120, 120, 124));
    p.hline(1, top + 10, 43, rgb(150, 150, 154));
    p.rect(5, top + 12, 6, 3, rgb(70, 70, 74));
    p.rect(13, top + 12, 6, 3, rgb(70, 70, 74));
    p.dot(3, top + 13, RED_LAMP);
    // The black end with the two ports.
    p.rect(44, top, 21, 18, black);
    p.vline(44, top, 18, rgb(70, 70, 76));
    for px in [47, 56] {
        p.port(
            px,
            top + 11,
            6,
            4,
            rgb(120, 120, 124),
            rgb(20, 20, 22),
            black,
        );
    }
    p.rim(w, top, 2, light, lerp_color(light, g[3], 0.4));
}

/// The first PlayStation: grey, the round lid in the middle, reset and power
/// on its left, open on its right, the raised strip at the back, and on the
/// front two ports under their memory card slots and the grooves on the right.
fn psx(p: &mut Pen, light: Color) {
    let g = ramp(rgb(186, 186, 192));
    let dark = rgb(60, 60, 64);
    let (w, top, front) = (66, 20, 9);
    p.case(w, top, front, 3, g[3], g[1], g[2]);
    p.rect(24, 0, 18, 3, g[4]);
    p.ellipse(34, 10, 17, 8, g[3]);
    p.arc(34, 10, 17, 8, 0.5, 0.75, g[4]);
    p.arc(34, 10, 17, 8, 0.0, 0.5, g[1]);
    for (i, c) in [0xe03c3c, 0xf0c028, 0x3cb45a, 0x3c78dc]
        .into_iter()
        .enumerate()
    {
        p.dot(33 + (i as i32 % 2), 11 + (i as i32 / 2), c);
    }
    p.ellipse(7, 5, 2, 1, g[2]);
    p.ellipse(8, 11, 5, 3, g[2]);
    p.arc(8, 11, 5, 3, 0.5, 0.8, g[4]);
    p.dot(4, 15, 0x3cdc5a);
    p.ellipse(59, 15, 5, 3, g[2]);
    p.arc(59, 15, 5, 3, 0.5, 0.8, g[4]);
    for px in [18, 34] {
        p.hline(px + 1, top + 1, 9, dark);
        p.port(px, top + 3, 11, 4, g[2], dark, g[2]);
    }
    for x in (52..64).step_by(2) {
        p.vline(x, top + 1, 7, g[0]);
    }
    p.rim(w, top, 3, light, lerp_color(light, g[3], 0.4));
}

/// The Nintendo 64, as the photographs have it: the deck spreading into two
/// wings at the front with the middle set back between them, the raised
/// back with the grey slot cover and the row of vents, the power slider and
/// the reset button, the memory lid, and on the front the black window with
/// the name and the N, between two pairs of grey ports.
fn n64(p: &mut Pen, light: Color) {
    let n = ramp(rgb(66, 66, 74));
    let grey = ramp(rgb(166, 166, 172));
    // The front faces first. The wings are round and come down lower than
    // the body, like feet; the middle between them is set back, in shadow.
    p.rect(13, 19, 40, 12, n[1]);
    p.rect(13, 29, 40, 2, n[0]);
    for cx in [8, 58] {
        p.ellipse(cx, 27, 8, 6, n[0]);
        p.ellipse(cx, 26, 8, 5, n[1]);
    }
    // The deck: wide at the back, swelling into the wings, recessed between.
    p.poly(
        &[
            (9, 2),
            (57, 2),
            (62, 5),
            (65, 12),
            (66, 20),
            (52, 21),
            (50, 19),
            (16, 19),
            (14, 21),
            (0, 20),
            (1, 12),
            (4, 5),
        ],
        n[2],
    );
    for cx in [8, 58] {
        p.ellipse(cx, 20, 8, 3, n[2]);
        p.arc(cx, 20, 8, 3, 0.05, 0.45, n[3]);
    }
    p.hline(16, 19, 34, n[3]);
    // The raised back: its top, the grey slot cover, the vents on its slope.
    p.poly(
        &[(18, 1), (48, 1), (51, 4), (51, 12), (15, 12), (15, 4)],
        n[3],
    );
    p.hline(18, 1, 30, n[4]);
    p.rect(15, 9, 36, 3, n[2]);
    for x in (17..50).step_by(2) {
        p.vline(x, 9, 2, n[0]);
    }
    p.rect(22, 3, 22, 4, grey[2]);
    p.hline(22, 3, 22, grey[4]);
    p.hline(24, 5, 18, grey[0]);
    // Power slider on the left, reset on the right, the memory lid between.
    p.ellipse(12, 14, 4, 2, n[0]);
    p.rect(10, 13, 3, 2, grey[1]);
    p.ellipse(54, 14, 3, 2, n[0]);
    p.ellipse(54, 14, 2, 1, n[2]);
    p.rect(27, 13, 12, 5, n[3]);
    p.hline(27, 13, 12, n[4]);
    p.hline(27, 17, 12, n[1]);
    // The window on the front: the name in white, the N in its colours.
    p.rect(27, 20, 12, 8, 0x0c0c10);
    p.hline(28, 21, 10, 0xe6e6ea);
    for (a, b, c) in [
        (30, 23, 0xdc3232),
        (30, 24, 0xdc3232),
        (30, 25, 0xf0c828),
        (31, 24, 0x32aa50),
        (32, 25, 0x32aa50),
        (33, 23, 0x325adc),
        (33, 24, 0x325adc),
        (33, 25, 0xf0c828),
    ] {
        p.dot(a, b, c);
    }
    p.dot(33, 29, RED_LAMP);
    // Four ports, light grey with their three holes, two either side.
    for cx in [17, 23, 43, 49] {
        p.ellipse(cx, 25, 3, 2, grey[2]);
        p.arc(cx, 25, 3, 2, 0.55, 0.95, grey[4]);
        for i in -1..=1 {
            p.dot(cx + i, 25, n[0]);
        }
    }
    // The lamp's rim along the back edge and down the left.
    p.hline(9, 2, 48, light);
    p.arc(33, 6, 18, 5, 0.6, 0.9, lerp_color(light, n[4], 0.3));
    for j in 6..20 {
        p.dot(1, j, lerp_color(light, n[2], 0.5));
    }
}

/// The Dreamcast: pale and square, the round lid over most of the top with
/// the swirl and the triangle, power on the left, open on the right, and four
/// ports on the front under the name.
fn dreamcast(p: &mut Pen, light: Color) {
    let g = ramp(rgb(214, 214, 210));
    let (w, top, front) = (60, 20, 10);
    p.case(w, top, front, 4, g[3], g[1], g[2]);
    p.ellipse(29, 9, 19, 8, g[4]);
    p.arc(29, 9, 19, 8, 0.0, 1.0, g[2]);
    p.arc(29, 9, 19, 8, 0.55, 0.95, rgb(248, 248, 250));
    p.poly(&[(27, 15), (32, 15), (29, 19)], g[1]);
    let blue = 0x2a5ad2;
    for (a, b) in [(35, 5), (36, 5), (37, 6), (36, 7), (35, 7), (34, 6)] {
        p.dot(a, b, blue);
    }
    p.ellipse(5, 15, 3, 2, g[2]);
    p.arc(5, 15, 3, 2, 0.5, 0.9, g[4]);
    p.ellipse(55, 15, 3, 2, g[2]);
    p.arc(55, 15, 3, 2, 0.5, 0.9, g[4]);
    p.hline(26, top + 1, 8, g[0]);
    for px in [8, 20, 32, 44] {
        p.rect(px, top + 3, 8, 5, g[2]);
        p.rect(px + 2, top + 4, 4, 3, rgb(34, 34, 36));
    }
    p.rim(w, top, 4, light, lerp_color(light, g[3], 0.4));
}

/// The first Saturn: near black, the cartridge slot at the back, the big lid
/// in the middle with the name, power, open and access along the front edge
/// of the top, and two ports low on the front.
fn saturn(p: &mut Pen, light: Color) {
    let b = ramp(rgb(58, 58, 68));
    let (w, top, front) = (66, 21, 9);
    p.case(w, top, front, 4, b[2], b[1], b[3]);
    p.rect(18, 1, 30, 3, b[0]);
    p.hline(18, 1, 30, b[4]);
    p.hline(26, 2, 14, b[3]);
    // The lid: a rounded block, its top edge lit, the name in white.
    p.poly(
        &[(15, 5), (51, 5), (54, 9), (52, 16), (14, 16), (12, 9)],
        b[3],
    );
    p.hline(15, 5, 36, b[4]);
    p.hline(18, 8, 12, b[2]);
    p.hline(34, 9, 14, b[2]);
    p.hline(25, 12, 16, 0xe6e6ea);
    p.dot(33, 7, rgb(170, 170, 180));
    // Power, open and access along the front edge.
    p.ellipse(10, 18, 4, 1, b[3]);
    p.ellipse(33, 19, 6, 1, b[3]);
    p.ellipse(56, 19, 4, 1, b[3]);
    p.hline(3, 16, 5, 0xe6e6ea);
    for px in [20, 36] {
        p.port(px, top + 4, 10, 4, b[0], b[3], b[2]);
    }
    p.rim(w, top, 4, light, lerp_color(light, b[2], 0.5));
}

/// The Neo Geo AES: long, low and black, the raised ring round the wide slot,
/// the vents behind it, the big round button on the left, the gold name on
/// the front of the top, the two controller sockets and the memory card slot.
fn neogeo(p: &mut Pen, light: Color) {
    let b = ramp(rgb(50, 50, 58));
    let (w, top, front) = (66, 15, 9);
    p.case(w, top, front, 2, b[2], b[1], b[3]);
    // The ring, raised, lit on its back edge, and the slot inside it.
    p.rect(16, 2, 44, 9, b[3]);
    p.hline(17, 2, 42, b[4]);
    p.hline(17, 10, 42, b[1]);
    p.rect(22, 4, 32, 4, 0x0a0a0c);
    p.hline(22, 4, 32, b[0]);
    for x in (40..58).step_by(3) {
        p.vline(x, 1, 1, b[0]);
    }
    // The big round button.
    p.ellipse(8, 7, 4, 2, b[3]);
    p.ellipse(8, 6, 2, 1, b[4]);
    // The name in gold on the front of the top.
    p.hline(22, 13, 10, rgb(206, 168, 72));
    // Two sockets and the memory card slot on the front.
    p.port(8, top + 3, 9, 4, b[0], b[3], b[2]);
    p.port(30, top + 3, 9, 4, b[0], b[3], b[2]);
    p.rect(48, top + 4, 14, 2, 0x0a0a0c);
    p.dot(4, top + 4, RED_LAMP);
    p.rim(w, top, 2, light, lerp_color(light, b[2], 0.5));
}

/// An upright cabinet: T-molding in the brand's colour, the lit marquee, a
/// screen with its scanlines and something small playing on it, the stick
/// and buttons, and the coin slots glowing on the door.
fn cabinet(p: &mut Pen, light: Color) {
    let b = ramp(rgb(58, 48, 70));
    let side = ramp(rgb(40, 34, 50));
    let brand = ramp(0xff4fa3);
    // The side panel, seen past the front on the left.
    p.rect(0, 3, 5, 54, side[1]);
    p.vline(0, 3, 54, side[3]);
    // The front of the body.
    p.rect(5, 1, 31, 56, b[2]);
    // The marquee, lit from inside.
    p.rect(6, 2, 29, 8, brand[3]);
    p.hline(6, 2, 29, brand[4]);
    p.hline(6, 9, 29, brand[1]);
    for x in [10, 14, 18, 22, 26, 30] {
        p.rect(x, 5, 2, 2, lerp_color(brand[4], 0xffffff, 0.5));
    }
    // The screen in its bezel, with scanlines and a tiny game on it.
    p.rect(7, 11, 27, 20, b[0]);
    let glass = rgb(18, 36, 48);
    p.rect(9, 13, 23, 16, glass);
    for r in (13..29).step_by(2) {
        p.hline(9, r, 23, lerp_color(glass, 0, 0.35));
    }
    for (x, y) in [(12, 15), (18, 16), (25, 14), (28, 18), (14, 20)] {
        p.dot(x, y, 0xb4c8ff);
    }
    p.rect(19, 25, 3, 1, 0x78ff96);
    p.dot(20, 24, 0x78ff96);
    p.rect(14, 18, 2, 1, 0xff6e6e);
    p.rect(24, 19, 2, 1, 0xff6e6e);
    // The control panel, sloping towards the player.
    p.rect(5, 31, 31, 6, b[3]);
    p.hline(5, 31, 31, b[4]);
    p.vline(13, 29, 3, rgb(40, 40, 40));
    p.ellipse(13, 29, 1, 1, 0xdc2828);
    for (x, c) in [(21, 0xdc2828), (25, 0xf0c828), (29, 0x3c78dc)] {
        p.dot(x, 34, c);
        p.dot(x + 1, 34, c);
    }
    // The coin door and its two lit slots.
    p.rect(14, 41, 13, 12, b[1]);
    p.hline(14, 41, 13, b[3]);
    for x in [17, 22] {
        p.rect(x, 44, 2, 3, 0xff9e3c);
    }
    p.rect(5, 55, 31, 2, b[0]);
    // The T-molding along the front edges, in the brand's colour.
    p.vline(5, 1, 56, brand[2]);
    p.vline(35, 1, 56, brand[2]);
    p.hline(5, 1, 31, lerp_color(brand[3], light, 0.5));
    p.vline(5, 1, 30, lerp_color(brand[3], light, 0.4));
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
