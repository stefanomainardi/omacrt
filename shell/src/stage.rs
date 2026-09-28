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
}

/// The consoles, as sprites traced from Evan Amos's public domain
/// photographs on Wikimedia Commons and detailed by hand: the silhouette and
/// the light come from the photograph, the faces and the details that make
/// each one recognisable were painted over it, pixel by pixel. They are
/// PNGs so they can be looked at and redrawn with any tool.
const CONSOLES: &[(&[&str], &[u8])] = &[
    (
        &["nes", "famicom"],
        include_bytes!("../assets/consoles/nes.png"),
    ),
    (
        &["snes", "sfc"],
        include_bytes!("../assets/consoles/snes.png"),
    ),
    (
        &["megadrive", "genesis", "md"],
        include_bytes!("../assets/consoles/megadrive.png"),
    ),
    (&["n64"], include_bytes!("../assets/consoles/n64.png")),
    (
        &["psx", "playstation"],
        include_bytes!("../assets/consoles/psx.png"),
    ),
    (&["saturn"], include_bytes!("../assets/consoles/saturn.png")),
    (
        &["dreamcast", "dc"],
        include_bytes!("../assets/consoles/dreamcast.png"),
    ),
    (&["neogeo"], include_bytes!("../assets/consoles/neogeo.png")),
];

fn console_image(name: &str) -> Option<&'static crate::art::Image> {
    static DECODED: std::sync::OnceLock<Vec<Option<crate::art::Image>>> =
        std::sync::OnceLock::new();
    let all = DECODED.get_or_init(|| {
        CONSOLES
            .iter()
            .map(|(_, bytes)| crate::art::decode_bytes(bytes))
            .collect()
    });
    let i = CONSOLES
        .iter()
        .position(|(names, _)| names.contains(&name))?;
    all[i].as_ref()
}

fn is_arcade(name: &str) -> bool {
    matches!(name, "arcade" | "mame" | "mame2003" | "fbneo" | "naomi")
}

/// Stand a console on the floor at `cx`, with its shadow. False when there
/// is no sprite for it and a picture has to do instead.
pub fn stand(
    fb: &mut Framebuffer,
    th: &Theme,
    name: &str,
    cx: i32,
    floor: i32,
    light: Color,
) -> bool {
    if let Some(img) = console_image(name) {
        let (w, h) = (img.w as i32, img.h as i32);
        shadow(fb, th, cx, floor + 12, w / 2 + 4, 6);
        fb.blit(cx - w / 2, floor + 14 - h, img);
        return true;
    }
    if is_arcade(name) {
        let (w, h) = ((36.0 * SCALE) as i32, (58.0 * SCALE) as i32);
        shadow(fb, th, cx, floor + 12, w / 2 + 6, 6);
        let mut pen = Pen {
            fb,
            x: cx - w / 2,
            y: floor + 14 - h,
            k: SCALE,
        };
        cabinet(&mut pen, light);
        return true;
    }
    false
}

const SCALE: f32 = 1.5;

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
