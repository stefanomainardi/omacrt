//! A face for the clock, and the weather's icons.
//!
//! The digits are five cells by nine, drawn at a few pixels a cell, each
//! cell with a bevel: lit along the top and the left edge of a stroke,
//! shaded along its bottom, and a hard shadow one pixel down and right of
//! the whole figure. At three pixels a cell the time is twenty seven pixels
//! tall, which is what a clock across a room needs.

use crate::fb::{Color, Framebuffer, lerp_color, rgb};
use omacrt_shell::ambient::Kind;

fn glyph(ch: char) -> Option<[&'static str; 9]> {
    Some(match ch {
        '0' => [
            "01110", "11011", "11011", "11011", "11011", "11011", "11011", "11011", "01110",
        ],
        '1' => [
            "00110", "01110", "11110", "00110", "00110", "00110", "00110", "00110", "11111",
        ],
        '2' => [
            "01110", "11011", "00011", "00011", "00110", "01100", "11000", "11000", "11111",
        ],
        '3' => [
            "11110", "00011", "00011", "01110", "00011", "00011", "00011", "11011", "01110",
        ],
        '4' => [
            "00011", "00111", "01011", "11011", "11011", "11111", "00011", "00011", "00011",
        ],
        '5' => [
            "11111", "11000", "11000", "11110", "00011", "00011", "00011", "11011", "01110",
        ],
        '6' => [
            "01110", "11000", "11000", "11110", "11011", "11011", "11011", "11011", "01110",
        ],
        '7' => [
            "11111", "00011", "00011", "00110", "00110", "01100", "01100", "01100", "01100",
        ],
        '8' => [
            "01110", "11011", "11011", "01110", "11011", "11011", "11011", "11011", "01110",
        ],
        '9' => [
            "01110", "11011", "11011", "11011", "01111", "00011", "00011", "00011", "01110",
        ],
        ':' => [
            "000", "000", "010", "000", "000", "000", "010", "000", "000",
        ],
        'C' => [
            "01110", "11011", "11000", "11000", "11000", "11000", "11000", "11011", "01110",
        ],
        // The degree sign.
        'o' => [
            "010", "101", "010", "000", "000", "000", "000", "000", "000",
        ],
        '-' => [
            "000", "000", "000", "000", "111", "000", "000", "000", "000",
        ],
        _ => return None,
    })
}

/// How wide `text` is at `cell` pixels a cell.
pub fn width(text: &str, cell: i32) -> i32 {
    let w: i32 = text
        .chars()
        .filter_map(glyph)
        .map(|g| (g[0].len() as i32 + 1) * cell)
        .sum();
    (w - cell).max(0)
}

/// Draw `text` with its top left at (x, y). The colon is left out when
/// `colon` is false, which is how it beats.
pub fn draw(
    fb: &mut Framebuffer,
    mut x: i32,
    y: i32,
    text: &str,
    cell: i32,
    (face, hi, lo): (Color, Color, Color),
    colon: bool,
) {
    for ch in text.chars() {
        let Some(g) = glyph(ch) else { continue };
        let w = g[0].len() as i32;
        let on = |r: i32, c: i32| -> bool {
            r >= 0
                && c >= 0
                && (r as usize) < g.len()
                && (c as usize) < g[0].len()
                && g[r as usize].as_bytes()[c as usize] == b'1'
        };
        if ch != ':' || colon {
            for r in 0..9 {
                for c in 0..w {
                    if on(r, c) {
                        fb.rect(
                            x + c * cell + 1,
                            y + r * cell + 1,
                            cell,
                            cell,
                            rgb(6, 6, 12),
                        );
                    }
                }
            }
            for r in 0..9 {
                for c in 0..w {
                    if !on(r, c) {
                        continue;
                    }
                    let (px, py) = (x + c * cell, y + r * cell);
                    fb.rect(px, py, cell, cell, face);
                    if !on(r - 1, c) {
                        fb.rect(px, py, cell, 1, hi);
                    }
                    if !on(r, c - 1) {
                        fb.rect(px, py, 1, cell, hi);
                    }
                    if !on(r + 1, c) {
                        fb.rect(px, py + cell - 1, cell, 1, lo);
                    }
                }
            }
        }
        x += (w + 1) * cell;
    }
}

fn disc(fb: &mut Framebuffer, cx: f32, cy: f32, r: f32, c: Color, rim: Option<Color>) {
    for y in (cy - r) as i32 - 1..=(cy + r) as i32 + 1 {
        for x in (cx - r) as i32 - 1..=(cx + r) as i32 + 1 {
            let d = (x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy);
            if d <= r {
                fb.put(
                    x,
                    y,
                    match rim {
                        Some(rc) if d > r - 1.2 => rc,
                        _ => c,
                    },
                );
            }
        }
    }
}

/// The weather as a twenty pixel icon: a sun or a moon, a cloud, what
/// falls from it.
pub fn icon(fb: &mut Framebuffer, x: i32, y: i32, kind: Kind, night: bool) {
    let (fx, fy) = (x as f32, y as f32);
    let sunny = matches!(kind, Kind::Clear | Kind::Partly);
    if sunny {
        if night {
            disc(fb, fx + 8.0, fy + 7.0, 6.0, rgb(236, 236, 214), None);
            disc(fb, fx + 11.0, fy + 5.0, 5.0, fb.at(x + 18, y + 1), None);
        } else {
            for k in 0..8 {
                let a = k as f32 / 8.0 * std::f32::consts::TAU;
                for s in [8.0f32, 9.0] {
                    fb.put(
                        (fx + 8.0 + a.cos() * s) as i32,
                        (fy + 7.0 + a.sin() * s) as i32,
                        rgb(255, 200, 80),
                    );
                }
            }
            disc(
                fb,
                fx + 8.0,
                fy + 7.0,
                5.5,
                rgb(255, 222, 110),
                Some(rgb(255, 170, 60)),
            );
        }
    }
    if kind != Kind::Clear {
        let (grey, shade) = if matches!(kind, Kind::Heavy | Kind::Thunder | Kind::Overcast) {
            (rgb(170, 176, 192), rgb(120, 126, 144))
        } else {
            (rgb(230, 234, 244), rgb(170, 178, 196))
        };
        let (cx, cy) = (fx + 11.0, fy + 11.0);
        for (bx, by, br) in [
            (cx - 4.0, cy + 1.0, 4.0f32),
            (cx + 1.0, cy - 2.0, 5.0),
            (cx + 6.0, cy + 1.0, 4.0),
        ] {
            disc(fb, bx, by, br + 1.0, rgb(10, 10, 20), None);
        }
        for (bx, by, br) in [
            (cx - 4.0, cy + 1.0, 4.0f32),
            (cx + 1.0, cy - 2.0, 5.0),
            (cx + 6.0, cy + 1.0, 4.0),
        ] {
            disc(fb, bx, by, br, grey, None);
        }
        fb.rect((cx - 7.0) as i32, (cy + 3.0) as i32, 16, 3, shade);
    }
    match kind {
        Kind::Rain | Kind::Heavy => {
            for k in 0..4 {
                fb.rect(
                    x + 6 + k * 4,
                    y + 17 + (k % 2) * 2,
                    1,
                    3,
                    rgb(110, 170, 255),
                );
            }
        }
        Kind::Snow => {
            for k in 0..4 {
                fb.put(x + 6 + k * 4, y + 18 + (k % 2) * 2, rgb(255, 255, 255));
            }
        }
        Kind::Thunder => {
            for (dx, dy) in [(10, 15), (9, 17), (11, 17), (10, 19), (9, 21)] {
                fb.put(x + dx, y + dy, rgb(255, 230, 90));
            }
        }
        Kind::Fog => {
            for k in 0..3 {
                fb.rect(
                    x + 3,
                    y + 16 + k * 2,
                    16,
                    1,
                    lerp_color(rgb(200, 204, 214), rgb(120, 124, 136), k as f32 / 3.0),
                );
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_time_is_as_wide_as_its_digits() {
        // Four digits of five cells and a colon of three, a cell between
        // each: 27 cells.
        assert_eq!(width("12:34", 3), 27 * 3);
    }
}
