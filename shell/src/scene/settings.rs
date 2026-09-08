//! The settings pages, the diagnostics, the about page, and the wizard that
//! maps a pad SDL does not know.

use super::*;

impl Scene {
    pub(super) fn activate_settings(&mut self, sel: usize) -> Action {
        self.pending.push(Sound::Select);
        match settings_page(sel) {
            Page::Profile => self.go(Screen::Profile { sel: 0 }),
            Page::Fit => self.go(Screen::VideoFit { sel: 0 }),
            Page::Pads => {
                if Bluetooth::available() {
                    self.go(Screen::Pair { sel: 0 });
                    if self.bt.devices.is_empty() {
                        self.bt.start_scan();
                    }
                } else {
                    self.message = Some(("bluetoothctl not found".into(), self.now + 4.0));
                }
            }
            Page::Saver => self.go(Screen::Saver { sel: 0 }),
            Page::Style => {
                let cur = self.settings.theme.clone();
                let sel = if cur == "system" {
                    0
                } else {
                    self.themes
                        .iter()
                        .position(|(n, _)| *n == cur)
                        .map(|i| i + 1)
                        .unwrap_or(0)
                };
                self.go(Screen::Style { sel });
            }
            Page::Diag => {
                self.diag = self.gather_diagnostics();
                self.go(Screen::Diag { top: 0 });
            }
            Page::Music => self.go(Screen::MusicSettings { sel: 0 }),
            Page::Videos => self.go(Screen::VideoSettings { sel: 0 }),
            Page::Frame => self.go(Screen::FrameSettings { sel: 0 }),
            Page::Ambient => self.go(Screen::AmbientSettings { sel: 0 }),
            Page::Sound => self.go(Screen::SoundSettings { sel: 0 }),
            Page::About => self.go(Screen::About { top: 0 }),
        }
        Action::None
    }

    /// The settings page: two columns, four headings the cursor skips over,
    /// and twelve rows.
    ///
    /// `sel` counts rows, the left column first, so the drawing has to work
    /// out which column the cursor is in and where in it the row landed.
    pub(super) fn draw_settings_menu(&mut self, fb: &mut Framebuffer, sel: usize) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32 + self.slide();
        let width = w - 2 * (w as f32 * 0.05) as i32;
        let gutter = 10;
        let col_w = (width - gutter) / 2;
        let y0 = self.draw_header(fb, "Settings");

        for (i, lines) in [SETTINGS_LEFT, SETTINGS_RIGHT].iter().enumerate() {
            let x = left + i as i32 * (col_w + gutter);
            // Rows of this column, in the numbering `sel` uses.
            let first = i * SETTINGS_HALF;
            let mut y = y0;
            let mut row = first;
            for line in lines.iter() {
                match line {
                    SettingsLine::Heading(text) => {
                        // The gap belongs above the heading, which is what
                        // separates one group from the one before it.
                        fb.text(x, y + 10, text, scale(self.theme.dim, 0.62), 1);
                        y += SETTINGS_HEAD_H;
                    }
                    SettingsLine::Row(icon, label, _) => {
                        let on = row == sel;
                        if on {
                            fb.rect(
                                x,
                                self.band(y),
                                col_w,
                                SETTINGS_ROW_H - 2,
                                self.theme.selection,
                            );
                        }
                        // No chevron: every row here opens a page, so one on
                        // each of them says nothing and sits in the gutter.
                        self.draw_menu_row(fb, x, y, col_w, icon, label, false, on, 1.0);
                        row += 1;
                        y += SETTINGS_ROW_H;
                    }
                }
            }
        }

        let max_cols = (width / 8) as usize;
        if let Some((msg, _)) = &self.message {
            let m: String = msg.chars().take(max_cols).collect();
            fb.text(left, h - 28, &m, self.theme.cyan, 1);
        }
        let hint = self.hint(&[("^v<>", "move"), ("A", "select"), ("B", "back")]);
        fb.text(left, h - 14, &hint, scale(self.theme.dim, 0.7), 1);
    }

    /// The Power submenu: back to the desktop, power off with confirmation.
    pub(super) fn activate_power(&mut self, sel: usize) -> Action {
        self.pending.push(Sound::Select);
        match sel {
            0 => Action::Quit,
            1 => Action::Restart,
            _ => {
                let still_armed =
                    matches!(self.armed, Some((i, until)) if i == sel && self.now < until);
                if !still_armed {
                    self.armed = Some((sel, self.now + 3.0));
                    self.message = Some(("press again to power off".into(), self.now + 3.0));
                    return Action::None;
                }
                self.armed = None;
                Action::Launch(vec!["systemctl".into(), "poweroff".into()])
            }
        }
    }

    /// The values the text effect row walks through.
    fn saver_choices() -> Vec<&'static str> {
        std::iter::once("random")
            .chain(effects::ALL.iter().map(|k| k.name()))
            .collect()
    }

    /// Screensaver settings rows: enabled, idle time, how long each page
    /// stays, which text effect, preview, then one row per page.
    pub(super) fn adjust_saver(&mut self, row: usize, dir: i32) {
        let sv = &mut self.settings.screensaver;
        match row {
            0 => sv.enabled = !sv.enabled,
            1 => {
                let v = sv.idle_secs as i32 + dir * 30;
                sv.idle_secs = v.clamp(30, 900) as u32;
            }
            2 => {
                let opts = [0u32, 60, 120, 240, 600, 1800];
                let i = opts.iter().position(|o| *o == sv.cycle_secs).unwrap_or(3) as i32;
                sv.cycle_secs = opts[(i + dir).rem_euclid(opts.len() as i32) as usize];
            }
            3 => {
                let names = Self::saver_choices();
                let i = names.iter().position(|n| *n == sv.effect).unwrap_or(0) as i32;
                let next = (i + dir).rem_euclid(names.len() as i32) as usize;
                sv.effect = names[next].to_string();
            }
            4 => {}
            row => {
                // One page in or out of the rotation. The last one cannot go:
                // an idle television has to show something.
                let Some(page) = omarchy_crt_shell::settings::PAGES.get(row - 5) else {
                    return;
                };
                if !sv.toggle(page) {
                    self.message = Some(("one page has to stay on".into(), self.now + 3.0));
                    self.pending.push(Sound::Crunch);
                }
            }
        }
    }

    /// Facts about the machine and the setup, for the Diagnostics screen.
    pub(super) fn gather_diagnostics(&self) -> Vec<(String, String)> {
        let read = |p: &str| {
            std::fs::read_to_string(p)
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        let mut out = vec![
            ("shell".into(), env!("CARGO_PKG_VERSION").to_string()),
            ("kernel".into(), self.info.kernel.clone()),
            ("host".into(), self.info.host.clone()),
            ("mode".into(), self.info.mode.clone()),
            ("theme".into(), self.theme.name.clone()),
        ];
        // GPU driver of the first card.
        let uevent = read("/sys/class/drm/card0/device/uevent");
        let driver = uevent
            .lines()
            .find_map(|l| l.strip_prefix("DRIVER="))
            .unwrap_or("?")
            .to_string();
        let pci = uevent
            .lines()
            .find_map(|l| l.strip_prefix("PCI_ID="))
            .unwrap_or("")
            .to_string();
        out.push(("gpu".into(), format!("{driver} {pci}").trim().to_string()));
        // Connectors and their status.
        if let Ok(rd) = std::fs::read_dir("/sys/class/drm") {
            let mut conns: Vec<String> = rd
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    let status = read(&format!("/sys/class/drm/{name}/status"));
                    (name.contains('-') && !name.contains("Writeback") && !status.is_empty())
                        .then(|| format!("{} {}", name.trim_start_matches("card"), &status[..1]))
                })
                .collect();
            conns.sort();
            for c in conns {
                out.push(("output".into(), c));
            }
        }
        // RetroArch and cores.
        let ra = std::process::Command::new(&self.library.retroarch)
            .arg("--version")
            .output()
            .ok()
            .and_then(|o| {
                let text = String::from_utf8_lossy(&o.stdout).to_string();
                text.lines()
                    .find(|l| l.contains("RetroArch"))
                    .map(|l| l.split_whitespace().take(2).collect::<Vec<_>>().join(" "))
            })
            .unwrap_or_else(|| "not found".into());
        out.push(("retroarch".into(), ra));
        let cores = std::fs::read_dir(&self.library.core_dir)
            .map(|rd| rd.filter_map(|e| e.ok()).count())
            .unwrap_or(0);
        out.push((
            "cores".into(),
            format!("{cores} in {}", self.library.core_dir.display()),
        ));
        out.push(("systems".into(), self.library.systems.len().to_string()));
        out.push((
            "switching".into(),
            if self.library.switching {
                "on".into()
            } else {
                "off".into()
            },
        ));
        out.push(("monitor".into(), self.profile.monitor.clone()));
        out.push(("pad".into(), format!("{:?}", self.pad).to_lowercase()));
        out.push((
            "bluetooth".into(),
            if Bluetooth::available() {
                "available".into()
            } else {
                "missing".into()
            },
        ));
        out
    }

    pub(super) fn adjust_profile(&mut self, row: usize, dir: i32) {
        match row {
            0 => self.profile.cycle_preset(dir),
            1 => {
                self.profile.h_shift = (self.profile.h_shift + dir).clamp(-16, 16);
                self.profile_preview = true;
            }
            2 => {
                self.profile.v_shift = (self.profile.v_shift + dir).clamp(-16, 16);
                self.profile_preview = true;
            }
            3 => self.profile.h_size = (self.profile.h_size + dir as f32 * 0.01).clamp(0.8, 1.2),
            4 => self.profile.invert_sync = !self.profile.invert_sync,
            _ => {}
        }
    }

    pub fn save_profile(&self) {
        if let Err(e) = self.profile.save(&self.library.config_dir) {
            eprintln!("profile: {e}");
        }
    }

    /// Screensaver settings: which pages an idle television shows, and how
    /// long each of them keeps the screen.
    pub(super) fn draw_saver_settings(&mut self, fb: &mut Framebuffer, sel: usize) {
        use omarchy_crt_shell::settings::PAGES;
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32 + self.slide();
        let width = w - 2 * (w as f32 * 0.05) as i32;
        let y0 = self.draw_header(fb, "Screensaver");
        let sv = self.settings.screensaver.clone();
        let onoff = |b: bool| if b { "on" } else { "off" }.to_string();
        let turns = sv.rotation().len();
        let mut rows: Vec<(String, String)> = vec![
            ("enabled".into(), onoff(sv.enabled)),
            ("after".into(), format!("{} s", sv.idle_secs)),
            (
                "each page stays".into(),
                if sv.cycle_secs == 0 {
                    "for ever".into()
                } else {
                    format!("{} s", sv.cycle_secs)
                },
            ),
            ("text effect".into(), sv.effect.clone()),
            ("show one now".into(), String::new()),
        ];
        for page in PAGES.iter() {
            let (label, _) = saver_page_label(page);
            rows.push((format!("  {label}"), onoff(sv.shows(page))));
        }
        let row_h = 14;
        let band_y = self.band(y0 + sel as i32 * row_h);
        fb.rect(left, band_y, width, row_h - 1, self.theme.selection);
        for (i, (label, value)) in rows.iter().enumerate() {
            let y = y0 + i as i32 * row_h;
            let on = i == sel;
            let colour = if on {
                self.theme.accent
            } else {
                self.theme.paper
            };
            fb.text(left + 18, y + 2, label, colour, 1);
            // Every row here changes something; only the preview has no
            // value of its own, so it is the only one without arrows.
            let right = if i == 4 {
                String::new()
            } else {
                format!("< {value} >")
            };
            fb.text(
                left + width - 8 - Framebuffer::text_width(&right, 1),
                y + 2,
                &right,
                if on {
                    self.theme.accent
                } else {
                    self.theme.dim
                },
                1,
            );
        }
        // The note is about the row under the cursor: the only place there
        // is room to say what a page is, or what a number means.
        let note: String = match sel {
            0 => "off leaves the menu up all night".into(),
            1 => "seconds of nothing before it starts".into(),
            2 => match turns {
                0 | 1 => "one page on, so it keeps the screen".into(),
                n if sv.cycle_secs == 0 => format!("{n} on, but the first keeps the screen"),
                n => format!("{n} pages take turns, in this order"),
            },
            3 => format!("for the wordmark ({} of them)", effects::ALL.len()),
            4 => "start the rotation now".into(),
            row => PAGES
                .get(row - 5)
                .map(|p| saver_page_label(p).1.to_string())
                .unwrap_or_default(),
        };
        let max_cols = (width / 8) as usize;
        fb.text(
            left,
            h - 28,
            &note.chars().take(max_cols).collect::<String>(),
            scale(self.theme.dim, 0.7),
            1,
        );
        let hint = self.hint(&[("<>", "change"), ("A", "preview"), ("B", "save")]);
        fb.text(left, h - 14, &hint, scale(self.theme.dim, 0.7), 1);
    }

    /// Diagnostics: key and value rows, scrollable.
    pub(super) fn draw_diag(&mut self, fb: &mut Framebuffer, top: usize) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32 + self.slide();
        let width = w - 2 * (w as f32 * 0.05) as i32;
        let y0 = self.draw_header(fb, "Diagnostics");
        let row_h = 12;
        let max_cols = (width / 8) as usize;
        let rows = self.diag.clone();
        for (i, (k, v)) in rows.iter().enumerate().skip(top).take(12) {
            let y = y0 + (i - top) as i32 * row_h;
            fb.text(left, y, k, self.theme.dim, 1);
            let room = max_cols.saturating_sub(11);
            let v: String = v.chars().take(room).collect();
            fb.text(left + 11 * 8, y, &v, self.theme.paper, 1);
        }
        if rows.len() > 12 {
            let pos = format!("{}-{}/{}", top + 1, (top + 12).min(rows.len()), rows.len());
            fb.text(
                w - left - Framebuffer::text_width(&pos, 1),
                h - 28,
                &pos,
                self.theme.dim,
                1,
            );
        }
        let hint = self.hint(&[("^v", "scroll"), ("B", "back")]);
        fb.text(left, h - 14, &hint, scale(self.theme.dim, 0.7), 1);
    }

    /// Style: pick one of the installed Omarchy themes, previewed live.
    pub(super) fn draw_style(&mut self, fb: &mut Framebuffer, sel: usize) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32 + self.slide();
        let width = w - 2 * (w as f32 * 0.05) as i32;
        let y0 = self.draw_header(fb, "Style");
        let row_h = 12;
        let page = 12usize;
        let names: Vec<String> = std::iter::once("system (follow Omarchy)".to_string())
            .chain(self.themes.iter().map(|(n, _)| n.clone()))
            .collect();
        let top = sel
            .saturating_sub(page - 1)
            .min(names.len().saturating_sub(page));
        let band_y = self.band(y0 + (sel - top) as i32 * row_h - 2);
        fb.rect(left, band_y, width, row_h, self.theme.selection);
        for (row, i) in (top..(top + page).min(names.len())).enumerate() {
            let y = y0 + row as i32 * row_h;
            let on = i == sel;
            fb.text(
                left + 18,
                y,
                &names[i],
                if on {
                    self.theme.accent
                } else {
                    self.theme.paper
                },
                1,
            );
            // Swatch: the theme's accent and green, read from disk once per frame for the visible rows.
            if i > 0 {
                if let Some(t) = Theme::load_named(&self.themes[i - 1].1, &self.themes[i - 1].0) {
                    fb.rect(left + 4, y + 1, 4, 6, t.accent);
                    fb.rect(left + 9, y + 1, 4, 6, t.green);
                }
            } else {
                fb.rect(left + 4, y + 1, 4, 6, self.theme.accent);
                fb.rect(left + 9, y + 1, 4, 6, self.theme.green);
            }
        }
        let pos = format!("{}/{}", sel + 1, names.len());
        fb.text(
            w - left - Framebuffer::text_width(&pos, 1),
            h - 28,
            &pos,
            self.theme.dim,
            1,
        );
        let hint = self.hint(&[("^v", "preview"), ("A", "keep"), ("B", "back saves")]);
        fb.text(left, h - 14, &hint, scale(self.theme.dim, 0.7), 1);
    }

    /// A settings table: label left, `< value >` right, a note for the
    /// selected row above the hints. `footer` is a second line for a page
    /// with something to say about itself rather than about one row.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_settings_table(
        &mut self,
        fb: &mut Framebuffer,
        title: &str,
        rows: &[(String, String)],
        notes: &[&str],
        sel: usize,
        row_h: i32,
        footer: Option<&str>,
    ) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32 + self.slide();
        let width = w - 2 * (w as f32 * 0.05) as i32;
        let y0 = self.draw_header(fb, title);
        let band_y = self.band(y0 + sel as i32 * row_h);
        fb.rect(left, band_y, width, row_h - 1, self.theme.selection);
        for (i, (label, value)) in rows.iter().enumerate() {
            let y = y0 + i as i32 * row_h;
            let on = i == sel;
            fb.text(
                left + 18,
                y + 2,
                label,
                if on {
                    self.theme.accent
                } else {
                    self.theme.paper
                },
                1,
            );
            // The value is right aligned and the label is not going
            // anywhere, so the value is what has to give: a page whose
            // words happen to be long must not write them over each other.
            let label_w = Framebuffer::text_width(label, 1);
            let room = width - 18 - label_w - 8 - 8 - 4 * 8;
            let fits = (room / 8).max(3) as usize;
            let value: String = if value.chars().count() > fits {
                value.chars().take(fits).collect()
            } else {
                value.clone()
            };
            let right = format!("< {value} >");
            fb.text(
                left + width - 8 - Framebuffer::text_width(&right, 1),
                y + 2,
                &right,
                if on {
                    self.theme.accent
                } else {
                    self.theme.dim
                },
                1,
            );
        }
        let max_cols = (width / 8) as usize;
        // With a footer the note moves up a line to make room for it.
        let note_y = if footer.is_some() { h - 40 } else { h - 28 };
        if let Some(note) = notes.get(sel) {
            fb.text(
                left,
                note_y,
                &note.chars().take(max_cols).collect::<String>(),
                scale(self.theme.dim, 0.8),
                1,
            );
        }
        if let Some(line) = footer {
            fb.text(
                left,
                h - 28,
                &line.chars().take(max_cols).collect::<String>(),
                scale(self.theme.dim, 0.7),
                1,
            );
        }
        let hint = self.hint(&[("<>", "change"), ("B", "save")]);
        fb.text(left, h - 14, &hint, scale(self.theme.dim, 0.7), 1);
    }

    /// About: goals and credits, scrollable text.
    pub(super) fn draw_about(&mut self, fb: &mut Framebuffer, top: usize) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32 + self.slide();
        let y0 = self.draw_header(fb, "About");
        let row_h = 11;
        for (i, line) in ABOUT.iter().enumerate().skip(top).take(15) {
            let y = y0 + (i - top) as i32 * row_h;
            let c = if i == 0 {
                self.theme.accent
            } else if line.ends_with("Goals") || line.starts_with("Made by") {
                self.theme.paper
            } else {
                self.theme.fg
            };
            fb.text(left, y, line, c, 1);
        }
        let hint = self.hint(&[("^v", "scroll"), ("B", "back")]);
        fb.text(left, h - 14, &hint, scale(self.theme.dim, 0.7), 1);
    }

    // --------------------------------------------------------- pad wizard

    pub fn pad_wizard_active(&self) -> bool {
        self.wizard.is_some()
    }

    /// A pad SDL has no mapping for: ask for its buttons one by one. When the
    /// menu is not up yet (boot, a game) the pad waits its turn.
    pub fn pad_wizard_start(&mut self, name: &str, guid: &str, which: u32) {
        if !self.menu_live
            || self.running.is_some()
            || self.launching.is_some()
            || self.wizard.is_some()
            || self.saver.is_some()
        {
            self.pending_wizard = Some((name.to_string(), guid.to_string(), which));
            return;
        }
        self.wizard = Some(Wizard::new(name, guid, which));
        self.pending.push(Sound::Insert);
        self.go(Screen::PadWizard);
    }

    /// A raw joystick input while the wizard runs. Returns the finished
    /// mapping line once the last control is answered.
    pub fn pad_wizard_raw(&mut self, which: u32, raw: Raw) -> Option<String> {
        let now = self.now;
        let w = self.wizard.as_mut()?;
        if w.which != which {
            return None;
        }
        if w.feed(raw, now) {
            self.pending.push(Sound::Click);
        }
        if self.wizard.as_ref().is_some_and(|w| w.finished) {
            return self.pad_wizard_finish();
        }
        None
    }

    pub(super) fn pad_wizard_finish(&mut self) -> Option<String> {
        let w = self.wizard.take()?;
        self.screen = Screen::Settings {
            sel: settings_row(Page::Pads),
        };
        if !w.usable() {
            self.pending.push(Sound::Crunch);
            self.message = Some((
                "pad not mapped: A, B and a way to move are needed".into(),
                self.now + 5.0,
            ));
            return None;
        }
        let mapping = w.mapping();
        match padmap::save(&mapping) {
            Ok(()) => {
                self.pending.push(Sound::Lock);
                self.message = Some((format!("pad mapped: {}", w.name), self.now + 5.0));
            }
            Err(e) => {
                self.pending.push(Sound::Crunch);
                self.message = Some((format!("pad mapping not saved: {e}"), self.now + 5.0));
            }
        }
        Some(mapping)
    }

    pub fn take_remap_request(&mut self) -> bool {
        std::mem::take(&mut self.remap_request)
    }

    pub(super) fn draw_pad_wizard(&mut self, fb: &mut Framebuffer) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32 + self.slide();
        let width = w - 2 * (w as f32 * 0.05) as i32;
        let y0 = self.draw_header(fb, "New pad");
        let Some(wiz) = self.wizard.as_ref() else {
            return;
        };
        let max_cols = (width / 8) as usize;
        let name: String = wiz.name.chars().take(max_cols).collect();
        fb.text(left, y0 + 2, &name, self.theme.paper, 1);
        fb.text(
            left,
            y0 + 14,
            &format!("step {} of {}", wiz.step + 1, padmap::STEPS.len()),
            self.theme.dim,
            1,
        );
        if let Some((_, what)) = wiz.current() {
            fb.text(left, y0 + 34, "Press", self.theme.dim, 1);
            let what: String = what.chars().take(max_cols).collect();
            fb.text(
                left,
                y0 + 46,
                &what,
                self.theme.bright_green,
                2.min(1 + (what.len() * 16 <= width as usize) as i32),
            );
        }
        // What is set so far, newest last.
        let mut y = y0 + 74;
        let start = wiz.binds.len().saturating_sub(6);
        for (field, bind) in &wiz.binds[start..] {
            fb.text(
                left,
                y,
                &format!("{field:<14} {bind}"),
                scale(self.theme.dim, 0.9),
                1,
            );
            y += 10;
        }
        fb.text(
            left,
            h - 40,
            "press A again to skip a control the pad lacks",
            scale(self.theme.dim, 0.8),
            1,
        );
        let hint = self.hint(&[("Enter", "skip"), ("Esc", "cancel")]);
        fb.text(left, h - 14, &hint, scale(self.theme.dim, 0.7), 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every row on the settings page, in the order they are drawn.
    fn pages() -> Vec<Page> {
        (0..SETTINGS_ROWS).map(settings_page).collect()
    }

    #[test]
    fn a_row_and_the_page_it_opens_agree() {
        // The bug this replaces: a screen came back to Settings by counting
        // to a number, and two of them counted to the wrong row once the
        // list grew. Nothing counts now, and this is why.
        for (row, page) in pages().into_iter().enumerate() {
            assert_eq!(settings_row(page), row);
        }
    }

    #[test]
    fn the_page_holds_twelve_rows_under_four_headings() {
        assert_eq!(SETTINGS_ROWS, 12);
        let headings = |lines: &[SettingsLine]| {
            lines
                .iter()
                .filter(|l| matches!(l, SettingsLine::Heading(_)))
                .count()
        };
        assert_eq!(headings(&SETTINGS_LEFT) + headings(&SETTINGS_RIGHT), 4);
        // The two columns hold the same number of rows, which is what makes
        // left and right cross at the same height.
        assert_eq!(SETTINGS_HALF, SETTINGS_ROWS - SETTINGS_HALF);
        // No two rows open the same page, and none of them is missed.
        let mut seen = pages();
        let before = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), before);
    }

    #[test]
    fn a_column_clears_the_message_line() {
        // 52 is what `draw_header` leaves, and a message is drawn at h - 28.
        for lines in [SETTINGS_LEFT, SETTINGS_RIGHT] {
            let bottom: i32 = 52
                + lines
                    .iter()
                    .map(|l| match l {
                        SettingsLine::Heading(_) => SETTINGS_HEAD_H,
                        SettingsLine::Row(..) => SETTINGS_ROW_H,
                    })
                    .sum::<i32>();
            assert!(bottom <= 240 - 28, "a column ends at {bottom} of 240");
        }
    }

    #[test]
    fn a_label_fits_the_column_it_is_in() {
        // Half the width, less the margins and the gutter, less the room the
        // icon takes: fifteen characters at eight pixels each.
        let col = (320 - 2 * 16 - 10) / 2;
        for lines in [SETTINGS_LEFT, SETTINGS_RIGHT] {
            for line in lines.iter() {
                if let SettingsLine::Row(_, label, _) = line {
                    let wide = 18 + Framebuffer::text_width(label, 1);
                    assert!(wide <= col, "{label} needs {wide} of {col}");
                }
            }
        }
    }
}
