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
        self.t0 = now - BOOT_DONE as f64;
        self.hall_cues = u8::MAX;
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
        // The gate draws the same mark, at rest, and every so often the
        // beam runs back: the only thing that moves on a screen that is
        // waiting.
        let size = 72.0f32;
        let (gx, gy) = ((w - size as i32) / 2, (h as f32 * 0.22) as i32);
        let r = &crate::assets::RETRACE;
        let since = (self.now as f32) % 5.0;
        let k = since / Self::RETRACE_LASTS;
        let base = Self::retrace_base(r, k);
        let cycle = if k < 1.05 { 0.0 } else { -1.0 };
        self.draw_retrace(fb, gx, gy, size, base, 1.0, cycle >= 0.0);
        fb.text_centered(
            w / 2,
            (h as f32 * 0.58) as i32,
            &NAME.to_uppercase(),
            self.theme.green,
            2,
        );
        // The cursor belongs to the line, so the line has to be centred with
        // it: centring the words alone pushes the pair off to the left by
        // half the cursor's width plus its gap.
        let msg = "PRESS START";
        let y = (h as f32 * 0.70) as i32;
        let tw = Framebuffer::text_width(msg, 1);
        let x0 = w / 2 - (tw + 4 + 6) / 2;
        fb.text(x0, y, msg, self.theme.dim, 1);
        if (self.now * 2.0).floor() as i64 % 2 == 0 {
            fb.rect(x0 + tw + 4, y, 6, 8, self.theme.green);
        }
        fb.text_centered(
            w / 2,
            h - 16,
            "(C) 2026 OMACRT / 15KHZ EDITION",
            scale(self.theme.dim, 0.6),
            1,
        );
    }

    fn post_lines(&self) -> Vec<(String, Color, bool)> {
        let th = &self.theme;
        vec![
            // The BIOS pastiche does not sign itself Omacom, and no longer
            // signs itself for Omarchy either: the launcher runs on any
            // Hyprland desktop, and what it is for is the tube. Thirty six
            // columns at most: "a real television" ran off the edge.
            (format!("{NAME} BIOS 4.01 / 15kHz"), th.green, false),
            (format!("(C) 2026 {NAME}, for a real tube"), th.dim, false),
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
                format!(
                    "DSK  {NAME_LOWER} {}               OK",
                    env!("CARGO_PKG_VERSION")
                ),
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

    /// The mark's resting size and place. Thirty pixels of the 240 the tube
    /// has: the mark needs that much to keep four bars apart, and the budget
    /// below it is unforgiving. The wordmark is sixty tall and the menu wants
    /// a hundred and thirteen, so a mark much over forty pushes the last row
    /// off the bottom.
    pub(super) fn logo_final(&self, fb: &Framebuffer) -> (i32, i32, i32) {
        const SIZE: i32 = 30;
        // The status bar lives in the first ten rows, so eight put the mark
        // on top of it.
        const TOP: i32 = 9;
        ((fb.w as i32 - SIZE) / 2, TOP, SIZE)
    }

    /// Where the wordmark starts.
    pub(super) fn mark_x(&self, fb: &Framebuffer) -> i32 {
        (fb.w as i32 - self.mark_cols * MARK_SCALE) / 2
    }

    pub(super) fn mark_final_y(&self, fb: &Framebuffer) -> i32 {
        let (_, ly, lsize) = self.logo_final(fb);
        // Less air between the mark and the wordmark than around the pair, or
        // the two read as two things instead of one.
        ly + lsize + 8
    }

    /// The mark, and how it arrives.
    ///
    /// There is no Omarchy mark here to transform. The project changed its
    /// name to keep its distance, and showing somebody else's mark in the
    /// first two seconds of every boot is the opposite of distance; the
    /// attribution belongs in the post, where it can be read.
    ///
    /// The mark arrives with the two gestures the machine has instead: it is
    /// written from the top down behind a beam, and then the beam runs back
    /// across it once and stops where it started.
    pub(super) const SCAN_FROM: f32 = 2.2;
    pub(super) const SCAN_TO: f32 = 2.78;
    pub(super) const RETRACE_FROM: f32 = 2.88;
    pub(super) const RETRACE_TO: f32 = 3.34;
    /// Ogni tanto il fascio torna, dovunque il marchio sia: e' il suo stato
    /// di riposo che si rinnova, non un'animazione del boot.
    pub(super) const RETRACE_EVERY: f32 = 9.0;

    /// Il taglio dove sta adesso, per chi disegna il marchio fuori dal boot:
    /// un ritorno ogni RETRACE_EVERY secondi, e a riposo in mezzo.
    pub(super) fn retrace_now(&self) -> (i32, bool) {
        let r = &crate::assets::RETRACE;
        let k = ((self.now as f32) % Self::RETRACE_EVERY) / Self::RETRACE_LASTS;
        (Self::retrace_base(r, k), k < 1.05)
    }
    pub(super) const RETRACE_LASTS: f32 = 0.46;

    /// La posizione del taglio a un certo istante, dentro un ritorno o a
    /// riposo. Un intero: e' tutto il disegno e tutta l'animazione.
    pub(super) fn retrace_base(r: &crate::assets::Retrace, k: f32) -> i32 {
        if !(0.0..1.0).contains(&k) {
            return r.rest;
        }
        let jump = 0.09;
        let travel = if k < jump {
            r.thick as f32 * (k / jump)
        } else {
            let p = ease(((k - jump) / (1.0 - jump)).clamp(0.0, 1.0));
            r.thick as f32 + (r.span() - r.thick) as f32 * p
        };
        let (span, lo) = (r.span() as f32, r.low() as f32);
        (((r.rest as f32 + travel - lo) % span + span) % span + lo).round() as i32
    }

    /// Il marchio, alla misura e con il taglio dove chiede `base`.
    ///
    /// `revealed` e' la rivelazione dall'alto in basso, come nel boot
    /// vecchio: sopra il fronte il segno c'e', sotto no, e sul fronte corre
    /// la barra del fascio col suo bagliore.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_retrace(
        &self,
        fb: &mut Framebuffer,
        x: i32,
        y: i32,
        size: f32,
        base: i32,
        revealed: f32,
        hot: bool,
    ) {
        let r = &crate::assets::RETRACE;
        let green = self.theme.green;
        let warm = lerp_color(green, self.theme.paper, 0.45);
        let side = size.round() as i32;
        let front = (size * revealed.clamp(0.0, 1.0)).round() as i32;
        for seg in r.segments(size, base) {
            if seg.y >= front {
                continue;
            }
            // La barra tagliata a meta' dal fronte della rivelazione.
            let h = (front - seg.y).min(seg.h);
            if h <= 0 || seg.w <= 0 {
                continue;
            }
            fb.rect(x + seg.x, y + seg.y, seg.w, h, green);
            if hot && seg.hot && h == seg.h {
                let hw = (size / r.grid as f32).round().max(1.0) as i32;
                fb.rect(x + seg.x, y + seg.y, hw, h, warm);
            }
        }
        if revealed < 1.0 && front > 0 {
            fb.rect_add(x, y + front - 5, side, 8, scale(self.theme.cyan, 0.22));
            fb.rect(x, y + front - 2, side, 2, scale(self.theme.green, 0.9));
        }
    }

    pub(super) fn draw_logo(&mut self, fb: &mut Framebuffer, t: f32) {
        if t < Self::SCAN_FROM {
            return;
        }
        let (w, h) = (fb.w as f32, fb.h as f32);
        let up = ease(clamp((t - 3.45) / 0.5, 0.0, 1.0));
        let settle = ease(clamp((t - 5.75) / 0.65, 0.0, 1.0));
        let (fx, fy, fsize) = self.logo_final(fb);
        let big = (h * 0.40).min(96.0);
        let size = lerp(lerp(big, 40.0, up), fsize as f32, settle);
        let cx = lerp(w * 0.5, fx as f32 + fsize as f32 * 0.5, settle);
        let cy = lerp(lerp(h * 0.46, h * 0.22, up), fy as f32 + size * 0.5, settle);
        let x = (cx - size * 0.5).round() as i32;
        let y = (cy - size * 0.5).round() as i32;

        let r = &crate::assets::RETRACE;
        let revealed = ease(clamp(
            (t - Self::SCAN_FROM) / (Self::SCAN_TO - Self::SCAN_FROM),
            0.0,
            1.0,
        ));
        // Il ritorno: nel boot una volta, e poi ogni nove secondi per
        // sempre, dovunque il marchio stia.
        let (rf, rt) = (Self::RETRACE_FROM, Self::RETRACE_TO);
        let (base, hot) = if t < rt {
            let k = (t - rf) / (rt - rf);
            (Self::retrace_base(r, k), t >= rf)
        } else {
            let since = (t - rt) % Self::RETRACE_EVERY;
            let k = since / Self::RETRACE_LASTS;
            (Self::retrace_base(r, k), k < 1.05)
        };
        self.draw_retrace(fb, x, y, size, base, revealed, hot);
    }

    pub(super) fn draw_etch(&mut self, fb: &mut Framebuffer, t: f32) {
        if t < ETCH_START {
            return;
        }
        let fade = ease(clamp((t - 3.6) / 0.35, 0.0, 1.0));
        let (w, h) = (fb.w as f32, fb.h as f32);
        let mw = self.mark_cols * MARK_SCALE;
        let settle = ease(clamp((t - 5.75) / 0.65, 0.0, 1.0));
        let final_y = self.mark_final_y(fb);
        let x = ((w - mw as f32) * 0.5).round() as i32;
        let y = lerp(h * 0.36, final_y as f32, settle).round() as i32;
        let jolt_for_etch = self.act_jolt(t);
        if let Some(etch) = self.etch.as_mut() {
            if !self.etch_sound_played {
                self.etch_sound_played = true;
                self.pending_samples.push(etch.synth(crate::audio::RATE));
            }
            etch.advance_to(t - ETCH_START);
            etch.draw(
                fb,
                x + jolt_for_etch.0,
                y + jolt_for_etch.1,
                MARK_SCALE,
                fade,
            );
        }
    }

    /// The second act, with its beat sheet.
    ///
    /// The floor unrolls from the horizon while the word is still settling.
    /// The glass under the word only appears once the word has landed: a
    /// reflection of something still moving is a reflection in the wrong
    /// place. Then the light comes up the floor, crosses the glass, crosses
    /// the letters, and leaves them lit in phosphor.
    pub(super) const ACT_SETTLED: f32 = 6.45;
    pub(super) const ACT_FLOOR_IN: f32 = 0.55;
    pub(super) const ACT_NEAR: f32 = 1.55;
    pub(super) const ACT_GLASS: f32 = 1.8;
    pub(super) const ACT_IMPACT: f32 = 2.0;

    /// The shake, for everything in the picture.
    pub(super) fn act_jolt(&self, t: f32) -> (i32, i32) {
        let local = t - TAG_START;
        if !(Self::ACT_IMPACT..Self::ACT_IMPACT + 0.2).contains(&local) {
            return (0, 0);
        }
        let k = (1.0 - (local - Self::ACT_IMPACT) / 0.2) * 3.5;
        let n = (local * 90.0) as u32;
        (
            ((crate::crt_tag::hash(n) - 0.5) * 2.0 * k).round() as i32,
            ((crate::crt_tag::hash(n + 31) - 0.5) * 2.0 * k).round() as i32,
        )
    }

    /// The act, in two layers. The floor, the light travelling on it and
    /// the reflection all belong UNDER the wordmark, or the floor cuts the
    /// letters off while they are still coming down; the sweep and the
    /// copper bar belong OVER it, because crossing the letters is the point.
    pub(super) fn draw_crt_tag(&mut self, fb: &mut Framebuffer, t: f32, above: bool) {
        if t < TAG_START {
            return;
        }
        let local = t - TAG_START;
        let look = crate::crt_tag::Look {
            bg: self.theme.bg,
            floor_light: self.theme.dim,
            floor_dark: self.theme.fg_dark_floor(),
            orange: self.theme.orange,
            yellow: self.theme.yellow,
        };
        const SPLIT: i32 = 39;
        const FLOOR_OUT: f32 = 2.2;
        let (floor_in, near, glass_out, impact) = (
            Self::ACT_FLOOR_IN,
            Self::ACT_NEAR,
            Self::ACT_GLASS,
            Self::ACT_IMPACT,
        );

        let rows = self.etch.as_ref().map(|e| e.rows).unwrap_or(10);
        let word_h = rows * 2 * MARK_SCALE;
        let y = self.mark_final_y(fb);
        let foot = y + word_h;
        // Never above the wordmark's feet, whatever the framebuffer is.
        let horizon = crate::crt_tag::horizon(fb.h as i32).max(foot + 4);
        let bottom = fb.h as i32;
        let jolt = self.act_jolt(t);
        let scroll = crate::crt_tag::floor_scroll(local);

        let alpha = if local < FLOOR_OUT {
            1.0
        } else {
            1.0 - ((local - FLOOR_OUT) / 0.8).clamp(0.0, 1.0)
        };
        if alpha > 0.0 && !above {
            let flash = (impact..impact + 0.035).contains(&local);
            let open = (local / floor_in).clamp(0.0, 1.0);
            crate::crt_tag::draw_floor(fb, horizon, local, alpha, jolt, &look, flash, open);
        }

        // The beam, while it is still on the floor: an object in the floor's
        // own world, so row, width, thickness and haze all come out of the
        // projection the checkerboard is drawn with.
        let mut beam_y = None;
        let run = floor_in + 0.15;
        if (run..near).contains(&local) {
            let p = (local - run) / (near - run);
            // Constant speed toward the viewer: the depth halves in equal
            // times, so it crawls far away and rushes at the end.
            let dy = 1.2f32 * (110.0f32 / 1.2).powf(p);
            let on = crate::crt_tag::on_floor(horizon, dy, 900.0, 26.0);
            let (cx, cy) = (fb.w as i32 / 2 + jolt.0, on.y + jolt.1);
            beam_y = Some(cy);
            for i in 0..on.rows {
                if above {
                    break;
                }
                let f = 1.0 - i as f32 / on.rows.max(1) as f32;
                let c = crate::fb::lerp_color(look.orange, 0xffffff, f);
                let haze = (0.35 + 0.65 * on.fog) * alpha;
                fb.rect(
                    cx - on.half,
                    cy - i,
                    on.half * 2,
                    1,
                    crate::fb::lerp_color(look.bg, c, haze),
                );
            }
            // What a light does to the surface it travels on.
            for i in 1..(on.rows * 2).max(3) {
                if above {
                    break;
                }
                let f = 1.0 - i as f32 / (on.rows * 2).max(3) as f32;
                let c = crate::fb::lerp_color(look.bg, look.yellow, 0.30 * f * on.fog * alpha);
                let yy = cy - on.rows - i;
                let mut xx = cx - on.half + (i & 1);
                while xx < cx + on.half {
                    fb.put(xx, yy, c);
                    xx += 2;
                }
            }
        }

        // Off the floor: it turns and sweeps up, through the glass first and
        // then the letters, thinning into a scan line as it goes.
        if (near..impact).contains(&local) {
            let p = (local - near) / (impact - near);
            let by = bottom - (p * (bottom - y) as f32) as i32;
            let thick = (14.0 - 11.0 * p).round().max(3.0) as i32;
            beam_y = Some(by);
            for i in 0..thick {
                if !above {
                    break;
                }
                let f = i as f32 / thick as f32;
                let c = if i == thick / 2 {
                    0xffffff
                } else {
                    crate::fb::lerp_color(look.yellow, look.orange, f)
                };
                fb.rect(jolt.0, by + i, fb.w as i32, 1, c);
            }
        }

        // The glass: only once the word has landed, and it answers the light.
        let x = self.mark_x(fb);
        let glass_fade = ((t - Self::ACT_SETTLED) / 0.25).clamp(0.0, 1.0) * alpha;
        if let Some(etch) = self.etch.as_ref().filter(|_| !above) {
            // While the beam is in the glass, the rows it crosses light up.
            let lit = beam_y
                .filter(|_| (near..glass_out + 0.1).contains(&local))
                .map(|b| (b - jolt.1, 0.9));
            etch.draw_reflection(
                fb,
                x + jolt.0,
                foot + jolt.1,
                MARK_SCALE,
                crate::etch::Glass {
                    fade: glass_fade,
                    scroll,
                    lit,
                },
            );
        }

        if local >= 0.0 && !above && !self.tag_sound_played {
            self.tag_sound_played = true;
            self.pending.push(Sound::TagReveal);
        }
        if above && (impact..impact + 0.3).contains(&local) {
            let p = (local - impact) / 0.3;
            let by = bottom - (p * (bottom - horizon) as f32) as i32;
            crate::crt_tag::draw_copper_bar(fb, by, &look, 1.0 - p * 0.5);
        }

        let phosphor = [self.theme.green, self.theme.bright_green, self.theme.paper];
        // The letters change only while the beam is inside them, which is
        // the last stretch of the sweep.
        let crossed = if local < glass_out {
            0
        } else {
            let p = ((local - near) / (impact - near)).clamp(0.0, 1.0);
            let by = bottom - (p * (bottom - y) as f32) as i32;
            ((foot - by) / (2 * MARK_SCALE)).clamp(0, rows)
        };
        let after = (local - impact).max(0.0);
        let swap = crate::etch::Swap {
            from_col: SPLIT,
            rows_done: crossed,
            stops: phosphor,
            flash: (1.0 - after / 0.1).clamp(0.0, 1.0),
            bright: if local >= impact {
                0.94 + 0.06 * (after * 2.0).sin()
            } else {
                1.0
            },
            scanlines: (1.0 - after / 0.5).clamp(0.0, 1.0),
        };
        let below = crate::etch::Swap {
            rows_done: if local >= impact + 0.15 { rows } else { 0 },
            flash: 0.0,
            scanlines: 0.0,
            ..swap
        };
        if let Some(etch) = self.etch.as_mut().filter(|_| !above) {
            etch.swap = Some(swap);
            etch.swap_below = Some(below);
        }
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
        // Under the cursor the words are the light colour on their shadow:
        // the bar is already the accent, and accent on accent does not read.
        let (icon_c, text_c, chev_c) = if on {
            (self.theme.accent, self.theme.paper, self.theme.paper)
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
        icons::paint(
            fb,
            x + 4,
            y + 2,
            icon,
            &self.theme,
            icon_c,
            on,
            fade * pulse,
        );
        if on {
            let shadow = crate::paint::Tones::of(&self.theme).shadow;
            fb.text(x + 19, y + 3, label, scale(shadow, fade), 1);
        }
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
        // Pictures for a public page carry no figures from the library.
        if shots() && matches!(label, "Games" | "Videos" | "Favorites" | "Recent") {
            return None;
        }
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
                // The window is narrow: the count of games, and the count of
                // systems only when it fits.
                let _ = systems;
                (games > 0).then(|| format!("{games} games"))
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

    /// The bridge from the boot into the hall, and the hall itself once the
    /// boot is over: the laser climbs the screen with the hall standing up
    /// behind it, the word and the mark fly to the far wall, the word strikes
    /// as the neon sign and the hall powers on around it.
    pub(super) fn draw_hall_bridge(&mut self, fb: &mut Framebuffer, t: f32) {
        use crate::hall;
        let tick = (self.now * hall::TICK_HZ) as u32;
        let lights = if t < HALL_LANDS {
            hall::dark()
        } else {
            hall::power_on(t - HALL_LANDS)
        };
        let board = hall::board(&self.theme);
        let sign_c = self.theme.cyan;
        let climb_to = HALL_FROM + HALL_CLIMB;
        if t < climb_to {
            // The boot's own picture is already in the framebuffer; the
            // hall replaces it below the laser.
            let mut scratch = self
                .hall_scratch
                .take()
                .filter(|s| s.w == fb.w && s.h == fb.h)
                .unwrap_or_else(|| Framebuffer::new(fb.w, fb.h));
            self.hall
                .draw(&mut scratch, &self.theme, &lights, sign_c, tick);
            let p = ease(clamp((t - HALL_FROM) / HALL_CLIMB, 0.0, 1.0));
            let line = ((fb.h as f32) * (1.0 - p)) as i32;
            let from = line.clamp(0, fb.h as i32) as usize * fb.w;
            fb.px[from..].copy_from_slice(&scratch.px[from..]);
            self.hall_scratch = Some(scratch);
            self.draw_flying_word(fb, 0.0);
            // The laser: the same orange as the one that wrote the word,
            // white in its middle, a haze under it.
            let orange = self.theme.orange;
            fb.rect(0, line - 1, fb.w as i32, 3, orange);
            fb.rect(0, line, fb.w as i32, 1, 0xfff0dc);
            for x in 0..fb.w as i32 {
                if crate::paint::BAYER[((line + 2) & 3) as usize][(x & 3) as usize] < 8 {
                    let under = fb.at(x, line + 2);
                    fb.put(x, line + 2, lerp_color(under, orange, 0.5));
                }
            }
            if self.hall_cues & 1 == 0 {
                self.hall_cues |= 1;
                self.pending.push(Sound::Laser);
            }
            return;
        }
        if t >= BOOT_DONE {
            self.crowd.start(self.now);
        }
        self.hall.fx = self.crowd.fx(self.now);
        self.hall.draw(fb, &self.theme, &lights, sign_c, tick);
        if t < HALL_LANDS {
            let f = ease(clamp((t - climb_to) / HALL_FLY, 0.0, 1.0));
            self.draw_flying_word(fb, f);
            return;
        }
        let (cols, rows) = (self.hall_mask.1 as i32, self.hall_mask.2 as i32);
        let (ox, oy) = hall::Hall::sign_at(fb, cols);
        let tubes = (
            hall::Tube::of(self.theme.cyan, board),
            hall::Tube::of(self.theme.green, board),
        );
        hall::draw_sign(fb, &self.hall_mask, (ox, oy), tubes, &lights, board, tick);
        let (base, hot) = self.retrace_now();
        self.draw_retrace(fb, ox - 16, oy + rows / 2 - 5, 10.0, base, 1.0, hot);
        hall::draw_arrow(fb, ox + cols + 6, oy + rows / 2 - 4, &lights, tick);
        // Whoever is in the hall stands in front of the sign on the far wall.
        self.crowd.draw(fb, &self.hall, board, self.now);
        // The sounds of it: the sign striking, a clunk for every pair of
        // cabinets switched on.
        let local = t - HALL_LANDS;
        if self.hall_cues & 2 == 0 {
            self.hall_cues |= 2;
            self.pending.push(Sound::Neon);
        }
        for (k, at) in hall::CAB_TIMES.iter().enumerate() {
            let bit = 4u8 << k;
            if local >= *at && self.hall_cues & bit == 0 {
                self.hall_cues |= bit;
                self.pending.push(Sound::Cabinet);
            }
        }
    }

    /// The word and the mark on their way from where the boot left them
    /// (`f` 0) to the far wall (`f` 1), shrinking as they go.
    fn draw_flying_word(&mut self, fb: &mut Framebuffer, f: f32) {
        let (cols, rows) = (self.hall_mask.1 as i32, self.hall_mask.2 as i32);
        let (ox, oy) = crate::hall::Hall::sign_at(fb, cols);
        let (lx, ly, lsize) = self.logo_final(fb);
        let (base, hot) = self.retrace_now();
        let size = lerp(lsize as f32, 10.0, f);
        let mx = lerp(lx as f32, (ox - 16) as f32, f).round() as i32;
        let my = lerp(ly as f32, (oy + rows / 2 - 5) as f32, f).round() as i32;
        self.draw_retrace(fb, mx, my, size, base, 1.0, hot && f == 0.0);
        let Some(etch) = self.etch.as_ref() else {
            return;
        };
        let (x0, y0) = (self.mark_x(fb), self.mark_final_y(fb));
        if f <= 0.0 {
            etch.draw(fb, x0, y0, MARK_SCALE, 1.0);
            return;
        }
        // Drawn once at its own size, then scaled pixel by pixel.
        const KEY: Color = 0x010203;
        let mut word = Framebuffer::new(fb.w, fb.h);
        word.clear(KEY);
        etch.draw(&mut word, x0, y0, MARK_SCALE, 1.0);
        let (ww, wh) = (self.mark_cols * MARK_SCALE, self.mark_rows * 2 * MARK_SCALE);
        let (scx, scy) = (x0 as f32 + ww as f32 / 2.0, y0 as f32 + wh as f32 / 2.0);
        let (tcx, tcy) = (ox as f32 + cols as f32 / 2.0, oy as f32 + rows as f32 / 2.0);
        let s = lerp(1.0, cols as f32 / ww as f32, f);
        let (cx, cy) = (lerp(scx, tcx, f), lerp(scy, tcy, f));
        let (hw, hh) = ((ww as f32 / 2.0 + 5.0) * s, (wh as f32 / 2.0 + 5.0) * s);
        for y in (cy - hh) as i32..=(cy + hh) as i32 {
            for x in (cx - hw) as i32..=(cx + hw) as i32 {
                let sx = (scx + (x as f32 + 0.5 - cx) / s).floor() as i32;
                let sy = (scy + (y as f32 + 0.5 - cy) / s).floor() as i32;
                let c = word.at(sx, sy);
                if c != KEY && !(sx < 0 || sy < 0 || sx >= fb.w as i32 || sy >= fb.h as i32) {
                    fb.put(x, y, c);
                }
            }
        }
    }

    /// The home menu, in a window of dark glass with a neon frame over the
    /// aisle. Its frame strikes from the middle outwards, the glass darkens,
    /// then the rows come up one after another.
    pub(super) fn draw_home(&mut self, fb: &mut Framebuffer, t: f32) {
        let m = ease(clamp((t - MENU_FROM) / 0.8, 0.0, 1.0));
        if m <= 0.0 {
            return;
        }
        let w = fb.w as i32;
        let h = fb.h as i32;
        let width = 180;
        let left = (w - width) / 2;
        let top = (h as f32 * 0.525) as i32;
        let row_h = 12;
        let height = HOME.len() as i32 * row_h + 12;
        let glass = clamp((m - 0.2) / 0.3, 0.0, 1.0);
        let dark = crate::hall::board(&self.theme);
        if glass > 0.0 {
            for y in top..top + height {
                for x in left..left + width {
                    let under = fb.at(x, y);
                    let c = if glass >= 1.0 {
                        lerp_color(under, dark, 0.8)
                    } else {
                        crate::paint::dither(x, y, glass, under, lerp_color(under, dark, 0.8))
                    };
                    fb.put(x, y, c);
                }
            }
        }
        let strike = clamp(m / 0.35, 0.0, 1.0);
        let half = (width as f32 / 2.0 * strike) as i32;
        let cx = left + width / 2;
        let (hot, cold) = (self.theme.magenta, self.theme.cyan);
        fb.rect(cx - half, top, half * 2, 1, hot);
        fb.rect(cx - half, top - 1, half * 2, 1, lerp_color(dark, hot, 0.5));
        if strike >= 1.0 {
            let side = ((height - 4) as f32 * clamp((m - 0.35) / 0.15, 0.0, 1.0)) as i32;
            let c = lerp_color(dark, cold, 0.55);
            fb.rect(left, top + 2, 1, side, c);
            fb.rect(left + width - 1, top + 2, 1, side, c);
        }
        let shown = ((clamp((m - 0.5) / 0.5, 0.0, 1.0) * HOME.len() as f32) + 0.001) as usize;
        let rows_y = top + 4;
        let (inner, iw) = (left + 3, width - 6);
        if self.menu_live && shown > self.sel {
            let band_y = self.band(rows_y + self.sel as i32 * row_h);
            self.select_bar(fb, inner, band_y - 1, iw, row_h);
        }
        for (i, (icon, label, submenu)) in HOME.iter().enumerate().take(shown) {
            let y = rows_y + i as i32 * row_h;
            self.draw_menu_row(
                fb,
                inner,
                y,
                iw,
                icon,
                label,
                *submenu,
                self.menu_live && i == self.sel,
                1.0,
            );
        }
        let ready = m >= 1.0;
        // What plays, on the Music row itself.
        if ready && self.music.status.active() {
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
            let vis_w = 20;
            let room =
                ((iw - 18 - vis_w - 24 - Framebuffer::text_width("Music", 1)) / 8).max(0) as usize;
            let text: String = text
                .chars()
                .take(room)
                .collect::<String>()
                .trim_end()
                .to_string();
            let tw = Framebuffer::text_width(&text, 1);
            let tx = inner + iw - 16 - tw;
            fb.text(tx, y + 2, &text, self.theme.dim, 1);
            self.draw_vis(fb, tx - vis_w - 4, y + 9, vis_w, 7);
        }
        // The row under the cursor says what it holds, between its label and
        // its chevron. Music has already written there.
        if self.menu_live
            && ready
            && HOME.get(self.sel).map(|(_, l, _)| *l) != Some("Music")
            && let Some(detail) = self.home_detail(self.sel)
        {
            let y = rows_y + self.sel as i32 * row_h;
            let label = HOME[self.sel].1;
            let room = ((iw - 18 - 10 - Framebuffer::text_width(label, 1) - 8) / 8).max(0) as usize;
            let text: String = detail.chars().take(room).collect();
            let tx = inner + iw - 14 - Framebuffer::text_width(&text, 1);
            let shadow = crate::paint::Tones::of(&self.theme).shadow;
            crate::paint::text_shadow(fb, tx, y + 2, &text, self.theme.fg, shadow);
        }
        // What the set is doing, and the time, in the two top corners.
        let edge = (w as f32 * 0.05) as i32;
        if self.menu_live && ready {
            // On their shadow: the wall tubes run through the corners.
            let shadow = crate::hall::board(&self.theme);
            let status = self.home_status(fb.h);
            crate::paint::text_shadow(fb, edge, 8, &status, self.theme.dim, shadow);
            let clock = crate::clock::now(self.now).format("%H:%M").to_string();
            let cx = w - edge - Framebuffer::text_width(&clock, 1);
            crate::paint::text_shadow(fb, cx, 8, &clock, self.theme.dim, shadow);
        }
        if let Some((msg, _)) = &self.message {
            let cols = ((w - 2 * edge) / 8) as usize;
            let msg: String = msg.chars().take(cols).collect();
            let msg = msg.clone();
            self.draw_message(fb, edge, top - 14, &msg);
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

    /// A lit stage on the right of a hub with one of the voxel models on
    /// it, the way the systems list shows its console; it arrives the same
    /// way when the hub opens. Returns how much width it takes from the rows.
    pub(super) fn draw_prop_stage(
        &mut self,
        fb: &mut Framebuffer,
        y0: i32,
        name: &str,
        brand: Color,
    ) -> i32 {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32;
        let panel = 124;
        let px = w - left - panel;
        let stage_h = (h - 34 - y0).max(80);
        let th = self.theme.clone();
        let (floor, br) = crate::stage::draw(fb, &th, px, y0, panel, stage_h, brand);
        // The same model switched on or off is the same thing on the stage:
        // it does not arrive again.
        let base = name.trim_end_matches("-off");
        if self.stage_name != base {
            self.stage_name = base.to_string();
            self.stage_since = self.now;
        }
        let lamp = lerp_color(br[4], 0xffecbe, 0.5);
        crate::stage::stand(
            fb,
            &th,
            name,
            px + panel / 2,
            floor,
            lamp,
            self.now - self.stage_since,
        );
        panel + 8
    }

    /// A submenu drawn like the home rows under the compact header.
    pub(super) fn draw_menu_screen(
        &mut self,
        fb: &mut Framebuffer,
        title: &str,
        items: &[(icons::Icon, &str, bool)],
        sel: usize,
    ) {
        self.draw_menu_screen_with(fb, title, items, sel, None);
    }

    /// A submenu with a stage beside it, `prop` naming the model and the
    /// colour of its light.
    pub(super) fn draw_menu_screen_with(
        &mut self,
        fb: &mut Framebuffer,
        title: &str,
        items: &[(icons::Icon, &str, bool)],
        sel: usize,
        prop: Option<(&str, Color)>,
    ) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32 + self.slide();
        let y0 = self.draw_header(fb, title);
        let shrink = match prop {
            Some((name, brand)) => self.draw_prop_stage(fb, y0, name, brand),
            None => 0,
        };
        let width = w - 2 * (w as f32 * 0.05) as i32 - shrink;
        let row_h = 14;
        let band_y = self.band(y0 + sel as i32 * row_h);
        self.select_bar(fb, left, band_y, width, row_h - 1);
        for (i, (icon, label, sub)) in items.iter().enumerate() {
            let y = y0 + i as i32 * row_h;
            self.draw_menu_row(fb, left, y, width, icon, label, *sub, i == sel, 1.0);
        }
        let max_cols = (width / 8) as usize;
        if let Some((msg, _)) = &self.message {
            let m: String = msg.chars().take(max_cols).collect();
            self.draw_message(fb, left, h - 28, &m);
        }
        self.draw_hint(fb, left, h - 14, &[("A", "select"), ("B", "back")]);
    }

    /// The launch on a stage: the console, large, lit in its own colour, and
    /// its game going in, a cartridge with the cover as its label dropped
    /// into the slot, a disc into the lid, a coin into the cabinet. It clicks
    /// home at 0.55 s and the console gives a small jolt.
    fn draw_launch_stage(&mut self, fb: &mut Framebuffer, system: &str, u: f32) {
        let (w, h) = (fb.w as i32, fb.h as i32);
        let th = self.theme.clone();
        let brand = icons::system_logo(system)
            .map(|(_, c)| c)
            .unwrap_or(th.accent);
        let (sx, sy, sw, sh) = (24, 20, w - 48, h - 20 - 40);
        let (floor, br) = crate::stage::draw(fb, &th, sx, sy, sw, sh, brand);
        // The timeline. A console with a lid opens it, takes the disc,
        // shuts it and clicks; one with a slot takes the cartridge and
        // clicks. The power lamp lights on the click.
        let ease_out = |t: f32| 1.0 - (1.0 - t) * (1.0 - t);
        let ease_in = |t: f32| t * t;
        let span = |a: f32, b: f32| ((u - a) / (b - a)).clamp(0.0, 1.0);
        let lidded = crate::consoles::media(system).is_some_and(|m| m.lid.is_some());
        let (lid, p, click) = if lidded {
            let opening = ease_out(span(0.05, 0.25));
            let closing = ease_in(span(0.62, 0.80));
            let t = span(0.22, 0.58);
            let p = if u < 0.18 {
                -1.0
            } else {
                t * t * (3.0 - 2.0 * t)
            };
            (opening * (1.0 - closing), p, 0.80)
        } else {
            (0.0, ease_in(span(0.08, 0.55)), 0.55)
        };
        let lamps = u >= click;
        // The lid's catch as it opens, the lock as the game clicks home,
        // each once.
        let cue = |bit: u8, at: f32, s: &Self| -> bool {
            u >= at && s.launching.as_ref().is_some_and(|l| l.cues & bit == 0)
        };
        if lidded && cue(1, 0.05, self) {
            self.pending.push(Sound::Click);
            if let Some(l) = self.launching.as_mut() {
                l.cues |= 1;
            }
        }
        if cue(2, click, self) {
            self.pending.push(Sound::Lock);
            if let Some(l) = self.launching.as_mut() {
                l.cues |= 2;
            }
        }
        let jolt = (click..click + 0.07).contains(&u);
        let game = self.running_path.clone();
        self.draw_console_scene(
            fb,
            system,
            game,
            lid,
            p,
            lamps,
            jolt,
            (floor, br, sy, fb.w as i32 / 2, 170.0),
        );
    }

    /// The console on its stage at one moment of taking or giving back its
    /// game, as the launch and the return both draw it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_console_scene(
        &mut self,
        fb: &mut Framebuffer,
        system: &str,
        game: Option<(String, PathBuf)>,
        lid: f32,
        p: f32,
        lamps: bool,
        jolt: bool,
        (floor, br, sy, mid, max_w): (i32, [Color; 5], i32, i32, f32),
    ) {
        let th = self.theme.clone();
        let Some((model, rect, face)) = crate::consoles::scene(system, lid, p, lamps) else {
            return;
        };
        let cover = game
            .as_ref()
            .and_then(|(sys, path)| self.art.cover(sys, path, 96, 96).cloned());
        let yaw = -0.62f32;
        let pitch = 0.55f32;
        // Sized on the console alone: the room above it, where the game
        // starts from, is allowed to run up the stage.
        let Some(bare) = crate::consoles::model(system) else {
            return;
        };
        let (s, c) = yaw.sin_cos();
        let width = bare.w as f32 * c.abs() + bare.d as f32 * s.abs();
        let deep = bare.w as f32 * s.abs() + bare.d as f32 * c.abs();
        let height = deep * pitch.sin() + bare.h as f32 * pitch.cos();
        // Room above for a game that comes down into it; one pushed in from
        // the front needs none, and a tall console (the cabinet, the PC)
        // would otherwise stand small for the sake of it.
        let above = match crate::consoles::media(system).map(|m| m.way) {
            Some(crate::consoles::Way::Down) => 40,
            _ => 14,
        };
        let scale = (max_w / width)
            .min((floor - sy - above) as f32 / height)
            .min(3.0);
        let v = crate::voxel::View { yaw, pitch, scale };
        let lamp = crate::fb::lerp_color(br[4], 0xffecbe, 0.5);
        let img = crate::voxel::render_decal(
            &model,
            &v,
            lamp,
            0x0a0a0e,
            cover.as_ref().map(|img| (img, rect, face)),
            true,
        );
        // Placed by the console, not by the picture: the room left in front
        // for a game pushed in would otherwise lift it off the floor and
        // push it to one side.
        let front = (model.d - bare.d) as f32;
        let (mut left, mut right, mut bottom) = (f32::MAX, f32::MIN, f32::MIN);
        for x in [0.0, bare.w as f32] {
            for y in [front, model.d as f32] {
                for z in [0.0, bare.h as f32] {
                    let (px, py) = crate::voxel::project(&model, &v, [x, y, z]);
                    left = left.min(px);
                    right = right.max(px);
                    bottom = bottom.max(py);
                }
            }
        }
        let half = ((right - left) / 2.0).round() as i32;
        let cx = ((left + right) / 2.0).round() as i32;
        crate::stage::shadow(fb, &th, mid, floor + 8, half, 7);
        fb.blit(
            mid - cx,
            floor + 10 - bottom.round() as i32 + jolt as i32,
            &img,
        );
    }

    /// Back from a game: the console stands with the game in it while the
    /// television locks to the launcher's mode, then the lamp goes out and
    /// the game comes out of its console, a cartridge rising from the slot,
    /// a lid opening and the disc lifting from the well, and the picture
    /// fades to the list.
    pub(super) fn draw_ejecting(&mut self, fb: &mut Framebuffer) {
        let Some((system, path, since)) = self.ejecting.clone() else {
            return;
        };
        let u = (self.now - since) as f32;
        let (w, h) = (fb.w as i32, fb.h as i32);
        fb.clear(self.theme.bg);
        let th = self.theme.clone();
        let brand = icons::system_logo(&system)
            .map(|(_, c)| c)
            .unwrap_or(th.accent);
        let (sx, sy, sw, sh) = (24, 20, w - 48, h - 20 - 40);
        let (floor, br) = crate::stage::draw(fb, &th, sx, sy, sw, sh, brand);
        let span = |a: f32, b: f32| ((u - a) / (b - a)).clamp(0.0, 1.0);
        let ease_out = |t: f32| 1.0 - (1.0 - t) * (1.0 - t);
        let lidded = crate::consoles::media(&system).is_some_and(|m| m.lid.is_some());
        let (lid, p) = if lidded {
            (ease_out(span(0.05, 0.25)), 1.0 - ease_out(span(0.22, 0.6)))
        } else {
            (0.0, 1.0 - ease_out(span(0.08, 0.5)))
        };
        let game = Some((system.clone(), path));
        // The lamp stays lit while the console stands still, and goes out
        // as the game starts to come out.
        let lamps = u < 0.0;
        self.draw_console_scene(
            fb,
            &system,
            game,
            lid,
            p,
            lamps,
            false,
            (floor, br, sy, w / 2, 170.0),
        );
        let fade = 1.0 - span(EJECT_SECS - 0.3, EJECT_SECS);
        if fade < 1.0 {
            fb.apply_gain(fade);
        }
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
        if crate::consoles::has(&system) {
            self.draw_launch_stage(fb, &system, u);
        } else if disc {
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
