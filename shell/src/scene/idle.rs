//! What an idle television shows, and which page has the screen.
//!
//! Four pages take turns: the wordmark under a text effect, the photographs,
//! the weather, and the monitor. The rules for who wins are in
//! `idle_reached`, and they are the ones worth knowing.

use super::*;

impl Scene {
    pub fn start_screensaver(&mut self, now: f64, kind: Option<Kind>) {
        let kind = kind
            .unwrap_or_else(|| effects::ALL[(self.rand() % effects::ALL.len() as u32) as usize]);
        let seed = (now * 997.0) as u32 ^ self.rand();
        let effect = Effect::new(kind, seed, self.stops(), self.palette());
        self.saver = Some(Saver {
            effect,
            started: now,
        });
    }

    /// Idle seconds before the screensaver, from settings (0 disables).
    pub(super) fn idle_limit(&self) -> f32 {
        if !self.settings.screensaver.enabled {
            return 0.0;
        }
        if self.idle_secs > 0.0 && self.settings.screensaver.idle_secs == 60 {
            // Command line override while the setting is at its default.
            return self.idle_secs;
        }
        self.settings.screensaver.idle_secs as f32
    }

    fn chosen_effect(&self) -> Option<Kind> {
        effects::ALL
            .iter()
            .copied()
            .find(|k| k.name() == self.settings.screensaver.effect)
    }

    /// What an idle television does.
    ///
    /// Music playing wins: the visualizer has something to show that the
    /// other pages do not. Otherwise the screensaver setting decides, and it
    /// there is more than one, they take turns.
    pub(super) fn idle_reached(&mut self, now: f64) {
        if self.music.status.playing() && self.settings.music.saver {
            self.music_saver_start(now);
            return;
        }
        if self.saver_run.is_some() {
            return;
        }
        // A page that draws itself is already a screensaver, and one you
        // opened on purpose is the one you want up: putting another over it
        // is an error of category, whether or not it is in the rotation.
        // Somewhere static, a list of games, is what a screensaver is for.
        if self.on_saver_page() {
            return;
        }
        // Nothing in the rotation leaves the wordmark, which is still better
        // than a lit screen showing the menu all night.
        let page = self.next_saver_page(None).unwrap_or_default();
        self.start_saver_page(page, now);
    }

    /// Is the screen already one of the pages an idle television shows?
    fn on_saver_page(&self) -> bool {
        matches!(
            self.screen,
            Screen::Frame | Screen::Ambient | Screen::Monitor { .. }
        )
    }

    /// Which pages are in the rotation, as indices into `settings::PAGES`.
    fn saver_rotation(&self) -> Vec<usize> {
        let sv = &self.settings.screensaver;
        omacrt_shell::settings::PAGES
            .iter()
            .enumerate()
            .filter(|(_, p)| sv.shows(p))
            .map(|(i, _)| i)
            .collect()
    }

    /// The next page in the rotation after `from`. `None` when it is empty.
    fn next_saver_page(&mut self, from: Option<usize>) -> Option<usize> {
        let on = self.saver_rotation();
        if on.is_empty() {
            return None;
        }
        match from {
            // The first page of an evening is any of them, so a television
            // left alone twice does not open the same way twice.
            None => Some(on[(self.rand() as usize) % on.len()]),
            Some(current) => {
                let at = on.iter().position(|i| *i == current);
                Some(match at {
                    // After that they go round in the order they are drawn
                    // in, whatever order they were switched on in.
                    Some(i) => on[(i + 1) % on.len()],
                    None => on[0],
                })
            }
        }
    }

    /// Put one screensaver page up.
    fn start_saver_page(&mut self, page: usize, now: f64) {
        // Where to come back to, taken before the page changes the screen.
        let back = match self.saver_run {
            Some((_, _, was)) => was,
            None => self.screen,
        };
        self.saver_run = Some((page, now, back));
        // The idle clock starts again: the page is what idling looks like,
        // and the check must not fire on every frame from here on.
        self.last_input = now;
        match omacrt_shell::settings::PAGES.get(page).copied() {
            Some("photos") => self.open_frame(),
            Some("ambient") => self.go(Screen::Ambient),
            Some("system") => {
                self.sysmon.sample();
                self.go(Screen::Monitor { page: 0 });
            }
            _ => {
                let kind = self.chosen_effect();
                self.start_screensaver(now, kind);
            }
        }
    }

    /// Turn the page when its time is up.
    pub(super) fn cycle_saver_page(&mut self, now: f64) {
        let Some((page, since, back)) = self.saver_run else {
            return;
        };
        let sv = &self.settings.screensaver;
        // One page on its own stays up, and so does any page when the time
        // is set to nothing.
        if sv.cycle_secs == 0 || sv.pages.len() < 2 {
            return;
        }
        if now - since < sv.cycle_secs as f64 {
            return;
        }
        let next = self.next_saver_page(Some(page)).unwrap_or(page);
        if next == page {
            // The only page that is on: leave it up rather than restarting it.
            self.saver_run = Some((page, now, back));
            return;
        }
        self.saver = None;
        self.start_saver_page(next, now);
    }

    pub(super) fn draw_saver(&mut self, fb: &mut Framebuffer) {
        let (w, h) = (fb.w as i32, fb.h as i32);
        let now = self.now;
        let (mw, mh) = (self.mark_cols * MARK_SCALE, self.mark_rows * 2 * MARK_SCALE);
        let mut restart = false;
        if let Some(saver) = self.saver.as_mut() {
            let t = (now - saver.started) as f32;
            saver.effect.advance_to(t);
            let x = (w - mw) / 2;
            let y = (h - mh) / 2 - 8;
            saver.effect.draw(fb, x, y, MARK_SCALE, t, 1.0);
            // Hold the finished picture for a while, then move on to another effect.
            restart = t > saver.effect.length() + 3.0;
        }
        if restart {
            // The effect that was asked for, not any effect: `random` is the
            // value that means any.
            let kind = self.chosen_effect();
            self.start_screensaver(now, kind);
        }
    }
}
