//! A running game, and the pause menu over it. The launcher has no way to
//! reach inside an emulator, so every action here is a real hotkey pressed
//! by the compositor.

use super::*;

impl Scene {
    /// Pause the running game and open the pause menu, or resume it.
    pub fn toggle_pause(&mut self) -> PauseOutcome {
        if self.running.is_none() || self.player.is_some() {
            return PauseOutcome::None;
        }
        if let Some(l) = &self.launching
            && !l.spawned
        {
            return PauseOutcome::None;
        }
        if self.paused.is_some() {
            return self.resume_game();
        }
        match omacrt_shell::game::pause_toggle() {
            Ok(()) => {
                self.paused = Some(0);
                // What the core is drawing right now, which is the only
                // moment it can be known: the picture choice is applied at
                // the next start, when no core is running to ask.
                if let Some((system, _)) = &self.running_path
                    && let Some(picture) = crate::library::logged_picture()
                {
                    crate::library::remember_picture(system, picture);
                }
                self.pending.push(Sound::Select);
                if let Some((_, path)) = &self.running_path {
                    let path = path.clone();
                    self.states.forget(&path);
                    if let Some(st) = self.states.latest(&path) {
                        self.message = Some((format!("state {}", st.label()), self.now + 6.0));
                    }
                }
                PauseOutcome::Shown
            }
            Err(e) => {
                self.message = Some((format!("cannot pause: {e}"), self.now + 3.0));
                PauseOutcome::None
            }
        }
    }

    fn resume_game(&mut self) -> PauseOutcome {
        // Leaving with the centring page up is leaving without A: nothing
        // is kept that was not asked to be.
        if let Some(c) = self.centring.clone() {
            self.close_centring(&c, false);
        }
        let _ = omacrt_shell::game::pause_toggle();
        self.paused = None;
        self.pending.push(Sound::Select);
        PauseOutcome::Resumed
    }

    fn game_cmd(&mut self, result: std::io::Result<()>, done: &str) {
        match result {
            Ok(()) => {
                self.pending.push(Sound::Lock);
                self.message = Some((done.into(), self.now + 2.5));
            }
            Err(e) => {
                self.pending.push(Sound::Crunch);
                self.message = Some((format!("{e}"), self.now + 3.0));
            }
        }
    }

    /// Pad and keyboard while the pause menu is up.
    ///
    /// `jump` is the shoulders. The menu is one page, so there is nothing to
    /// page through: they go to the first and the last row, which is the fast
    /// way to Resume at the top and Back to launcher at the bottom.
    pub fn pause_input(&mut self, nav: Option<Nav>, fire: bool, jump: i32) -> PauseOutcome {
        let Some(mut sel) = self.paused else {
            return PauseOutcome::None;
        };
        if self.centring.is_some() {
            self.centring_input(nav, fire, jump);
            return PauseOutcome::None;
        }
        if let Some(to) = pause_jump(sel, jump, PAUSE_ROWS.len()) {
            sel = to;
            self.paused = Some(sel);
            self.pending.push(Sound::Move);
        }
        let row = PAUSE_ROWS[sel.min(PAUSE_ROWS.len() - 1)].0;
        match nav {
            Some(Nav::Up) if sel > 0 => {
                self.paused = Some(sel - 1);
                self.pending.push(Sound::Move);
            }
            Some(Nav::Down) if sel + 1 < PAUSE_ROWS.len() => {
                self.paused = Some(sel + 1);
                self.pending.push(Sound::Move);
            }
            Some(Nav::Left) => self.pause_choice(row, -1),
            Some(Nav::Right) => self.pause_choice(row, 1),
            Some(Nav::Back) => return self.resume_game(),
            _ => {}
        }
        if !fire {
            return PauseOutcome::None;
        }
        match row {
            PauseRow::Resume => self.resume_game(),
            PauseRow::Save => {
                self.game_cmd(omacrt_shell::game::save_state(), "state saved");
                if let Some((_, path)) = &self.running_path {
                    let path = path.clone();
                    self.states.forget(&path);
                }
                PauseOutcome::None
            }
            PauseRow::Load => {
                self.game_cmd(omacrt_shell::game::load_state(), "state loaded");
                PauseOutcome::None
            }
            PauseRow::Rewind => {
                // Rewind runs only where the system allows it: the launch
                // override writes rewind_enable from that flag.
                if !self.running_rewinds() {
                    self.pending.push(Sound::Crunch);
                    self.message = Some(("rewind is off for this system".into(), self.now + 3.0));
                    return PauseOutcome::None;
                }
                let _ = omacrt_shell::game::rewind();
                self.message = Some(("rewinding".into(), self.now + 2.5));
                self.resume_game()
            }
            PauseRow::FastForward => {
                // Toggle fast forward and let the game run: RetroArch keeps
                // the speed until the next toggle from the same menu.
                let _ = omacrt_shell::game::fast_forward();
                self.message = Some(("fast forward toggled".into(), self.now + 2.5));
                self.resume_game()
            }
            PauseRow::SlowMotion => {
                let _ = omacrt_shell::game::slow_motion();
                self.message = Some(("slow motion toggled".into(), self.now + 2.5));
                self.resume_game()
            }
            PauseRow::Aspect | PauseRow::Shader => {
                // A is the same as right on a choice: one list, one direction.
                self.pause_choice(row, 1);
                PauseOutcome::None
            }
            PauseRow::Centre => {
                self.open_centring();
                PauseOutcome::None
            }
            PauseRow::Reset => {
                let _ = omacrt_shell::game::reset();
                self.resume_game()
            }
            PauseRow::Quit => {
                // The launcher started the emulator and stops it itself: see
                // the main loop's handling of PauseOutcome::Quit.
                self.paused = None;
                self.pending.push(Sound::Select);
                PauseOutcome::Quit
            }
        }
    }

    /// Cycle the picture or the shader for the running system, one step in
    /// either direction, and remember it in `systems.toml`.
    fn pause_choice(&mut self, row: PauseRow, dir: i32) {
        let Some((system, _)) = self.running_path.clone() else {
            return;
        };
        let Some(i) = self.library.systems.iter().position(|s| s.name == system) else {
            return;
        };
        let (key, value, said) = match row {
            PauseRow::Aspect => {
                let names: Vec<&str> = crate::library::ASPECTS.iter().map(|(n, _)| *n).collect();
                let next = step(&names, &self.library.systems[i].aspect, dir);
                let label = crate::library::ASPECTS
                    .iter()
                    .find(|(n, _)| *n == next)
                    .map(|(_, l)| *l)
                    .unwrap_or(next);
                self.library.systems[i].aspect = next.to_string();
                ("aspect", next.to_string(), format!("picture: {label}"))
            }
            PauseRow::Shader => {
                let shaders = crate::library::installed_shaders();
                let names: Vec<&str> = shaders.iter().map(|(n, _)| *n).collect();
                let next = step(&names, &self.library.systems[i].shader, dir);
                let label = shaders
                    .iter()
                    .find(|(n, _)| *n == next)
                    .map(|(_, l)| *l)
                    .unwrap_or(next);
                self.library.systems[i].shader = next.to_string();
                ("shader", next.to_string(), format!("shader: {label}"))
            }
            _ => return,
        };
        self.pending.push(Sound::Move);
        match crate::library::set_system_field(&system, key, &value) {
            // Both are read from the config when a core starts, so the
            // player has to be told when it will be seen. Quitting saves the
            // state and starting again picks it up, so nothing is lost.
            Ok(()) => self.message = Some((format!("{said}, at the next start"), self.now + 4.0)),
            Err(e) => {
                self.pending.push(Sound::Crunch);
                self.message = Some((format!("cannot save: {e}"), self.now + 4.0));
            }
        }
    }

    /// What each choice row shows on its right: the name of the setting the
    /// running system carries now.
    fn pause_value(&self, row: PauseRow) -> Option<String> {
        let system = self
            .running_path
            .as_ref()
            .and_then(|(n, _)| self.library.systems.iter().find(|s| &s.name == n))?;
        match row {
            PauseRow::Aspect => {
                let choice = if system.aspect.is_empty() {
                    "fill"
                } else {
                    &system.aspect
                };
                let label = crate::library::ASPECTS
                    .iter()
                    .find(|(n, _)| *n == choice)
                    .map(|(_, l)| *l)
                    .unwrap_or("fill the screen");
                // A choice that is not `fill` needs the size the core draws,
                // and until a core has run there is nothing to work it out
                // from.
                let unknown = choice != "fill"
                    && crate::library::picture_of(&system.name)
                        .and_then(|p| p.wanted(choice))
                        .is_none();
                Some(if unknown {
                    format!("{label}?")
                } else {
                    label.to_string()
                })
            }
            PauseRow::Centre => {
                let path = self.running_path.as_ref().map(|(_, p)| p.clone())?;
                let (x, y) =
                    omacrt_shell::centring::effective((system.shift_x, system.shift_y), &path);
                let own = omacrt_shell::centring::game(&path).is_some();
                Some(format!("{}x{x:+} y{y:+}", if own { "game " } else { "" }))
            }
            PauseRow::Shader => {
                let shaders = crate::library::installed_shaders();
                Some(
                    shaders
                        .iter()
                        .find(|(n, _)| *n == system.shader)
                        .map(|(_, l)| (*l).to_string())
                        .unwrap_or_else(|| "off".to_string()),
                )
            }
            _ => None,
        }
    }

    /// Whether the system of the running game was launched with rewind on.
    fn running_rewinds(&self) -> bool {
        let Some((system, _)) = &self.running_path else {
            return false;
        };
        self.library
            .systems
            .iter()
            .find(|s| &s.name == system)
            .map(|s| s.rewind)
            .unwrap_or(false)
    }

    pub(super) fn draw_pause(&mut self, fb: &mut Framebuffer) {
        let sel = self.paused.unwrap_or(0);
        let title = match &self.running {
            Some((title, _)) => {
                let w = fb.w as i32;
                let max_cols = ((w - 2 * (w as f32 * 0.05) as i32) / 8) as usize;
                let room = max_cols.saturating_sub("Paused  ".len() + 6);
                let t: String = title.chars().take(room).collect();
                format!("Paused  {t}")
            }
            None => "Paused".to_string(),
        };
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32;
        let y0 = self.draw_header(fb, &title);
        // The console on its stage on the right, the game in it and its
        // lamp lit: what the menu is pausing, where the systems list showed
        // it before the game went in.
        let stage_w = 108;
        let sx = w - left - stage_w;
        let stage_h = (h - 34 - y0).max(80);
        let system = self.running_path.as_ref().map(|(s, _)| s.clone());
        let has_model = system.as_deref().is_some_and(crate::consoles::has);
        let width = if has_model {
            sx - left - 6
        } else {
            w - 2 * left
        };
        if let Some(system) = system.filter(|_| has_model) {
            let th = self.theme.clone();
            let brand = icons::system_logo(&system)
                .map(|(_, c)| c)
                .unwrap_or(th.accent);
            let (floor, br) = crate::stage::draw(fb, &th, sx, y0, stage_w, stage_h, brand);
            let game = self.running_path.clone();
            self.draw_console_scene(
                fb,
                &system,
                game,
                0.0,
                1.0,
                true,
                false,
                (floor, br, y0, sx + stage_w / 2, (stage_w - 18) as f32),
            );
            // At the stage's foot, what the row under the cursor is set to,
            // or else where the game can be resumed from.
            let chip = PAUSE_ROWS
                .get(sel)
                .and_then(|(row, _, _)| self.pause_value(*row))
                .map(|v| (v, th.cyan))
                .or_else(|| {
                    self.running_path
                        .as_ref()
                        .and_then(|(_, p)| self.states.latest(p))
                        .map(|st| (st.label(), th.green))
                });
            if let Some((text, c)) = chip {
                // A value too long for one chip goes on two, split at a word.
                let cols = ((stage_w - 10) / 8) as usize;
                let lines: Vec<String> = if text.chars().count() <= cols {
                    vec![text]
                } else {
                    let cut = text[..text.len().min(cols + 1)]
                        .rfind(' ')
                        .unwrap_or(cols.min(text.len()));
                    vec![
                        text[..cut].to_string(),
                        text[cut..].trim().chars().take(cols).collect(),
                    ]
                };
                let ground = lerp_color(th.bg, c, 0.25);
                for (k, line) in lines.iter().enumerate() {
                    let cw = Framebuffer::text_width(line, 1) + 6;
                    let up = (lines.len() - 1 - k) as i32 * 12;
                    let (x, y) = (sx + (stage_w - cw) / 2, y0 + stage_h - 13 - up);
                    fb.rect(x + 1, y, cw - 2, 11, ground);
                    fb.rect(x, y + 1, cw, 9, ground);
                    fb.text(x + 3, y + 2, line, c, 1);
                }
            }
        }
        // Fourteen pixels a row where they fit. A 224 line game pauses in a
        // 224 line frame, and eleven rows of fourteen would run into the line
        // a message is written on.
        let row_h = ((h - 30 - y0) / PAUSE_ROWS.len() as i32).clamp(11, 14);
        let band_y = self.band(y0 + sel as i32 * row_h);
        self.select_bar(fb, left, band_y, width, row_h - 1);
        for (i, (row, icon, label)) in PAUSE_ROWS.iter().enumerate() {
            let y = y0 + i as i32 * row_h;
            let room = ((width - 26) / 8).max(0) as usize;
            let label: String = label.chars().take(room).collect();
            self.draw_menu_row(fb, left, y, width, icon, &label, false, i == sel, 1.0);
            // A choice says what it is set to on its own right, when there
            // is no stage to say it for the row under the cursor.
            if let Some(value) = self.pause_value(*row).filter(|_| !has_model) {
                let room =
                    ((width - 30 - 18 - Framebuffer::text_width(&label, 1)) / 8).max(0) as usize;
                let text: String = value.chars().take(room).collect();
                let tx = left + width - 8 - Framebuffer::text_width(&text, 1);
                if i == sel {
                    let shadow = crate::paint::Tones::of(&self.theme).shadow;
                    crate::paint::text_shadow(fb, tx, y + 2, &text, self.theme.fg, shadow);
                } else {
                    fb.text(tx, y + 2, &text, self.theme.dim, 1);
                }
            }
        }
        let max_cols = (width / 8) as usize;
        if let Some((msg, _)) = &self.message {
            let m: String = msg.chars().take(max_cols).collect();
            self.draw_message(fb, left, h - 28, &m);
        }
        // A choice is changed, not selected: the hint says so on its row.
        let hint: &[(&str, &str)] = match PAUSE_ROWS.get(sel).map(|(row, _, _)| *row) {
            Some(PauseRow::Aspect) | Some(PauseRow::Shader) => &[("A", "change"), ("B", "back")],
            Some(PauseRow::Centre) => &[("A", "open"), ("B", "back")],
            _ => &[("A", "select"), ("B", "back")],
        };
        self.draw_hint(fb, left, h - 14, hint);
    }

    pub(super) fn draw_running(&mut self, fb: &mut Framebuffer) {
        let Some((title, system)) = self.running.clone() else {
            return;
        };
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32;
        let y0 = self.draw_header(fb, &format!("Playing {system}"));
        let max_cols = ((w - 2 * left) / 8) as usize;
        let title: String = title.chars().take(max_cols).collect();
        fb.text(left, y0 + 8, &title, self.theme.bright_green, 1);
        let dots = ((self.now * 2.0) as usize) % 4;
        fb.text(
            left,
            y0 + 24,
            &format!("running{}", ".".repeat(dots)),
            self.theme.dim,
            1,
        );
        fb.text(
            left,
            h - 16,
            "select+start or esc to come back",
            scale(self.theme.dim, 0.7),
            1,
        );
    }
}

/// Where a shoulder press takes the selection, or nothing when it is already
/// there. Backward goes to the first row, forward to the last.
impl Scene {
    /// Open the centring page for the running game, at the level the game
    /// is placed by now: its own when it has one, its system's otherwise.
    fn open_centring(&mut self) {
        let Some((system, path)) = self.running_path.clone() else {
            return;
        };
        let sys = self
            .library
            .systems
            .iter()
            .find(|s| s.name == system)
            .map(|s| (s.shift_x, s.shift_y))
            .unwrap_or((0, 0));
        let game = omacrt_shell::centring::game(&path);
        let profile = (self.profile.h_shift, self.profile.v_shift);
        let (x, y) = game.unwrap_or(sys);
        self.centring = Some(Centring {
            scope: if game.is_some() { 0 } else { 1 },
            x,
            y,
            profile,
            system: sys,
            game,
        });
        self.pending.push(Sound::Select);
    }

    /// What the centring page's level is set to, as saved when it opened.
    fn centring_saved(c: &Centring, scope: usize) -> (i32, i32) {
        match scope {
            0 => c.game.unwrap_or(c.system),
            1 => c.system,
            _ => c.profile,
        }
    }

    fn centring_input(&mut self, nav: Option<Nav>, fire: bool, jump: i32) {
        let Some(mut c) = self.centring.clone() else {
            return;
        };
        if nav == Some(Nav::Back) {
            self.close_centring(&c, false);
            return;
        }
        if fire {
            self.close_centring(&c, true);
            return;
        }
        let lim = omacrt_shell::centring::LIMIT;
        let mut moved = true;
        match nav {
            Some(Nav::Left) => c.x = (c.x - 1).max(-lim),
            Some(Nav::Right) => c.x = (c.x + 1).min(lim),
            Some(Nav::Up) => c.y = (c.y - 1).max(-lim),
            Some(Nav::Down) => c.y = (c.y + 1).min(lim),
            _ => moved = false,
        }
        if jump != 0 {
            // A level changed without saving is put back: the picture shows
            // what the new level holds, so what is on the tube is always what
            // the numbers say.
            c.scope = (c.scope as i32 + jump).rem_euclid(3) as usize;
            (c.x, c.y) = Self::centring_saved(&c, c.scope);
            self.profile.h_shift = c.profile.0;
            self.profile.v_shift = c.profile.1;
            moved = true;
        }
        if moved {
            self.pending.push(Sound::Move);
            self.centring_show(&c);
            self.centring = Some(c);
        }
    }

    /// Put the page's picture on the tube: the level being set at its new
    /// place, every other level as it is saved.
    fn centring_show(&mut self, c: &Centring) {
        let shift = match c.scope {
            0 | 1 => (c.x, c.y),
            _ => {
                // The television's own level lives in the TV profile, which
                // the mode command reads from disk.
                self.profile.h_shift = c.x;
                self.profile.v_shift = c.y;
                self.save_profile();
                c.game.unwrap_or(c.system)
            }
        };
        self.shift_request = Some(shift);
    }

    /// Leave the page, keeping what it set or putting every level back.
    fn close_centring(&mut self, c: &Centring, keep: bool) {
        self.centring = None;
        let Some((system, path)) = self.running_path.clone() else {
            return;
        };
        if !keep {
            self.profile.h_shift = c.profile.0;
            self.profile.v_shift = c.profile.1;
            self.save_profile();
            self.shift_request = Some(c.game.unwrap_or(c.system));
            self.pending.push(Sound::Select);
            return;
        }
        let saved = match c.scope {
            0 => omacrt_shell::centring::set_game(&path, Some((c.x, c.y)))
                .map(|_| "centred for this game".to_string())
                .map_err(|e| e.to_string()),
            1 => {
                // A system saved from a game's page is what that game is to
                // follow from now on, so the game gives up its own place.
                let r = crate::library::set_system_field(&system, "shift_x", &c.x.to_string())
                    .and_then(|_| {
                        crate::library::set_system_field(&system, "shift_y", &c.y.to_string())
                    })
                    .and_then(|_| {
                        omacrt_shell::centring::set_game(&path, None).map_err(|e| e.to_string())
                    });
                if r.is_ok()
                    && let Some(s) = self.library.systems.iter_mut().find(|s| s.name == system)
                {
                    s.shift_x = c.x;
                    s.shift_y = c.y;
                }
                r.map(|_| format!("centred for every {system} game"))
            }
            _ => Ok("centred for every game".to_string()),
        };
        match saved {
            Ok(said) => {
                self.pending.push(Sound::Lock);
                self.message = Some((said, self.now + 3.0));
            }
            Err(e) => {
                self.pending.push(Sound::Crunch);
                self.message = Some((format!("cannot save: {e}"), self.now + 4.0));
            }
        }
    }

    /// The centring page: the edges of the frame the tube has, nested a few
    /// pixels apart so how much the set hides on each side can be counted,
    /// ticks along the edges, the middle marked, and one line at the foot.
    /// Nothing in the middle of the screen, because the edges are what is
    /// being looked at.
    pub(super) fn draw_centring(&mut self, fb: &mut Framebuffer) {
        let Some(c) = self.centring.clone() else {
            return;
        };
        let th = self.theme.clone();
        let (w, h) = (fb.w as i32, fb.h as i32);
        fb.clear(0x000000);
        let edge = |fb: &mut Framebuffer, s: i32, col: Color| {
            fb.rect(s, s, w - 2 * s, 1, col);
            fb.rect(s, h - 1 - s, w - 2 * s, 1, col);
            fb.rect(s, s, 1, h - 2 * s, col);
            fb.rect(w - 1 - s, s, 1, h - 2 * s, col);
        };
        edge(fb, 0, th.yellow);
        edge(fb, 4, th.dim);
        edge(fb, 8, th.dim);
        for x in (16..w).step_by(16) {
            let len = if x % 32 == 0 { 5 } else { 3 };
            fb.rect(x, 0, 1, len, th.fg);
            fb.rect(x, h - len, 1, len, th.fg);
        }
        for y in (16..h).step_by(16) {
            let len = if y % 32 == 0 { 5 } else { 3 };
            fb.rect(0, y, len, 1, th.fg);
            fb.rect(w - len, y, len, 1, th.fg);
        }
        let n = 14;
        for (x, y, dx, dy) in [
            (0, 0, 1, 1),
            (w - 1, 0, -1, 1),
            (0, h - 1, 1, -1),
            (w - 1, h - 1, -1, -1),
        ] {
            for i in 0..n {
                for k in 0..2 {
                    fb.put(x + dx * i, y + dy * k, th.green);
                    fb.put(x + dx * k, y + dy * i, th.green);
                }
            }
        }
        fb.rect(w / 2 - 10, h / 2, 21, 1, th.dim);
        fb.rect(w / 2, h / 2 - 10, 1, 21, th.dim);
        // The line at the foot: the game, where it is, which level, keys.
        let (bx, by, bw, bh) = (16, h - 44, w - 32, 30);
        fb.rect(bx, by, bw, bh, th.bg);
        fb.rect(bx, by, bw, 1, th.selection);
        fb.rect(bx, by + bh - 1, bw, 1, th.selection);
        fb.rect(bx, by, 1, bh, th.selection);
        fb.rect(bx + bw - 1, by, 1, bh, th.selection);
        let title = self
            .running
            .as_ref()
            .map(|(t, _)| t.clone())
            .unwrap_or_default();
        let room = ((bw - 12) / 8) as usize;
        let t: String = title.chars().take(room.saturating_sub(10)).collect();
        fb.text(bx + 6, by + 4, &t, th.paper, 1);
        let place = format!("x{:+} y{:+}", c.x, c.y);
        fb.text(
            bx + bw - 6 - Framebuffer::text_width(&place, 1),
            by + 4,
            &place,
            th.green,
            1,
        );
        let system = self
            .running_path
            .as_ref()
            .map(|(s, _)| s.clone())
            .unwrap_or_default();
        let mut x = bx + 6;
        for (i, label) in ["game", system.as_str(), "all"].iter().enumerate() {
            let cw = Framebuffer::text_width(label, 1) + 6;
            if i == c.scope {
                fb.rect(x, by + 15, cw, 11, th.selection);
                fb.rect(x, by + 25, cw, 1, th.accent);
                fb.text(x + 3, by + 17, label, th.paper, 1);
            } else {
                fb.text(x + 3, by + 17, label, th.dim, 1);
            }
            x += cw + 4;
        }
        // Measured first, because the keys are named for the device in use
        // (Enter on a keyboard, A on a pad) and their width changes with it.
        let keys: &[(&str, &str)] = &[("A", "keep"), ("B", "undo")];
        let mut scratch = Framebuffer::new(fb.w, 12);
        let hint_w = self.draw_hint(&mut scratch, 0, 2, keys) - 10;
        self.draw_hint(fb, bx + bw - 6 - hint_w, by + 17, keys);
    }
}

fn pause_jump(sel: usize, jump: i32, rows: usize) -> Option<usize> {
    if jump == 0 || rows == 0 {
        return None;
    }
    let to = if jump < 0 { 0 } else { rows - 1 };
    (to != sel).then_some(to)
}

/// The next name in a list after the one held now, wrapping either way. An
/// empty or unknown name starts from the first.
fn step<'a>(names: &[&'a str], now: &str, dir: i32) -> &'a str {
    if names.is_empty() {
        return "";
    }
    let at = names.iter().position(|n| *n == now).unwrap_or(0) as i32;
    names[(at + dir).rem_euclid(names.len() as i32) as usize]
}

#[cfg(test)]
mod tests {
    use super::{pause_jump, step};

    #[test]
    fn the_shoulders_go_to_the_ends_of_the_pause_menu() {
        assert_eq!(pause_jump(4, -1, 10), Some(0));
        assert_eq!(pause_jump(4, 1, 10), Some(9));
        // Already there, and no press at all, both move nothing: a menu that
        // plays its move sound without moving is a menu that feels broken.
        assert_eq!(pause_jump(0, -1, 10), None);
        assert_eq!(pause_jump(9, 1, 10), None);
        assert_eq!(pause_jump(4, 0, 10), None);
        assert_eq!(pause_jump(0, 1, 0), None);
    }

    #[test]
    fn a_choice_cycles_both_ways_and_wraps() {
        let names = ["fill", "core", "pixel"];
        assert_eq!(step(&names, "fill", 1), "core");
        assert_eq!(step(&names, "pixel", 1), "fill");
        assert_eq!(step(&names, "fill", -1), "pixel");
        // Nothing set yet, and a value from a newer version of the launcher,
        // both start from the first choice.
        assert_eq!(step(&names, "", 1), "core");
        assert_eq!(step(&names, "something else", 1), "core");
        assert_eq!(step(&[], "fill", 1), "");
    }
}
