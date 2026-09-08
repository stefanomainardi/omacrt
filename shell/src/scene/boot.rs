//! The boot sequence and the home menu.
//!
//! Power surge and roll, the POST lines with live data, the icon revealed
//! band by band, the chime, the wordmark etched by a laser, the CRT tag over
//! a Mode 7 floor, and then the menu everything else is reached from.

use super::*;

impl Scene {
    pub fn start_boot(&mut self, now: f64) {
        if self.boot_started {
            return;
        }
        self.boot_started = true;
        self.t0 = now;
        self.now = now;
        self.last_input = now;
        let seed = (now * 1000.0) as u32 ^ self.rand();
        self.etch = Some(LaserEtch::new(seed, self.stops()));
        self.pending.push(Sound::PowerOn);
    }

    /// Jump to the end of the boot sequence: menu up, sounds that would have
    /// played are marked as done so nothing fires late.
    pub fn skip_boot(&mut self, now: f64) {
        if !self.boot_started || self.menu_live {
            return;
        }
        self.t0 = now - 9.75;
        self.chime_played = true;
        self.tag_sound_played = true;
        self.etch_sound_played = true;
        self.post_clicks = usize::MAX / 2;
        self.pending.push(Sound::Lock);
    }

    pub fn booting(&self) -> bool {
        self.boot_started && !self.menu_live
    }

    pub(super) fn draw_gate(&mut self, fb: &mut Framebuffer) {
        let (w, h) = (fb.w as i32, fb.h as i32);
        let s = 3;
        fb.bitmap(
            (w - 24 * s) / 2,
            (h as f32 * 0.26) as i32,
            &ICON_24,
            self.theme.green,
            s,
            24,
        );
        fb.text_centered(
            w / 2,
            (h as f32 * 0.58) as i32,
            "OMARCHY",
            self.theme.green,
            2,
        );
        let msg = "PRESS START";
        let y = (h as f32 * 0.70) as i32;
        fb.text_centered(w / 2, y, msg, self.theme.dim, 1);
        if (self.now * 2.0).floor() as i64 % 2 == 0 {
            let x = w / 2 + Framebuffer::text_width(msg, 1) / 2 + 4;
            fb.rect(x, y, 6, 8, self.theme.green);
        }
        fb.text_centered(
            w / 2,
            h - 16,
            "(C) 2026 OMACOM  15KHZ EDITION",
            scale(self.theme.dim, 0.6),
            1,
        );
    }

    fn post_lines(&self) -> Vec<(String, Color, bool)> {
        let th = &self.theme;
        vec![
            ("OmarchyBIOS 4.01 / Omacom".into(), th.green, false),
            ("(C) 2026 Omacom Foundation".into(), th.dim, false),
            (String::new(), th.green, false),
            (format!("CPU  {}", self.info.host), th.paper, false),
            ("MEM  counting...".into(), th.paper, true),
            (
                format!("VGA  {} 15kHz {}", self.info.mode, th.name),
                th.paper,
                false,
            ),
            (format!("KRN  {}", self.info.kernel), th.cyan, false),
            (
                format!("DSK  omarchy-crt {}          OK", env!("CARGO_PKG_VERSION")),
                th.paper,
                false,
            ),
        ]
    }

    pub(super) fn draw_post(&mut self, fb: &mut Framebuffer, t: f32) {
        let lines = self.post_lines();
        let start = 0.45;
        let shown = (((t - start) / 0.18).floor() as i32 + 1).clamp(0, lines.len() as i32);
        if shown > 0 && shown != self.last_post_line {
            self.last_post_line = shown;
            let (text, _, _) = &lines[(shown - 1) as usize];
            if !text.is_empty() && self.crunches < 5 {
                self.crunches += 1;
                self.pending.push(Sound::Crunch);
            }
        }
        if shown >= 5 {
            self.mem = ((t - start - 0.18 * 4.0) * 90_000.0).clamp(0.0, 65_536.0) as u32;
        }
        let fade = 1.0 - ease(clamp((t - 1.75) / 0.28, 0.0, 1.0));
        if fade <= 0.0 {
            return;
        }
        let x = (fb.w as f32 * 0.05) as i32;
        let mut y = (fb.h as f32 * 0.08) as i32;
        let row_h = 11;
        let max_cols = (fb.w as i32 - 2 * x) / 8;
        let mut last_end = (x, y);
        let mut clicks_due = 0usize;
        for (i, (text, color, is_mem)) in lines.iter().take(shown as usize).enumerate() {
            let mut s = text.clone();
            if *is_mem {
                s = if self.mem >= 65_536 {
                    "MEM  65536K OK".into()
                } else {
                    format!("MEM  {:05}K", self.mem)
                };
            }
            // Typewriter: the newest line is revealed over its 0.18 s slot.
            let line_start = start + i as f32 * 0.18;
            let progress = ((t - line_start) / 0.16).clamp(0.0, 1.0);
            let visible = (s.chars().count() as f32 * progress).ceil() as usize;
            clicks_due += visible / 4;
            let s: String = s.chars().take(visible.min(max_cols as usize)).collect();
            fb.text(x, y, &s, scale(*color, fade), 1);
            last_end = (x + Framebuffer::text_width(&s, 1) + 4, y);
            y += row_h;
        }
        while self.post_clicks < clicks_due {
            self.post_clicks += 1;
            if self.post_clicks.is_multiple_of(2) {
                self.pending.push(Sound::Click);
            }
        }
        // Block cursor after the last POST line, blinking fast like a BIOS.
        if shown > 0 && (self.now * 6.0).floor() as i64 % 2 == 0 {
            fb.rect(last_end.0, last_end.1, 6, 8, scale(self.theme.paper, fade));
        }
    }

    pub(super) fn logo_final(&self, fb: &Framebuffer) -> (i32, i32, i32) {
        let size = 24;
        ((fb.w as i32 - size) / 2, (fb.h as f32 * 0.035) as i32, size)
    }

    pub(super) fn mark_final_y(&self, fb: &Framebuffer) -> i32 {
        let (_, ly, lsize) = self.logo_final(fb);
        ly + lsize + 8
    }

    pub(super) fn draw_logo(&mut self, fb: &mut Framebuffer, t: f32) {
        let appear = clamp((t - 2.2) / 1.45, 0.0, 1.0);
        if appear <= 0.0 {
            return;
        }
        let (w, h) = (fb.w as f32, fb.h as f32);
        let up = ease(clamp((t - 4.0) / 0.5, 0.0, 1.0));
        let settle = ease(clamp((t - 6.9) / 0.55, 0.0, 1.0));
        let (_, fy, fsize) = self.logo_final(fb);
        let big = (h * 0.40).min(96.0);
        let size = lerp(lerp(big, 32.0, up), fsize as f32, settle);
        let cy = lerp(lerp(h * 0.46, h * 0.22, up), fy as f32 + size * 0.5, settle);
        let x = (w * 0.5 - size * 0.5).round() as i32;
        let y = (cy - size * 0.5).round() as i32;
        let px = (size / 24.0).max(1.0);
        let band = 4;
        let bands = (size / band as f32).ceil() as i32;
        let revealed = (bands as f32 * ease(appear)).ceil() as i32;
        let max_y = y + revealed * band;
        let green = self.theme.green;
        for (ry, row) in ICON_24.iter().enumerate() {
            for rx in 0..24 {
                if row.as_bytes()[rx] != b'#' {
                    continue;
                }
                let x0 = x + (rx as f32 * px).round() as i32;
                let x1 = x + ((rx + 1) as f32 * px).round() as i32;
                let y0 = y + (ry as f32 * px).round() as i32;
                let y1 = (y + ((ry + 1) as f32 * px).round() as i32).min(max_y);
                if y1 > y0 {
                    fb.rect(x0, y0, x1 - x0, y1 - y0, green);
                }
            }
        }
        if appear < 1.0 {
            let beam_y = max_y;
            let sw = size.round() as i32;
            fb.rect_add(x, beam_y - 5, sw, 8, scale(self.theme.cyan, 0.22));
            fb.rect(x, beam_y - 2, sw, 2, scale(self.theme.green, 0.9));
        }
    }

    pub(super) fn draw_etch(&mut self, fb: &mut Framebuffer, t: f32) {
        if t < ETCH_START {
            return;
        }
        let fade = ease(clamp((t - 4.2) / 0.35, 0.0, 1.0));
        let (w, h) = (fb.w as f32, fb.h as f32);
        let mw = self.mark_cols * MARK_SCALE;
        let settle = ease(clamp((t - 6.9) / 0.55, 0.0, 1.0));
        let final_y = self.mark_final_y(fb);
        let x = ((w - mw as f32) * 0.5).round() as i32;
        let y = lerp(h * 0.36, final_y as f32, settle).round() as i32;
        if let Some(etch) = self.etch.as_mut() {
            if !self.etch_sound_played {
                self.etch_sound_played = true;
                self.pending_samples.push(etch.synth(crate::audio::RATE));
            }
            etch.advance_to(t - ETCH_START);
            etch.draw(fb, x, y, MARK_SCALE, fade);
        }
    }

    fn tag_geometry(&self, fb: &Framebuffer) -> (i32, i32, i32, i32) {
        // Right-aligned under the wordmark's last letters.
        let mw = self.mark_cols * MARK_SCALE;
        let mark_x = (fb.w as i32 - mw) / 2;
        let mark_bottom = self.mark_final_y(fb) + self.mark_rows * 2 * MARK_SCALE;
        let tw = crate::crt_tag::COLS * TAG_SCALE;
        let th = crate::crt_tag::ROWS * TAG_SCALE;
        (mark_x + mw - tw - 2, mark_bottom + 2, tw, th)
    }

    /// "CRT" appears like a tape hunting for sync (TTE `vhstape`): torn lines,
    /// a tracking wave, snow, then a clean redraw with a lock click.
    /// "CRT" traced by an electron beam, letter notes, a stamp and a glint.
    pub(super) fn draw_crt_tag(&mut self, fb: &mut Framebuffer, t: f32) {
        if t < TAG_START {
            return;
        }
        let local = t - TAG_START;
        let (x, y, _, _) = self.tag_geometry(fb);
        if !self.tag_sound_played {
            self.tag_sound_played = true;
            self.pending.push(Sound::TagReveal);
        }
        let look = crate::crt_tag::Look {
            stops: self.stops(),
            bg: self.theme.bg,
            floor_light: self.theme.dim,
            floor_dark: self.theme.fg_dark_floor(),
            orange: self.theme.orange,
            yellow: self.theme.yellow,
        };
        crate::crt_tag::draw(fb, x, y, TAG_SCALE, local, &look);
    }

    /// One Omarchy style menu row: icon, label, chevron; the selected row sits
    /// on a band in the theme's selection color with accent colored text.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_menu_row(
        &self,
        fb: &mut Framebuffer,
        x: i32,
        y: i32,
        width: i32,
        icon: &icons::Icon,
        label: &str,
        submenu: bool,
        on: bool,
        fade: f32,
    ) {
        let (icon_c, text_c, chev_c) = if on {
            (self.theme.accent, self.theme.accent, self.theme.accent)
        } else {
            (self.theme.dim, self.theme.paper, self.theme.dim)
        };
        // The icon of the row under the cursor breathes, a slow half second
        // in and out. Brightness rather than a pixel of movement: a menu that
        // jitters is a menu with a fault.
        let pulse = if on {
            0.86 + 0.14 * (self.now as f32 * 2.2).sin()
        } else {
            1.0
        };
        fb.bitmap(x + 4, y + 2, icon, scale(icon_c, fade * pulse), 1, 8);
        fb.text(x + 18, y + 2, label, scale(text_c, fade), 1);
        if submenu {
            let cx = x + width - 8;
            for i in 0..3 {
                fb.put(cx + i, y + 3 + i, scale(chev_c, fade));
                fb.put(cx + i, y + 9 - i, scale(chev_c, fade));
            }
        }
    }

    /// Home menu under the logo: Play..., six entries, footer.
    /// What the row under the cursor actually holds, for the dim line drawn
    /// on the right of the row itself.
    ///
    /// Only the selected row carries one. Eight rows each holding a number
    /// would be a table, and this is a menu; the cursor asks the question and
    /// the row answers it. The Music row is the exception and says what is
    /// playing whether it is selected or not, because that is live state
    /// rather than context.
    fn home_detail(&self, row: usize) -> Option<String> {
        let label = HOME.get(row).map(|(_, l, _)| *l)?;
        match label {
            "Games" => {
                let index = self.library.index.as_ref()?;
                let games = index.items.iter().filter(|i| i.system != "videos").count();
                // Systems the index actually found something for, not every
                // system the catalogue knows about.
                let mut names: Vec<&str> = index
                    .items
                    .iter()
                    .filter(|i| i.system != "videos")
                    .map(|i| i.system.as_str())
                    .collect();
                names.sort_unstable();
                names.dedup();
                let systems = names.len();
                (games > 0).then(|| format!("{games} in {systems} systems"))
            }
            "Videos" => {
                let index = self.library.index.as_ref()?;
                let films = index.items.iter().filter(|i| i.system == "videos").count();
                (films > 0).then(|| format!("{films} films"))
            }
            "Favorites" => {
                let n = self.favorites.len();
                (n > 0).then(|| format!("{n} starred"))
            }
            "Recent" => {
                // The last thing played, named the way the lists name it.
                let (_, path) = self.recent.first()?;
                Some(crate::library::clean_title(path))
            }
            // What the hub holds, not what the idle rotation is: on this row
            // a count of pages reads as a count of *this* page, and which
            // pages an idle television shows is the screensaver's business
            // and is written on its own settings page.
            "Ambient" => Some(AMBIENT_SUMMARY.into()),
            "Settings" => {
                let theme = self.settings.theme.clone();
                (theme != "system").then_some(theme)
            }
            _ => None,
        }
    }

    pub(super) fn draw_home(&mut self, fb: &mut Framebuffer, t: f32) {
        let fade = ease(clamp((t - 9.65) / 0.45, 0.0, 1.0));
        if fade <= 0.0 {
            return;
        }
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32;
        let width = w - 2 * left;
        let (_, tag_y, _, tag_h) = self.tag_geometry(fb);
        let y0 = tag_y + tag_h + 4;
        fb.text(left + 4, y0, "Play...", scale(self.theme.dim, fade), 1);
        let rows_y = y0 + 11;
        let row_h = 12;
        let band_y = self.band(rows_y + self.sel as i32 * row_h);
        if self.menu_live {
            fb.rect(
                left,
                band_y,
                width,
                row_h - 1,
                scale(self.theme.selection, fade),
            );
        }
        for (i, (icon, label, submenu)) in HOME.iter().enumerate() {
            let y = rows_y + i as i32 * row_h;
            self.draw_menu_row(
                fb,
                left,
                y,
                width,
                icon,
                label,
                *submenu,
                self.menu_live && i == self.sel,
                fade,
            );
        }
        // What plays, on the Music row itself: the home list leaves no room
        // for a line of its own.
        if fade > 0.9 && self.music.status.active() {
            let row = HOME
                .iter()
                .position(|(_, label, _)| *label == "Music")
                .unwrap_or(0);
            let y = rows_y + row as i32 * row_h;
            let label = self
                .music
                .status
                .track
                .as_ref()
                .map(|t| t.label())
                .unwrap_or_default();
            let state = if self.music.status.playing() {
                ""
            } else {
                "  paused"
            };
            let text = format!("{label}{state}");
            let vis_w = 30;
            // Room between the row's own label and the chevron.
            let room = ((width - 18 - vis_w - 30 - Framebuffer::text_width("Music", 1)) / 8).max(0)
                as usize;
            let text: String = text
                .chars()
                .take(room)
                .collect::<String>()
                .trim_end()
                .to_string();
            let tw = Framebuffer::text_width(&text, 1);
            let tx = left + width - 16 - tw;
            fb.text(tx, y + 2, &text, scale(self.theme.dim, 1.0), 1);
            self.draw_vis(fb, tx - vis_w - 6, y + 9, vis_w, 7);
        }
        // The row under the cursor says what it holds, dim, on its own right,
        // between the label and the chevron. Music has already written there.
        if self.menu_live
            && fade > 0.9
            && HOME.get(self.sel).map(|(_, l, _)| *l) != Some("Music")
            && let Some(detail) = self.home_detail(self.sel)
        {
            let y = rows_y + self.sel as i32 * row_h;
            let label = HOME[self.sel].1;
            let room = ((width - 30 - 18 - Framebuffer::text_width(label, 1)) / 8).max(0) as usize;
            let text: String = detail.chars().take(room).collect();
            let tx = left + width - 16 - Framebuffer::text_width(&text, 1);
            fb.text(tx, y + 2, &text, scale(self.theme.dim, 0.95), 1);
        }
        // The two corners the wordmark leaves empty, which is the only room
        // this screen has: what the set is doing on the left, and the time on
        // the right, both spent right down.
        if self.menu_live && fade > 0.9 {
            fb.text(
                left,
                8,
                &self.home_status(fb.h),
                scale(self.theme.dim, 0.55),
                1,
            );
            let clock = chrono::Local::now().format("%H:%M").to_string();
            fb.text(
                w - left - Framebuffer::text_width(&clock, 1),
                8,
                &clock,
                scale(self.theme.dim, 0.55),
                1,
            );
        }
        let max_cols = (width / 8) as usize;
        let cut = |s: &str| -> String { s.chars().take(max_cols).collect() };
        if let Some((msg, _)) = &self.message {
            fb.text(left, h - 28, &cut(msg), scale(self.theme.cyan, fade), 1);
        }
    }

    /// What the set is doing with itself, for the corner the wordmark leaves
    /// empty: the standard where there is one, the picture, and the line rate
    /// that makes this a television rather than a monitor.
    fn home_status(&self, height: usize) -> String {
        match self.profile.monitor.as_str() {
            "pal" => format!("PAL  {height}p  15.6 kHz"),
            "ntsc" => format!("NTSC  {height}p  15.7 kHz"),
            // Every other preset is an arcade monitor: the same line rate,
            // and no broadcast standard to name.
            _ => format!("{height}p  15.7 kHz"),
        }
    }

    /// A submenu drawn like the home rows under the compact header.
    pub(super) fn draw_menu_screen(
        &mut self,
        fb: &mut Framebuffer,
        title: &str,
        items: &[(icons::Icon, &str, bool)],
        sel: usize,
    ) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32 + self.slide();
        let width = w - 2 * (w as f32 * 0.05) as i32;
        let y0 = self.draw_header(fb, title);
        let row_h = 14;
        let band_y = self.band(y0 + sel as i32 * row_h);
        fb.rect(left, band_y, width, row_h - 1, self.theme.selection);
        for (i, (icon, label, sub)) in items.iter().enumerate() {
            let y = y0 + i as i32 * row_h;
            self.draw_menu_row(fb, left, y, width, icon, label, *sub, i == sel, 1.0);
        }
        let max_cols = (width / 8) as usize;
        if let Some((msg, _)) = &self.message {
            let m: String = msg.chars().take(max_cols).collect();
            fb.text(left, h - 28, &m, self.theme.cyan, 1);
        }
        let hint = self.hint(&[("A", "select"), ("B", "back")]);
        fb.text(left, h - 14, &hint, scale(self.theme.dim, 0.7), 1);
    }

    /// Launch animation: a cartridge slides into its slot (or a disc spins
    /// up), a click, then the picture cuts to black for the emulator.
    pub(super) fn draw_launching(&mut self, fb: &mut Framebuffer) {
        let Some(l) = self.launching.as_ref() else {
            return;
        };
        let (w, h) = (fb.w as i32, fb.h as i32);
        let u = (self.now - l.started) as f32;
        let color = l.color;
        let title = l.title.clone();
        let system = l.system.clone();
        let disc = l.disc;
        let left = (w as f32 * 0.05) as i32;
        // Fade to black in the last 0.2 s.
        let fade = 1.0 - ((u - (LAUNCH_SECS - 0.2)) / 0.2).clamp(0.0, 1.0);
        fb.text(
            left,
            h - 28,
            &title.chars().take(36).collect::<String>(),
            scale(self.theme.paper, fade),
            1,
        );
        fb.text(
            left,
            h - 14,
            &format!("{system}  loading"),
            scale(self.theme.dim, fade),
            1,
        );
        let cx = w / 2;
        let cy = h / 2 - 10;
        if disc {
            // Disc: spinning hub and spokes, speeding up.
            let spin = u * u * 9.0;
            let r = 30;
            for a in 0..360 {
                let rad = (a as f32).to_radians();
                let (sx, sy) = (rad.cos(), rad.sin());
                let stripe = ((rad * 6.0 + spin).sin() > 0.4) as i32;
                let c = if stripe == 1 {
                    color
                } else {
                    scale(color, 0.35)
                };
                for rr in 8..r {
                    let px = cx + (sx * rr as f32) as i32;
                    let py = cy + (sy * rr as f32 * 0.55) as i32;
                    fb.put(px, py, scale(c, fade));
                }
            }
            fb.rect(cx - 4, cy - 2, 8, 4, scale(self.theme.bg, fade));
        } else {
            // Slot: a dark bay with a lip; the cartridge drops in with ease-in.
            let slot_w = 64;
            let slot_y = cy + 10;
            fb.rect(
                cx - slot_w / 2 - 6,
                slot_y,
                slot_w + 12,
                22,
                scale(self.theme.selection, fade),
            );
            fb.rect(
                cx - slot_w / 2 - 2,
                slot_y + 2,
                slot_w + 4,
                4,
                scale(self.theme.bg, fade),
            );
            let p = (u / 0.42).clamp(0.0, 1.0);
            let drop = p * p;
            let cart_h = 28;
            let y = (cy - 60) as f32 + ((slot_y - cy + 60 - 6) as f32) * drop;
            let y = y.round() as i32;
            let visible_h = (slot_y + 2 - y).clamp(0, cart_h);
            let cw = 48;
            fb.rect(cx - cw / 2, y, cw, visible_h, scale(color, fade));
            fb.rect(
                cx - cw / 2 + 4,
                y + 4,
                cw - 8,
                (visible_h - 8).max(0),
                scale(self.theme.bg, 0.6 * fade),
            );
            if visible_h > 12 {
                let label: String = system.to_uppercase().chars().take(5).collect();
                fb.text_centered(cx, y + 6, &label, scale(color, fade), 1);
            }
            // Settle jolt right after the click.
            if (0.42..0.5).contains(&u) {
                fb.rect(
                    cx - slot_w / 2 - 6,
                    slot_y + 1,
                    slot_w + 12,
                    1,
                    scale(0xffffff, 0.5 * fade),
                );
            }
        }
        if u >= LAUNCH_SECS - 0.2 {
            // Power on: a bright horizontal line collapsing, then black.
            let k = ((u - (LAUNCH_SECS - 0.2)) / 0.2).clamp(0.0, 1.0);
            let lw = ((1.0 - k) * w as f32) as i32;
            fb.rect(cx - lw / 2, cy, lw, 1, 0xffffff);
        }
    }
}
