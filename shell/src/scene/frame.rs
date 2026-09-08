//! The photo frame and the ambient page: photographs from the house's own
//! server, and the weather drawn with the time under it.

use super::*;

impl Scene {
    /// What the photo supply should be fetching, out of the settings and the
    /// shape of the screen it is drawn on.
    fn photo_supply(&self, width: usize, height: usize) -> crate::photos::Wanted {
        let f = &self.settings.frame;
        crate::photos::Wanted {
            config_dir: self.library.config_dir.clone(),
            source: omarchy_crt_shell::immich::Source::named(&f.source),
            album: f.album.clone(),
            place: f.weather.clone(),
            calendar: f.calendar.clone(),
            width,
            height,
        }
    }

    /// Open the photo frame, starting the supply if it is not running.
    pub fn open_frame(&mut self) {
        self.frame_since = self.now;
        self.go(Screen::Frame);
    }

    pub(super) fn close_frame(&mut self) {
        // The pictures already prepared stay in hand: coming back to the
        // frame should not mean waiting for the network again.
        self.screen = Screen::AmbientHub { sel: 0 };
    }

    /// Put the next photograph up now, rather than at the end of its turn.
    pub(super) fn next_photo(&mut self) {
        match self.photos.take() {
            Some(next) => {
                self.frame_previous = self.frame_now.take();
                self.frame_now = Some(next);
                self.frame_since = self.now;
                self.pending.push(Sound::Move);
            }
            None => self.pending.push(Sound::Crunch),
        }
    }

    /// Paint one photograph over the whole screen, drifted and faded.
    ///
    /// A picture that fills the screen was prepared larger than it on
    /// purpose, and drifts across that margin while it is up: a still
    /// photograph on a television for half a minute is a photograph of a
    /// television, and one pixel of movement a frame is enough to stop that
    /// without the picture ever looking like it is moving.
    fn paint_photo(&self, fb: &mut Framebuffer, shown: &crate::photos::Shown, alpha: f32, at: f32) {
        let img = &shown.image;
        let spare_x = img.w.saturating_sub(fb.w) as f32;
        let spare_y = img.h.saturating_sub(fb.h) as f32;
        // Which way it drifts is decided by the picture itself, so the frame
        // does not pan the same way all evening.
        let sign = if img.px.first().copied().unwrap_or(0) & 1 == 0 {
            at
        } else {
            1.0 - at
        };
        let ox = (spare_x * sign).round() as usize;
        let oy = (spare_y * sign).round() as usize;
        let bg = self.theme.bg;
        for y in 0..fb.h {
            let sy = (y + oy).min(img.h.saturating_sub(1));
            for x in 0..fb.w {
                let sx = (x + ox).min(img.w.saturating_sub(1));
                if sx >= img.w || sy >= img.h {
                    continue;
                }
                let c = img.over(sx, sy, bg);
                let out = if alpha >= 0.999 {
                    c
                } else {
                    lerp_color(fb.px[y * fb.w + x], c, alpha)
                };
                fb.px[y * fb.w + x] = out;
            }
        }
    }

    /// Darken everything already drawn, so writing over it can be read.
    fn scrim(&self, fb: &mut Framebuffer, amount: f32) {
        let bg = self.theme.bg;
        for p in fb.px.iter_mut() {
            *p = lerp_color(*p, bg, amount);
        }
    }

    /// Text with a hard shadow behind it, which is the only way small type
    /// stays readable over a photograph.
    fn shadowed(&self, fb: &mut Framebuffer, x: i32, y: i32, text: &str, c: Color, size: i32) {
        fb.text(x + size, y + size, text, 0x0000_0000, size);
        fb.text(x, y, text, c, size);
    }

    /// The photo frame.
    pub(super) fn draw_frame(&mut self, fb: &mut Framebuffer) {
        // The supply runs for the size of this screen; asking again while it
        // is already running for this size does nothing.
        self.photos.start(self.photo_supply(fb.w, fb.h));
        self.photos.poll();

        let f = self.settings.frame.clone();
        let dwell = f.seconds.clamp(5, 600) as f64;
        let up_for = self.now - self.frame_since;
        if self.frame_now.is_none() || up_for >= dwell {
            if let Some(next) = self.photos.take() {
                self.frame_previous = self.frame_now.take();
                self.frame_now = Some(next);
                self.frame_since = self.now;
            } else if self.frame_now.is_some() {
                // Nothing new ready: keep the picture up rather than showing
                // a hole, and try again on the next frame.
                self.frame_since = self.now - dwell + 1.0;
            }
        }

        fb.clear(self.theme.bg);
        let h = fb.h as i32;
        let w = fb.w as i32;
        let left = (w as f32 * 0.05) as i32;

        let Some(now_shown) = self.frame_now.take() else {
            self.draw_frame_waiting(fb);
            return;
        };
        let fade = clamp(((self.now - self.frame_since) / 1.4) as f32, 0.0, 1.0);
        let at = clamp(((self.now - self.frame_since) / dwell) as f32, 0.0, 1.0);
        if fade < 1.0
            && let Some(before) = self.frame_previous.take()
        {
            self.paint_photo(fb, &before, 1.0, 1.0);
            self.frame_previous = Some(before);
        }
        self.paint_photo(fb, &now_shown, ease(fade), at);

        match f.style.as_str() {
            // Nothing over the picture at all.
            "photos" => {}
            "panel" => self.draw_frame_panel(fb, &now_shown),
            // The time, and what the picture is.
            _ => {
                let now = chrono::Local::now();
                let clock = now.format("%H:%M").to_string();
                let cw = Framebuffer::text_width(&clock, 4);
                // The clock and the date stack in the corner; the caption
                // gets the whole bottom line to itself, because a place and
                // a date and three names need it.
                let base = h - 14;
                let middle = base - 12;
                self.shadowed(fb, w - left - cw, base - 48, &clock, self.theme.paper, 4);
                let date = now.format("%a %d %b").to_string().to_uppercase();
                let dw = Framebuffer::text_width(&date, 1);
                self.shadowed(fb, w - left - dw, middle, &date, self.theme.dim, 1);
                if !now_shown.ago.is_empty() {
                    let room = (((w - 2 * left - dw - 8) / 8).max(0)) as usize;
                    let ago: String = now_shown.ago.to_uppercase().chars().take(room).collect();
                    self.shadowed(fb, left, middle, &ago, self.theme.accent, 1);
                }
                let room = ((w - 2 * left) / 8).max(0) as usize;
                let caption: String = now_shown.caption().chars().take(room).collect();
                if !caption.is_empty() {
                    self.shadowed(fb, left, base, &caption, self.theme.paper, 1);
                }
            }
        }
        self.frame_now = Some(now_shown);
    }

    /// The whole ambient page: the picture behind, everything else in front.
    fn draw_frame_panel(&mut self, fb: &mut Framebuffer, shown: &crate::photos::Shown) {
        self.scrim(fb, 0.62);
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32;
        let now = chrono::Local::now();

        let clock = now.format("%H:%M").to_string();
        let size = if w >= 320 { 6 } else { 4 };
        let cw = Framebuffer::text_width(&clock, size);
        self.shadowed(fb, (w - cw) / 2, 34, &clock, self.theme.paper, size);

        let date = now.format("%A %d %B").to_string().to_uppercase();
        let dw = Framebuffer::text_width(&date, 1);
        self.shadowed(
            fb,
            (w - dw) / 2,
            34 + size * 8 + 8,
            &date,
            self.theme.dim,
            1,
        );

        let mut y = 34 + size * 8 + 30;
        let room = ((w - 2 * left) / 8).max(4) as usize;
        let info = self.photos.info.clone();
        let mut middle = |fb: &mut Framebuffer, text: &str, colour: Color| {
            let line: String = text.chars().take(room).collect();
            let tw = Framebuffer::text_width(&line, 1);
            fb.text((w - tw) / 2 + 1, y + 1, &line, 0x0000_0000, 1);
            fb.text((w - tw) / 2, y, &line, colour, 1);
            y += 13;
        };
        if !info.weather.is_empty() {
            middle(fb, &info.weather, self.theme.cyan);
        }
        if !info.next.is_empty() {
            let line = format!("NEXT  {}", info.next);
            middle(fb, &line, self.theme.yellow);
        }
        if self.music.status.active()
            && let Some(track) = self.music.status.track.as_ref()
        {
            let label = track.label();
            middle(fb, &label, self.theme.green);
        }

        let base = h - 14;
        let caption = shown.caption();
        let ago = shown.ago.to_uppercase();
        let aw = if ago.is_empty() {
            0
        } else {
            Framebuffer::text_width(&ago, 1) + 8
        };
        if !ago.is_empty() {
            self.shadowed(fb, w - left - aw + 8, base, &ago, self.theme.accent, 1);
        }
        if !caption.is_empty() {
            let fits = (((w - 2 * left - aw) / 8).max(0)) as usize;
            let caption: String = caption.chars().take(fits).collect();
            self.shadowed(fb, left, base, &caption, self.theme.paper, 1);
        }
    }

    /// Before the first picture arrives, or when none can.
    fn draw_frame_waiting(&mut self, fb: &mut Framebuffer) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let now = chrono::Local::now();
        let clock = now.format("%H:%M").to_string();
        let size = if w >= 320 { 6 } else { 4 };
        let cw = Framebuffer::text_width(&clock, size);
        fb.text(
            (w - cw) / 2,
            h / 2 - size * 8,
            &clock,
            self.theme.paper,
            size,
        );
        let date = now.format("%A %d %B").to_string().to_uppercase();
        let dw = Framebuffer::text_width(&date, 1);
        fb.text((w - dw) / 2, h / 2 + 6, &date, self.theme.dim, 1);
        let line = match &self.photos.trouble {
            Some(why) => why.clone(),
            None => "reading the collection...".to_string(),
        };
        let room = ((w as f32 * 0.9) as i32 / 8) as usize;
        let line: String = line.chars().take(room).collect();
        let lw = Framebuffer::text_width(&line, 1);
        fb.text((w - lw) / 2, h / 2 + 26, &line, self.theme.cyan, 1);
        let hint = self.hint(&[("A", "next"), ("B", "back")]);
        fb.text(
            (w as f32 * 0.05) as i32,
            h - 14,
            &hint,
            scale(self.theme.dim, 0.7),
            1,
        );
    }

    /// Photo frame settings: what goes over the picture, how long each one
    /// stays, and where they come from.
    pub(super) fn adjust_frame(&mut self, row: usize, dir: i32) {
        fn step<'a>(cur: &str, opts: &[&'a str], dir: i32) -> &'a str {
            let i = opts.iter().position(|o| *o == cur).unwrap_or(0) as i32;
            opts[(i + dir).rem_euclid(opts.len() as i32) as usize]
        }
        let f = &mut self.settings.frame;
        match row {
            0 => f.style = step(&f.style, &["photos", "clock", "panel"], dir).to_string(),
            1 => {
                let opts = [10u32, 15, 25, 45, 90, 300];
                let i = opts.iter().position(|o| *o == f.seconds).unwrap_or(2) as i32;
                f.seconds = opts[(i + dir).rem_euclid(opts.len() as i32) as usize];
            }
            2 => {
                f.source =
                    step(&f.source, &["memories", "favorites", "album", "all"], dir).to_string();
            }
            3 => f.pan = !f.pan,
            _ => {
                // The album is a name typed into a file, not something to
                // spell out with a pad; this row only says which one it is.
            }
        }
        // Anything that changes what comes next means starting the supply
        // again, and dropping what is already in hand.
        if row != 3 {
            self.photos = crate::photos::Feed::new();
            self.frame_now = None;
            self.frame_previous = None;
        }
    }

    pub(super) fn draw_frame_settings(&mut self, fb: &mut Framebuffer, sel: usize) {
        let f = self.settings.frame.clone();
        let style = match f.style.as_str() {
            "photos" => "nothing",
            "panel" => "ambient",
            _ => "the time",
        };
        let source = match f.source.as_str() {
            "favorites" => "favourites",
            "album" => "an album",
            "all" => "everything",
            _ => "memories",
        };
        let rows: Vec<(String, String)> = vec![
            ("over the picture".into(), style.into()),
            ("each one stays".into(), format!("{} s", f.seconds)),
            ("pictures from".into(), source.into()),
            (
                "let them drift".into(),
                if f.pan { "on".into() } else { "off".into() },
            ),
            (
                "album".into(),
                if f.album.is_empty() {
                    "not set".into()
                } else {
                    f.album.clone()
                },
            ),
            (
                "weather for".into(),
                if f.weather.trim().is_empty() {
                    let zone = omarchy_crt_shell::ambient::zone_place();
                    if zone.is_empty() {
                        "unknown".into()
                    } else {
                        zone
                    }
                } else {
                    f.weather.clone()
                },
            ),
        ];
        let notes = [
            "what is written over the photograph",
            "how long before the next one",
            "which photographs the server sends",
            "a picture that fills the screen drifts",
            "the album's name, from settings.toml",
            if f.weather.trim().is_empty() {
                "the town, from this machine's timezone"
            } else {
                "the town, from settings.toml"
            },
        ];
        self.draw_settings_table(fb, "Photo frame", &rows, &notes, sel, 14, None);
        // A settings page for the frame is not the frame, and nothing else on
        // it says that the frame is one button away. The table wrote its own
        // hint on that line first, so the line is cleared before this one.
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32 + self.slide();
        fb.rect(0, h - 15, w, 11, self.theme.bg);
        let hint = self.hint(&[("<>", "change"), ("A", "show it"), ("B", "save")]);
        fb.text(left, h - 14, &hint, scale(self.theme.dim, 0.7), 1);
    }

    /// The ambient page: a window with the weather in it, and the time.
    ///
    /// The picture is the point. What is written over it sits in the dark
    /// band under the horizon, where it can be read without a shadow behind
    /// every letter, and the sky above it is left alone.
    pub(super) fn draw_ambient(&mut self, fb: &mut Framebuffer) {
        // The supply thread carries the weather and the calendar as well as
        // the photographs, so this page starts it too. With no photograph
        // server set up it goes on fetching the outside world alone.
        self.photos.start(self.photo_supply(fb.w, fb.h));
        self.photos.poll();

        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32;
        let now = chrono::Local::now();
        let minutes = now.format("%H").to_string().parse::<u32>().unwrap_or(0) * 60
            + now.format("%M").to_string().parse::<u32>().unwrap_or(0);
        let info = self.photos.info.clone();
        let reading = info.sky.clone();

        let theme = self.theme.clone();
        let clock = self.now;
        self.sky.draw(fb, &theme, &reading, minutes, clock);
        let horizon = crate::sky::Sky::horizon(fb.h);

        // ------------------------------------------------------- the time
        let time = now.format("%H:%M").to_string();
        let size = if w >= 320 { 4 } else { 3 };
        let clock_y = horizon + 5;
        fb.text(left, clock_y, &time, self.theme.paper, size);
        // The colon on the second, the way a clock radio did it. The digits
        // stay where they are: a proportional blink is a wobble.
        if now.format("%S").to_string().parse::<u32>().unwrap_or(0) % 2 == 1 {
            fb.text(
                left + 2 * 8 * size,
                clock_y,
                ":",
                lerp_color(self.theme.bg, self.theme.paper, 0.30),
                size,
            );
        }

        // The temperature, as big as the space beside the clock allows.
        if let Some(t) = reading.temp {
            let temp = format!("{t:.0}C");
            let tw = Framebuffer::text_width(&temp, 3);
            let hot = ((t + 5.0) / 35.0).clamp(0.0, 1.0);
            fb.text(
                w - left - tw,
                clock_y + 6,
                &temp,
                lerp_color(self.theme.cyan, self.theme.orange, hot),
                3,
            );
        }

        // ------------------------------------------------- the three lines
        let row = |i: i32| h - 32 + i * 11;
        // Cutting a line short is fine; cutting it in the middle of a word
        // looks like a fault.
        let cut = |text: &str, room: i32| -> String {
            let fits = (room / 8).max(0) as usize;
            if text.chars().count() <= fits {
                return text.to_string();
            }
            let short: String = text.chars().take(fits).collect();
            match short.rfind(' ') {
                Some(at) if at > fits / 2 => short[..at].to_string(),
                _ => short,
            }
        };
        let date = now.format("%A %d %B").to_string().to_uppercase();
        fb.text(
            left,
            row(0),
            &cut(&date, w - 2 * left - 80),
            self.theme.dim,
            1,
        );
        if !reading.place.is_empty() {
            let place = reading.place.to_uppercase();
            let pw = Framebuffer::text_width(&place, 1);
            fb.text(w - left - pw, row(0), &place, self.theme.dim, 1);
        }

        if reading.known {
            let words = reading.condition.to_uppercase();
            fb.text(
                left,
                row(1),
                &cut(&words, w - 2 * left - 104),
                self.theme.paper,
                1,
            );
            if let Some(wind) = reading.wind_kmh {
                let text = format!("WIND {wind:.0} KM/H");
                let tw = Framebuffer::text_width(&text, 1);
                fb.text(w - left - tw, row(1), &text, self.theme.dim, 1);
            }
        } else {
            fb.text(
                left,
                row(1),
                "no weather yet",
                scale(self.theme.dim, 0.8),
                1,
            );
        }

        // The last line is whichever of these there is something to say
        // about: what is next, or what is playing.
        let mut last = String::new();
        let mut colour = self.theme.yellow;
        if !info.next.is_empty() {
            last = format!("NEXT  {}", info.next);
        } else if self.music.status.active()
            && let Some(track) = self.music.status.track.as_ref()
        {
            last = track.label();
            colour = self.theme.green;
        }
        let hint = self.hint(&[("B", "back")]);
        let hw = Framebuffer::text_width(&hint, 1);
        if !last.is_empty() {
            fb.text(left, row(2), &cut(&last, w - 2 * left - hw - 8), colour, 1);
        }
        fb.text(w - left - hw, row(2), &hint, scale(self.theme.dim, 0.5), 1);
    }
}
