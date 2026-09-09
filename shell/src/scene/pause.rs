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
    pub fn pause_input(&mut self, nav: Option<Nav>, fire: bool) -> PauseOutcome {
        let Some(sel) = self.paused else {
            return PauseOutcome::None;
        };
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
            PauseRow::Reset => {
                let _ = omacrt_shell::game::reset();
                self.resume_game()
            }
            PauseRow::Quit => {
                // Escape quits RetroArch (quit_press_twice is off).
                let _ = omacrt_shell::game::quit();
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
        let width = w - 2 * (w as f32 * 0.05) as i32;
        let y0 = self.draw_header(fb, &title);
        let row_h = 14;
        fb.rect(
            left,
            self.band(y0 + sel as i32 * row_h),
            width,
            row_h - 1,
            self.theme.selection,
        );
        for (i, (row, icon, label)) in PAUSE_ROWS.iter().enumerate() {
            let y = y0 + i as i32 * row_h;
            self.draw_menu_row(fb, left, y, width, icon, label, false, i == sel, 1.0);
            // A choice says what it is set to, on its own right, dim.
            if let Some(value) = self.pause_value(*row) {
                let room =
                    ((width - 30 - 18 - Framebuffer::text_width(label, 1)) / 8).max(0) as usize;
                let text: String = value.chars().take(room).collect();
                let tx = left + width - 8 - Framebuffer::text_width(&text, 1);
                fb.text(tx, y + 2, &text, scale(self.theme.dim, 1.0), 1);
            }
        }
        let max_cols = (width / 8) as usize;
        if let Some((msg, _)) = &self.message {
            let m: String = msg.chars().take(max_cols).collect();
            fb.text(left, h - 28, &m, self.theme.cyan, 1);
        }
        // A choice is changed, not selected: the hint says so on its row.
        let hint = match PAUSE_ROWS.get(sel).map(|(row, _, _)| *row) {
            Some(PauseRow::Aspect) | Some(PauseRow::Shader) => {
                self.hint(&[("A", "change"), ("B", "back")])
            }
            _ => self.hint(&[("A", "select"), ("B", "back")]),
        };
        fb.text(left, h - 14, &hint, scale(self.theme.dim, 0.7), 1);
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
    use super::step;

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
