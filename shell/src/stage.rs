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

/// A game's box standing on the stage: the cover on its front, its spine in
/// the console's colour, turned a little to show the spine, as a box on a
/// shelf is seen. A newly chosen one turns in from its spine to its cover.
#[allow(clippy::too_many_arguments)]
pub fn stand_box(
    fb: &mut Framebuffer,
    th: &Theme,
    cover: &crate::art::Image,
    key: &str,
    brand: Color,
    cx: i32,
    floor: i32,
    max_w: i32,
    max_h: i32,
    light: Color,
    age: f64,
) {
    const TURN_IN: f64 = 0.45;
    const BOX_YAW: f32 = -0.42;
    const BOX_PITCH: f32 = 0.16;
    if cover.w == 0 || cover.h == 0 {
        return;
    }
    let bw = 30;
    let bh = ((bw as f32 * cover.h as f32 / cover.w as f32).round() as i32).clamp(18, 50);
    let bd = 5;
    let mut model = crate::voxel::Model::new(bw, bd, bh);
    let spine = model.mat(lerp_color(brand, 0x101014, 0.45));
    model.cube(0, 0, 0, bw, bd, bh, spine);
    let (sy, cy) = BOX_YAW.sin_cos();
    let (sp, cp) = BOX_PITCH.sin_cos();
    let width = bw as f32 * cy.abs() + bd as f32 * sy.abs();
    let height = (bw as f32 * sy.abs() + bd as f32 * cy.abs()) * sp + bh as f32 * cp;
    let scale = (max_w as f32 / width).min(max_h as f32 / height);
    let p = (age / TURN_IN).clamp(0.0, 1.0) as f32;
    let outline = 0x0a0a0e;
    let render = |yaw: f32| {
        let v = crate::voxel::View {
            yaw,
            pitch: BOX_PITCH,
            scale,
        };
        crate::voxel::render_faced(&model, &v, light, outline, Some(cover))
    };
    let img = if p >= 1.0 {
        RESTING.with(|r| {
            r.borrow_mut()
                .entry((format!("box:{key}"), light))
                .or_insert_with(|| std::rc::Rc::new(render(BOX_YAW)))
                .clone()
        })
    } else {
        let e = 1.0 - (1.0 - p).powi(3);
        std::rc::Rc::new(render(BOX_YAW - (1.0 - e) * std::f32::consts::FRAC_PI_2))
    };
    let (w, h) = (img.w as i32, img.h as i32);
    shadow(fb, th, cx + 3, floor + 8, w / 2 + 2, 5);
    fb.blit(cx - w / 2, floor + 11 - h, &img);
}

/// How long a console takes to arrive on the stage: it comes down, bounces
/// once and turns a full circle, slowing into its resting angle.
pub const ARRIVE: f64 = 0.7;
/// The resting view: turned to show the front and the left side, as the
/// photographs do, seen from about thirty degrees above.
const REST_YAW: f32 = -0.62;
const PITCH: f32 = 0.55;
/// The widest and tallest a console may be drawn, in pixels.
const MAX_W: f32 = 116.0;
const MAX_H: f32 = 96.0;

thread_local! {
    static MODELS: std::cell::RefCell<std::collections::HashMap<String, std::rc::Rc<crate::voxel::Model>>> =
        Default::default();
    static RESTING: std::cell::RefCell<std::collections::HashMap<(String, Color), std::rc::Rc<crate::art::Image>>> =
        Default::default();
}

fn model_of(name: &str) -> Option<std::rc::Rc<crate::voxel::Model>> {
    MODELS.with(|m| {
        let mut m = m.borrow_mut();
        if let Some(model) = m.get(name) {
            return Some(model.clone());
        }
        let model = std::rc::Rc::new(crate::consoles::model(name)?);
        m.insert(name.to_string(), model.clone());
        Some(model)
    })
}

/// Where the arrival is at `p` (0 to 1): the turn still to go, and how high
/// above the floor the console is.
fn arrival(p: f32) -> (f32, f32) {
    let turn = (1.0 - p).powi(3) * std::f32::consts::TAU;
    let lift = if p < 0.6 {
        let q = p / 0.6;
        24.0 * (1.0 - q * q)
    } else {
        let q = (p - 0.6) / 0.4;
        4.0 * (q * std::f32::consts::PI).sin()
    };
    (REST_YAW + turn, lift)
}

/// Draw a voxel console standing on the floor, `age` seconds after it was
/// put there. False when the system has no model.
fn stand_voxel(
    fb: &mut Framebuffer,
    th: &Theme,
    name: &str,
    cx: i32,
    floor: i32,
    light: Color,
    age: f64,
) -> bool {
    let Some(model) = model_of(name) else {
        return false;
    };
    let (sy, cy) = REST_YAW.sin_cos();
    let width = model.w as f32 * cy.abs() + model.d as f32 * sy.abs();
    let deep = model.w as f32 * sy.abs() + model.d as f32 * cy.abs();
    let (sp, cp) = PITCH.sin_cos();
    let height = deep * sp + model.h as f32 * cp;
    let scale = (MAX_W / width).min(MAX_H / height).min(1.8);
    let p = (age / ARRIVE).clamp(0.0, 1.0) as f32;
    let outline = 0x0a0a0e;
    let img = if p >= 1.0 {
        RESTING.with(|r| {
            r.borrow_mut()
                .entry((name.to_string(), light))
                .or_insert_with(|| {
                    let v = crate::voxel::View {
                        yaw: REST_YAW,
                        pitch: PITCH,
                        scale,
                    };
                    std::rc::Rc::new(crate::voxel::render(&model, &v, light, outline))
                })
                .clone()
        })
    } else {
        let (yaw, _) = arrival(p);
        let v = crate::voxel::View {
            yaw,
            pitch: PITCH,
            scale,
        };
        std::rc::Rc::new(crate::voxel::render(&model, &v, light, outline))
    };
    let (_, lift) = arrival(p);
    let (w, h) = (img.w as i32, img.h as i32);
    let spread = (1.0 - lift / 40.0).clamp(0.3, 1.0);
    shadow(
        fb,
        th,
        cx,
        floor + 8,
        ((w / 2 + 2) as f32 * spread) as i32,
        6,
    );
    fb.blit(cx - w / 2, floor + 12 - h - lift as i32, &img);
    true
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
    age: f64,
) -> bool {
    crate::consoles::has(name) && stand_voxel(fb, th, name, cx, floor, light, age)
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
