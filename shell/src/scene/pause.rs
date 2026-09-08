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
        match omarchy_crt_shell::game::pause_toggle() {
            Ok(()) => {
                self.paused = Some(0);
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
        let _ = omarchy_crt_shell::game::pause_toggle();
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
        match nav {
            Some(Nav::Up) if sel > 0 => {
                self.paused = Some(sel - 1);
                self.pending.push(Sound::Move);
            }
            Some(Nav::Down) if sel + 1 < PAUSE_ITEMS.len() => {
                self.paused = Some(sel + 1);
                self.pending.push(Sound::Move);
            }
            Some(Nav::Back) => return self.resume_game(),
            _ => {}
        }
        if !fire {
            return PauseOutcome::None;
        }
        match sel {
            0 => self.resume_game(),
            1 => {
                self.game_cmd(omarchy_crt_shell::game::save_state(), "state saved");
                if let Some((_, path)) = &self.running_path {
                    let path = path.clone();
                    self.states.forget(&path);
                }
                PauseOutcome::None
            }
            2 => {
                self.game_cmd(omarchy_crt_shell::game::load_state(), "state loaded");
                PauseOutcome::None
            }
            3 => {
                // Rewind runs only where the system allows it: the launch
                // override writes rewind_enable from that flag.
                if !self.running_rewinds() {
                    self.pending.push(Sound::Crunch);
                    self.message = Some(("rewind is off for this system".into(), self.now + 3.0));
                    return PauseOutcome::None;
                }
                let _ = omarchy_crt_shell::game::rewind();
                self.message = Some(("rewinding".into(), self.now + 2.5));
                self.resume_game()
            }
            4 => {
                // Toggle fast forward and let the game run: RetroArch keeps
                // the speed until the next toggle from the same menu.
                let _ = omarchy_crt_shell::game::fast_forward();
                self.message = Some(("fast forward toggled".into(), self.now + 2.5));
                self.resume_game()
            }
            5 => {
                let _ = omarchy_crt_shell::game::slow_motion();
                self.message = Some(("slow motion toggled".into(), self.now + 2.5));
                self.resume_game()
            }
            6 => {
                let _ = omarchy_crt_shell::game::reset();
                self.resume_game()
            }
            _ => {
                // Escape quits RetroArch (quit_press_twice is off).
                let _ = omarchy_crt_shell::game::quit();
                self.paused = None;
                self.pending.push(Sound::Select);
                PauseOutcome::Quit
            }
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
        self.draw_menu_screen(fb, &title, &PAUSE_ITEMS, sel);
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
