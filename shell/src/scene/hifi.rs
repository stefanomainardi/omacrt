//! The music screens, on top of cliamp: the sources, a list, the deck and
//! the visualizers, the equaliser, and what the tuner does with left and
//! right.

use super::*;

impl Scene {
    /// Music settings rows: how the deck and the visualizers behave.
    pub(super) fn adjust_music(&mut self, row: usize, dir: i32) {
        fn step(cur: u32, opts: &[u32], dir: i32) -> u32 {
            let i = opts.iter().position(|o| *o == cur).unwrap_or(0) as i32;
            opts[(i + dir).rem_euclid(opts.len() as i32) as usize]
        }
        let m = &mut self.settings.music;
        match row {
            0 => m.idle_secs = step(m.idle_secs, &[0, 3, 6, 10, 20, 60], dir),
            1 => m.cycle_secs = step(m.cycle_secs, &[0, 30, 45, 90, 180], dir),
            2 => m.saver = !m.saver,
            3 => m.lyrics = !m.lyrics,
            4 => {
                let looks = ["auto", "cassette", "turntable"];
                let i = looks.iter().position(|l| *l == m.look).unwrap_or(0) as i32;
                m.look = looks[(i + dir).rem_euclid(3) as usize].to_string();
            }
            5 => {
                let i = COUNTRIES.iter().position(|c| *c == m.country).unwrap_or(0) as i32;
                m.country =
                    COUNTRIES[(i + dir).rem_euclid(COUNTRIES.len() as i32) as usize].to_string();
                self.art =
                    crate::art::Art::new(crate::covers::regions_for(&self.settings.music.country));
            }
            6 => m.rumble = !m.rumble,
            r => {
                let name = deck::MODE_NAMES[(r - MUSIC_ROWS).min(deck::MODES - 1)].to_string();
                if let Some(i) = m.disabled_visualizers.iter().position(|d| *d == name) {
                    m.disabled_visualizers.remove(i);
                } else if m.disabled_visualizers.len() + 1 < deck::MODES {
                    m.disabled_visualizers.push(name);
                }
            }
        }
    }

    fn visualizer_enabled(&self, mode: usize) -> bool {
        !self
            .settings
            .music
            .disabled_visualizers
            .iter()
            .any(|d| d == deck::MODE_NAMES[mode])
    }

    /// Next or previous visualizer among the ones switched on.
    pub(super) fn music_mode_step(&mut self, dir: i32) {
        let now = self.now;
        for _ in 0..deck::MODES {
            if dir > 0 {
                self.deck.next_mode(now);
            } else {
                self.deck.prev_mode(now);
            }
            if self.visualizer_enabled(self.deck.mode) {
                return;
            }
        }
    }

    /// The music list as shown: the open source's items through the search.
    pub(super) fn music_visible(&self) -> Vec<MusicItem> {
        let Some(items) = self.music_current() else {
            return Vec::new();
        };
        let q = self.search.clone().unwrap_or_default().to_lowercase();
        let words: Vec<&str> = q.split_whitespace().collect();
        if words.is_empty() {
            return items.clone();
        }
        items
            .iter()
            .filter(|it| {
                let l = it.label().to_lowercase();
                words.iter().all(|w| l.contains(w))
            })
            .cloned()
            .collect()
    }

    pub(super) fn initial(e: &Entry) -> char {
        e.game
            .title
            .chars()
            .find(|c| c.is_alphanumeric())
            .map(|c| {
                if c.is_ascii_digit() {
                    '#'
                } else {
                    c.to_ascii_uppercase()
                }
            })
            .unwrap_or('#')
    }

    /// Shoulder buttons on the deck: cassette or turntable; on the
    /// visualizer: the previous or next mode.
    pub(super) fn music_view_step(&mut self, dir: i32) {
        if self.music_visual {
            self.music_mode_step(dir);
        } else {
            let auto = self
                .music
                .status
                .track
                .as_ref()
                .map(|t| t.path.starts_with("spotify:") || (!t.stream && !t.album.is_empty()))
                .unwrap_or(false);
            let now_turntable = self.deck_look.unwrap_or(auto);
            self.deck_look = Some(!now_turntable);
            self.deck.insert_at = self.now;
            self.pending.push(Sound::Whoosh);
        }
    }

    // ------------------------------------------------------------ music

    pub(super) fn open_music(&mut self) {
        if !Music::available() {
            self.message = Some((
                "cliamp not found: Omarchy's music player is needed".into(),
                self.now + 4.0,
            ));
            return;
        }
        self.music.ensure();
        self.music_path.clear();
        let sel = self.music_root_sel;
        self.go(Screen::Music { sel, top: 0 });
    }

    /// Rows of the music screen. At the root: what plays, the radio hub, one
    /// hub per provider cliamp has configured, history and the live queue.
    /// Inside a hub: its own cuts.
    pub(super) fn music_rows(&self) -> Vec<(icons::Icon, String, String, MusicRow)> {
        let mut rows = Vec::new();
        let st = &self.music.status;
        match &self.music_hub {
            Some(Hub::Radio) => {
                if let Some((code, name)) = music::home_country(&self.settings.music.country) {
                    rows.push((
                        icons::PULSE,
                        format!("Radio  {name}"),
                        String::new(),
                        MusicRow::Source(Source::Country(code, name)),
                    ));
                }
                rows.push((
                    icons::FOLDER,
                    "By country".into(),
                    String::new(),
                    MusicRow::Source(Source::Countries),
                ));
                rows.push((
                    icons::FOLDER,
                    "By genre".into(),
                    String::new(),
                    MusicRow::Source(Source::Tags),
                ));
                rows.push((
                    icons::STAR,
                    "cliamp picks".into(),
                    String::new(),
                    MusicRow::Source(Source::ProviderPlaylists(
                        "radio".into(),
                        "cliamp picks".into(),
                    )),
                ));
                rows.push((
                    icons::STAR,
                    "Favourite stations".into(),
                    if self.music.favorites.is_empty() {
                        String::new()
                    } else {
                        format!("{:>4}", self.music.favorites.len())
                    },
                    MusicRow::Source(Source::Favorites),
                ));
                return rows;
            }
            Some(Hub::Provider(key, name)) => {
                rows.push((
                    icons::NOTE,
                    "Search".into(),
                    String::new(),
                    MusicRow::Search(key.clone()),
                ));
                rows.push((
                    icons::FOLDER,
                    "Playlists and albums".into(),
                    String::new(),
                    MusicRow::Source(Source::ProviderPlaylists(key.clone(), name.clone())),
                ));
                return rows;
            }
            None => {}
        }
        if st.active() {
            let label = st.track.as_ref().map(|t| t.label()).unwrap_or_default();
            let right: String = label.chars().take(20).collect();
            rows.push((icons::NOTE, "Now playing".into(), right, MusicRow::Now));
        }
        rows.push((
            icons::PULSE,
            "Radio".into(),
            String::new(),
            MusicRow::Hub(Hub::Radio),
        ));
        for p in &self.music.providers {
            if p.key == "radio" || p.key == "local" {
                continue;
            }
            let icon = match p.key.as_str() {
                "spotify" => icons::SPOTIFY,
                "youtube" | "ytmusic" | "yt" => icons::RESUME,
                _ => icons::FOLDER,
            };
            rows.push((
                icon,
                p.name.clone(),
                String::new(),
                MusicRow::Hub(Hub::Provider(p.key.clone(), p.name.clone())),
            ));
        }
        rows.push((
            icons::CLOCK,
            "Recently played".into(),
            String::new(),
            MusicRow::Source(Source::History),
        ));
        rows.push((
            icons::FOLDER,
            "Queue".into(),
            if st.total > 0 {
                format!("{:>4}", st.total)
            } else {
                String::new()
            },
            MusicRow::Source(Source::Queue),
        ));
        rows.push((
            icons::PULSE,
            "Equalizer".into(),
            if self.music.status.eq_preset.is_empty() {
                String::new()
            } else {
                self.music.status.eq_preset.clone()
            },
            MusicRow::Equalizer,
        ));
        rows
    }

    /// The list open on the music list screen, once it arrived.
    fn music_current(&self) -> Option<&Vec<MusicItem>> {
        let (src, _, _) = self.music_path.last()?;
        self.music.lists.get(src).and_then(|r| r.as_ref().ok())
    }

    pub(super) fn music_enter(&mut self, src: Source) {
        self.search = None;
        self.osk = None;
        self.music_query = None;
        self.pending.push(Sound::Select);
        self.music.open(&src);
        self.music_path.push((src, 0, 0));
        self.go(Screen::MusicList { sel: 0, top: 0 });
    }

    pub(super) fn music_back(&mut self) {
        self.search = None;
        self.osk = None;
        self.music_query = None;
        self.music_path.pop();
        self.screen = match self.music_path.last() {
            Some((_, sel, top)) => Screen::MusicList {
                sel: *sel,
                top: *top,
            },
            None => Screen::Music {
                sel: self.music_root_sel,
                top: 0,
            },
        };
        self.pending.push(Sound::Move);
    }

    /// Selected item of the open music list, as filtered.
    pub(super) fn music_selected(&self, sel: usize) -> Option<MusicItem> {
        self.music_visible().get(sel).cloned()
    }

    pub(super) fn music_play_item(&mut self, sel: usize, item: MusicItem) {
        match item {
            MusicItem::Track(t) => {
                // The list becomes the tuner's band: left and right walk it.
                let tracks: Vec<Track> = self
                    .music_visible()
                    .into_iter()
                    .filter_map(|it| match it {
                        MusicItem::Track(x) => Some(x),
                        _ => None,
                    })
                    .collect();
                let idx = tracks.iter().position(|x| x.path == t.path).unwrap_or(0);
                let count = tracks.len();
                self.tuning = Some((tracks, idx));
                if t.stream {
                    self.deck.tune(idx, count, self.now);
                } else {
                    self.deck.insert_at = self.now;
                }
                self.deck_change_sound(t.stream);
                self.music_root_sel = 0;
                self.music_visual = false;
                self.deck_look = match self.settings.music.look.as_str() {
                    "cassette" => Some(false),
                    "turntable" => Some(true),
                    _ => None,
                };
                self.go(Screen::NowPlaying);
                let queue = matches!(self.music_path.last(), Some((Source::Queue, _, _)));
                if queue {
                    let idx = self
                        .music_current()
                        .and_then(|l| {
                            l.iter().position(
                                |it| matches!(it, MusicItem::Track(x) if x.path == t.path),
                            )
                        })
                        .unwrap_or(sel);
                    self.music.play_index(idx);
                } else {
                    self.music.play(&t);
                }
                self.message = Some((format!("playing {}", t.label()), self.now + 3.0));
            }
            MusicItem::Source(Source::ProviderPlaylist(provider, id, name), _) => {
                self.music.load(&provider, &id);
                self.message = Some((format!("playing {name}"), self.now + 3.0));
                self.tuning = None;
                self.deck.insert_at = self.now;
                self.deck_change_sound(false);
                self.music_visual = false;
                self.go(Screen::NowPlaying);
            }
            MusicItem::Source(src, _) => {
                if let Some(last) = self.music_path.last_mut() {
                    last.1 = sel;
                    if let Screen::MusicList { top, .. } = self.screen {
                        last.2 = top;
                    }
                }
                self.music_enter(src);
            }
        }
    }

    /// The X button on the music screens: play a whole playlist, else
    /// pause or resume whatever plays.
    pub(super) fn music_alt(&mut self, sel: Option<usize>) {
        if let Some(sel) = sel
            && let Some(MusicItem::Source(Source::ProviderPlaylist(provider, id, name), _)) =
                self.music_selected(sel)
        {
            self.pending.push(Sound::Select);
            self.music.load(&provider, &id);
            self.message = Some((format!("playing {name}"), self.now + 3.0));
            return;
        }
        if self.music.status.active() {
            self.music.toggle();
            self.pending.push(Sound::Select);
        } else {
            self.message = Some(("nothing is playing".into(), self.now + 2.0));
        }
    }

    pub(super) fn draw_music_settings(&mut self, fb: &mut Framebuffer, sel: usize) {
        let m = self.settings.music.clone();
        let onoff = |b: bool| {
            if b {
                "on".to_string()
            } else {
                "off".to_string()
            }
        };
        let secs = |s: u32| {
            if s == 0 {
                "never".to_string()
            } else {
                format!("{s} s")
            }
        };
        let mut rows: Vec<(String, String)> = vec![
            ("visualizer after".into(), secs(m.idle_secs)),
            (
                "change mode every".into(),
                if m.cycle_secs == 0 {
                    "keep one".into()
                } else {
                    format!("{} s", m.cycle_secs)
                },
            ),
            ("as screensaver".into(), onoff(m.saver)),
            ("lyrics".into(), onoff(m.lyrics)),
            ("deck".into(), m.look.clone()),
            (
                "radio country".into(),
                if m.country.is_empty() {
                    "locale".into()
                } else {
                    m.country.clone()
                },
            ),
            ("pad rumble".into(), onoff(m.rumble)),
        ];
        for (i, name) in deck::MODE_NAMES.iter().enumerate() {
            rows.push((format!("  {name}"), onoff(self.visualizer_enabled(i))));
        }
        let notes: Vec<&str> = vec![
            "idle time on the deck before the show starts",
            "how long each visualizer plays before the next",
            "with music on, the visualizer replaces the screensaver",
            "synced lyrics from cliamp when a song has them",
            "auto: turntable for albums and Spotify, cassette otherwise",
            "whose stations come first, and which region's box art",
            "a short rumble on the beat, pads that support it",
        ];
        let mut notes = notes;
        notes.extend(std::iter::repeat_n(
            "in the rotation, or skipped",
            deck::MODES,
        ));
        // Fourteen rows: seven settings and one per visualizer. At ten
        // pixels each the last one still clears the note.
        self.draw_settings_table(fb, "Music", &rows, &notes, sel, 10, None);
    }

    /// Advance the music mirror and ease the visualiser toward the last frame.
    pub(super) fn tick_music(&mut self, now: f64) {
        let vis = matches!(
            self.screen,
            Screen::Music { .. } | Screen::MusicList { .. } | Screen::NowPlaying
        );
        self.music
            .tick(now, vis && self.menu_live && self.running.is_none());
        if let Some(e) = self.music.error.take() {
            self.message = Some((e, now + 4.0));
        }
        for (b, target) in self.vis.iter_mut().zip(self.music.bands.iter()) {
            *b += (target - *b) * 0.45;
        }
        let playing = self.music.status.playing();
        let bands = self.music.bands.clone();
        self.deck.tick(&bands, playing, now);
        if self.deck.beat && playing && vis && self.settings.music.rumble {
            self.rumble_pending = true;
        }
        // Sleep timer: the volume glides down over the last two minutes, then stop.
        if let Some((deadline, restore)) = self.sleep {
            let left = deadline - now;
            if left <= 0.0 {
                self.music.stop();
                self.music.volume_set(restore);
                self.sleep = None;
                self.message = Some(("sleep timer: stopped".into(), now + 4.0));
            } else if left < 120.0 && now - self.sleep_set_at > 2.0 {
                self.sleep_set_at = now;
                let v = restore - 30.0 * (1.0 - left / 120.0);
                self.music.volume_set(v);
            }
        }
    }

    /// Idle with music on: the visualizer stands in for the screensaver.
    pub(super) fn music_saver_start(&mut self, now: f64) {
        if self.music_saver.is_none() {
            self.music_saver = Some(self.screen);
            self.screen = Screen::NowPlaying;
            self.deck.mode_since = now;
        }
    }

    /// Y on the deck: no timer, 15, 30, 60 minutes, none again.
    pub(super) fn sleep_cycle(&mut self) {
        let mins = match self.sleep {
            None => Some(15.0),
            Some((d, _)) => {
                let left = ((d - self.now) / 60.0).round();
                if left <= 15.0 {
                    Some(30.0)
                } else if left <= 30.0 {
                    Some(60.0)
                } else {
                    None
                }
            }
        };
        let restore = self
            .sleep
            .map(|(_, r)| r)
            .unwrap_or(self.music.status.volume);
        self.sleep = mins.map(|m| (self.now + m * 60.0, restore));
        if mins.is_none() && self.music.status.volume != restore {
            self.music.volume_set(restore);
        }
        self.message = Some((
            match mins {
                Some(m) => format!("sleep in {m:.0} min"),
                None => "sleep timer off".into(),
            },
            self.now + 3.0,
        ));
        self.pending.push(Sound::Select);
    }

    /// Left or right on the deck: the next station, or the next track.
    pub(super) fn tune(&mut self, dir: i32) -> bool {
        let Some((list, idx)) = &self.tuning else {
            return false;
        };
        let n = list.len();
        if n < 2 {
            return false;
        }
        let next = ((*idx as i32 + dir).rem_euclid(n as i32)) as usize;
        let t = list[next].clone();
        self.tuning = Some((list.clone(), next));
        self.deck.tune(next, n, self.now);
        self.deck_change_sound(t.stream);
        self.music.play(&t);
        true
    }

    /// The noise the deck makes when what is playing changes: static for a
    /// radio, a needle for a record. Off unless it was asked for, because a
    /// sound every time a track changes is a sound every three minutes, and
    /// it lands on top of the music rather than beside it.
    fn deck_change_sound(&mut self, stream: bool) {
        if !self.settings.sound.deck {
            return;
        }
        self.pending
            .push(if stream { Sound::Static } else { Sound::Needle });
    }

    /// Ten bars of the spectrum, bottom aligned in the given box.
    pub(super) fn draw_vis(
        &self,
        fb: &mut Framebuffer,
        x: i32,
        bottom: i32,
        width: i32,
        height: i32,
    ) {
        let n = self.vis.len().max(1) as i32;
        let gap = if width >= n * 6 { 2 } else { 1 };
        let bar_w = ((width - (n - 1) * gap) / n).max(1);
        for (i, v) in self.vis.iter().enumerate() {
            let v = v.clamp(0.0, 1.0);
            let h = ((v * height as f32) as i32).min(height);
            let bx = x + i as i32 * (bar_w + gap);
            fb.rect(
                bx,
                bottom - height,
                bar_w,
                height,
                scale(self.theme.selection, 0.8),
            );
            if h > 0 {
                let c = crate::fb::lerp_color(self.theme.green, self.theme.yellow, v);
                fb.rect(bx, bottom - h, bar_w, h, c);
                fb.rect(bx, bottom - h, bar_w, 1, self.theme.paper);
            }
        }
    }

    /// What plays, in one line with a small spectrum, above the hints.
    fn draw_music_strip(&self, fb: &mut Framebuffer, y: i32) {
        let st = &self.music.status;
        if !st.active() {
            return;
        }
        let w = fb.w as i32;
        let left = (w as f32 * 0.05) as i32;
        let width = w - 2 * left;
        fb.rect(left, y - 3, width, 1, scale(self.theme.dim, 0.5));
        let vis_w = 39;
        self.draw_vis(fb, left, y + 8, vis_w, 8);
        let label = st.track.as_ref().map(|t| t.label()).unwrap_or_default();
        let state = if st.playing() { "" } else { "  paused" };
        let text = format!("{label}{state}");
        // The page counter sits at the right end of this line.
        let room = ((width - vis_w - 8) / 8).saturating_sub(10) as usize;
        let text: String = text
            .chars()
            .take(room)
            .collect::<String>()
            .trim_end()
            .to_string();
        fb.text(left + vis_w + 8, y, &text, self.theme.paper, 1);
    }

    pub(super) fn draw_music(&mut self, fb: &mut Framebuffer, sel: usize, top: usize) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32;
        let title = match &self.music_hub {
            Some(Hub::Radio) => "Radio".to_string(),
            Some(Hub::Provider(_, name)) => name.clone(),
            None => "Music".to_string(),
        };
        let y0 = self.draw_header(fb, &title);
        let ox = self.slide();
        let rows = self.music_rows();
        let row_h = 12;
        let end = (top + Self::ROWS_PER_PAGE).min(rows.len());
        for (row, i) in (top..end).enumerate() {
            let y = y0 + row as i32 * row_h;
            let on = i == sel;
            let (icon, label, right, _) = &rows[i];
            self.draw_row(fb, y, label, right, on, self.theme.paper);
            let c = if on {
                self.theme.accent
            } else {
                self.theme.dim
            };
            fb.bitmap(left + ox + 4, y + 1, icon, c, 1, 8);
        }
        match &self.music.ready {
            None => fb.text(left, h - 42, "starting cliamp", self.theme.dim, 1),
            Some(Err(e)) => {
                let m: String = e.chars().take(((w - 2 * left) / 8) as usize).collect();
                fb.text(left, h - 42, &m, self.theme.red, 1);
            }
            Some(Ok(())) => {}
        }
        self.draw_music_strip(fb, h - 30);
        let hint = self.hint(&[("A", "open"), ("X", "pause"), ("B", "back")]);
        fb.text(left, h - 14, &hint, scale(self.theme.dim, 0.7), 1);
    }

    /// The ten bands as vertical sliders, the picked one lit, with the
    /// preset's name and the gain in dB of the band under the cursor.
    pub(super) fn draw_equalizer(&mut self, fb: &mut Framebuffer, band: usize) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32;
        let width = w - 2 * left;
        let y0 = self.draw_header(fb, "Equalizer");
        let th = self.theme.clone();
        let preset = if self.music.status.eq_preset.is_empty() {
            "Custom".to_string()
        } else {
            self.music.status.eq_preset.clone()
        };
        fb.text(left, y0, &format!("preset  {preset}"), th.paper, 1);
        // The plot: zero in the middle, the range of the engine top to bottom.
        let top = y0 + 18;
        let plot_h = h - 52 - top;
        let mid = top + plot_h / 2;
        let bands = self.music.eq_bands();
        let step = width / music::EQ_BANDS as i32;
        let slot = (step - 6).max(4);
        // Grid: zero line and the two extremes, each labelled once.
        fb.rect(left, mid, width, 1, lerp_color(th.bg, th.dim, 0.7));
        for (dy, lab) in [(-plot_h / 2, "+12"), (plot_h / 2, "-12")] {
            let y = mid + dy;
            for x in (left..left + width).step_by(4) {
                fb.put(x, y, lerp_color(th.bg, th.dim, 0.35));
            }
            fb.text(left - 2, y - 4, lab, scale(th.dim, 0.8), 1);
        }
        for (i, db) in bands.iter().enumerate() {
            let cx = left + i as i32 * step + step / 2;
            let on = i == band;
            // Track.
            fb.rect(cx - 1, top, 2, plot_h, lerp_color(th.bg, th.dim, 0.25));
            // Bar from the zero line to the gain.
            let span = ((db / music::EQ_MAX) as f32 * (plot_h / 2) as f32) as i32;
            let c = if on {
                th.accent
            } else if *db >= 0.0 {
                lerp_color(th.cyan, th.bg, 0.25)
            } else {
                lerp_color(th.magenta, th.bg, 0.25)
            };
            let (by, bh) = if span >= 0 {
                (mid - span, span)
            } else {
                (mid, -span)
            };
            if bh > 0 {
                fb.rect(cx - slot / 2, by, slot, bh, scale(c, 0.55));
            }
            // Handle.
            let hy = mid - span;
            fb.rect(cx - slot / 2 - 1, hy - 1, slot + 2, 3, c);
            if on {
                fb.rect(cx - slot / 2 - 2, hy - 2, slot + 4, 5, th.paper);
                fb.rect(cx - slot / 2 - 1, hy - 1, slot + 2, 3, c);
            }
            // Frequency under the slider, the picked one lit.
            let f = music::EQ_FREQS[i];
            let tx = cx - Framebuffer::text_width(f, 1) / 2;
            fb.text(
                tx,
                top + plot_h + 4,
                f,
                if on { th.paper } else { scale(th.dim, 0.9) },
                1,
            );
        }
        // The gain of the picked band, up on the preset's line.
        let db = bands[band.min(bands.len() - 1)];
        let read = format!("{}Hz  {db:+.0} dB", music::EQ_FREQS[band]);
        fb.text(
            left + width - Framebuffer::text_width(&read, 1),
            y0,
            &read,
            th.bright_green,
            1,
        );
        // Four controls do not fit on one line at this width.
        let hint1 = self.hint(&[("<>", "band"), ("^v", "gain")]);
        let hint2 = self.hint(&[("A", "preset"), ("B", "back")]);
        fb.text(left, h - 24, &hint1, scale(th.dim, 0.7), 1);
        fb.text(left, h - 14, &hint2, scale(th.dim, 0.7), 1);
    }

    pub(super) fn draw_music_list(&mut self, fb: &mut Framebuffer, sel: usize, top: usize) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32;
        let width = w - 2 * left;
        let Some((src, _, _)) = self.music_path.last().cloned() else {
            return;
        };
        let mut y0 = self.draw_header(fb, &src.title());
        let visible = self.music_visible();
        if let Some(q) = self.search.clone() {
            y0 = self.draw_search_bar(fb, y0, &q, visible.len());
        }
        let page = self.page_rows();
        let row_h = 12;
        let playing = self
            .music
            .status
            .track
            .as_ref()
            .map(|t| t.path.clone())
            .unwrap_or_default();
        let max_cols = (width / 8) as usize;
        match self.music.lists.get(&src) {
            None => {
                let dots = ".".repeat(1 + ((self.now * 3.0) as usize % 3));
                fb.text(left, y0 + 8, &format!("fetching{dots}"), self.theme.dim, 1);
            }
            Some(Err(e)) => {
                // Wrapped, so cliamp's whole explanation reads.
                let mut yy = y0 + 8;
                let mut line = String::new();
                for word in e.split_whitespace() {
                    if !line.is_empty()
                        && line.chars().count() + 1 + word.chars().count() > max_cols
                    {
                        fb.text(left, yy, &line, self.theme.red, 1);
                        yy += 10;
                        line.clear();
                    }
                    if !line.is_empty() {
                        line.push(' ');
                    }
                    line.push_str(word);
                }
                if !line.is_empty() {
                    fb.text(left, yy, &line, self.theme.red, 1);
                }
            }
            Some(Ok(items)) if items.is_empty() && self.music_query.is_some() => {
                fb.text(
                    left,
                    y0 + 8,
                    "type what to look for, then Enter",
                    self.theme.dim,
                    1,
                );
            }
            Some(Ok(items)) if items.is_empty() => {
                fb.text(left, y0 + 8, "nothing here yet", self.theme.dim, 1);
            }
            Some(Ok(_)) if visible.is_empty() => {
                fb.text(left, y0 + 8, "no title matches", self.theme.dim, 1);
            }
            Some(Ok(_)) => {
                let items = visible;
                let end = (top + page).min(items.len());
                for (row, i) in (top..end).enumerate() {
                    let y = y0 + row as i32 * row_h;
                    let on = i == sel;
                    let item = &items[i];
                    let now_playing = matches!(item, MusicItem::Track(t) if !playing.is_empty() && t.path == playing);
                    let color = if now_playing {
                        self.theme.bright_green
                    } else {
                        self.theme.paper
                    };
                    self.draw_row(fb, y, &item.label(), &item.right(), on, color);
                    if now_playing {
                        let c = if on {
                            self.theme.accent
                        } else {
                            self.theme.bright_green
                        };
                        fb.bitmap(left + self.slide() + 4, y + 1, &icons::NOTE, c, 1, 8);
                    } else if matches!(item, MusicItem::Track(t) if self.music.is_favorite(t)) {
                        fb.bitmap(
                            left + self.slide() + 4,
                            y + 1,
                            &icons::STAR,
                            self.theme.yellow,
                            1,
                            8,
                        );
                    }
                }
                let page = format!("{}/{}", sel + 1, items.len());
                fb.text(
                    w - left - Framebuffer::text_width(&page, 1),
                    h - 30,
                    &page,
                    scale(self.theme.dim, 0.7),
                    1,
                );
            }
        }
        if self.osk.is_some() {
            self.draw_osk(fb);
        } else {
            self.draw_music_strip(fb, h - 30);
        }
        let playlist = matches!(
            self.music_selected(sel),
            Some(MusicItem::Source(Source::ProviderPlaylist(..), _))
        );
        let keyboard = self.pad == PadKind::Keyboard;
        let hint = if self.osk.is_some() {
            self.hint(&[("A", "type"), ("X", "del"), ("Y", "space"), ("B", "done")])
        } else if playlist {
            self.hint(&[("A", "play"), ("B", "back")])
        } else if keyboard {
            self.hint(&[("A", "play"), ("Y", "star"), ("/", "find"), ("B", "back")])
        } else {
            self.hint(&[("A", "play"), ("Y", "star"), ("LT", "find"), ("B", "back")])
        };
        fb.text(left, h - 14, &hint, scale(self.theme.dim, 0.7), 1);
    }

    pub(super) fn draw_now_playing(&mut self, fb: &mut Framebuffer) {
        let h = fb.h as i32;
        let w = fb.w as i32;
        let left = (w as f32 * 0.05) as i32;
        let st = self.music.status.clone();
        let track = st.track.clone().unwrap_or_default();
        let title = if !track.title.is_empty() {
            track.title.clone()
        } else {
            track.label()
        };
        let sub = if !track.artist.is_empty() && !track.station.is_empty() {
            format!("{}  on {}", track.artist, track.station)
        } else if !track.artist.is_empty() {
            track.artist.clone()
        } else if !track.station.is_empty() && track.station != title {
            track.station.clone()
        } else if track.stream {
            "live stream".to_string()
        } else {
            track.album.clone()
        };
        let idle_secs = self.settings.music.idle_secs;
        let idle = idle_secs > 0 && self.now - self.last_input > idle_secs as f64;
        let visual = self.music_visual || self.music_saver.is_some() || (idle && st.playing());
        let now = self.now;
        let theme = self.theme.clone();
        if visual {
            // Cycle the modes while nobody touches anything.
            let cycle = self.settings.music.cycle_secs;
            if !self.visualizer_enabled(self.deck.mode)
                || (!self.music_visual && cycle > 0 && now - self.deck.mode_since > cycle as f64)
            {
                self.music_mode_step(1);
            }
            self.deck.draw_visual(fb, &theme, now, &title);
            // Lyrics line up with a song's clock, not with a stream's.
            if self.settings.music.lyrics && !self.music.lyrics.is_empty() && st.duration > 0.0 {
                // Lower third, shadowed glyphs straight on the picture.
                deck::draw_lyrics(fb, &theme, &self.music.lyrics, st.position, h - 96, 70);
            }
            return;
        }
        let y0 = self.draw_header(fb, "Music");
        if track.path.starts_with("spotify:") {
            // The source, up in the header next to the screen's name.
            let bx = left + Framebuffer::text_width("Music", 1) + 10;
            fb.bitmap(bx, 39, &icons::SPOTIFY, theme.green, 1, 8);
            fb.text(bx + 11, 40, "SPOTIFY", theme.green, 1);
        }
        let station = self
            .tuning
            .as_ref()
            .filter(|_| track.stream)
            .map(|(l, i)| (*i, l.len()));
        match &self.music.cover {
            Some(p) if self.cover_img.as_ref().map(|(q, _)| q) != Some(p) => {
                let img = crate::art::decode(p).map(|i| crate::art::fit(&i, 22, 22));
                self.cover_img = img.map(|i| (p.clone(), i));
            }
            None => self.cover_img = None,
            _ => {}
        }
        let cover = self.cover_img.as_ref().map(|(_, i)| i.clone());
        let info = deck::Info {
            title: &title,
            sub: &sub,
            position: st.position,
            duration: st.duration,
            playing: st.playing(),
            radio: track.stream && track.duration_secs == 0 && st.duration <= 0.0,
            turntable: self.deck_look.unwrap_or(
                track.path.starts_with("spotify:") || (!track.stream && !track.album.is_empty()),
            ),
            spotify: track.path.starts_with("spotify:"),
            station,
            cover: cover.as_ref(),
            volume_db: st.volume,
        };
        self.deck.draw(fb, &theme, y0, now, &info);
        if self.settings.music.lyrics && !self.music.lyrics.is_empty() && st.duration > 0.0 {
            deck::draw_lyrics(fb, &theme, &self.music.lyrics, st.position, h - 36, 10);
        }
        if let Some((deadline, _)) = self.sleep {
            let m = ((deadline - now) / 60.0).ceil().max(0.0);
            let s = format!("sleep {m:.0}m");
            fb.text(
                w - left - Framebuffer::text_width(&s, 1),
                y0 - 12,
                &s,
                theme.orange,
                1,
            );
        }
        // Two lines of hints: the deck has more controls than fit in one.
        let skip = if track.stream { "tune" } else { "track" };
        let hint1 = self.hint(&[("A", "pause"), ("<>", skip), ("^v", "volume")]);
        let deck_key = if self.pad == PadKind::Keyboard {
            "PgUp"
        } else {
            "LB"
        };
        let hint2 = self.hint(&[
            ("X", "show"),
            (deck_key, "deck"),
            ("Y", "timer"),
            ("B", "back"),
        ]);
        fb.text(left, h - 24, &hint1, scale(theme.dim, 0.7), 1);
        fb.text(left, h - 14, &hint2, scale(theme.dim, 0.7), 1);
    }
}
