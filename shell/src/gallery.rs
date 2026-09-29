//! An art gallery for an idle television: three paintings by René Magritte
//! redrawn in pixels, each hung on a dark wall under a spotlight in a gilt
//! frame with its title underneath, and each moving a little in its own
//! mood.
//!
//! They are homages, drawn here from shapes: nothing is read from a file.
//! Le fils de l'homme is laid out in four by three, wider than the painting,
//! to fill the tube. What moves is slow and quiet, and now and then there is
//! a small surprise of the kind he liked: the apple steps aside to show
//! another apple, a light goes out in the house under the day sky, one of
//! the men in the air raises his hat.

use crate::fb::{Color, Framebuffer, lerp_color, rgb};

/// The paintings, in the order the gallery shows them.
pub const PAINTINGS: [(&str, &str); 3] = [
    ("Le fils de l'homme", "Rene Magritte, 1964"),
    ("L'empire des lumieres", "Rene Magritte, 1954"),
    ("Golconde", "Rene Magritte, 1953"),
];

/// The canvas, in pixels: four by three.
const PW: i32 = 208;
const PH: i32 = 156;

#[inline]
fn od(x: i32, y: i32) -> f32 {
    (crate::paint::BAYER[(y & 3) as usize][(x & 3) as usize] as f32 + 0.5) / 16.0
}

/// A blend in `n` steps with the ordered dither between them.
#[inline]
fn steps(a: Color, b: Color, t: f32, x: i32, y: i32, n: f32) -> Color {
    let q = ((t.clamp(0.0, 1.0) * n + od(x, y)).floor() / n).clamp(0.0, 1.0);
    lerp_color(a, b, q)
}

fn ch(c: Color) -> (u32, u32, u32) {
    ((c >> 16) & 255, (c >> 8) & 255, c & 255)
}

/// The small generator every scatter here comes from, so a painting is the
/// same painting every time it is hung.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        let mut s = self.0;
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        self.0 = s;
        s
    }

    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * (self.next() as f32 / 4_294_967_296.0)
    }
}

/// A cumulus as balls: a wide bottom row, a smaller row on it, a crown.
fn cloud_shape(seed: u32, w: f32) -> Vec<(f32, f32, f32)> {
    let mut r = Rng(seed.max(1));
    let mut balls = Vec::new();
    for (lift, span, n) in [(0.0f32, 1.0f32, 5), (0.30, 0.62, 3), (0.55, 0.34, 2)] {
        for i in 0..n {
            let f = (i as f32 + 0.5) / n as f32;
            let x = w * (0.5 + (f - 0.5) * span) + r.range(-w * 0.04, w * 0.04);
            let rad = w
                * (0.13 + 0.05 * (std::f32::consts::PI * f).sin())
                * (1.25 - lift * 0.5)
                * r.range(0.85, 1.15);
            let y = -w * lift * 0.42 - rad * 0.35;
            balls.push((x, y, rad));
        }
    }
    balls
}

/// A sky graded top to bottom with clouds in it, each cloud lit from
/// `light` by the ball whose surface each pixel is on.
#[allow(clippy::too_many_arguments)]
fn sky_clouds(
    fb: &mut Framebuffer,
    (x0, y0, w, h): (i32, i32, i32, i32),
    top: Color,
    bottom: Color,
    clouds: &[(f32, f32, f32, u32)],
    light: (f32, f32),
    hi: Color,
    lo: Color,
) {
    for y in 0..h {
        for x in 0..w {
            fb.put(
                x0 + x,
                y0 + y,
                steps(top, bottom, y as f32 / h as f32, x0 + x, y0 + y, 5.0),
            );
        }
    }
    for &(cx, cy, cw, seed) in clouds {
        let balls = cloud_shape(seed, cw);
        let top_y = balls.iter().map(|b| b.1 - b.2).fold(f32::MAX, f32::min) as i32 - 1;
        for yy in top_y..1 {
            for xx in -3..cw as i32 + 4 {
                let (px, py) = ((x0 as f32 + cx) as i32 + xx, y0 + cy as i32 + yy);
                if px < x0 || px >= x0 + w || py < y0 || py >= y0 + h {
                    continue;
                }
                let mut best: Option<(f32, f32, f32)> = None;
                for &(bx, by, br) in &balls {
                    let (dx, dy) = (xx as f32 + 0.5 - bx, yy as f32 + 0.5 - by);
                    let d2 = dx * dx + dy * dy;
                    if d2 <= br * br {
                        let z = (br * br - d2).sqrt();
                        if best.is_none_or(|b| z > b.0) {
                            best = Some((z, dx / br, dy / br));
                        }
                    }
                }
                let Some((_, nx, ny)) = best else { continue };
                let v =
                    nx * light.0 + ny * light.1 + 0.35 * (1.0 - nx * nx - ny * ny).max(0.0).sqrt();
                let k = (v + 0.6) / 1.3 + (od(px, py) - 0.5) * 0.25;
                let mut c = lerp_color(lo, hi, k.clamp(0.0, 1.0));
                if (yy as f32) > -cw * 0.07 {
                    c = lerp_color(c, lo, 0.5);
                }
                fb.put(px, py, c);
            }
        }
    }
}

/// The wall, the spotlight falling on it and the gilt frame.
fn wall(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32) {
    let (sw, sh) = (fb.w as i32, fb.h as i32);
    let cx = x as f32 + w as f32 / 2.0;
    for yy in 0..sh {
        for xx in 0..sw {
            let f = yy as f32 / sh as f32;
            let mut c = steps(0x2e2836, 0x141118, f, xx, yy, 4.0);
            let spread = 30.0 + yy as f32 * 0.55;
            let d = (xx as f32 - cx).abs() / spread;
            if d < 1.0 {
                c = steps(
                    c,
                    0x5a4c50,
                    (1.0 - d).powf(1.5) * (1.0 - f * 0.5),
                    xx,
                    yy,
                    4.0,
                );
            }
            fb.put(xx, yy, c);
        }
    }
    let f = 8;
    fb.rect(x - f + 3, y - f + 3, w + 2 * f, h + 2 * f, 0x0c0a0e);
    let (gold, gold_hi, gold_lo, gold_dk) = (0xb8913a, 0xf0d27a, 0x7a5c24, 0x4e3a16);
    for j in -f..h + f {
        for i in -f..w + f {
            if (0..w).contains(&i) && (0..h).contains(&j) {
                continue;
            }
            let ring = (i + f).min(j + f).min(w + f - 1 - i).min(h + f - 1 - j);
            let mut c = match ring {
                0 => gold_dk,
                1 => {
                    if i + f < f || j + f < 2 {
                        gold_hi
                    } else {
                        gold_lo
                    }
                }
                3 | 4 => {
                    if i < 0 || j < 0 {
                        gold_hi
                    } else {
                        gold_lo
                    }
                }
                r if r == f - 1 => gold_dk,
                _ => gold,
            };
            // Beads along the middle of the moulding.
            if ring == 5 && (i + j).rem_euclid(4) == 0 {
                c = gold_hi;
            }
            fb.put(x + i, y + j, c);
        }
    }
}

fn disc(
    fb: &mut Framebuffer,
    cx: f32,
    cy: f32,
    r: f32,
    mut shade: impl FnMut(f32, f32, i32, i32) -> Color,
) {
    for y in (cy - r) as i32 - 1..(cy + r) as i32 + 2 {
        for x in (cx - r) as i32 - 1..(cx + r) as i32 + 2 {
            let (dx, dy) = ((x as f32 + 0.5 - cx) / r, (y as f32 + 0.5 - cy) / r);
            if dx * dx + dy * dy <= 1.0 {
                let c = shade(dx, dy, x, y);
                fb.put(x, y, c);
            }
        }
    }
}

/// A green apple of two lobes, lit from the upper left, its stalk and three
/// leaves stirring.
fn apple(fb: &mut Framebuffer, ax: i32, ay: i32, r: i32, t: f32, idx: f32) {
    for lob in [-3.0f32, 3.0] {
        disc(fb, ax as f32 + lob, ay as f32, r as f32, |dx, dy, x, y| {
            let lit = -dx * 0.55 - dy * 0.75;
            if (dx + 0.35).powi(2) + (dy + 0.45).powi(2) < 0.02 {
                rgb(236, 252, 210)
            } else {
                steps(0x3a6a24, 0xa8d468, (lit + 1.0) / 2.0, x, y, 5.0)
            }
        });
    }
    fb.rect(ax, ay - r - 4, 2, 6, 0x5a3a1e);
    for (n, (lx, ly, dir)) in [
        (ax + 2, ay - r - 3, 1),
        (ax - 1, ay - r - 2, -1),
        (ax + 1, ay - r - 5, 1),
    ]
    .into_iter()
    .enumerate()
    {
        let sway = (t * 1.7 + n as f32 * 1.3 + idx).sin();
        for k in 0..8i32 {
            let lift = (sway * k as f32 / 8.0).round() as i32;
            let span = 1 - (k - 4).abs() / 3;
            for j in -span..=span.max(0) {
                let c = if j <= 0 { 0x4e8a30 } else { 0x33621e };
                fb.put(lx + dir * k, ly - k / 3 + j - lift, c);
            }
        }
    }
}

fn son_of_man(fb: &mut Framebuffer, x0: i32, y0: i32, w: i32, h: i32, t: f32) {
    let horizon = (h as f32 * 0.56) as i32;
    let wall_top = (h as f32 * 0.70) as i32;
    let fw = 132 * h / 168;
    let drift = t * 1.2;
    sky_clouds(
        fb,
        (x0, y0, w, horizon),
        0x7f97ac,
        0xc4d0d6,
        &[
            (-30.0 + drift, 30.0, 64.0, 21),
            (120.0 + drift, 24.0, 74.0, 23),
            (50.0 + drift, 62.0, 46.0, 27),
            (190.0 + drift, 56.0, 40.0, 29),
        ],
        (0.2, -0.95),
        rgb(226, 230, 232),
        rgb(150, 162, 172),
    );
    // The sea, lighter at the horizon, long waves moving on it.
    for y in horizon..wall_top {
        for x in 0..w {
            let f = (y - horizon) as f32 / (wall_top - horizon) as f32;
            let mut c = steps(0x9aaeb0, 0x56707a, f, x0 + x, y0 + y, 4.0);
            let wave = (x as f32 * 0.2 + (x as f32 * 0.11 + t * 0.7).sin() * 3.0 + t * 3.0) as i32;
            if (y * 7 + wave).rem_euclid(11) == 0 {
                c = lerp_color(c, rgb(200, 214, 214), 0.35);
            }
            fb.put(x0 + x, y0 + y, c);
        }
    }
    // The low wall, its top in the light.
    for y in wall_top..h {
        for x in 0..w {
            let c = if y < wall_top + 3 {
                if y == wall_top { 0xb4b2ac } else { 0x9a9892 }
            } else if ((y - wall_top) / 6 + x / 14) % 2 == 0
                && (x % 14 == 0 || (y - wall_top) % 6 == 0)
            {
                0x64625e
            } else {
                0x7e7c78
            };
            fb.put(x0 + x, y0 + y, c);
        }
    }
    let cx = x0 + w / 2;
    let (coat, coat_hi, coat_lo, coat_dk) = (0x3e4048, 0x5c5f6a, 0x2a2c32, 0x1c1d22);
    let top = y0 + (h as f32 * 0.40) as i32;
    // The coat, cut off at the thighs by the edge of the canvas.
    for y in top..y0 + h {
        let f = (y - top) as f32 / (y0 + h - top) as f32;
        let mut half = (fw as f32 * 0.30 - f * fw as f32 * 0.02) as i32;
        if y < top + 6 {
            half = (fw as f32 * 0.30 - (6 - (y - top)) as f32 * 1.6) as i32;
        }
        for x in cx - half..=cx + half {
            let u = (x - cx) as f32 / half.max(1) as f32;
            let mut c = coat;
            if u < -0.78 || (-0.3..-0.2).contains(&u) {
                c = coat_hi;
            }
            if u > 0.72 {
                c = coat_lo;
            }
            if (u + 0.55).abs() < 0.03 || (u - 0.55).abs() < 0.03 {
                c = coat_dk;
            }
            fb.put(x, y, c);
        }
    }
    // Lapels and shirt, the red tie, the buttons.
    for y in top..top + (h as f32 * 0.22) as i32 {
        let half = (9 - (y - top) / 3).max(0);
        for x in cx - half..=cx + half {
            fb.put(x, y, rgb(236, 236, 232));
        }
        for i in 0..4 {
            fb.put(cx - (half + 1 + i), y, coat_hi);
            fb.put(cx + (half + 1 + i), y, coat_lo);
        }
    }
    for y in top + 2..top + (h as f32 * 0.20) as i32 {
        let wide = if y < top + 5 {
            2
        } else if y < top + (h as f32 * 0.17) as i32 {
            3
        } else {
            1
        };
        fb.rect(cx - wide / 2 - 1, y, wide + 1, 1, 0xa42a2c);
    }
    fb.rect(cx - 2, top + 1, 5, 3, 0xb83234);
    for k in 0..3 {
        fb.rect(
            cx - 1,
            top + (h as f32 * 0.26) as i32 + k * 12,
            2,
            2,
            coat_dk,
        );
    }
    fb.rect(
        cx - (fw as f32 * 0.29) as i32 - 3,
        y0 + h - 12,
        7,
        8,
        rgb(214, 170, 146),
    );
    fb.rect(
        cx + (fw as f32 * 0.30) as i32 - 3,
        y0 + h - 16,
        7,
        8,
        rgb(214, 170, 146),
    );
    fb.rect(cx + (fw as f32 * 0.26) as i32, top + 40, 4, 16, coat_lo);
    // Neck, ears and the face, mostly behind the apple.
    fb.rect(cx - 6, top - 8, 13, 9, rgb(212, 170, 144));
    for y in top - 38..top - 5 {
        for x in cx - 16..cx + 17 {
            if ((x - cx) as f32 / 15.5).powi(2) + ((y - top + 21) as f32 / 17.0).powi(2) <= 1.0 {
                let c = if x - cx > 9 {
                    rgb(206, 162, 136)
                } else {
                    rgb(226, 184, 156)
                };
                fb.put(x, y, c);
            }
        }
    }
    for s in [-1, 1] {
        fb.rect(
            cx + s * 16 - if s < 0 { 2 } else { 0 },
            top - 26,
            3,
            8,
            rgb(196, 150, 124),
        );
        fb.rect(
            cx + s * 16 - if s < 0 { 1 } else { 0 },
            top - 24,
            1,
            4,
            rgb(160, 118, 96),
        );
    }
    // The eye over the apple's edge, which blinks; the brows.
    if t.rem_euclid(5.0) < 4.84 {
        fb.rect(cx + 5, top - 31, 5, 3, rgb(242, 238, 232));
        fb.rect(cx + 7, top - 31, 2, 3, rgb(46, 66, 76));
    } else {
        fb.rect(cx + 5, top - 30, 5, 1, rgb(150, 106, 84));
    }
    fb.rect(cx + 4, top - 34, 7, 1, rgb(120, 84, 60));
    fb.rect(cx - 10, top - 34, 7, 1, rgb(120, 84, 60));
    // The bowler hat.
    fb.rect(cx - 21, top - 37, 43, 3, 0x141418);
    for y in top - 56..top - 37 {
        let half = if y > top - 52 {
            14
        } else {
            13 - (top - 52 - y)
        };
        fb.rect(cx - half, y, 2 * half + 1, 1, 0x18181c);
    }
    fb.rect(cx - 14, top - 41, 29, 3, 0x2c2c32);
    fb.rect(cx - 10, top - 54, 3, 10, 0x34343c);
    // The apple, breathing; every so often it steps aside to show the apple
    // behind it.
    let (ax, ay, r) = (
        cx - 2,
        top - 17 + ((t * 1.1).sin() * 0.8).round() as i32,
        11,
    );
    let k = t.rem_euclid(12.0);
    let mut aside = 0.0f32;
    if (8.0..11.0).contains(&k) {
        let f = (k - 8.0) / 3.0;
        aside = if f < 0.72 {
            (std::f32::consts::PI * (f * 1.4).min(1.0)).sin()
        } else {
            (std::f32::consts::PI * (1.0 - (f - 0.72) / 0.28) * 0.5).sin()
        };
        apple(fb, ax, ay, r, t, 5.0);
    }
    apple(
        fb,
        ax + (aside * 20.0).round() as i32,
        ay - (aside * 3.0).round() as i32,
        r,
        t,
        0.0,
    );
}

fn empire(fb: &mut Framebuffer, x0: i32, y0: i32, w: i32, h: i32, t: f32) {
    let line = (h as f32 * 0.50) as i32;
    let drift = t * 2.0;
    sky_clouds(
        fb,
        (x0, y0, w, line + 20),
        0x4a84cc,
        0xa8cce8,
        &[
            (10.0 + drift, 30.0, 58.0, 41),
            (98.0 + drift, 18.0, 72.0, 43),
            (150.0 + drift, 48.0, 46.0, 47),
        ],
        (0.4, -0.9),
        rgb(246, 247, 248),
        rgb(176, 186, 198),
    );
    // A bird crossing the day, over a street at night.
    let k = t.rem_euclid(12.0);
    if (1.5..7.5).contains(&k) {
        let bx = (x0 as f32 + (k - 1.5) / 6.0 * (w + 20) as f32 - 10.0) as i32;
        let by = y0 + 36 + ((k * 2.0).sin() * 2.0) as i32;
        let flap = (k * 8.0) as i32 % 2;
        for (dx, dy) in [
            (0, 0),
            (-1, -flap),
            (1, -flap),
            (-2, -1 + flap),
            (2, -1 + flap),
        ] {
            if (x0..x0 + w).contains(&(bx + dx)) {
                fb.put(bx + dx, by + dy, 0x2a3040);
            }
        }
    }
    // The trees: round bumpy crowns, dark against the day.
    let crowns = [
        (-4.0f32, line + 14, 18.0f32),
        (24.0, line + 10, 15.0),
        (60.0, line + 4, 20.0),
        (98.0, line + 12, 14.0),
        (140.0, line - 2, 22.0),
        (178.0, line + 8, 17.0),
        (206.0, line + 14, 15.0),
    ];
    for y in line - 60..h {
        for x in 0..w {
            let mut tree = y > line + 12;
            if !tree {
                for &(tx, ty, tr) in &crowns {
                    let bump = 2.2 * (x as f32 * 0.6 + y as f32 * 0.3).sin()
                        + 1.6 * (x as f32 * 0.23 - y as f32 * 0.5).sin();
                    let (u, v) = ((x as f32 - tx) / tr, (y - ty) as f32 / (tr * 1.25));
                    if (u * u + v * v).sqrt() * tr < tr + bump {
                        tree = true;
                        break;
                    }
                }
            }
            if tree {
                let mut c = if (x * 3 + y) % 7 != 0 {
                    0x0a0e0b
                } else {
                    0x111812
                };
                if y < line + 4 && (fb.at(x0 + x, y0 + y - 1) & 255) > 120 {
                    c = 0x1e2a22;
                }
                fb.put(x0 + x, y0 + y, c);
            }
        }
    }
    // The house, pale in the dark, its lit windows, a door.
    let (hx0, hy0, hw, hh) = (
        (w as f32 * 0.42) as i32,
        line + 10,
        (w as f32 * 0.30) as i32,
        (h as f32 * 0.30) as i32,
    );
    fb.rect(x0 + hx0, y0 + hy0, hw, hh, 0x5a5650);
    fb.rect(x0 + hx0, y0 + hy0, hw, 2, 0x6e6a62);
    fb.rect(x0 + hx0 - 3, y0 + hy0 - 4, hw + 6, 4, 0x2a2826);
    for (i, (wx, wy, lit)) in [
        (6, 7, true),
        (hw - 14, 7, true),
        (6, 24, true),
        (hw - 14, 24, false),
    ]
    .into_iter()
    .enumerate()
    {
        // Somebody inside puts a light out for a while.
        let lit = lit && !(i == 0 && (6.0..9.5).contains(&t.rem_euclid(12.0)));
        let (glass, bars) = if lit {
            (0xf4c86c, 0x7a5a26)
        } else {
            (0x22221f, 0x141412)
        };
        fb.rect(x0 + hx0 + wx, y0 + hy0 + wy, 8, 11, glass);
        fb.rect(x0 + hx0 + wx + 3, y0 + hy0 + wy, 1, 11, bars);
        fb.rect(x0 + hx0 + wx, y0 + hy0 + wy + 4, 8, 1, bars);
    }
    fb.rect(x0 + hx0 + hw / 2 - 4, y0 + hy0 + hh - 17, 8, 17, 0x1c1a18);
    // The street lamp, its light on the wall and the ground.
    let (lx, ly) = (x0 + (w as f32 * 0.34) as i32, y0 + line + 8);
    fb.rect(lx, ly, 2, (h as f32 * 0.36) as i32, 0x0a0a0a);
    fb.rect(lx, ly, 1, (h as f32 * 0.30) as i32, 0x6a5a3a);
    fb.rect(lx - 1, ly + (h as f32 * 0.34) as i32, 4, 2, 0x1a1a1a);
    for yy in ly - 14..ly + 50 {
        for xx in lx - 30..lx + 32 {
            let d = ((xx - lx) as f32).hypot((yy - ly + 3) as f32 * 0.9);
            if d < 34.0 && od(xx, yy) < (1.0 - d / 34.0) * 0.6 && yy > y0 + line && yy < y0 + h {
                let under = fb.at(xx, yy);
                fb.put(xx, yy, lerp_color(under, 0xffd890, 0.4));
            }
        }
    }
    let k = t.rem_euclid(9.0);
    let flick = (4.0..4.12).contains(&k) || (4.3..4.36).contains(&k);
    fb.rect(
        lx - 3,
        ly - 6,
        8,
        6,
        if flick { 0xb09a6a } else { 0xfff0c0 },
    );
    fb.rect(lx - 2, ly - 7, 6, 1, 0x3a3a3a);
    // The water in front: the lamp and the windows, broken into ripples.
    let wt = (h as f32 * 0.86) as i32;
    for y in wt..h {
        for x in 0..w {
            let src = wt - (y - wt) * 2 - 4;
            let ripple = ((y as f32 * 1.3 + t * 3.0).sin() * 1.5) as i32;
            let s = fb.at(x0 + x + ripple, y0 + src);
            let (r, g, b) = ch(s);
            let sum = r + g + b;
            let mut c = 0x070a0a;
            if sum > 260 {
                c = lerp_color(c, s, if (y + x) % 2 != 0 { 0.6 } else { 0.3 });
            } else if sum > 120 {
                c = lerp_color(c, s, 0.25);
            }
            fb.put(x0 + x, y0 + y, c);
        }
    }
}

/// A man in a dark overcoat and a bowler hat, `size` pixels tall.
fn bowler(fb: &mut Framebuffer, x: i32, y: i32, size: i32, facing: bool, lift: i32) {
    let u = size as f32 / 24.0;
    let (coat, coat_hi, skin) = (0x24262e, 0x3c3f4a, 0xd8b8a0);
    let body_top = y + (8.0 * u) as i32;
    for yy in body_top..y + size {
        let f = (yy - body_top) as f32 / (y + size - body_top).max(1) as f32;
        let half = ((6.0 - f * 1.2) * u).max(1.0) as i32;
        for xx in x - half..=x + half {
            fb.put(
                xx,
                yy,
                if (xx as f32) < x as f32 - half as f32 * 0.5 {
                    coat_hi
                } else {
                    coat
                },
            );
        }
    }
    if size >= 14 {
        fb.put(x, body_top, rgb(230, 230, 230));
        fb.put(x, body_top + 1, 0x702020);
    }
    let hr = ((3.0 * u) as i32).max(1);
    for yy in y + (3.0 * u) as i32..body_top {
        for xx in x - hr..=x + hr {
            fb.put(xx, yy, skin);
        }
    }
    if facing && size >= 14 {
        fb.put(x - u as i32, y + (5.0 * u) as i32, rgb(60, 50, 50));
        fb.put(x + u as i32, y + (5.0 * u) as i32, rgb(60, 50, 50));
    }
    let brim = ((5.0 * u) as i32).max(2);
    let crown = (3.0 * u) as i32;
    if lift > 0 {
        // The hat raised off his head, and the hand that holds it.
        for yy in y..y + crown {
            fb.rect(x - hr, yy, 2 * hr + 1, 1, 0x3a2c24);
        }
        fb.rect(x + hr + 1, y - lift + crown, (u as i32).max(1), lift, coat);
    }
    fb.rect(
        x - brim,
        y + crown - lift,
        2 * brim + 1,
        (u as i32).max(1),
        0x101014,
    );
    for yy in y - lift..y + crown - lift {
        fb.rect(x - hr, yy, 2 * hr + 1, 1, 0x15151a);
    }
}

fn golconda(fb: &mut Framebuffer, x0: i32, y0: i32, w: i32, h: i32, t: f32) {
    for y in 0..h {
        for x in 0..w {
            fb.put(
                x0 + x,
                y0 + y,
                steps(0x9fbcd6, 0xd4e2ea, y as f32 / h as f32, x0 + x, y0 + y, 5.0),
            );
        }
    }
    // The houses: tiled roofs, pale fronts, windows with shutters.
    let roof_y = (h as f32 * 0.60) as i32;
    let mut walls = Vec::new();
    for (hx0, hw, dy) in [(0, 70, 0), (64, 60, 6), (118, 90, -4)] {
        let top = roof_y + dy;
        for x in hx0..(hx0 + hw).min(w) {
            let edge = (1.0 - (((x - hx0) as f32 / hw as f32) - 0.5).abs() * 2.0).max(0.0);
            let ridge = top - (12.0 * edge.powf(0.3)) as i32;
            for y in ridge..top + 4 {
                fb.put(
                    x0 + x,
                    y0 + y,
                    if (y + x / 3) % 3 != 0 {
                        0xb25a38
                    } else {
                        0x8e4228
                    },
                );
            }
            for y in top + 4..h {
                fb.put(
                    x0 + x,
                    y0 + y,
                    if x > hx0 + 2 { 0xd9c9a6 } else { 0xb8a888 },
                );
            }
        }
        walls.push((hx0, top + 4, hw));
        let mut wy = top + 9;
        while wy < h - 4 {
            let mut wx = hx0 + 6;
            while wx < hx0 + hw - 6 {
                if wx + 6 < w {
                    fb.rect(x0 + wx, y0 + wy, 6, 8, 0x3a3430);
                    fb.rect(x0 + wx - 2, y0 + wy, 2, 8, 0x6a7a5a);
                    fb.rect(x0 + wx + 6, y0 + wy, 2, 8, 0x6a7a5a);
                    fb.rect(x0 + wx, y0 + wy + 8, 6, 1, 0xf0e6d0);
                }
                wx += 12;
            }
            wy += 12;
        }
    }
    let on_wall = |x: i32, y: i32| {
        walls
            .iter()
            .any(|&(hx0, top, hw)| (hx0..hx0 + hw).contains(&x) && y >= top)
    };
    // The men, three lattices of three sizes, each floating at his own slow
    // rate; the nearer ones cast their shadows on the walls behind them.
    for (size, gap, rows, dx) in [(9, 18, 9, 0), (15, 30, 6, 9), (26, 50, 4, 20)] {
        for row in 0..rows {
            for k in -1..w / gap + 2 {
                let x = k * gap + (row % 2) * gap / 2 + dx;
                let phase = k as f32 * 1.7 + row as f32 * 2.3 + size as f32 * 0.37;
                let bob = (t * 0.8 + phase).sin() * size as f32 / 14.0;
                let y =
                    (4.0 + row as f32 * gap as f32 * 0.85 + dx as f32 * 0.4 + bob).round() as i32;
                if y as f32 > h as f32 - size as f32 * 0.3 {
                    continue;
                }
                if size >= 15 {
                    for yy in 0..size {
                        for xx in -4..5i32 {
                            let (sx, sy) = (x + xx + 6, y + yy + 3);
                            if (0..w).contains(&sx)
                                && (0..h).contains(&sy)
                                && on_wall(sx, sy)
                                && xx.abs() < 3 + yy / 8
                            {
                                let under = fb.at(x0 + sx, y0 + sy);
                                fb.put(x0 + sx, y0 + sy, lerp_color(under, rgb(60, 50, 44), 0.35));
                            }
                        }
                    }
                }
                if x > -10 && x < w + 10 {
                    let mut lift = 0;
                    let m = t.rem_euclid(12.0);
                    if size >= 26 && row == 1 && k == 2 && (6.0..8.0).contains(&m) {
                        lift =
                            (5.0 * (std::f32::consts::PI * (m - 6.0) / 2.0).sin()).round() as i32;
                    }
                    bowler(fb, x0 + x, y0 + y, size, (k + row).rem_euclid(3) != 0, lift);
                }
            }
        }
    }
}

/// Draw painting `which` on its wall, `t` seconds into its life.
pub fn draw(fb: &mut Framebuffer, which: usize, t: f32) {
    let x = (fb.w as i32 - PW) / 2;
    let y = 12 + (fb.h as i32 - 240) / 2;
    wall(fb, x, y, PW, PH);
    // The painting is drawn over a copy and only its canvas is taken back:
    // a man or a cloud that runs past the edge stops at the frame.
    let mut canvas = Framebuffer::new(fb.w, fb.h);
    canvas.px.copy_from_slice(&fb.px);
    match which % PAINTINGS.len() {
        0 => son_of_man(&mut canvas, x, y, PW, PH, t),
        1 => empire(&mut canvas, x, y, PW, PH, t),
        _ => golconda(&mut canvas, x, y, PW, PH, t),
    }
    for j in 0..PH {
        let row = (y + j) as usize * fb.w;
        let (a, b) = (row + x as usize, row + (x + PW) as usize);
        fb.px[a..b].copy_from_slice(&canvas.px[a..b]);
    }
    let (title, by) = PAINTINGS[which % PAINTINGS.len()];
    let ty = y + PH + 12;
    fb.text_centered(fb.w as i32 / 2, ty, title, rgb(224, 214, 190), 1);
    fb.text_centered(fb.w as i32 / 2, ty + 11, by, rgb(150, 140, 128), 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_painting_hangs_and_moves() {
        for which in 0..PAINTINGS.len() {
            let mut a = Framebuffer::new(320, 240);
            let mut b = Framebuffer::new(320, 240);
            draw(&mut a, which, 1.0);
            draw(&mut b, which, 9.0);
            let colours: std::collections::HashSet<_> = a.px.iter().collect();
            assert!(
                colours.len() > 30,
                "painting {which} has {} colours",
                colours.len()
            );
            assert_ne!(a.px, b.px, "painting {which} does not move");
        }
    }

    /// `cargo test --release gallery::tests::cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn cost() {
        let mut fb = Framebuffer::new(320, 240);
        for (which, (title, _)) in PAINTINGS.iter().enumerate() {
            let t0 = std::time::Instant::now();
            for k in 0..30 {
                draw(&mut fb, which, k as f32 * 0.08);
            }
            println!("{title}: {:.2} ms", t0.elapsed().as_secs_f64() * 1000.0 / 30.0);
        }
    }
}
