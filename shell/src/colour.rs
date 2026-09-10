//! Colour, on its own so both halves of the project can have it.
//!
//! The launcher draws with these and so does the theme; the command line
//! borrows the same values to wear the desktop's colours. They live here
//! rather than in `fb` because `fb` is a framebuffer, which the terminal has
//! no use for, and a colour is four arithmetic operations.

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

pub fn ch(c: Color, shift: u32) -> f32 {
    ((c >> shift) & 0xff) as f32
}

pub fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let f = |s: u32| (ch(a, s) + (ch(b, s) - ch(a, s)) * t).round() as u8;
    rgb(f(16), f(8), f(0))
}
