//! Software framebuffer at the CRT's native resolution.
//!
//! Everything is drawn here in integer pixels, then handed to SDL as one
//! streaming texture. No shader tries to fake a tube: the real tube does that.

use crate::font8x8::FONT8X8;

pub type Color = u32; // 0x00RRGGBB

pub fn rgb(r: u8, g: u8, b: u8) -> Color {
    ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

pub fn parse_hex(s: &str) -> Option<Color> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    u32::from_str_radix(s, 16).ok()
}

fn ch(c: Color, shift: u32) -> f32 {
    ((c >> shift) & 0xff) as f32
}

/// Scale a color by `a` (0.0 .. 1.0+). Values above 1.0 bloom toward white.
pub fn scale(c: Color, a: f32) -> Color {
    let f = |v: f32| (v * a).round().clamp(0.0, 255.0) as u8;
    rgb(f(ch(c, 16)), f(ch(c, 8)), f(ch(c, 0)))
}

pub fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let f = |s: u32| (ch(a, s) + (ch(b, s) - ch(a, s)) * t).round() as u8;
    rgb(f(16), f(8), f(0))
}

pub fn add(a: Color, b: Color) -> Color {
    let f = |s: u32| (ch(a, s) + ch(b, s)).min(255.0) as u8;
    rgb(f(16), f(8), f(0))
}

pub struct Framebuffer {
    pub w: usize,
    pub h: usize,
    pub px: Vec<Color>,
}

impl Framebuffer {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            px: vec![0; w * h],
        }
    }

    pub fn clear(&mut self, c: Color) {
        self.px.fill(c);
    }

    #[inline]
    pub fn put(&mut self, x: i32, y: i32, c: Color) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        self.px[y as usize * self.w + x as usize] = c;
    }

    /// Draw an image with straight alpha over what is already there.
    pub fn blit(&mut self, x: i32, y: i32, img: &crate::art::Image) {
        for iy in 0..img.h {
            let dy = y + iy as i32;
            if dy < 0 || dy >= self.h as i32 {
                continue;
            }
            for ix in 0..img.w {
                let dx = x + ix as i32;
                if dx < 0 || dx >= self.w as i32 {
                    continue;
                }
                let bg = self.px[dy as usize * self.w + dx as usize];
                self.px[dy as usize * self.w + dx as usize] = img.over(ix, iy, bg);
            }
        }
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Color) {
        for yy in y.max(0)..(y + h).min(self.h as i32) {
            for xx in x.max(0)..(x + w).min(self.w as i32) {
                self.px[yy as usize * self.w + xx as usize] = c;
            }
        }
    }

    pub fn rect_add(&mut self, x: i32, y: i32, w: i32, h: i32, c: Color) {
        for yy in y.max(0)..(y + h).min(self.h as i32) {
            for xx in x.max(0)..(x + w).min(self.w as i32) {
                let i = yy as usize * self.w + xx as usize;
                self.px[i] = add(self.px[i], c);
            }
        }
    }

    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Color) {
        let (mut x, mut y) = (x0, y0);
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.put(x, y, c);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Draw one 8x8 glyph scaled by `s`.
    pub fn glyph(&mut self, x: i32, y: i32, ch: char, c: Color, s: i32) {
        let code = ch as usize;
        let rows = if code < 128 {
            &FONT8X8[code]
        } else {
            &FONT8X8[b'?' as usize]
        };
        for (ry, bits) in rows.iter().enumerate() {
            for rx in 0..8 {
                if bits & (1 << rx) != 0 {
                    self.rect(x + rx as i32 * s, y + ry as i32 * s, s, s, c);
                }
            }
        }
    }

    pub fn text(&mut self, x: i32, y: i32, s: &str, c: Color, scale: i32) {
        let mut cx = x;
        for ch in s.chars() {
            self.glyph(cx, y, ch, c, scale);
            cx += 8 * scale;
        }
    }

    pub fn text_width(s: &str, scale: i32) -> i32 {
        s.chars().count() as i32 * 8 * scale
    }

    pub fn text_centered(&mut self, cx: i32, y: i32, s: &str, c: Color, scale: i32) {
        let w = Self::text_width(s, scale);
        self.text(cx - w / 2, y, s, c, scale);
    }

    /// Draw a 1-bit bitmap (`rows[y][x] == '#'`) scaled by `s`, only rows below `max_rows`.
    pub fn bitmap(&mut self, x: i32, y: i32, rows: &[&str], c: Color, s: i32, max_rows: usize) {
        for (ry, row) in rows.iter().enumerate().take(max_rows) {
            for (rx, ch) in row.chars().enumerate() {
                if ch == '#' {
                    self.rect(x + rx as i32 * s, y + ry as i32 * s, s, s, c);
                }
            }
        }
    }

    /// Multiply the whole picture by `gain` (power-on curve).
    pub fn apply_gain(&mut self, gain: f32) {
        if (gain - 1.0).abs() < 0.001 {
            return;
        }
        for p in self.px.iter_mut() {
            *p = scale(*p, gain);
        }
    }

    /// Vertical roll: every column is displaced by a sine of its x, like a
    /// tube losing vertical hold for a moment. `amount` 0..1.
    pub fn roll(&mut self, amount: f32, time: f32) {
        if amount <= 0.001 {
            return;
        }
        let src = self.px.clone();
        let w = self.w as i32;
        let h = self.h as i32;
        for x in 0..w {
            let px = (x as f32 / w as f32) - 0.5;
            let dy = (amount * 0.08 * h as f32 * (time * 40.0 + px * 8.0).sin()).round() as i32;
            for y in 0..h {
                let sy = y + dy;
                let c = if sy >= 0 && sy < h {
                    src[sy as usize * self.w + x as usize]
                } else {
                    0
                };
                self.px[y as usize * self.w + x as usize] = c;
            }
        }
    }

    /// Bytes for an SDL ARGB8888 streaming texture (little endian: B G R A).
    pub fn to_bgra(&self, out: &mut Vec<u8>) {
        out.clear();
        out.reserve(self.px.len() * 4);
        for p in &self.px {
            out.extend_from_slice(&(*p | 0xff00_0000).to_le_bytes());
        }
    }

    /// Debug dump as binary PPM.
    pub fn write_ppm(&self, path: &std::path::Path) -> std::io::Result<()> {
        let mut data = format!("P6\n{} {}\n255\n", self.w, self.h).into_bytes();
        for p in &self.px {
            data.push((p >> 16) as u8);
            data.push((p >> 8) as u8);
            data.push(*p as u8);
        }
        std::fs::write(path, data)
    }
}

impl Framebuffer {
    /// Like `blit_scaled`, but the left edge is `h_left` tall and the right
    /// edge `h_right`, both centred on the box: a cover seen at an angle.
    /// With `flip` the image is drawn upside down (a reflection); nothing is
    /// drawn at or below `clip_y`, and with `fade_rows` > 0 the alpha thins
    /// out over that many rows from the top of the box.
    #[allow(clippy::too_many_arguments)]
    pub fn blit_trapezoid(
        &mut self,
        img: &crate::art::Image,
        x: i32,
        y: i32,
        w: i32,
        h_left: i32,
        h_right: i32,
        shade: f32,
        alpha: f32,
        flip: bool,
        clip_y: i32,
        fade_rows: i32,
    ) {
        if img.w == 0 || img.h == 0 || w <= 0 {
            return;
        }
        let cy = y as f32 + h_left.max(h_right) as f32 / 2.0;
        for c in 0..w {
            let t = c as f32 / w as f32;
            let hcol = (h_left as f32 + (h_right - h_left) as f32 * t).max(1.0);
            let sx = ((c as usize) * img.w / w as usize).min(img.w - 1);
            let top = cy - hcol / 2.0;
            let dx = x + c;
            if dx < 0 || dx >= self.w as i32 {
                continue;
            }
            for r in 0..hcol as i32 {
                let dy = top as i32 + r;
                if dy < 0 || dy >= self.h as i32 || dy >= clip_y {
                    continue;
                }
                let row_alpha = if fade_rows > 0 {
                    alpha * (1.0 - (dy - y) as f32 / fade_rows as f32).clamp(0.0, 1.0)
                } else {
                    alpha
                };
                let mut sy = (r as usize) * img.h / hcol as usize;
                if flip {
                    sy = img.h - 1 - sy.min(img.h - 1);
                }
                let p = img.px[sy.min(img.h - 1) * img.w + sx];
                let a = ((p >> 24) & 0xff) as f32 / 255.0 * row_alpha;
                if a <= 0.0 {
                    continue;
                }
                let c = scale(p & 0x00ff_ffff, shade);
                let bg = self.px[dy as usize * self.w + dx as usize];
                self.px[dy as usize * self.w + dx as usize] = lerp_color(bg, c, a);
            }
        }
    }
}

#[cfg(test)]
mod trapezoid_tests {
    use super::*;

    #[test]
    fn flipped_reflection_draws_rows() {
        let img = crate::art::Image {
            w: 4,
            h: 4,
            px: vec![0xffff_0000; 16],
        };
        let mut fb = Framebuffer::new(20, 40);
        fb.clear(0);
        // A cover bottom at row 10; the reflection starts at 12 and fades over 10 rows.
        fb.blit_trapezoid(&img, 2, 12, 8, 8, 8, 1.0, 0.5, true, 30, 10);
        let drawn: Vec<usize> = (0..40).filter(|y| (0..20).any(|x| fb.px[y * 20 + x] != 0)).collect();
        assert!(!drawn.is_empty(), "nothing drawn");
        assert_eq!(drawn[0], 12, "rows start at the box top, got {drawn:?}");
    }
}

#[cfg(test)]
mod reflection_scene_numbers {
    use super::*;

    #[test]
    fn reflection_with_scene_numbers() {
        let img = crate::art::Image { w: 100, h: 126, px: vec![0xffff_0000; 100 * 126] };
        let mut fb = Framebuffer::new(320, 240);
        fb.clear(0);
        let floor_y = 162;
        fb.blit_trapezoid(&img, 110, 171, 100, 126, 126, 0.6, 0.35, true, floor_y + 30, 30);
        let drawn: Vec<usize> = (0..240).filter(|y| (0..320).any(|x| fb.px[y * 320 + x] != 0)).collect();
        assert_eq!((drawn.first().copied(), drawn.last().copied()), (Some(171), Some(191)), "{drawn:?}");
    }
}
