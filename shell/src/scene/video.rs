//! Films, YouTube and the player's own overlay, with the settings that
//! decide how modern video is fitted to a 4:3 tube.

use super::*;

impl Scene {
    pub(super) fn adjust_videos(&mut self, row: usize, dir: i32) {
        fn step(cur: u32, opts: &[u32], dir: i32) -> u32 {
            let i = opts.iter().position(|o| *o == cur).unwrap_or(0) as i32;
            opts[(i + dir).rem_euclid(opts.len() as i32) as usize]
        }
        let v = &mut self.settings.videos;
        match row {
            0 => v.yt_quality = step(v.yt_quality, &[360, 480, 720, 1080], dir),
            1 => v.yt_results = step(v.yt_results, &[10, 20, 40], dir),
            _ => {}
        }
    }

    /// Video fit rows: standard, film 24, aspect, overscan, retro 240p.
    pub(super) fn adjust_fit(&mut self, row: usize, dir: i32) {
        fn cycle(cur: &str, opts: &[&str], dir: i32) -> String {
            let i = opts.iter().position(|o| *o == cur).unwrap_or(0) as i32;
            opts[(i + dir).rem_euclid(opts.len() as i32) as usize].to_string()
        }
        let v = &mut self.settings.video;
        match row {
            0 => v.standard = cycle(&v.standard, &["auto", "ntsc", "pal"], dir),
            1 => v.film24 = cycle(&v.film24, &["pulldown", "speedup"], dir),
            2 => v.aspect = cycle(&v.aspect, &["letterbox", "crop", "anamorphic"], dir),
            3 => v.overscan = !v.overscan,
            4 => v.retro_240p = !v.retro_240p,
            5 => v.monitor_in_games = !v.monitor_in_games,
            _ => {}
        }
    }

    /// The X button: pause or play on the music screens, otherwise start
    /// converting the selected video for the CRT.
    pub fn convert_selected(&mut self) {
        match self.screen {
            Screen::Pair { .. } => {
                self.remap_request = true;
                return;
            }
            Screen::NowPlaying => {
                // The visualizer in and out; the shoulders pick the deck's look.
                self.music_visual = !self.music_visual;
                self.deck.mode_since = self.now;
                self.pending.push(Sound::Whoosh);
                return;
            }
            Screen::Music { .. } => {
                self.music_alt(None);
                return;
            }
            Screen::MusicList { sel, .. } => {
                self.music_alt(Some(sel));
                return;
            }
            _ => {}
        }
        let Screen::Games { sel, .. } = self.screen else {
            return;
        };
        let Some(entry) = self.games.get(sel).cloned() else {
            return;
        };
        if !self.library.systems[entry.sys].is_video() {
            // Games: the cover flow, in and out.
            self.flow_view = !self.flow_view;
            self.flow_pos = sel as f32;
            self.pending.push(Sound::Whoosh);
            return;
        }
        if self.conversion.is_some() {
            self.message = Some(("a conversion is already running".into(), self.now + 3.0));
            return;
        }
        if entry.game.crt_path.is_some() {
            self.message = Some(("already CRT ready".into(), self.now + 3.0));
            return;
        }
        match Conversion::start(
            &entry.game.path,
            &entry.game.title,
            &self.settings.video,
            &self.library.config_dir,
        ) {
            Ok(c) => {
                self.pending.push(Sound::Select);
                self.message = Some((format!("converting {}", c.title), self.now + 3.0));
                self.conversion = Some(c);
            }
            Err(e) => self.message = Some((format!("ffmpeg: {e}"), self.now + 4.0)),
        }
    }

    /// Advance a running conversion; refresh the list when it finishes.
    pub(super) fn tick_conversion(&mut self) {
        let Some(c) = self.conversion.as_mut() else {
            return;
        };
        if let Some(ok) = c.poll() {
            let title = c.title.clone();
            self.conversion = None;
            self.pending
                .push(if ok { Sound::Lock } else { Sound::Crunch });
            self.message = Some((
                if ok {
                    format!("{title} is CRT ready")
                } else {
                    format!("conversion of {title} failed")
                },
                self.now + 4.0,
            ));
            if let Screen::Games { sys: Some(i), .. } = self.screen {
                let sel_keep = match self.screen {
                    Screen::Games { sel, .. } => sel,
                    _ => 0,
                };
                self.set_games(self.entries_for(i));
                if let Screen::Games { sel, .. } = &mut self.screen {
                    *sel = sel_keep.min(self.games.len().saturating_sub(1));
                }
            }
        }
    }

    pub(super) fn video_system(&self) -> Option<usize> {
        self.library.systems.iter().position(|s| s.is_video())
    }

    /// Y on a link: in or out of the watch later list.
    pub(super) fn toggle_watch_later(&mut self, entry: &Entry) {
        let path = self.library.config_dir.join("watch-later.tsv");
        let target = entry.game.path.to_string_lossy().to_string();
        let mut lines: Vec<String> = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .map(|l| l.to_string())
            .collect();
        let before = lines.len();
        lines.retain(|l| l.split('\t').next() != Some(target.as_str()));
        let kept = if lines.len() == before {
            lines.push(format!("{target}\t{}", entry.game.title));
            true
        } else {
            false
        };
        if let Err(e) = std::fs::write(&path, lines.join("\n") + "\n") {
            self.message = Some((format!("watch later: {e}"), self.now + 3.0));
            return;
        }
        self.message = Some((
            if kept {
                format!("watch later: {}", entry.game.title)
            } else {
                format!("removed {}", entry.game.title)
            },
            self.now + 2.5,
        ));
        self.pending.push(Sound::Select);
    }

    /// Enter on the search bar while it asks for a query: run the search
    /// and show the hits as the list.
    pub(super) fn yt_submit(&mut self) {
        let q = self.search.clone().unwrap_or_default();
        if q.trim().is_empty() {
            return;
        }
        let n = self.settings.videos.yt_results as usize;
        self.yt_search = Some(yt::search(&q, n));
        self.message = Some((format!("searching YouTube for {q}"), self.now + 8.0));
        self.pending.push(Sound::Select);
    }

    /// The search answered: hits become the list, still under Videos.
    pub(super) fn yt_poll(&mut self) {
        let Some(rx) = &self.yt_search else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(hits)) => {
                self.yt_search = None;
                let Screen::Games { sys: Some(i), .. } = self.screen else {
                    return;
                };
                let entries: Vec<Entry> = hits
                    .into_iter()
                    .map(|h| Entry {
                        game: Game {
                            title: if h.channel.is_empty() {
                                h.title
                            } else {
                                format!("{}  ({})", h.title, h.channel)
                            },
                            path: PathBuf::from(h.url),
                            crt_path: None,
                            folder: false,
                        },
                        sys: i,
                    })
                    .collect();
                let n = entries.len();
                let back = self.games_back;
                self.set_games(entries);
                self.games_back = back;
                self.yt_query = false;
                self.yt_results = true;
                self.message = Some((format!("{n} videos"), self.now + 3.0));
                self.pending.push(Sound::Lock);
            }
            Ok(Err(e)) => {
                self.yt_search = None;
                self.yt_query = false;
                self.message = Some((format!("YouTube: {e}"), self.now + 5.0));
                self.pending.push(Sound::Crunch);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => self.yt_search = None,
        }
    }

    /// Play a video file or URL through the Videos system, from the control
    /// pipe (`omarchy-crt watch`). A URL goes to mpv as it is; yt-dlp
    /// resolves it.
    pub fn watch(&mut self, target: &str) {
        if !self.menu_live || self.running.is_some() || self.launching.is_some() {
            self.message = Some(("busy: cannot start a video now".into(), self.now + 3.0));
            return;
        }
        let Some(sys) = self.library.systems.iter().position(|s| s.is_video()) else {
            self.message = Some(("no video system in systems.toml".into(), self.now + 4.0));
            return;
        };
        let entry = Entry {
            game: Game {
                title: watch_title(target, ""),
                path: PathBuf::from(target),
                crt_path: None,
                folder: false,
            },
            sys,
        };
        let _ = self.run_entry(&entry);
    }

    /// Entries kept with `omarchy-crt watch --later`, for the top of Videos.
    pub(super) fn watch_later(&self, sys: usize) -> Vec<Entry> {
        let path = self.library.config_dir.join("watch-later.tsv");
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let mut parts = l.split('\t');
                let target = parts.next()?.trim();
                if target.is_empty() {
                    return None;
                }
                let title = parts.next().unwrap_or("").trim();
                Some(Entry {
                    game: Game {
                        title: watch_title(target, title),
                        path: PathBuf::from(target),
                        crt_path: None,
                        folder: false,
                    },
                    sys,
                })
            })
            .collect()
    }

    pub fn player_active(&self) -> bool {
        self.player.is_some()
    }

    /// Pad input while a video plays: pause, seek, volume, stop.
    pub fn player_input(&mut self, nav: Option<Nav>, fire: bool) {
        let Some(p) = self.player.as_mut() else {
            return;
        };
        match nav {
            Some(Nav::Left) => p.seek(-10),
            Some(Nav::Right) => p.seek(10),
            Some(Nav::Up) => p.volume(5),
            Some(Nav::Down) => p.volume(-5),
            Some(Nav::Back) => p.quit(),
            None if fire => p.toggle_pause(),
            _ => {}
        }
        self.pending.push(Sound::Move);
    }

    /// Video overlay: title, progress bar, times, pause state. mpv draws the
    /// picture in its own fullscreen window; this is what the shell shows
    /// underneath and on a second output.
    pub(super) fn draw_player(&mut self, fb: &mut Framebuffer) {
        let now = self.now;
        let Some(p) = self.player.as_mut() else {
            return;
        };
        p.poll(now);
        let (time, duration, paused, connected) = (p.time, p.duration, p.paused, p.connected);
        let title = p.title.clone();
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32;
        let width = w - 2 * left;
        let y0 = self.draw_header(fb, "Playing videos");
        let max_cols = (width / 8) as usize;
        fb.text(
            left,
            y0 + 8,
            &title.chars().take(max_cols).collect::<String>(),
            self.theme.bright_green,
            1,
        );
        let bar_y = y0 + 40;
        fb.rect(left, bar_y, width, 6, self.theme.selection);
        if duration > 0.0 {
            let filled = ((time / duration).clamp(0.0, 1.0) * width as f64) as i32;
            fb.rect(left, bar_y, filled, 6, self.theme.accent);
        }
        let times = format!(
            "{} / {}",
            crate::player::clock(time),
            crate::player::clock(duration)
        );
        fb.text(left, bar_y + 12, &times, self.theme.paper, 1);
        let state = if !connected {
            "starting mpv"
        } else if paused {
            "paused"
        } else {
            "playing"
        };
        fb.text(
            w - left - Framebuffer::text_width(state, 1),
            bar_y + 12,
            state,
            self.theme.dim,
            1,
        );
        if paused {
            fb.rect(w / 2 - 8, h / 2 + 10, 5, 16, self.theme.paper);
            fb.rect(w / 2 + 3, h / 2 + 10, 5, 16, self.theme.paper);
        }
        let hint = self.hint(&[
            ("A", "pause"),
            ("<>", "seek"),
            ("^v", "volume"),
            ("B", "stop"),
        ]);
        fb.text(left, h - 14, &hint, scale(self.theme.dim, 0.7), 1);
    }

    pub(super) fn draw_video_settings(&mut self, fb: &mut Framebuffer, sel: usize) {
        let v = self.settings.videos.clone();
        let rows: Vec<(String, String)> = vec![
            ("youtube quality".into(), format!("{}p", v.yt_quality)),
            ("youtube results".into(), format!("{}", v.yt_results)),
            ("video fit".into(), "open".into()),
        ];
        let notes = [
            "the tube shows 240 lines; 480p decodes cool, 1080p heats the room",
            "hits per search from the tube",
            "standard, pulldown, aspect, overscan, retro 240p",
        ];
        self.draw_settings_table(fb, "Videos", &rows, &notes, sel, 14);
    }

    pub(super) fn draw_video_fit(&mut self, fb: &mut Framebuffer, sel: usize) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32 + self.slide();
        let width = w - 2 * (w as f32 * 0.05) as i32;
        let y0 = self.draw_header(fb, "Video fit");
        let v = self.settings.video.clone();
        let rows: [(&str, String); FIT_ROWS] = [
            ("standard", v.standard.clone()),
            ("film 24 fps", v.film24.clone()),
            ("16:9 to 4:3", v.aspect.clone()),
            (
                "overscan 5%",
                if v.overscan {
                    "on".into()
                } else {
                    "off".into()
                },
            ),
            (
                "retro 240p",
                if v.retro_240p {
                    "on".into()
                } else {
                    "off".into()
                },
            ),
            (
                "preview in games",
                if v.monitor_in_games {
                    "on".into()
                } else {
                    "off".into()
                },
            ),
        ];
        let row_h = 14;
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
        let notes: [&str; FIT_ROWS] = [
            "auto: 25/50 fps -> 576i, else 480i",
            "3:2 pulldown at 59.94, or PAL +4%",
            "black bars, center crop, or squeeze",
            "keeps titles inside the safe area",
            "4:3 sources back to 320x240",
            "keep the desktop preview window up while playing",
        ];
        let max_cols = (width / 8) as usize;
        fb.text(
            left,
            h - 40,
            &notes[sel].chars().take(max_cols).collect::<String>(),
            scale(self.theme.dim, 0.8),
            1,
        );
        fb.text(
            left,
            h - 28,
            &"live in mpv; X on a video converts it"
                .chars()
                .take(max_cols)
                .collect::<String>(),
            scale(self.theme.dim, 0.7),
            1,
        );
        let hint = self.hint(&[("<>", "change"), ("B", "back saves")]);
        fb.text(left, h - 14, &hint, scale(self.theme.dim, 0.7), 1);
    }
}
