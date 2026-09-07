//! The hi-fi deck and the visualizers of the music screens.
//!
//! The BeoCenter 1 this project grew up with is a tape deck with a radio, so
//! the now playing screen is one too: a cassette whose reels turn with the
//! music (a radio dial with a gliding needle for stations), two VU meters
//! with needles that have inertia, and after a few idle seconds a full
//! screen visualizer in the spirit of the demos of the nineties: a Mode 7
//! equalizer, copper bars, an oscilloscope, a starfield, plasma, pixel fire,
//! a spectrum tower. Everything is drawn into the 320x240 framebuffer from
//! ten spectrum bands cliamp reports and the playback position; a kick
//! detector on the bass gives every mode a beat to react to.

use crate::art::Image;
use crate::fb::{Color, Framebuffer, lerp_color, scale};
use crate::theme::Theme;

pub const MODES: usize = 7;
pub const MODE_NAMES: [&str; MODES] = [
    "mode 7 equalizer",
    "silk ribbons",
    "oscilloscope",
    "starfield",
    "plasma",
    "pixel fire",
    "spectrum tower",
];

/// Seconds without input before the deck gives way to the visualizer.
pub const IDLE_TO_VISUAL: f64 = 6.0;
/// Seconds a visualizer mode stays before the next, when cycling.
pub const MODE_SECS: f64 = 45.0;

/// Beat detection on the bass: a hit when the low bands jump above their
/// running average. `hit` is 1.0 on the beat and decays.
pub struct Kick {
    avg: f32,
    last: f64,
    pub hit: f32,
}

impl Kick {
    pub fn new() -> Self {
        Self {
            avg: 0.0,
            last: 0.0,
            hit: 0.0,
        }
    }

    pub fn update(&mut self, bands: &[f32], now: f64, dt: f32) -> bool {
        let bass = bands.iter().take(2).sum::<f32>() / 2.0;
        self.avg += (bass - self.avg) * 0.04;
        self.hit *= (1.0 - 6.0 * dt).clamp(0.0, 1.0);
        if bass > 0.14 && bass > self.avg * 1.45 && now - self.last > 0.22 {
            self.last = now;
            self.hit = 1.0;
            return true;
        }
        false
    }
}

struct Star {
    x: f32,
    y: f32,
    z: f32,
    pz: f32,
}

/// What the deck shows this frame.
pub struct Info<'a> {
    pub title: &'a str,
    pub sub: &'a str,
    pub position: f64,
    pub duration: f64,
    pub playing: bool,
    pub radio: bool,
    /// A record on the turntable instead of a cassette (Spotify and albums).
    pub turntable: bool,
    /// The track comes from Spotify: its badge shows on the deck.
    pub spotify: bool,
    /// Station index and count of the tuned list, for the dial position.
    pub station: Option<(usize, usize)>,
    pub cover: Option<&'a Image>,
    pub volume_db: f64,
}

pub struct Deck {
    pub mode: usize,
    pub mode_since: f64,
    reel: f32,
    vu: [f32; 2],
    peak: [f32; 2],
    peak_at: [f64; 2],
    dial: f32,
    /// Where the dial needle is heading (0..1 on the scale).
    pub dial_target: f32,
    /// Static hiss and needle travel until then.
    pub tuning_until: f64,
    /// Start of the cassette insert animation.
    pub insert_at: f64,
    pub kick: Kick,
    /// True on the frame a beat was detected.
    pub beat: bool,
    smooth: [f32; 10],
    energy: f32,
    stars: Vec<Star>,
    fire: Vec<u8>,
    scope_prev: Vec<Vec<i32>>,
    phases: [f32; 10],
    last: f64,
    rng: u32,
}

const FIRE_W: usize = 80;
const FIRE_H: usize = 58;

impl Deck {
    pub fn new() -> Self {
        let mut d = Self {
            mode: 0,
            mode_since: 0.0,
            reel: 0.0,
            vu: [0.0; 2],
            peak: [0.0; 2],
            peak_at: [0.0; 2],
            dial: 0.3,
            dial_target: 0.3,
            tuning_until: 0.0,
            insert_at: -10.0,
            kick: Kick::new(),
            beat: false,
            smooth: [0.0; 10],
            energy: 0.0,
            stars: Vec::new(),
            fire: vec![0; FIRE_W * FIRE_H],
            scope_prev: Vec::new(),
            phases: [0.0; 10],
            last: 0.0,
            rng: 0x9e37_79b9,
        };
        for i in 0..10 {
            d.phases[i] = d.rand() * std::f32::consts::TAU;
        }
        for _ in 0..260 {
            let s = d.new_star();
            d.stars.push(s);
        }
        d
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    fn new_star(&mut self) -> Star {
        let z = 0.15 + self.rand() * 0.85;
        Star {
            x: (self.rand() - 0.5) * 2.0,
            y: (self.rand() - 0.5) * 2.0,
            z,
            pz: z,
        }
    }

    /// Advance the moving parts. `bands` are cliamp's ten values in 0..1.
    pub fn tick(&mut self, bands: &[f32], playing: bool, now: f64) {
        let dt = if self.last == 0.0 {
            1.0 / 60.0
        } else {
            ((now - self.last) as f32).clamp(0.0, 0.1)
        };
        self.last = now;
        for (i, s) in self.smooth.iter_mut().enumerate() {
            let target = bands.get(i).copied().unwrap_or(0.0).clamp(0.0, 1.0);
            let k = if target > *s { 0.6 } else { 0.18 };
            *s += (target - *s) * k;
        }
        self.energy = self.smooth.iter().sum::<f32>() / 10.0;
        self.beat = self.kick.update(&self.smooth, now, dt);
        if playing {
            self.reel += dt * (2.2 + self.energy * 1.5);
        }
        // Two channels from the bands: lows and mids left, mids and highs right.
        let left = (self.smooth[0] + self.smooth[1] + self.smooth[2] + self.smooth[4]) / 4.0;
        let right = (self.smooth[3] + self.smooth[5] + self.smooth[6] + self.smooth[7]) / 4.0;
        for (i, target) in [left, right].into_iter().enumerate() {
            let target = if playing { (target * 1.6).min(1.0) } else { 0.0 };
            let k = if target > self.vu[i] { 0.35 } else { 0.08 };
            self.vu[i] += (target - self.vu[i]) * k;
            if self.vu[i] > self.peak[i] {
                self.peak[i] = self.vu[i];
                self.peak_at[i] = now;
            } else if now - self.peak_at[i] > 0.8 {
                self.peak[i] = (self.peak[i] - dt * 0.6).max(0.0);
            }
        }
        self.dial += (self.dial_target - self.dial) * 0.12;
    }

    pub fn next_mode(&mut self, now: f64) {
        self.mode = (self.mode + 1) % MODES;
        self.mode_since = now;
    }

    pub fn prev_mode(&mut self, now: f64) {
        self.mode = (self.mode + MODES - 1) % MODES;
        self.mode_since = now;
    }

    /// Start a station change: the needle heads for the new place on the
    /// dial and static plays for a moment.
    pub fn tune(&mut self, index: usize, count: usize, now: f64) {
        self.dial_target = if count > 1 {
            0.06 + 0.88 * index as f32 / (count - 1) as f32
        } else {
            0.5
        };
        self.tuning_until = now + 0.7;
    }

    // ------------------------------------------------------------- deck

    /// The deck between the header and the hints: cassette or dial on the
    /// left, VU meters on the right, title and times underneath.
    pub fn draw(&mut self, fb: &mut Framebuffer, th: &Theme, y0: i32, now: f64, info: &Info) {
        let w = fb.w as i32;
        let left = (w as f32 * 0.05) as i32;
        let width = w - 2 * left;
        let slot_w = 160;
        let slot_h = 92;
        let slot_x = left;
        let slot_y = y0 + 2;
        // The slot the cassette sits in.
        fb.rect(slot_x, slot_y, slot_w, slot_h, scale(th.selection, 0.55));
        fb.rect(slot_x, slot_y, slot_w, 1, scale(th.dim, 0.5));
        fb.rect(slot_x, slot_y + slot_h - 1, slot_w, 1, scale(th.fg, 0.15));
        if info.radio {
            let label = if info.station.is_some() { info.title } else { info.sub };
            self.draw_dial(fb, th, slot_x + 6, slot_y + 6, slot_w - 12, slot_h - 12, now, info.playing, label);
        } else if info.turntable {
            let progress = if info.duration > 0.0 {
                (info.position / info.duration).clamp(0.0, 1.0) as f32
            } else {
                0.0
            };
            self.draw_turntable(fb, th, slot_x + 4, slot_y + 4, slot_w - 8, slot_h - 8, progress, info);
        } else {
            let progress = if info.duration > 0.0 {
                (info.position / info.duration).clamp(0.0, 1.0) as f32
            } else {
                0.5
            };
            // Insert: the cassette drops in from above during the first half second.
            let ins = ((now - self.insert_at) / 0.55).clamp(0.0, 1.0) as f32;
            let ease = 1.0 - (1.0 - ins) * (1.0 - ins);
            let drop = ((1.0 - ease) * -(slot_h as f32 + 10.0)) as i32;
            let cy = slot_y + 6 + drop;
            if cy + slot_h - 12 > y0 - 12 {
                self.draw_cassette(fb, th, slot_x + 8, cy, slot_w - 16, slot_h - 12, progress, info);
            }
        }
        if info.spotify {
            // The badge sits in the slot's corner, phosphor green.
            fb.bitmap(slot_x + slot_w - 12, slot_y + 3, &crate::icons::SPOTIFY, th.green, 1, 8);
        }
        // VU meters on the right.
        let vx = slot_x + slot_w + 8;
        let vw = width - slot_w - 8;
        let vh = (slot_h - 4) / 2;
        self.draw_vu(fb, th, vx, slot_y, vw, vh, 0, "L");
        self.draw_vu(fb, th, vx, slot_y + vh + 4, vw, vh, 1, "R");
        // Title, subtitle, times.
        let ty = slot_y + slot_h + 8;
        let max_cols = (width / 8) as usize;
        let title: String = info.title.chars().take(max_cols).collect();
        fb.text(left, ty, &title, th.bright_green, 1);
        let sub: String = info.sub.chars().take(max_cols).collect();
        fb.text(left, ty + 12, &sub, th.paper, 1);
        let bar_y = ty + 26;
        fb.rect(left, bar_y, width, 4, th.selection);
        let times = if info.duration > 0.0 {
            let filled = ((info.position / info.duration).clamp(0.0, 1.0) * width as f64) as i32;
            fb.rect(left, bar_y, filled, 4, th.accent);
            format!(
                "{} / {}",
                crate::player::clock(info.position),
                crate::player::clock(info.duration)
            )
        } else {
            if info.playing {
                let x = left + ((now * 40.0) as i32 % (width - 6).max(1));
                fb.rect(x, bar_y, 6, 4, th.accent);
            }
            crate::player::clock(info.position)
        };
        fb.text(left, bar_y + 8, &times, th.paper, 1);
        let state = if info.playing { "playing" } else { "paused" };
        let right = format!("{state}  {:+.0} dB", info.volume_db);
        fb.text(
            w - left - Framebuffer::text_width(&right, 1),
            bar_y + 8,
            &right,
            th.dim,
            1,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_cassette(
        &mut self,
        fb: &mut Framebuffer,
        th: &Theme,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        progress: f32,
        info: &Info,
    ) {
        let shell = lerp_color(th.bg, th.fg, 0.16);
        let edge = lerp_color(th.bg, th.fg, 0.34);
        // Body with rounded corners: skip the corner pixels.
        fb.rect(x + 1, y, w - 2, h, shell);
        fb.rect(x, y + 1, w, h - 2, shell);
        fb.rect(x + 1, y, w - 2, 1, edge);
        fb.rect(x, y + 1, 1, h - 2, edge);
        fb.rect(x + w - 1, y + 1, 1, h - 2, scale(edge, 0.6));
        fb.rect(x + 1, y + h - 1, w - 2, 1, scale(edge, 0.6));
        // Screws.
        for (sx, sy) in [(x + 3, y + 3), (x + w - 5, y + 3), (x + 3, y + h - 5), (x + w - 5, y + h - 5)] {
            fb.rect(sx, sy, 2, 2, th.dim);
        }
        // Label: a paper strip with the title, or the cover art.
        let lx = x + 8;
        let ly = y + 6;
        let lw = w - 16;
        let lh = 26;
        let paper = lerp_color(th.paper, th.bg, 0.15);
        fb.rect(lx, ly, lw, lh, paper);
        fb.rect(lx, ly + lh - 3, lw, 1, th.accent);
        fb.rect(lx, ly + lh - 1, lw, 1, th.magenta);
        let ink = lerp_color(th.bg, th.selection, 0.5);
        let mut text_x = lx + 4;
        if let Some(img) = info.cover {
            let cw = (img.w as i32).min(lh - 4);
            let ch = (img.h as i32).min(lh - 4);
            if cw > 0 && ch > 0 {
                fb.blit(lx + 2, ly + 2, img);
                text_x = lx + 2 + cw + 4;
            }
        }
        let room = ((lx + lw - 2 - text_x) / 8).max(0) as usize;
        let t1: String = info.title.chars().take(room).collect();
        fb.text(text_x, ly + 4, &t1, ink, 1);
        let t2: String = info.sub.chars().take(room).collect();
        fb.text(text_x, ly + 14, &t2, scale(ink, 1.6).min(th.dim), 1);
        // Window with the two reels.
        let wx = x + 22;
        let wy = ly + lh + 5;
        let ww = w - 44;
        let wh = h - lh - 20;
        fb.rect(wx, wy, ww, wh, scale(th.bg, 0.9));
        fb.rect(wx, wy, ww, 1, th.dim);
        let cyr = wy + wh / 2;
        let hub = 5.0;
        let tape_l = (1.0 - progress) * 9.0;
        let tape_r = progress * 9.0;
        let angle = self.reel;
        for (cx, tape) in [(wx + ww / 4, tape_l), (wx + 3 * ww / 4, tape_r)] {
            // Tape wound on the reel.
            draw_disc(fb, cx, cyr, hub + tape, lerp_color(th.bg, th.fg, 0.28));
            draw_disc(fb, cx, cyr, hub, lerp_color(th.bg, th.paper, 0.5));
            // Three spokes turning.
            for k in 0..3 {
                let a = angle + k as f32 * std::f32::consts::TAU / 3.0;
                let (dx, dy) = (a.cos(), a.sin());
                fb.line(
                    cx + (dx * 2.0) as i32,
                    cyr + (dy * 2.0) as i32,
                    cx + (dx * (hub - 1.0)) as i32,
                    cyr + (dy * (hub - 1.0)) as i32,
                    th.bg,
                );
            }
        }
        // Tape path between the reels.
        fb.rect(wx + ww / 4, cyr + (hub as i32) + 2, ww / 2, 1, lerp_color(th.bg, th.fg, 0.3));
        // Side label: A.
        fb.text(x + 6, wy + 2, "A", th.dim, 1);
        // Playing light.
        if info.playing {
            fb.rect(x + w - 12, wy + 2, 3, 3, th.green);
        }
    }

    /// A record player: platter, a black record whose grooves catch a
    /// rotating sheen, the label (album art when there is one) and a tonearm
    /// that tracks inward with the position.
    #[allow(clippy::too_many_arguments)]
    fn draw_turntable(
        &mut self,
        fb: &mut Framebuffer,
        th: &Theme,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        progress: f32,
        info: &Info,
    ) {
        let plinth = lerp_color(th.bg, th.fg, 0.14);
        fb.rect(x, y, w, h, plinth);
        fb.rect(x, y, w, 1, lerp_color(th.bg, th.fg, 0.3));
        // Platter and record, off centre to leave room for the arm.
        let cx = x + h / 2 + 4;
        let cy = y + h / 2;
        let r_platter = (h / 2 - 4) as f32;
        let r_record = r_platter - 2.0;
        draw_disc(fb, cx, cy, r_platter, lerp_color(th.bg, th.dim, 0.6));
        draw_disc(fb, cx, cy, r_record, scale(th.bg, 0.6));
        // Grooves: rings every three pixels, lit where a sheen passes as the
        // record turns.
        let sheen = self.reel * 0.9;
        let mut r = r_record - 2.0;
        while r > 12.0 {
            let steps = (r * 6.0) as i32;
            for k in 0..steps {
                let a = k as f32 / steps as f32 * std::f32::consts::TAU;
                let glint = ((a - sheen).cos()).max(0.0).powi(6);
                let c = lerp_color(lerp_color(th.bg, th.fg, 0.10), th.paper, glint * 0.45);
                fb.put(cx + (a.cos() * r) as i32, cy + (a.sin() * r) as i32, c);
            }
            r -= 3.0;
        }
        // Label: the album art clipped to a disc, or the accent.
        let r_label = 11.0;
        match info.cover {
            Some(img) if img.w > 0 => {
                let ri = r_label as i32;
                for dy in -ri..=ri {
                    for dx in -ri..=ri {
                        if (dx * dx + dy * dy) as f32 > r_label * r_label {
                            continue;
                        }
                        let sx = ((dx + ri) as usize * img.w / (2 * ri as usize + 1)).min(img.w - 1);
                        let sy = ((dy + ri) as usize * img.h / (2 * ri as usize + 1)).min(img.h - 1);
                        fb.put(cx + dx, cy + dy, img.px[sy * img.w + sx] & 0x00ff_ffff);
                    }
                }
            }
            _ => {
                draw_disc(fb, cx, cy, r_label, th.accent);
                draw_disc(fb, cx, cy, r_label - 4.0, lerp_color(th.accent, th.paper, 0.35));
            }
        }
        // Spindle and a rotating mark on the label so the turn shows.
        fb.rect(cx, cy, 1, 1, th.paper);
        let (ms, mc) = (self.reel * 0.9).sin_cos();
        fb.put(cx + (mc * 8.0) as i32, cy + (ms * 8.0) as i32, th.paper);
        // Tonearm: pivot top right, the stylus moving from the edge inward.
        let px = x + w - 12;
        let py = y + 8;
        fb.rect(px - 3, py - 3, 7, 7, lerp_color(th.bg, th.fg, 0.4));
        let target_r = if info.playing || progress > 0.0 {
            r_record - 4.0 - (r_record - 4.0 - r_label - 3.0) * progress
        } else {
            r_record + 10.0
        };
        // Stylus point on the record's radius toward the arm side.
        let dir = ((py - cy) as f32).atan2((px - cx) as f32);
        let sx = cx as f32 + dir.cos() * target_r;
        let sy = cy as f32 + dir.sin() * target_r;
        let arm = lerp_color(th.bg, th.paper, 0.75);
        fb.line(px, py, sx as i32, sy as i32, arm);
        fb.line(px + 1, py, sx as i32 + 1, sy as i32, scale(arm, 0.6));
        fb.rect(sx as i32 - 1, sy as i32 - 1, 3, 3, th.paper);
        // Speed lamp.
        fb.rect(x + w - 8, y + h - 8, 3, 3, if info.playing { th.green } else { scale(th.green, 0.25) });
        fb.text(x + w - 30, y + h - 10, "33", th.dim, 1);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_vu(&self, fb: &mut Framebuffer, th: &Theme, x: i32, y: i32, w: i32, h: i32, ch: usize, label: &str) {
        let face = lerp_color(th.bg, th.paper, 0.08);
        fb.rect(x, y, w, h, face);
        fb.rect(x, y, w, 1, scale(th.fg, 0.2));
        fb.rect(x, y + h - 1, w, 1, scale(th.fg, 0.1));
        // Scale arc: ticks from -20 to +3 over 110 degrees, red past 0 dB.
        let px = x + w / 2;
        let py = y + h - 3;
        let r = (h - 8) as f32;
        let a0 = 215.0f32.to_radians();
        let a1 = 325.0f32.to_radians();
        let ticks = 12;
        for i in 0..=ticks {
            let f = i as f32 / ticks as f32;
            let a = a0 + (a1 - a0) * f;
            let c = if f > 0.78 { th.red } else { th.dim };
            let inner = if i % 3 == 0 { r - 4.0 } else { r - 2.0 };
            fb.line(
                px + (a.cos() * inner) as i32,
                py + (a.sin() * inner) as i32,
                px + (a.cos() * r) as i32,
                py + (a.sin() * r) as i32,
                c,
            );
        }
        // Peak marker and needle.
        let peak = self.peak[ch].clamp(0.0, 1.0);
        let ap = a0 + (a1 - a0) * peak;
        fb.rect(px + (ap.cos() * (r - 1.0)) as i32, py + (ap.sin() * (r - 1.0)) as i32, 2, 2, th.yellow);
        let lvl = self.vu[ch].clamp(0.0, 1.0);
        let an = a0 + (a1 - a0) * lvl;
        fb.line(px, py, px + (an.cos() * (r - 1.0)) as i32, py + (an.sin() * (r - 1.0)) as i32, th.paper);
        fb.rect(px - 1, py - 1, 3, 2, th.paper);
        fb.text(x + 3, y + 2, label, th.dim, 1);
        // Peak LED.
        fb.rect(x + w - 6, y + 3, 3, 3, if peak > 0.86 { th.red } else { scale(th.red, 0.25) });
        let vu = "VU";
        fb.text(x + w - 3 - Framebuffer::text_width(vu, 1) - 6, y + h - 10, vu, scale(th.dim, 0.8), 1);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_dial(&mut self, fb: &mut Framebuffer, th: &Theme, x: i32, y: i32, w: i32, h: i32, now: f64, playing: bool, label: &str) {
        let glass = lerp_color(th.bg, th.orange, 0.10);
        fb.rect(x, y, w, h, glass);
        fb.rect(x, y, w, 1, scale(th.fg, 0.25));
        // Frequency scale.
        let sy = y + 30;
        let sx0 = x + 10;
        let sx1 = x + w - 10;
        fb.rect(sx0, sy, sx1 - sx0, 1, th.dim);
        for i in 0..=20 {
            let f = i as f32 / 20.0;
            let tx = sx0 + ((sx1 - sx0) as f32 * f) as i32;
            let tall = i % 4 == 0;
            fb.rect(tx, sy - if tall { 5 } else { 3 }, 1, if tall { 5 } else { 3 }, th.dim);
            if tall {
                let mhz = 88 + i;
                let s = mhz.to_string();
                fb.text(tx - Framebuffer::text_width(&s, 1) / 2, sy - 15, &s, th.paper, 1);
            }
        }
        fb.text(x + 4, y + 3, "FM", th.orange, 1);
        // Static while tuning: sparkle in the glass and a jittery needle.
        let tuning = now < self.tuning_until;
        let mut jitter = 0.0;
        if tuning {
            for _ in 0..40 {
                let rx = x + 2 + (self.rand() * (w - 4) as f32) as i32;
                let ry = y + 2 + (self.rand() * (h - 4) as f32) as i32;
                fb.put(rx, ry, scale(th.paper, 0.5 + self.rand() * 0.5));
            }
            jitter = (self.rand() - 0.5) * 3.0;
        }
        let nx = sx0 + ((sx1 - sx0) as f32 * self.dial + jitter) as i32;
        fb.rect(nx, y + 8, 1, h - 16, th.red);
        fb.rect(nx - 1, y + 8, 3, 2, th.red);
        // Station name and the lamps.
        let room = ((w - 8) / 8) as usize;
        let name: String = if tuning {
            "tuning".into()
        } else {
            label.chars().take(room).collect()
        };
        fb.text(x + 4, y + h - 22, &name, if tuning { th.dim } else { th.bright_green }, 1);
        let stereo = playing && !tuning && self.energy > 0.05;
        let lamp = |on: bool, c: Color| if on { c } else { scale(c, 0.2) };
        fb.rect(x + 4, y + h - 8, 4, 4, lamp(stereo, th.green));
        fb.text(x + 11, y + h - 10, "STEREO", if stereo { th.paper } else { th.dim }, 1);
        let beat = self.kick.hit > 0.3;
        fb.rect(x + w - 24, y + h - 8, 4, 4, lamp(beat, th.red));
        fb.text(x + w - 17, y + h - 10, "ST", th.dim, 1);
    }

    // ------------------------------------------------------ visualizers

    /// The whole frame: the current mode, then the title and the mode name
    /// in the corners, both fading out a few seconds after a change.
    pub fn draw_visual(&mut self, fb: &mut Framebuffer, th: &Theme, now: f64, title: &str) {
        fb.clear(th.bg);
        match self.mode {
            0 => self.mode7_eq(fb, th, now),
            1 => self.ribbons(fb, th, now),
            2 => self.scope(fb, th, now),
            3 => self.starfield(fb, th),
            4 => self.plasma(fb, th, now),
            5 => self.fire(fb, th),
            _ => self.tower(fb, th),
        }
        let since = (now - self.mode_since) as f32;
        let fade = (1.0 - ((since - 2.5) / 1.0)).clamp(0.0, 1.0);
        if fade > 0.0 {
            let w = fb.w as i32;
            let h = fb.h as i32;
            let left = (w as f32 * 0.05) as i32;
            let max_cols = ((w - 2 * left) / 8) as usize;
            let t: String = title.chars().take(max_cols.saturating_sub(18)).collect();
            fb.text(left, h - 14, &t, scale(th.paper, fade), 1);
            let m = MODE_NAMES[self.mode];
            fb.text(w - left - Framebuffer::text_width(m, 1), h - 14, m, scale(th.dim, fade), 1);
        }
    }

    fn mode7_eq(&mut self, fb: &mut Framebuffer, th: &Theme, now: f64) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let horizon = 96;
        let light = lerp_color(th.bg, th.dim, 0.55);
        let dark = lerp_color(th.bg, th.dim, 0.25);
        let scroll = (now * (14.0 + self.energy as f64 * 40.0)) as f32;
        let angle = (now * 0.35) as f32;
        let (sa, ca) = angle.sin_cos();
        for y in horizon..h {
            let dy = (y - horizon) as f32 + 1.0;
            let depth = 9000.0 / dy;
            let fog = ((dy - 2.0) / 40.0).clamp(0.0, 1.0);
            for x in 0..w {
                let wx = (x - w / 2) as f32 * depth / 180.0;
                let wz = depth + scroll;
                // Rotate the checker with the camera so the floor turns with the bars.
                let rx = wx * ca - wz * sa;
                let rz = wx * sa + wz * ca;
                let parity = ((rx / 24.0).floor() as i32 + (rz / 24.0).floor() as i32) & 1;
                let base = if parity == 0 { light } else { dark };
                fb.put(x, y, lerp_color(th.bg, base, fog));
            }
        }
        // Ten bars on a ring, far ones first.
        let mut bars: Vec<(f32, f32, f32, usize)> = (0..10)
            .map(|i| {
                let a = angle + i as f32 * std::f32::consts::TAU / 10.0;
                let radius = 70.0;
                let x = a.sin() * radius;
                let z = 150.0 + a.cos() * radius;
                (z, x, self.smooth[i], i)
            })
            .collect();
        bars.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let stops = [th.magenta, th.accent, th.cyan, th.green, th.yellow];
        for (z, x, v, i) in bars {
            let s = 150.0 / z;
            let sx = w as f32 / 2.0 + x * s;
            let base_y = horizon as f32 + 40.0 * s;
            let bw = (14.0 * s).max(2.0);
            let bh = ((8.0 + v * 110.0 + self.kick.hit * 10.0) * s).max(1.0);
            let c0 = stops[i % stops.len()];
            let c = lerp_color(c0, th.bg, (1.0 - s).clamp(0.0, 0.6));
            let x0 = (sx - bw / 2.0) as i32;
            let top = (base_y - bh) as i32;
            fb.rect(x0, top, bw as i32, bh as i32, c);
            fb.rect(x0, top, bw as i32, 1, lerp_color(c, th.paper, 0.6));
            fb.rect(x0 + bw as i32 - 1, top, 1, bh as i32, scale(c, 0.6));
            // Reflection on the floor.
            let rh = (bh * 0.35) as i32;
            for k in 0..rh {
                let a = 0.35 * (1.0 - k as f32 / rh.max(1) as f32);
                let yy = base_y as i32 + 1 + k;
                for xx in x0..x0 + bw as i32 {
                    if xx >= 0 && xx < w && yy < h {
                        let bg = fb_get(fb, xx, yy);
                        fb.put(xx, yy, lerp_color(bg, c, a));
                    }
                }
            }
        }
    }

    /// Silk ribbons: five translucent sine waves drift across the frame,
    /// each tied to a band that sets its amplitude and glow. Additive light,
    /// soft edges, the theme's own hues, nothing else.
    fn ribbons(&mut self, fb: &mut Framebuffer, th: &Theme, now: f64) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let hues = [th.accent, th.magenta, th.cyan, th.green, th.yellow];
        let t = now as f32;
        for (k, hue) in hues.iter().enumerate() {
            let band = self.smooth[(k * 2 + 1).min(9)];
            let amp = 18.0 + 46.0 * band;
            let thick = 3.0 + 9.0 * band + self.kick.hit * 3.0;
            let speed = 0.35 + k as f32 * 0.11;
            let phase = t * speed + k as f32 * 1.3;
            let freq = 1.1 + k as f32 * 0.35;
            let base_y = h as f32 * (0.32 + 0.09 * k as f32);
            let glow = 0.18 + 0.32 * band;
            for x in 0..w {
                let u = x as f32 / w as f32;
                let yc = base_y + amp * (u * freq * std::f32::consts::TAU + phase).sin()
                    + 8.0 * (u * 3.0 + t * 0.7 + k as f32).cos();
                let half = thick.max(1.0);
                let y0 = (yc - half - 2.0) as i32;
                let y1 = (yc + half + 2.0) as i32;
                for y in y0.max(0)..=y1.min(h - 1) {
                    let d = ((y as f32 - yc).abs() / half).min(1.2);
                    // A bright core fading to a soft edge past the ribbon's width.
                    let a = if d < 1.0 { (1.0 - d * d) * glow } else { (1.2 - d) * 0.5 * glow };
                    if a <= 0.005 {
                        continue;
                    }
                    let bg = fb.px[y as usize * fb.w + x as usize];
                    let c = lerp_color(*hue, th.paper, (1.0 - d).max(0.0) * 0.35);
                    fb.put(x, y, crate::fb::add(bg, scale(c, a)));
                }
            }
        }
        // A thin horizon that the ribbons seem to float over.
        let hy = h - 40;
        fb.rect(0, hy, w, 1, lerp_color(th.bg, th.dim, 0.35));
        for x in (0..w).step_by(2) {
            fb.put(x, hy + 6, lerp_color(th.bg, th.dim, 0.18));
        }
    }

    fn scope(&mut self, fb: &mut Framebuffer, th: &Theme, now: f64) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        // Graticule.
        let grid = lerp_color(th.bg, th.green, 0.18);
        for x in (0..w).step_by(32) {
            for y in (0..h).step_by(4) {
                fb.put(x, y, grid);
            }
        }
        for y in (0..h).step_by(30) {
            for x in (0..w).step_by(4) {
                fb.put(x, y, grid);
            }
        }
        // A waveform the bands would produce: one sine per band, phases fixed.
        let mut wave = Vec::with_capacity(w as usize);
        let amp = 70.0 + self.kick.hit * 25.0;
        for x in 0..w {
            let t = x as f32 / w as f32;
            let mut v = 0.0;
            for (i, s) in self.smooth.iter().enumerate() {
                let f = 1.0 + i as f32 * 1.7;
                v += s * (std::f32::consts::TAU * f * t + self.phases[i] + now as f32 * (1.5 + i as f32 * 0.9)).sin();
            }
            let v = (v / 2.2).clamp(-1.0, 1.0);
            wave.push(h / 2 + (v * amp) as i32);
        }
        // Persistence: the last traces linger dimmer.
        for (k, prev) in self.scope_prev.iter().enumerate() {
            let a = 0.15 + 0.15 * k as f32;
            let c = lerp_color(th.bg, th.green, a);
            for x in 1..w {
                fb.line(x - 1, prev[(x - 1) as usize], x, prev[x as usize], c);
            }
        }
        for x in 1..w {
            fb.line(x - 1, wave[(x - 1) as usize], x, wave[x as usize], th.bright_green);
        }
        self.scope_prev.push(wave);
        if self.scope_prev.len() > 3 {
            self.scope_prev.remove(0);
        }
    }

    fn starfield(&mut self, fb: &mut Framebuffer, th: &Theme) {
        let w = fb.w as f32;
        let h = fb.h as f32;
        let speed = 0.006 + self.energy * 0.03 + self.kick.hit * 0.06;
        let warp = self.kick.hit;
        let mut respawn = Vec::new();
        for (i, s) in self.stars.iter_mut().enumerate() {
            s.pz = s.z;
            s.z -= speed * (0.6 + (i % 7) as f32 * 0.12);
            if s.z <= 0.03 {
                respawn.push(i);
                continue;
            }
            let sx = w / 2.0 + s.x / s.z * w / 2.0;
            let sy = h / 2.0 + s.y / s.z * h / 2.0;
            if sx < 0.0 || sx >= w || sy < 0.0 || sy >= h {
                respawn.push(i);
                continue;
            }
            let bright = (1.0 - s.z).clamp(0.05, 1.0).sqrt();
            let c = lerp_color(th.bg, th.paper, bright);
            if warp > 0.2 {
                let px = w / 2.0 + s.x / s.pz * w / 2.0;
                let py = h / 2.0 + s.y / s.pz * h / 2.0;
                fb.line(px as i32, py as i32, sx as i32, sy as i32, lerp_color(th.bg, th.cyan, bright * warp));
            }
            fb.put(sx as i32, sy as i32, c);
            if bright > 0.75 {
                fb.put(sx as i32 + 1, sy as i32, scale(c, 0.7));
                fb.put(sx as i32, sy as i32 + 1, scale(c, 0.7));
            }
        }
        for i in respawn {
            let s = self.new_star();
            self.stars[i] = s;
        }
    }

    fn plasma(&mut self, fb: &mut Framebuffer, th: &Theme, now: f64) {
        let t = now as f32 * (0.6 + self.energy * 1.5);
        let shift = self.kick.hit * 0.3;
        let stops = [th.blue, th.magenta, th.orange, th.yellow];
        for gy in 0..60 {
            for gx in 0..80 {
                let x = gx as f32;
                let y = gy as f32;
                let v = (x * 0.11 + t).sin()
                    + (y * 0.14 - t * 0.7).sin()
                    + ((x + y) * 0.07 + t * 0.5).sin()
                    + (((x - 40.0) * (x - 40.0) + (y - 30.0) * (y - 30.0)).sqrt() * 0.12 - t * 1.3).sin();
                let f = ((v + 4.0) / 8.0 + shift).fract();
                let seg = f * (stops.len() - 1) as f32;
                let i = (seg.floor() as usize).min(stops.len() - 2);
                let c = lerp_color(stops[i], stops[i + 1], seg - i as f32);
                fb.rect(gx * 4, gy * 4, 4, 4, lerp_color(th.bg, c, 0.85));
            }
        }
    }

    fn fire(&mut self, fb: &mut Framebuffer, th: &Theme) {
        let bass = (self.smooth[0] + self.smooth[1]) / 2.0;
        // Seed the bottom row from the bass, more on the kick.
        let heat = (16.0 + bass * 20.0 + self.kick.hit * 10.0).min(36.0) as u8;
        for x in 0..FIRE_W {
            let r = self.rand();
            let v = if r < 0.3 + bass { heat } else { heat.saturating_sub(6) };
            self.fire[(FIRE_H - 1) * FIRE_W + x] = v;
        }
        // Doom's fire: each cell cools a little and drifts sideways on its way up.
        let cool = 1.0 + (1.0 - bass) * 1.2;
        for y in 1..FIRE_H {
            for x in 0..FIRE_W {
                let src = y * FIRE_W + x;
                let r = (self.rand() * 3.0) as usize;
                let decay = (self.rand() * (cool + 2.4)) as u8;
                let dst_x = (x + FIRE_W + 1 - r) % FIRE_W;
                let dst = (y - 1) * FIRE_W + dst_x;
                let v = self.fire[src];
                self.fire[dst] = v.saturating_sub(decay);
            }
        }
        let stops = [th.bg, scale(th.red, 0.5), th.red, th.orange, th.yellow, th.paper];
        for y in 0..FIRE_H {
            for x in 0..FIRE_W {
                let v = self.fire[y * FIRE_W + x] as f32 / 36.0;
                let seg = v * (stops.len() - 1) as f32;
                let i = (seg.floor() as usize).min(stops.len() - 2);
                let c = lerp_color(stops[i], stops[i + 1], seg - i as f32);
                fb.rect(x as i32 * 4, y as i32 * 4 + 8, 4, 4, c);
            }
        }
    }

    fn tower(&mut self, fb: &mut Framebuffer, th: &Theme) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let floor = h - 70;
        let gap = 4;
        let bw = (w - 40 - 9 * gap) / 10;
        let x0 = 20;
        for (i, v) in self.smooth.iter().enumerate() {
            let x = x0 + i as i32 * (bw + gap);
            let bh = ((v * (floor - 30) as f32) as i32).max(1);
            for k in 0..bh {
                let f = k as f32 / (floor - 30) as f32;
                let c = if f < 0.5 {
                    lerp_color(th.green, th.yellow, f * 2.0)
                } else {
                    lerp_color(th.yellow, th.red, (f - 0.5) * 2.0)
                };
                fb.rect(x, floor - 1 - k, bw, 1, c);
            }
            // Peak cap.
            let peak = (self.smooth[i].max(*v) * (floor - 30) as f32) as i32;
            fb.rect(x, floor - 3 - peak, bw, 2, th.paper);
            // Reflection.
            for k in 0..(bh / 2) {
                let a = 0.3 * (1.0 - k as f32 / (bh / 2).max(1) as f32);
                let f = k as f32 / (floor - 30) as f32;
                let c = lerp_color(th.green, th.yellow, (f * 2.0).min(1.0));
                fb.rect(x, floor + 2 + k, bw, 1, lerp_color(th.bg, c, a));
            }
        }
        fb.rect(0, floor, w, 1, lerp_color(th.bg, th.dim, 0.6));
        for y in (0..h).step_by(3) {
            for x in 0..w {
                let c = fb_get(fb, x, y);
                fb.put(x, y, scale(c, 0.85));
            }
        }
        let flash = self.kick.hit;
        if flash > 0.5 {
            fb.rect(0, floor - 2, w, 1, lerp_color(th.bg, th.paper, flash));
        }
    }
}

/// Filled circle.
fn draw_disc(fb: &mut Framebuffer, cx: i32, cy: i32, r: f32, c: Color) {
    let ri = r.ceil() as i32;
    for dy in -ri..=ri {
        let half = (r * r - (dy * dy) as f32).max(0.0).sqrt() as i32;
        fb.rect(cx - half, cy + dy, 2 * half + 1, 1, c);
    }
}

fn fb_get(fb: &Framebuffer, x: i32, y: i32) -> Color {
    if x < 0 || y < 0 || x >= fb.w as i32 || y >= fb.h as i32 {
        return 0;
    }
    fb.px[y as usize * fb.w + x as usize]
}

/// Synced lyrics over the picture: the current line large and lit, the next
/// one small and dim, each glyph shadowed so it reads on any background.
/// Lines wrap to the width; `y` is the top of the block, `height` its room.
pub fn draw_lyrics(fb: &mut Framebuffer, th: &Theme, lines: &[(f64, String)], position: f64, y: i32, height: i32) {
    let w = fb.w as i32;
    let left = (w as f32 * 0.05) as i32;
    let idx = lines.iter().rposition(|(s, _)| *s <= position);
    let wrap = |s: &str, cols: usize| -> Vec<String> {
        let mut out = Vec::new();
        let mut line = String::new();
        for word in s.split_whitespace() {
            if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > cols {
                out.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        if !line.is_empty() {
            out.push(line);
        }
        out
    };
    let shadow = scale(th.bg, 0.6);
    let mut yy = y;
    match idx {
        Some(i) => {
            let cur = crate::music::fold(&lines[i].1);
            // Big when it fits in two lines, small otherwise.
            let big_cols = ((w - 2 * left) / 16) as usize;
            let big = wrap(&cur, big_cols);
            let (rows, sc, step) = if big.len() <= 2 {
                (big, 2, 18)
            } else {
                (wrap(&cur, ((w - 2 * left) / 8) as usize), 1, 10)
            };
            for l in rows.iter().take(3) {
                if yy + 8 * sc > y + height {
                    break;
                }
                text_shadow(fb, w / 2, yy, l, th.paper, shadow, sc);
                yy += step;
            }
            if let Some((_, next)) = lines.get(i + 1) {
                yy += 4;
                for l in wrap(&crate::music::fold(next), ((w - 2 * left) / 8) as usize).iter().take(2) {
                    if yy + 8 > y + height {
                        break;
                    }
                    text_shadow(fb, w / 2, yy, l, th.dim, shadow, 1);
                    yy += 10;
                }
            }
        }
        None => {
            if let Some((start, _)) = lines.first() {
                let s = format!("lyrics in {}", crate::player::clock((start - position).max(0.0)));
                text_shadow(fb, w / 2, yy, &s, th.dim, shadow, 1);
            }
        }
    }
}

/// Centred text with a one pixel shadow down and right.
fn text_shadow(fb: &mut Framebuffer, cx: i32, y: i32, s: &str, c: Color, shadow: Color, sc: i32) {
    let tw = Framebuffer::text_width(s, sc);
    let x = cx - tw / 2;
    fb.text(x + sc, y + sc, s, shadow, sc);
    fb.text(x, y, s, c, sc);
}
