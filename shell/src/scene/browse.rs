//! The browser: systems, games, folders, search and the cover flow.
//!
//! The two long matches here, one for navigation and one for drawing, are
//! long because they answer for every screen the browser has. They are
//! easier to read as one place that lists them all than as a dozen.

use super::*;

impl Scene {
    // -- game browser (systems, then games; recent and favorites on top) -----

    pub(super) const ROWS_PER_PAGE: usize = 13;
    const VIRTUAL: usize = 3; // recent/, favorites/, collections/

    pub(super) fn open_games(&mut self, sys: Option<usize>) {
        self.game_dir = None;
        self.search_global = false;
        self.games_back = None;
        let list = match sys {
            Some(i) => self.entries_for(i),
            None => Vec::new(),
        };
        self.set_games(list);
        self.screen = Screen::Games {
            sys,
            sel: 0,
            top: 0,
        };
    }

    /// Entries of system `i` in the folder currently browsed.
    pub(super) fn entries_for(&self, i: usize) -> Vec<Entry> {
        let system = &self.library.systems[i];
        // Indexed systems list from the index, one title each, no folders.
        // Folder browsing is for systems that only have a directory.
        let games = match &self.game_dir {
            None if self.library.uses_index(system) || system.dir.is_empty() => {
                self.library.games(system)
            }
            None => self
                .library
                .games_in(system, &crate::library::expand(&system.dir)),
            Some(dir) => self.library.games_in(system, dir),
        };
        games
            .into_iter()
            .map(|game| Entry { game, sys: i })
            .collect()
    }

    /// A list of links (watch later, recently watched, search hits) shown
    /// as a game list under the Videos system, going back to a hub.
    fn open_links(&mut self, entries: Vec<Entry>, back: Screen) {
        self.game_dir = None;
        self.search_global = false;
        self.list_from_home = false;
        self.open_collection = None;
        let sys = self.video_system();
        self.set_games(entries);
        self.games_back = Some(back);
        self.go(Screen::Games {
            sys,
            sel: 0,
            top: 0,
        });
    }

    /// Links played before, newest first, from the recent list.
    fn recent_links(&self, sys: usize) -> Vec<Entry> {
        self.recent
            .iter()
            .filter(|(_, p)| p.to_string_lossy().starts_with("http"))
            .map(|(_, p)| Entry {
                game: Game {
                    title: watch_title(&p.to_string_lossy(), ""),
                    path: p.clone(),
                    crt_path: None,
                    folder: false,
                },
                sys,
            })
            .collect()
    }

    /// Step into a subfolder of the current system.
    fn enter_folder(&mut self, sys: usize, dir: PathBuf) {
        self.game_dir = Some(dir);
        self.set_games(self.entries_for(sys));
        self.screen = Screen::Games {
            sys: Some(sys),
            sel: 0,
            top: 0,
        };
        self.pending.push(Sound::Select);
    }

    /// One folder up; false when already at the system root.
    fn leave_folder(&mut self, sys: usize) -> bool {
        let Some(cur) = self.game_dir.clone() else {
            return false;
        };
        let root = crate::library::expand(&self.library.systems[sys].dir);
        let parent = cur.parent().map(Path::to_path_buf);
        let leaving = cur.file_name().map(|n| n.to_string_lossy().into_owned());
        self.game_dir = match parent {
            Some(p) if p != root => Some(p),
            _ => None,
        };
        self.set_games(self.entries_for(sys));
        let sel = leaving
            .and_then(|name| {
                self.games
                    .iter()
                    .position(|e| e.game.folder && e.game.title == name)
            })
            .unwrap_or(0);
        self.screen = Screen::Games {
            sys: Some(sys),
            sel,
            top: sel.saturating_sub(Self::ROWS_PER_PAGE - 1),
        };
        self.pending.push(Sound::Move);
        true
    }

    pub(super) fn open_virtual(&mut self, list: &[(usize, PathBuf)]) {
        self.search_global = false;
        // The names of the systems, to turn an arcade set name into a title
        // the way the per system lists do.
        let names: Vec<String> = self
            .library
            .systems
            .iter()
            .map(|s| s.name.clone())
            .collect();
        let entries: Vec<Entry> = list
            .iter()
            .filter(|(i, p)| *i < names.len() && p.exists())
            .map(|(i, p)| Entry {
                game: Game {
                    title: crate::covers::title_for(&names[*i], &crate::library::clean_title(p)),
                    crt_path: {
                        let c = videofit::crt_path(p);
                        c.exists().then_some(c)
                    },
                    path: p.clone(),
                    folder: false,
                },
                sys: *i,
            })
            .collect();
        self.set_games(entries);
        self.screen = Screen::Games {
            sys: None,
            sel: 0,
            top: 0,
        };
    }

    /// A new list: unfiltered copy kept, search bar closed.
    pub(super) fn set_games(&mut self, list: Vec<Entry>) {
        self.games_all = list;
        self.search = None;
        self.osk = None;
        self.yt_query = false;
        self.yt_results = false;
        self.apply_search();
    }

    // ------------------------------------------------------------ search

    pub fn search_active(&self) -> bool {
        self.search.is_some() && self.running.is_none()
    }

    pub fn osk_active(&self) -> bool {
        self.osk.is_some() && self.search_active()
    }

    /// Rows a game list shows: the search bar and the on screen keyboard
    /// take theirs.
    pub(super) fn page_rows(&self) -> usize {
        let mut n = Self::ROWS_PER_PAGE;
        if self.search.is_some() {
            n -= 1;
        }
        if self.osk.is_some() {
            n -= 4;
        }
        n
    }

    /// Open the search bar: on a game list it filters that list; anywhere
    /// else it opens the whole collection to search across systems. With
    /// `osk` a pad types through the on screen keyboard.
    pub fn search_open(&mut self, osk: bool) {
        if !self.menu_live || self.running.is_some() || self.launching.is_some() {
            return;
        }
        match self.screen {
            Screen::Games { .. } | Screen::MusicList { .. } => {}
            Screen::Menu | Screen::Systems { .. } | Screen::Collections { .. } => {
                self.list_from_home = matches!(self.screen, Screen::Menu);
                self.open_collection = None;
                self.game_dir = None;
                let mut all: Vec<Entry> = Vec::new();
                for i in 0..self.library.systems.len() {
                    if self.library.systems[i].is_video() {
                        continue;
                    }
                    all.extend(self.entries_for(i));
                }
                all.sort_by_key(|a| a.game.title.to_lowercase());
                self.set_games(all);
                self.search_global = true;
                self.go(Screen::Games {
                    sys: None,
                    sel: 0,
                    top: 0,
                });
            }
            _ => return,
        }
        self.search = Some(String::new());
        self.osk = if osk { Some((1, 0)) } else { None };
        self.pending.push(Sound::Select);
        self.apply_search();
    }

    pub fn search_type(&mut self, text: &str) {
        let Some(s) = self.search.as_mut() else {
            return;
        };
        for c in text.chars() {
            if !c.is_control() {
                s.push(c);
            }
        }
        self.pending.push(Sound::Click);
        self.apply_search();
    }

    /// One character back; an empty bar closes.
    pub fn search_backspace(&mut self) {
        let Some(s) = self.search.as_mut() else {
            return;
        };
        if s.pop().is_none() {
            self.search = None;
            self.osk = None;
        }
        self.pending.push(Sound::Move);
        self.apply_search();
    }

    /// Escape on a search: clear the text first, then close the bar.
    fn search_clear_or_close(&mut self) {
        match self.search.as_mut() {
            Some(s) if !s.is_empty() => s.clear(),
            _ => {
                self.search = None;
                self.osk = None;
            }
        }
        self.pending.push(Sound::Move);
        self.apply_search();
    }

    /// Every word typed must appear in the title; titles starting with the
    /// first word come first, the list order holds otherwise.
    fn apply_search(&mut self) {
        if self.yt_query || self.music_query.is_some() {
            // The bar collects a YouTube query; the list stays as it is.
            return;
        }
        let q = self.search.clone().unwrap_or_default().to_lowercase();
        let words: Vec<&str> = q.split_whitespace().collect();
        self.games = if words.is_empty() {
            self.games_all.clone()
        } else {
            let mut hits: Vec<(bool, Entry)> = self
                .games_all
                .iter()
                .filter(|e| {
                    let t = e.game.title.to_lowercase();
                    words.iter().all(|w| t.contains(w))
                })
                .map(|e| (e.game.title.to_lowercase().starts_with(words[0]), e.clone()))
                .collect();
            hits.sort_by_key(|h| std::cmp::Reverse(h.0));
            hits.into_iter().map(|(_, e)| e).collect()
        };
        match &mut self.screen {
            Screen::Games { sel, top, .. } | Screen::MusicList { sel, top } => {
                *sel = 0;
                *top = 0;
            }
            _ => {}
        }
    }

    /// Jump to the first title of the next (or previous) initial letter.
    pub fn jump_letter(&mut self, dir: i32) {
        if matches!(self.screen, Screen::NowPlaying) {
            self.music_view_step(dir);
            return;
        }
        // The list of consoles has no letters worth jumping between, so the
        // shoulders move it a page at a time, which is what they are for on
        // every other screen.
        if let Screen::Systems { sel, top } = &mut self.screen {
            let rows = Self::VIRTUAL
                + self
                    .library
                    .systems
                    .iter()
                    .filter(|s| !s.is_video())
                    .count();
            if rows == 0 {
                return;
            }
            let step = SYS_PAGE as i32 - 1;
            let next = (*sel as i32 + dir.signum() * step).clamp(0, rows as i32 - 1) as usize;
            if next == *sel {
                return;
            }
            *sel = next;
            *top = next.saturating_sub(SYS_PAGE - 1).min(next);
            if next < *top {
                *top = next;
            }
            self.pending.push(Sound::Move);
            return;
        }
        let Screen::Games { sel, .. } = self.screen else {
            return;
        };
        let n = self.games.len();
        // The list can be shorter than the selection that was made on it: the
        // guard that keeps those in step lives three functions away, so this
        // one does not depend on it.
        if sel >= n {
            return;
        }
        let key = Self::initial(&self.games[sel]);
        let target = if dir > 0 {
            (sel + 1..n)
                .find(|&i| Self::initial(&self.games[i]) != key)
                .unwrap_or(n - 1)
        } else {
            // Start of this letter's group, or of the previous group when
            // already there.
            let mut start = sel;
            while start > 0 && Self::initial(&self.games[start - 1]) == key {
                start -= 1;
            }
            if start < sel {
                start
            } else if start == 0 {
                0
            } else {
                let prev = Self::initial(&self.games[start - 1]);
                let mut s = start - 1;
                while s > 0 && Self::initial(&self.games[s - 1]) == prev {
                    s -= 1;
                }
                s
            }
        };
        self.select_row(target);
    }

    /// First or last row of the list.
    pub fn jump_end(&mut self, last: bool) {
        if !matches!(self.screen, Screen::Games { .. }) || self.games.is_empty() {
            return;
        }
        let target = if last { self.games.len() - 1 } else { 0 };
        self.select_row(target);
    }

    /// Move the cursor to `target` and show it at the top of the page, so a
    /// letter jump lands on the first titles of that letter.
    fn select_row(&mut self, target: usize) {
        let page = self.page_rows();
        let n = self.games.len();
        if let Screen::Games { sel, top, .. } = &mut self.screen
            && *sel != target
        {
            *sel = target;
            *top = target.min(n.saturating_sub(page));
            self.pending.push(Sound::Move);
        }
    }

    /// Pad input while the on screen keyboard is up: move, type, delete,
    /// space; back puts the keyboard away and leaves the search as typed.
    pub fn osk_input(&mut self, nav: Option<Nav>, fire: bool, fav: bool, alt: bool) {
        let Some((r, c)) = self.osk else {
            return;
        };
        let rows = OSK_ROWS.len() as i32;
        let cols = OSK_ROWS[0].len() as i32;
        match nav {
            Some(Nav::Up) => self.osk = Some(((r - 1).rem_euclid(rows), c)),
            Some(Nav::Down) => self.osk = Some(((r + 1).rem_euclid(rows), c)),
            Some(Nav::Left) => self.osk = Some((r, (c - 1).rem_euclid(cols))),
            Some(Nav::Right) => self.osk = Some((r, (c + 1).rem_euclid(cols))),
            Some(Nav::Back) => {
                self.osk = None;
                self.pending.push(Sound::Move);
                return;
            }
            None => {}
        }
        if nav.is_some() {
            self.pending.push(Sound::Move);
        }
        if fire {
            let ch = OSK_ROWS[r as usize].chars().nth(c as usize).unwrap_or(' ');
            let s = ch.to_string();
            self.search_type(&s);
        }
        if fav {
            self.search_type(" ");
        }
        if alt {
            if self.search.as_deref().is_some_and(|s| s.is_empty()) {
                self.pending.push(Sound::Move);
            } else {
                self.search_backspace();
            }
        }
    }

    /// Rows of the systems screen: recent/, favorites/, then every system.
    /// Systems the Games browser lists: everything but the videos folder,
    /// which is not a console and has its own row on the home menu.
    fn browse_systems(&self) -> Vec<usize> {
        (0..self.library.systems.len())
            .filter(|&i| !self.library.systems[i].is_video())
            .collect()
    }

    /// The system a row of the browser stands for.
    fn system_at_row(&self, row: usize) -> Option<usize> {
        let n = row.checked_sub(Self::VIRTUAL)?;
        self.browse_systems().get(n).copied()
    }

    /// The row a system sits on, for coming back to the list on it.
    fn row_of_system(&self, sys: usize) -> usize {
        self.browse_systems()
            .iter()
            .position(|&i| i == sys)
            .map(|n| n + Self::VIRTUAL)
            .unwrap_or(self.virtual_row)
    }

    fn system_rows(&self) -> usize {
        Self::VIRTUAL + self.browse_systems().len()
    }

    pub(super) fn navigate_browser(&mut self, nav: Nav) {
        let mut moved = false;
        let system_rows = self.system_rows();
        let music_rows = self.music_rows().len();
        let music_items = self.music_visible().len();
        let page_rows = self.page_rows();
        if nav == Nav::Back
            && self.search.is_some()
            && matches!(self.screen, Screen::Games { .. } | Screen::MusicList { .. })
        {
            self.search_clear_or_close();
            return;
        }
        match &mut self.screen {
            Screen::Systems { sel, top } => {
                let mut back = false;
                match nav {
                    Nav::Up if *sel > 0 => {
                        *sel -= 1;
                        moved = true;
                    }
                    Nav::Down if *sel + 1 < system_rows => {
                        *sel += 1;
                        moved = true;
                    }
                    Nav::Back | Nav::Left => back = true,
                    _ => {}
                }
                if *sel < *top {
                    *top = *sel;
                } else if *sel >= *top + SYS_PAGE {
                    *top = *sel + 1 - SYS_PAGE;
                }
                if back {
                    self.screen = Screen::Menu;
                    moved = true;
                }
            }
            Screen::Collections { sel, .. } => {
                let n = self.library.collections().len();
                match nav {
                    Nav::Up if *sel > 0 => {
                        *sel -= 1;
                        moved = true;
                    }
                    Nav::Down if *sel + 1 < n => {
                        *sel += 1;
                        moved = true;
                    }
                    Nav::Right if n > 0 => {
                        *sel = (*sel + SYS_PAGE).min(n - 1);
                        moved = true;
                    }
                    Nav::Back | Nav::Left => {
                        self.screen = Screen::Systems { sel: 2, top: 0 };
                        moved = true;
                    }
                    _ => {}
                }
                if let Screen::Collections { sel, top } = &mut self.screen {
                    if *sel < *top {
                        *top = *sel;
                    } else if *sel >= *top + SYS_PAGE {
                        *top = *sel + 1 - SYS_PAGE;
                    }
                }
            }
            Screen::Games { sel, top, .. } if self.flow_view => {
                let n = self.games.len();
                match nav {
                    Nav::Left if *sel > 0 => {
                        *sel -= 1;
                        moved = true;
                    }
                    Nav::Right if *sel + 1 < n => {
                        *sel += 1;
                        moved = true;
                    }
                    Nav::Up if *sel > 0 => {
                        *sel = sel.saturating_sub(10);
                        moved = true;
                    }
                    Nav::Down if n > 0 && *sel + 1 < n => {
                        *sel = (*sel + 10).min(n - 1);
                        moved = true;
                    }
                    Nav::Back => {
                        self.flow_view = false;
                        moved = true;
                    }
                    _ => {}
                }
                if *sel < *top {
                    *top = *sel;
                } else if *sel >= *top + page_rows {
                    *top = *sel + 1 - page_rows;
                }
            }
            Screen::Games { sel, top, .. } => {
                let n = self.games.len();
                let page = page_rows;
                match nav {
                    Nav::Up if *sel > 0 => {
                        *sel -= 1;
                        moved = true;
                    }
                    Nav::Down if *sel + 1 < n => {
                        *sel += 1;
                        moved = true;
                    }
                    Nav::Right if n > 0 => {
                        *sel = (*sel + page).min(n - 1);
                        moved = true;
                    }
                    Nav::Left if n > 0 && *sel > 0 => {
                        *sel = sel.saturating_sub(page);
                        moved = true;
                    }
                    Nav::Back => {
                        if let Screen::Games { sys: Some(i), .. } = self.screen
                            && self.leave_folder(i)
                        {
                            return;
                        }
                        if let Some(back) = self.games_back.take() {
                            self.screen = back;
                            self.pending.push(Sound::Move);
                            return;
                        }
                        self.search_global = false;
                        if self.list_from_home {
                            self.screen = Screen::Menu;
                            self.pending.push(Sound::Move);
                            return;
                        }
                        if let Some(ci) = self.open_collection.take() {
                            self.screen = Screen::Collections {
                                sel: ci,
                                top: ci.saturating_sub(SYS_PAGE - 1),
                            };
                            self.pending.push(Sound::Move);
                            return;
                        }
                        let row = match self.screen {
                            Screen::Games { sys: Some(i), .. } => self.row_of_system(i),
                            _ => self.virtual_row,
                        };
                        self.screen = Screen::Systems {
                            sel: row,
                            top: row.saturating_sub(SYS_PAGE - 1),
                        };
                        self.pending.push(Sound::Move);
                        return;
                    }
                    _ => {}
                }
                if *sel < *top {
                    *top = *sel;
                } else if *sel >= *top + page {
                    *top = *sel + 1 - page;
                }
            }
            Screen::Profile { sel } => {
                let rows = 7;
                match nav {
                    Nav::Up if *sel > 0 => {
                        *sel -= 1;
                        moved = true;
                    }
                    Nav::Down if *sel + 1 < rows => {
                        *sel += 1;
                        moved = true;
                    }
                    Nav::Left | Nav::Right => {
                        let dir = if nav == Nav::Right { 1 } else { -1 };
                        let row = *sel;
                        self.adjust_profile(row, dir);
                        moved = true;
                    }
                    Nav::Back => {
                        if let Err(e) = self.profile.save(&self.library.config_dir) {
                            eprintln!("profile: {e}");
                        }
                        self.screen = Screen::Settings {
                            sel: settings_row(Page::Profile),
                        };
                        self.pending.push(Sound::Lock);
                        return;
                    }
                    _ => {}
                }
            }
            Screen::Pair { sel } => match nav {
                Nav::Up if *sel > 0 => {
                    *sel -= 1;
                    moved = true;
                }
                Nav::Down if *sel + 1 < self.bt.devices.len() => {
                    *sel += 1;
                    moved = true;
                }
                Nav::Back => {
                    self.screen = Screen::Settings {
                        sel: settings_row(Page::Pads),
                    };
                    moved = true;
                }
                _ => {}
            },
            Screen::Videos { sel } => match nav {
                Nav::Up if *sel > 0 => {
                    *sel -= 1;
                    moved = true;
                }
                Nav::Down if *sel + 1 < VIDEOS_ITEMS.len() => {
                    *sel += 1;
                    moved = true;
                }
                Nav::Back => {
                    self.screen = Screen::Menu;
                    moved = true;
                }
                _ => {}
            },
            Screen::YouTube { sel } => match nav {
                Nav::Up if *sel > 0 => {
                    *sel -= 1;
                    moved = true;
                }
                Nav::Down if *sel + 1 < YOUTUBE_ITEMS.len() => {
                    *sel += 1;
                    moved = true;
                }
                Nav::Back => {
                    self.screen = Screen::Videos { sel: 1 };
                    moved = true;
                }
                _ => {}
            },
            Screen::AmbientHub { sel } => match nav {
                Nav::Up if *sel > 0 => {
                    *sel -= 1;
                    moved = true;
                }
                Nav::Down if *sel + 1 < AMBIENT_ITEMS.len() => {
                    *sel += 1;
                    moved = true;
                }
                Nav::Back => {
                    self.screen = Screen::Menu;
                    self.sel = 5;
                    moved = true;
                }
                _ => {}
            },
            Screen::Ambient => {
                if nav == Nav::Back {
                    self.screen = Screen::AmbientHub { sel: 1 };
                    moved = true;
                }
            }
            Screen::Frame => match nav {
                Nav::Right | Nav::Down => {
                    self.next_photo();
                    moved = true;
                }
                Nav::Back => {
                    self.close_frame();
                    moved = true;
                }
                _ => {}
            },
            Screen::Monitor { page } => match nav {
                Nav::Left | Nav::Up if *page > 0 => {
                    *page -= 1;
                    moved = true;
                }
                Nav::Right | Nav::Down if *page + 1 < MONITOR_PAGES => {
                    *page += 1;
                    moved = true;
                }
                Nav::Back => {
                    self.screen = Screen::AmbientHub { sel: 2 };
                    moved = true;
                }
                _ => {}
            },
            Screen::Settings { sel } => match nav {
                // Two columns: up and down stay in one, left and right cross
                // to the other at the same height.
                Nav::Up if *sel % SETTINGS_HALF > 0 => {
                    *sel -= 1;
                    moved = true;
                }
                Nav::Down if *sel % SETTINGS_HALF + 1 < SETTINGS_HALF => {
                    *sel += 1;
                    moved = true;
                }
                Nav::Left | Nav::Right => {
                    *sel = (*sel + SETTINGS_HALF) % SETTINGS_ROWS;
                    moved = true;
                }
                Nav::Back => {
                    self.screen = Screen::Menu;
                    moved = true;
                }
                _ => {}
            },
            Screen::Power { sel } => match nav {
                Nav::Up if *sel > 0 => {
                    *sel -= 1;
                    self.armed = None;
                    moved = true;
                }
                Nav::Down if *sel + 1 < POWER_ITEMS.len() => {
                    *sel += 1;
                    self.armed = None;
                    moved = true;
                }
                Nav::Back => {
                    self.armed = None;
                    self.screen = Screen::Menu;
                    moved = true;
                }
                _ => {}
            },
            Screen::Saver { sel } => match nav {
                Nav::Up if *sel > 0 => {
                    *sel -= 1;
                    moved = true;
                }
                Nav::Down if *sel + 1 < SAVER_ROWS => {
                    *sel += 1;
                    moved = true;
                }
                Nav::Left | Nav::Right => {
                    let row = *sel;
                    let dir = if nav == Nav::Right { 1 } else { -1 };
                    self.adjust_saver(row, dir);
                    moved = true;
                }
                Nav::Back => {
                    self.save_settings();
                    self.screen = Screen::Settings {
                        sel: settings_row(Page::Saver),
                    };
                    self.pending.push(Sound::Lock);
                    return;
                }
                _ => {}
            },
            Screen::Diag { top } => match nav {
                Nav::Up if *top > 0 => {
                    *top -= 1;
                    moved = true;
                }
                Nav::Down if *top + 12 < self.diag.len() => {
                    *top += 1;
                    moved = true;
                }
                Nav::Back => {
                    self.screen = Screen::Settings {
                        sel: settings_row(Page::Diag),
                    };
                    moved = true;
                }
                _ => {}
            },
            Screen::Style { sel } => {
                let n = self.themes.len() + 1;
                let mut changed = false;
                match nav {
                    Nav::Up if *sel > 0 => {
                        *sel -= 1;
                        changed = true;
                    }
                    Nav::Down if *sel + 1 < n => {
                        *sel += 1;
                        changed = true;
                    }
                    Nav::Back => {
                        self.save_settings();
                        self.screen = Screen::Settings {
                            sel: settings_row(Page::Style),
                        };
                        self.pending.push(Sound::Lock);
                        return;
                    }
                    _ => {}
                }
                if changed {
                    let name = if *sel == 0 {
                        "system".to_string()
                    } else {
                        self.themes[*sel - 1].0.clone()
                    };
                    self.settings.theme = name.clone();
                    self.apply_theme(&name);
                    moved = true;
                }
            }
            Screen::VideoFit { sel } => match nav {
                Nav::Up if *sel > 0 => {
                    *sel -= 1;
                    moved = true;
                }
                Nav::Down if *sel + 1 < FIT_ROWS => {
                    *sel += 1;
                    moved = true;
                }
                Nav::Left | Nav::Right => {
                    let row = *sel;
                    let dir = if nav == Nav::Right { 1 } else { -1 };
                    self.adjust_fit(row, dir);
                    moved = true;
                }
                Nav::Back => {
                    self.save_settings();
                    self.screen = Screen::Settings {
                        sel: settings_row(Page::Fit),
                    };
                    self.pending.push(Sound::Lock);
                    return;
                }
                _ => {}
            },
            Screen::Resume { sel } => {
                match nav {
                    Nav::Up if *sel > 0 => {
                        *sel -= 1;
                        moved = true;
                    }
                    Nav::Down if *sel + 1 < 2 => {
                        *sel += 1;
                        moved = true;
                    }
                    Nav::Back => {
                        // Never mind: back to the list the game came from.
                        if let Some((_, from)) = self.pending_entry.take() {
                            self.screen = *from;
                        }
                        self.pending.push(Sound::Lock);
                        return;
                    }
                    _ => {}
                }
            }
            Screen::Equalizer { band } => {
                let b = *band;
                match nav {
                    Nav::Left if b > 0 => {
                        *band = b - 1;
                        moved = true;
                    }
                    Nav::Right if b + 1 < music::EQ_BANDS => {
                        *band = b + 1;
                        moved = true;
                    }
                    Nav::Up | Nav::Down => {
                        let step = if nav == Nav::Up { 1.0 } else { -1.0 };
                        let now = self.music.eq_bands()[b];
                        self.music.eq_set_band(b, now + step);
                        moved = true;
                    }
                    Nav::Back => {
                        self.screen = Screen::Music {
                            sel: self.music_root_sel,
                            top: 0,
                        };
                        self.pending.push(Sound::Lock);
                        return;
                    }
                    _ => {}
                }
            }
            Screen::MusicSettings { sel } => match nav {
                Nav::Up if *sel > 0 => {
                    *sel -= 1;
                    moved = true;
                }
                Nav::Down if *sel + 1 < MUSIC_ROWS + deck::MODES => {
                    *sel += 1;
                    moved = true;
                }
                Nav::Left | Nav::Right => {
                    let row = *sel;
                    let dir = if nav == Nav::Right { 1 } else { -1 };
                    self.adjust_music(row, dir);
                    moved = true;
                }
                Nav::Back => {
                    self.save_settings();
                    self.screen = Screen::Settings {
                        sel: settings_row(Page::Music),
                    };
                    self.pending.push(Sound::Lock);
                    return;
                }
                _ => {}
            },
            Screen::FrameSettings { sel } => match nav {
                Nav::Up if *sel > 0 => {
                    *sel -= 1;
                    moved = true;
                }
                Nav::Down if *sel + 1 < FRAME_ROWS => {
                    *sel += 1;
                    moved = true;
                }
                Nav::Left | Nav::Right => {
                    let row = *sel;
                    let dir = if nav == Nav::Right { 1 } else { -1 };
                    self.adjust_frame(row, dir);
                    moved = true;
                }
                Nav::Back => {
                    self.save_settings();
                    self.screen = Screen::Settings {
                        sel: settings_row(Page::Frame),
                    };
                    self.pending.push(Sound::Lock);
                    return;
                }
                _ => {}
            },
            Screen::AmbientSettings { sel } => match nav {
                Nav::Up if *sel > 0 => {
                    *sel -= 1;
                    moved = true;
                }
                Nav::Down if *sel + 1 < AMBIENT_ROWS => {
                    *sel += 1;
                    moved = true;
                }
                Nav::Left | Nav::Right => {
                    let row = *sel;
                    let dir = if nav == Nav::Right { 1 } else { -1 };
                    self.adjust_ambient(row, dir);
                    moved = true;
                }
                Nav::Back => {
                    self.save_settings();
                    self.screen = Screen::Settings {
                        sel: settings_row(Page::Ambient),
                    };
                    self.pending.push(Sound::Lock);
                    return;
                }
                _ => {}
            },
            Screen::SoundSettings { sel } => match nav {
                Nav::Up if *sel > 0 => {
                    *sel -= 1;
                    moved = true;
                }
                Nav::Down if *sel + 1 < SOUND_ROWS => {
                    *sel += 1;
                    moved = true;
                }
                Nav::Left | Nav::Right => {
                    let row = *sel;
                    let dir = if nav == Nav::Right { 1 } else { -1 };
                    self.adjust_sound(row, dir);
                    moved = true;
                }
                Nav::Back => {
                    self.save_settings();
                    self.screen = Screen::Settings {
                        sel: settings_row(Page::Sound),
                    };
                    self.pending.push(Sound::Lock);
                    return;
                }
                _ => {}
            },
            Screen::VideoSettings { sel } => match nav {
                Nav::Up if *sel > 0 => {
                    *sel -= 1;
                    moved = true;
                }
                Nav::Down if *sel + 1 < VIDEOS_ROWS => {
                    *sel += 1;
                    moved = true;
                }
                Nav::Left | Nav::Right => {
                    let row = *sel;
                    let dir = if nav == Nav::Right { 1 } else { -1 };
                    self.adjust_videos(row, dir);
                    moved = true;
                }
                Nav::Back => {
                    self.save_settings();
                    self.screen = Screen::Settings {
                        sel: settings_row(Page::Videos),
                    };
                    self.pending.push(Sound::Lock);
                    return;
                }
                _ => {}
            },
            Screen::About { top } => match nav {
                Nav::Up if *top > 0 => {
                    *top -= 1;
                    moved = true;
                }
                Nav::Down if *top + 14 < ABOUT.len() => {
                    *top += 1;
                    moved = true;
                }
                Nav::Back => {
                    self.screen = Screen::Settings {
                        sel: settings_row(Page::About),
                    };
                    moved = true;
                }
                _ => {}
            },
            Screen::Music { sel, .. } => {
                match nav {
                    Nav::Up if *sel > 0 => {
                        *sel -= 1;
                        moved = true;
                    }
                    Nav::Down if *sel + 1 < music_rows => {
                        *sel += 1;
                        moved = true;
                    }
                    Nav::Back => {
                        if self.music_hub.is_some() {
                            self.music_hub = None;
                            let back = self.music_hub_sel;
                            self.screen = Screen::Music { sel: back, top: 0 };
                        } else {
                            self.music_root_sel = *sel;
                            self.screen = Screen::Menu;
                        }
                        moved = true;
                    }
                    _ => {}
                }
                if let Screen::Music { sel, top } = &mut self.screen {
                    if *sel < *top {
                        *top = *sel;
                    } else if *sel >= *top + Self::ROWS_PER_PAGE {
                        *top = *sel + 1 - Self::ROWS_PER_PAGE;
                    }
                }
            }
            Screen::MusicList { sel, top } => {
                let page = page_rows;
                match nav {
                    Nav::Up if *sel > 0 => {
                        *sel -= 1;
                        moved = true;
                    }
                    Nav::Down if *sel + 1 < music_items => {
                        *sel += 1;
                        moved = true;
                    }
                    Nav::Right if music_items > 0 => {
                        *sel = (*sel + page).min(music_items - 1);
                        moved = true;
                    }
                    Nav::Left if *sel > 0 => {
                        *sel = sel.saturating_sub(page);
                        moved = true;
                    }
                    Nav::Back => {
                        self.music_back();
                        return;
                    }
                    _ => {}
                }
                if *sel < *top {
                    *top = *sel;
                } else if *sel >= *top + page {
                    *top = *sel + 1 - page;
                }
            }
            Screen::PadWizard => {
                if nav == Nav::Back {
                    self.wizard = None;
                    self.screen = Screen::Settings {
                        sel: settings_row(Page::Pads),
                    };
                    self.message = Some(("pad mapping cancelled".into(), self.now + 3.0));
                    moved = true;
                }
            }
            Screen::NowPlaying => match nav {
                Nav::Left | Nav::Right if self.music_visual => {
                    self.music_mode_step(if nav == Nav::Left { -1 } else { 1 });
                    moved = true;
                }
                Nav::Left => {
                    if !self.tune(-1) {
                        self.music.prev();
                    }
                    moved = true;
                }
                Nav::Right => {
                    if !self.tune(1) {
                        self.music.next();
                    }
                    moved = true;
                }
                Nav::Up => {
                    self.music.volume_step(3.0);
                    moved = true;
                }
                Nav::Down => {
                    self.music.volume_step(-3.0);
                    moved = true;
                }
                Nav::Back => {
                    if self.music_visual {
                        self.music_visual = false;
                    } else {
                        let sel = self.music_root_sel;
                        self.screen = Screen::Music { sel, top: 0 };
                    }
                    moved = true;
                }
            },
            Screen::Menu => {}
        }
        if moved {
            self.pending.push(Sound::Move);
        }
    }

    /// Start a game by its path, for `omacrt play` and the desktop.
    ///
    /// The path comes already resolved: the CLI has the index and does the
    /// matching, so what arrives here is a file the scan has seen. The system
    /// it belongs to comes from the same index, and the game itself from the
    /// library, so a launch from the desktop is the launch the launcher would
    /// have done from its own list.
    pub fn play(&mut self, target: &str) {
        if self.running.is_some() || self.launching.is_some() || self.player.is_some() {
            self.message = Some(("busy: something is already running".into(), self.now + 3.0));
            return;
        }
        self.wake();
        self.menu_live = true;
        self.chime_played = true;
        let path = PathBuf::from(target);
        let Some(index) = crate::index::Index::load() else {
            self.message = Some(("no library index; run a scan".into(), self.now + 4.0));
            return;
        };
        let Some(item) = index.items.iter().find(|i| i.path == path) else {
            self.message = Some(("not in the library".into(), self.now + 4.0));
            return;
        };
        let Some(sys) = self
            .library
            .systems
            .iter()
            .position(|s| s.name == item.system)
        else {
            let missing = item.system.clone();
            self.message = Some((
                format!("no system {missing} in systems.toml"),
                self.now + 4.0,
            ));
            return;
        };
        // The library's own game, so nothing about the launch differs from
        // choosing it on the tube: the CRT ready conversion beside a video,
        // the title as the list shows it.
        let game = self
            .library
            .games(&self.library.systems[sys])
            .into_iter()
            .find(|g| g.path == path)
            .unwrap_or(Game {
                title: item.title.clone(),
                path: path.clone(),
                crt_path: None,
                folder: false,
            });
        self.list_from_home = false;
        let _ = self.run_entry(&Entry { game, sys });
    }

    pub(super) fn activate_browser(&mut self) -> Action {
        match self.screen {
            Screen::Systems { sel, .. } => {
                self.pending.push(Sound::Select);
                self.list_from_home = false;
                self.open_collection = None;
                match sel {
                    0 => {
                        self.virtual_row = 0;
                        let list = self.recent.clone();
                        self.open_virtual(&list);
                    }
                    1 => {
                        self.virtual_row = 1;
                        let list = self.favorites.clone();
                        self.open_virtual(&list);
                    }
                    2 => self.go(Screen::Collections { sel: 0, top: 0 }),
                    i => match self.system_at_row(i) {
                        Some(sys) => self.open_games(Some(sys)),
                        None => return Action::None,
                    },
                }
                Action::None
            }
            Screen::Collections { sel, .. } => {
                let lists = self.library.collections();
                if let Some((_, items)) = lists.get(sel) {
                    self.pending.push(Sound::Select);
                    self.virtual_row = 2;
                    self.open_collection = Some(sel);
                    let list = items.clone();
                    self.open_virtual(&list);
                }
                Action::None
            }
            Screen::Games { sel, sys, .. } => {
                if self.yt_query && self.search.is_some() {
                    self.yt_submit();
                    return Action::None;
                }
                let Some(entry) = self.games.get(sel).cloned() else {
                    return Action::None;
                };
                if entry.game.folder {
                    if let Some(i) = sys {
                        self.enter_folder(i, entry.game.path.clone());
                    }
                    return Action::None;
                }
                self.run_entry(&entry)
            }
            Screen::Profile { sel } => match sel {
                5 => self.run_test_pattern(),
                6 => {
                    if let Err(e) = self.profile.save(&self.library.config_dir) {
                        eprintln!("profile: {e}");
                    }
                    self.pending.push(Sound::Lock);
                    self.message = Some(("profile saved".into(), self.now + 3.0));
                    Action::None
                }
                _ => Action::None,
            },
            Screen::Pair { sel } => {
                self.pending.push(Sound::Select);
                if self.bt.devices.is_empty() {
                    self.bt.start_scan();
                } else {
                    self.bt.pair(sel);
                }
                Action::None
            }
            Screen::Settings { sel } => self.activate_settings(sel),
            Screen::Videos { sel } => {
                self.pending.push(Sound::Select);
                let Some(i) = self.video_system() else {
                    return Action::None;
                };
                match sel {
                    0 => {
                        self.list_from_home = false;
                        self.open_games(Some(i));
                        self.games_back = Some(Screen::Videos { sel: 0 });
                        self.pending.push(Sound::Whoosh);
                    }
                    1 => self.go(Screen::YouTube { sel: 0 }),
                    _ => match yt::clipboard_link() {
                        Some(link) => {
                            let e = Entry {
                                game: Game {
                                    title: watch_title(&link, ""),
                                    path: PathBuf::from(link),
                                    crt_path: None,
                                    folder: false,
                                },
                                sys: i,
                            };
                            return self.run_entry(&e);
                        }
                        None => {
                            self.message =
                                Some(("no link in the clipboard".into(), self.now + 3.0));
                            self.pending.push(Sound::Crunch);
                        }
                    },
                }
                Action::None
            }
            Screen::YouTube { sel } => {
                self.pending.push(Sound::Select);
                let Some(i) = self.video_system() else {
                    return Action::None;
                };
                match sel {
                    0 => {
                        // An empty list with the bar asking for the query.
                        self.open_links(Vec::new(), Screen::YouTube { sel: 0 });
                        self.search = Some(String::new());
                        self.osk = if self.pad == PadKind::Keyboard {
                            None
                        } else {
                            Some((1, 0))
                        };
                        self.yt_query = true;
                    }
                    1 => {
                        let list = self.watch_later(i);
                        self.open_links(list, Screen::YouTube { sel: 1 });
                    }
                    _ => {
                        let list = self.recent_links(i);
                        self.open_links(list, Screen::YouTube { sel: 2 });
                    }
                }
                Action::None
            }
            Screen::Power { sel } => self.activate_power(sel),
            Screen::Saver { sel } => {
                if sel == 4 {
                    // Preview: whatever the setting says, right now.
                    self.pending.push(Sound::Select);
                    let now = self.now;
                    self.idle_reached(now);
                } else {
                    self.adjust_saver(sel, 1);
                    self.pending.push(Sound::Move);
                }
                Action::None
            }
            Screen::Style { .. } => {
                self.save_settings();
                self.pending.push(Sound::Lock);
                self.message = Some((
                    format!("theme {} saved", self.settings.theme),
                    self.now + 3.0,
                ));
                Action::None
            }
            Screen::VideoFit { sel } => {
                self.adjust_fit(sel, 1);
                self.pending.push(Sound::Move);
                Action::None
            }
            Screen::MusicSettings { sel } => {
                self.adjust_music(sel, 1);
                self.pending.push(Sound::Move);
                Action::None
            }
            Screen::VideoSettings { sel } => {
                if sel == 2 {
                    self.pending.push(Sound::Select);
                    self.go(Screen::VideoFit { sel: 0 });
                } else {
                    self.adjust_videos(sel, 1);
                    self.pending.push(Sound::Move);
                }
                Action::None
            }
            Screen::Diag { .. }
            | Screen::About { .. }
            | Screen::Monitor { .. }
            | Screen::Ambient => Action::None,
            Screen::AmbientHub { sel } => {
                self.pending.push(Sound::Select);
                match sel {
                    0 => self.open_frame(),
                    1 => self.go(Screen::Ambient),
                    _ => {
                        self.sysmon.sample();
                        self.sysmon_at = 0.0;
                        self.go(Screen::Monitor { page: 0 });
                    }
                }
                Action::None
            }
            Screen::Frame => {
                self.next_photo();
                Action::None
            }
            Screen::FrameSettings { .. } => {
                self.save_settings();
                self.open_frame();
                Action::None
            }
            Screen::AmbientSettings { .. } => {
                // The page itself, the way the frame's own settings show the
                // frame: a settings page is not the thing it sets up.
                self.save_settings();
                self.go(Screen::Ambient);
                Action::None
            }
            Screen::SoundSettings { .. } => {
                self.save_settings();
                Action::None
            }
            Screen::Music { sel, .. } => {
                match self.music_rows().get(sel).map(|r| r.3.clone()) {
                    Some(MusicRow::Now) => {
                        self.pending.push(Sound::Select);
                        self.music_root_sel = sel;
                        self.go(Screen::NowPlaying);
                    }
                    Some(MusicRow::Source(src)) => {
                        self.music_root_sel = sel;
                        self.music_path.clear();
                        self.music_enter(src);
                    }
                    Some(MusicRow::Hub(hub)) => {
                        self.music_hub_sel = sel;
                        self.music_hub = Some(hub);
                        self.pending.push(Sound::Select);
                        self.go(Screen::Music { sel: 0, top: 0 });
                    }
                    Some(MusicRow::Equalizer) => {
                        self.music_root_sel = sel;
                        self.pending.push(Sound::Select);
                        self.go(Screen::Equalizer { band: 0 });
                    }
                    Some(MusicRow::Search(key)) => {
                        // An empty list with the bar asking for the query.
                        self.music_root_sel = sel;
                        self.music_path.clear();
                        let src = Source::ProviderSearch(key.clone(), String::new());
                        self.music.lists.insert(src.clone(), Ok(Vec::new()));
                        self.music_path.push((src, 0, 0));
                        self.go(Screen::MusicList { sel: 0, top: 0 });
                        self.search = Some(String::new());
                        self.osk = if self.pad == PadKind::Keyboard {
                            None
                        } else {
                            Some((1, 0))
                        };
                        self.music_query = Some(key);
                    }
                    None => {}
                }
                Action::None
            }
            Screen::MusicList { sel, .. } => {
                if let Some(key) = self.music_query.clone() {
                    let q = self.search.clone().unwrap_or_default();
                    if !q.trim().is_empty() {
                        self.music_path.pop();
                        self.music_query = None;
                        self.music_enter(Source::ProviderSearch(key, q.trim().to_string()));
                    }
                    return Action::None;
                }
                if let Some(item) = self.music_selected(sel) {
                    self.music_play_item(sel, item);
                }
                Action::None
            }
            Screen::NowPlaying => {
                self.music_alt(None);
                Action::None
            }
            Screen::Resume { sel } => {
                let Some((entry, from)) = self.pending_entry.take() else {
                    return Action::None;
                };
                self.screen = *from;
                self.pending.push(Sound::Select);
                self.run_entry_resuming(&entry, sel == 0)
            }
            Screen::Equalizer { .. } => {
                // A walks the presets; Flat follows Custom.
                let now = self.music.status.eq_preset.clone();
                let i = music::EQ_PRESETS.iter().position(|p| *p == now);
                let next = match i {
                    Some(k) => music::EQ_PRESETS[(k + 1) % music::EQ_PRESETS.len()],
                    None => music::EQ_PRESETS[0],
                };
                self.music.eq_set_preset(next);
                self.pending.push(Sound::Select);
                self.message = Some((format!("equaliser: {next}"), self.now + 2.0));
                Action::None
            }
            Screen::PadWizard => {
                // Enter skips the control the pad does not have.
                if let Some(w) = self.wizard.as_mut() {
                    w.skip();
                    self.pending.push(Sound::Move);
                    if w.finished {
                        self.pad_wizard_finish();
                    }
                }
                Action::None
            }
            Screen::Menu => Action::None,
        }
    }

    /// The aspect the player picked for this system, as the ratio the picture
    /// should be shown at, or None when it is `fill` or when no core has run
    /// for this system yet and there is nothing to work it out from.
    fn chosen_aspect(&self, system: &crate::library::System) -> Option<f32> {
        if system.is_video() || system.aspect.is_empty() || system.aspect == "fill" {
            return None;
        }
        crate::library::picture_of(&system.name)?.wanted(&system.aspect)
    }

    /// Launch, asking first when the game was left in the middle: RetroArch
    /// would otherwise pick the state up without a word.
    pub(super) fn run_entry(&mut self, entry: &Entry) -> Action {
        let system = self.library.systems[entry.sys].clone();
        if !system.is_video() && self.states.latest(&entry.game.path).is_some() {
            self.pending_entry = Some((entry.clone(), Box::new(self.screen)));
            self.pending.push(Sound::Move);
            self.go(Screen::Resume { sel: 0 });
            return Action::None;
        }
        self.run_entry_resuming(entry, true)
    }

    fn run_entry_resuming(&mut self, entry: &Entry, resume: bool) -> Action {
        let system = self.library.systems[entry.sys].clone();
        let extra = if system.is_video() {
            let hex = |c: Color| format!("{c:06x}");
            let mut lines = vec![format!(
                "{},{},{},{}",
                hex(self.theme.accent),
                hex(self.theme.dim),
                hex(self.theme.paper),
                hex(self.theme.selection)
            )];
            let is_url = entry.game.path.to_string_lossy().starts_with("http");
            if entry.game.crt_path.is_none() {
                // A link cannot be probed before it plays: treat it as the
                // 16:9 progressive video it almost always is.
                let probe = if is_url {
                    videofit::Probe {
                        width: 1920,
                        height: 1080,
                        fps: 30.0,
                        duration: 0.0,
                        interlaced: false,
                        hdr: false,
                    }
                } else {
                    videofit::probe(&entry.game.path)
                };
                let plan = videofit::plan(&probe, &self.settings.video);
                self.message = Some((format!("fit: {}", plan.label()), self.now + 4.0));
                lines.extend(plan.mpv_args());
                if is_url {
                    // The tube shows 240 lines: a 480p H.264 stream is all it
                    // needs, and it decodes without heating the room.
                    let q = self.settings.videos.yt_quality;
                    lines.push(format!(
                        "--ytdl-format=bestvideo[height<={q}][vcodec^=avc1]+bestaudio/best[height<={q}]/best"
                    ));
                }
            }
            if self.wide_output() {
                lines.push("--keepaspect=no".into());
            }
            lines.join("\n")
        } else {
            let mut keys = self.profile.retroarch_keys();
            if self.wide_output() {
                // Fill the frame (aspect 24 = Full): the tube turns the wide frame back into 4:3.
                // The window is as tall as the mode the tube switches to for
                // this system, not as the mode showing right now.
                let (w, mut h) = self.output_size;
                if !system.is_video() {
                    let pinned = match crate::library::VideoPolicy::parse(&system.video) {
                        crate::library::VideoPolicy::Fixed(_, ph) => Some(ph),
                        _ => None,
                    };
                    if let Some(l) = system.lines.or(pinned) {
                        h = l;
                    }
                }
                keys.push_str(&format!(
                    // `video_fullscreen_x/y` as well as the viewport: it is
                    // the size the emulator lays its picture out for, and
                    // without it a mode change that lands late leaves the game
                    // in a column in the middle of a frame it thinks is
                    // smaller than it is.
                    "aspect_ratio_index = \"24\"\nvideo_aspect_ratio = \"{:.4}\"\nvideo_scale_integer = \"false\"\nvideo_fullscreen_x = \"{w}\"\nvideo_fullscreen_y = \"{h}\"\ncustom_viewport_x = \"0\"\ncustom_viewport_y = \"0\"\ncustom_viewport_width = \"{w}\"\ncustom_viewport_height = \"{h}\"\nvideo_windowed_position_width = \"{w}\"\nvideo_windowed_position_height = \"{h}\"\nvideo_window_auto_width_max = \"{w}\"\nvideo_window_auto_height_max = \"{h}\"\n",
                    w as f32 / h as f32
                ));
                // The aspect the player chose in the pause menu, which wins
                // because the emulator keeps the last value of a key. The
                // frame here is thousands of pixels wide and the set shows it
                // as 4:3, so the viewport is worked out from the picture the
                // core last drew rather than left to RetroArch, which would
                // fit square pixels into the frame and leave the game in a
                // sliver in the middle of the screen.
                if let Some(wanted) = self.chosen_aspect(&system) {
                    keys.push_str(&crate::library::viewport_keys(w, h, 4.0 / 3.0, wanted));
                }
            } else if self.chosen_aspect(&system).is_some() {
                // In a window the pixels are square and RetroArch's own
                // choice does the fitting.
                keys.push_str(if system.aspect == "pixel" {
                    "aspect_ratio_index = \"21\"\n"
                } else {
                    "aspect_ratio_index = \"22\"\n"
                });
            }
            keys
        };
        // Pinned frame heights become real line counts on a wide output the
        // host controls: a 224 line game gets 224 lines on the tube.
        // Geometry the tube switches to for this program: the system's own
        // line count, else a pinned frame height, plus its picture shift.
        let lines = if self.wide_output() && !system.is_video() {
            let pinned = match crate::library::VideoPolicy::parse(&system.video) {
                crate::library::VideoPolicy::Fixed(_, h) => Some(h),
                _ => None,
            };
            let l = system.lines.or(pinned);
            if l.is_some() || system.shift_x != 0 || system.shift_y != 0 {
                Some(Geometry {
                    lines: l,
                    shift_x: system.shift_x,
                    shift_y: system.shift_y,
                    follow: pinned.is_none(),
                })
            } else {
                None
            }
        } else {
            None
        };
        self.music.hush();
        match self
            .library
            .command_resuming(&system, &entry.game, &extra, resume)
        {
            Ok(cmd) if system.is_video() => {
                self.pending.push(Sound::Whoosh);
                self.player = Some(Player::new(self.library.mpv_socket(), &entry.game.title));
                self.launching = Some(Launch {
                    cmd,
                    title: entry.game.title.clone(),
                    system: system.name.clone(),
                    disc: true,
                    color: self.theme.yellow,
                    started: self.now - LAUNCH_SECS as f64, // no animation, start right away
                    spawned: false,
                    lines,
                });
                self.running = Some((entry.game.title.clone(), system.name.clone()));
                self.running_path = Some((system.name.clone(), entry.game.path.clone()));
                self.remember(entry);
                Action::None
            }
            Ok(cmd) => {
                let disc = matches!(
                    system.name.as_str(),
                    "psx" | "dreamcast" | "segacd" | "saturn" | "pcenginecd" | "neocd" | "3do"
                );
                let color = icons::system_logo(&system.name)
                    .map(|(_, c)| c)
                    .unwrap_or(self.theme.accent);
                self.pending
                    .push(if disc { Sound::Whoosh } else { Sound::Insert });
                self.launching = Some(Launch {
                    cmd,
                    title: entry.game.title.clone(),
                    system: system.name.clone(),
                    disc,
                    color,
                    started: self.now,
                    spawned: false,
                    lines,
                });
                self.running = Some((entry.game.title.clone(), system.name.clone()));
                self.running_path = Some((system.name.clone(), entry.game.path.clone()));
                self.remember(entry);
                Action::None
            }
            Err(e) => {
                self.message = Some((format!("cannot launch: {e}"), self.now + 4.0));
                Action::None
            }
        }
    }

    /// Any ROM whose title mentions 240p (the 240p Test Suite) doubles as a
    /// geometry test pattern.
    fn run_test_pattern(&mut self) -> Action {
        for (i, system) in self.library.systems.clone().iter().enumerate() {
            if let Some(game) = self
                .library
                .games(system)
                .into_iter()
                .find(|g| g.title.to_lowercase().contains("240p"))
            {
                return self.run_entry(&Entry { game, sys: i });
            }
        }
        self.message = Some(("no 240p test suite rom found".into(), self.now + 4.0));
        Action::None
    }

    fn remember(&mut self, entry: &Entry) {
        self.recent.retain(|(_, p)| *p != entry.game.path);
        self.recent.insert(0, (entry.sys, entry.game.path.clone()));
        self.recent.truncate(20);
        self.recent_at
            .insert(entry.game.path.clone(), chrono::Local::now().timestamp());
        save_recent(
            &self.library.config_dir.join("recent.txt"),
            &self.recent,
            &self.recent_at,
            &self.library,
        );
    }

    /// Toggle the selected game in the favorites list.
    pub fn toggle_favorite(&mut self) {
        if matches!(self.screen, Screen::NowPlaying) {
            self.sleep_cycle();
            return;
        }
        if let Screen::MusicList { sel, .. } = self.screen {
            if let Some(MusicItem::Track(t)) = self.music_selected(sel) {
                let label = t.label();
                let starred = self.music.toggle_favorite(&t);
                self.message = Some((
                    if starred {
                        format!("favourite: {label}")
                    } else {
                        format!("removed {label}")
                    },
                    self.now + 2.0,
                ));
                self.pending.push(Sound::Select);
            }
            return;
        }
        let Screen::Games { sel, .. } = self.screen else {
            return;
        };
        let Some(entry) = self.games.get(sel).cloned() else {
            return;
        };
        if entry.game.path.to_string_lossy().starts_with("http") {
            self.toggle_watch_later(&entry);
            return;
        }
        let key = (entry.sys, entry.game.path.clone());
        if let Some(pos) = self.favorites.iter().position(|k| *k == key) {
            self.favorites.remove(pos);
            self.message = Some((format!("removed {}", entry.game.title), self.now + 2.0));
        } else {
            self.favorites.push(key);
            self.message = Some((format!("favorite: {}", entry.game.title), self.now + 2.0));
        }
        self.pending.push(Sound::Select);
        save_list(
            &self.library.config_dir.join("favorites.txt"),
            &self.favorites,
            &self.library,
        );
    }

    fn is_favorite(&self, entry: &Entry) -> bool {
        self.favorites
            .iter()
            .any(|(i, p)| *i == entry.sys && *p == entry.game.path)
    }

    /// The game process ended; back to the list, cursor where it was.
    pub fn game_finished(&mut self, ok: bool) {
        self.running = None;
        if let Some((_, path)) = self.running_path.take() {
            self.states.forget(&path);
        }
        self.paused = None;
        self.launching = None;
        self.player = None;
        self.last_input = self.now;
        self.pending
            .push(if ok { Sound::Lock } else { Sound::Crunch });
        if !ok {
            self.message = Some(("retroarch exited with an error".into(), self.now + 4.0));
        }
    }

    pub(super) fn draw_row(
        &self,
        fb: &mut Framebuffer,
        y: i32,
        label: &str,
        right: &str,
        on: bool,
        color: Color,
    ) {
        let margin = (fb.w as f32 * 0.05) as i32;
        let w = fb.w as i32 - self.row_shrink;
        let left = margin + self.slide();
        let max_cols = ((w - 2 * margin) / 8) as usize;
        if on {
            fb.rect(left, y - 2, w - 2 * margin, 12, self.band_color());
        }
        let room = max_cols.saturating_sub(right.chars().count() + 1);
        let full = format!("  {label}");
        let count = full.chars().count();
        let text: String = if on && count > room {
            // Marquee: pause, scroll left, pause, from the start again.
            let span = (count - room + 2) as f64;
            let cycle = span * 0.28 + 1.6;
            let t = (self.now % cycle) - 0.9;
            let off = (t / 0.28).clamp(0.0, span).floor() as usize;
            let padded = format!("{full}   ");
            padded.chars().cycle().skip(off).take(room).collect()
        } else {
            full.chars().take(room).collect()
        };
        fb.text(
            left,
            y,
            &text,
            if on { self.theme.accent } else { color },
            1,
        );
        fb.text(
            left + w - 2 * margin - Framebuffer::text_width(right, 1),
            y,
            right,
            if on {
                self.theme.accent
            } else {
                self.theme.dim
            },
            1,
        );
    }

    pub(super) fn draw_browser(&mut self, fb: &mut Framebuffer) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32;
        let max_cols = ((w - 2 * left) / 8) as usize;
        let cut = |s: &str, n: usize| -> String { s.chars().take(n).collect() };
        let row_h = 12;
        match self.screen {
            Screen::Systems { sel, top } => {
                let y0 = self.draw_header(fb, "Games");
                let ox = self.slide();
                let browse = self.browse_systems();
                let systems: Vec<crate::library::System> = browse
                    .iter()
                    .map(|&i| self.library.systems[i].clone())
                    .collect();
                // The selected console sits on the right; rows make room.
                let panel = 72;
                self.row_shrink = panel + 8;
                if sel >= Self::VIRTUAL
                    && let Some(s) = systems.get(sel - Self::VIRTUAL)
                {
                    let name = s.name.clone();
                    let px = w - left - panel;
                    let py = y0 + 6;
                    if let Some(img) = self.art.system_image(&name, panel as usize) {
                        let img = img.clone();
                        fb.blit(
                            px + (panel - img.w as i32) / 2,
                            py + (panel - img.h as i32) / 2,
                            &img,
                        );
                    } else if let Some((logo, c)) = icons::system_logo(&name) {
                        fb.bitmap(px + panel / 2 - 10, py + panel / 2 - 10, logo, c, 2, 10);
                    }
                    let label = crate::index::catalog(&name)
                        .map(|(l, _, _)| l.to_string())
                        .unwrap_or_else(|| name.clone());
                    let words: Vec<&str> = label.split(' ').collect();
                    let mut line = String::new();
                    let mut ly = py + panel + 6;
                    for wd in words {
                        if !line.is_empty() && (line.len() + 1 + wd.len()) * 8 > panel as usize {
                            fb.text(px, ly, &line, scale(self.theme.dim, 0.9), 1);
                            ly += 10;
                            line.clear();
                        }
                        if !line.is_empty() {
                            line.push(' ');
                        }
                        line.push_str(wd);
                    }
                    if !line.is_empty() {
                        fb.text(px, ly, &line, scale(self.theme.dim, 0.9), 1);
                    }
                }
                let total = Self::VIRTUAL + systems.len();
                let end = (top + SYS_PAGE).min(total);
                for (row, i) in (top..end).enumerate() {
                    let y = y0 + row as i32 * row_h;
                    let on = i == sel;
                    let icon_c = if on {
                        self.theme.accent
                    } else {
                        self.theme.dim
                    };
                    match i {
                        0 => {
                            self.draw_row(
                                fb,
                                y,
                                "Recent",
                                &format!("{:>4}", self.recent.len()),
                                on,
                                self.theme.paper,
                            );
                            fb.bitmap(left + ox + 4, y + 1, &icons::CLOCK, icon_c, 1, 8);
                        }
                        1 => {
                            self.draw_row(
                                fb,
                                y,
                                "Favorites",
                                &format!("{:>4}", self.favorites.len()),
                                on,
                                self.theme.paper,
                            );
                            fb.bitmap(left + ox + 4, y + 1, &icons::STAR, icon_c, 1, 8);
                        }
                        2 => {
                            self.draw_row(
                                fb,
                                y,
                                "Collections",
                                &format!("{:>4}", self.library.collections().len()),
                                on,
                                self.theme.paper,
                            );
                            fb.bitmap(left + ox + 4, y + 1, &icons::FOLDER, icon_c, 1, 8);
                        }
                        _ => {
                            let sys = &systems[i - Self::VIRTUAL];
                            let count = browse
                                .get(i - Self::VIRTUAL)
                                .and_then(|&si| self.system_counts.get(si))
                                .copied()
                                .unwrap_or(0);
                            let right = format!(
                                "{count:>4}  {}",
                                crate::library::VideoPolicy::parse(&sys.video).label()
                            );
                            self.draw_row(fb, y, &sys.name, &right, on, self.theme.paper);
                            match icons::system_logo(&sys.name) {
                                Some((logo, c)) => fb.bitmap(
                                    left + ox + 2,
                                    y - 1,
                                    logo,
                                    if on { c } else { scale(c, 0.75) },
                                    1,
                                    10,
                                ),
                                None => {
                                    fb.bitmap(left + ox + 4, y + 1, &icons::CONSOLE, icon_c, 1, 8)
                                }
                            }
                        }
                    }
                }
                if sel >= Self::VIRTUAL
                    && let Some(sys) = systems.get(sel - Self::VIRTUAL)
                {
                    let core = self.library.core_path(sys);
                    let core_ok = core.exists();
                    let info = format!(
                        "{}{}  runahead {}  rewind {}",
                        sys.core,
                        if core_ok { "" } else { " (missing)" },
                        sys.runahead,
                        if sys.rewind { "on" } else { "off" }
                    );
                    fb.text(
                        left,
                        h - 28,
                        &cut(&info, max_cols),
                        if core_ok {
                            self.theme.dim
                        } else {
                            self.theme.red
                        },
                        1,
                    );
                }
                if total > SYS_PAGE {
                    let pos = format!("{}/{}", sel + 1, total);
                    fb.text(
                        w - left - Framebuffer::text_width(&pos, 1),
                        h - 28,
                        &pos,
                        self.theme.dim,
                        1,
                    );
                }
                let hint = self.hint(&[("A", "open"), ("B", "back")]);
                fb.text(left, h - 16, &hint, scale(self.theme.dim, 0.7), 1);
            }
            Screen::Collections { sel, top } => {
                let y0 = self.draw_header(fb, "Collections");
                let lists = self.library.collections();
                if lists.is_empty() {
                    fb.text(left, y0, "no collections yet", self.theme.dim, 1);
                    fb.text(
                        left,
                        y0 + 12,
                        "omacrt library collections import <folder>",
                        scale(self.theme.dim, 0.8),
                        1,
                    );
                } else {
                    let end = (top + SYS_PAGE).min(lists.len());
                    for (row, i) in (top..end).enumerate() {
                        let y = y0 + row as i32 * row_h;
                        let (name, items) = &lists[i];
                        self.draw_row(
                            fb,
                            y,
                            name,
                            &format!("{:>4}", items.len()),
                            i == sel,
                            self.theme.paper,
                        );
                        fb.bitmap(
                            left + self.slide() + 4,
                            y + 1,
                            &icons::FOLDER,
                            if i == sel {
                                self.theme.accent
                            } else {
                                self.theme.dim
                            },
                            1,
                            8,
                        );
                    }
                    let pos = format!("{}/{}", sel + 1, lists.len());
                    fb.text(
                        w - left - Framebuffer::text_width(&pos, 1),
                        h - 28,
                        &pos,
                        self.theme.dim,
                        1,
                    );
                }
                let hint = self.hint(&[("A", "open"), ("B", "back")]);
                fb.text(left, h - 16, &hint, scale(self.theme.dim, 0.7), 1);
            }
            Screen::Games { sys, sel, top } => {
                let prompt = match sys {
                    Some(i) if self.games_back.is_some() && self.library.systems[i].is_video() => {
                        match self.games_back {
                            Some(Screen::YouTube { sel: 0 }) => "YouTube search".to_string(),
                            Some(Screen::YouTube { sel: 1 }) => "Watch later".to_string(),
                            Some(Screen::YouTube { .. }) => "Recently watched".to_string(),
                            _ => "Local videos".to_string(),
                        }
                    }
                    Some(i) => match &self.game_dir {
                        Some(d) => {
                            // The whole path inside the system folder, one crumb per level.
                            let root = crate::library::expand(&self.library.systems[i].dir);
                            let crumbs: Vec<String> = d
                                .strip_prefix(&root)
                                .map(|r| {
                                    r.components()
                                        .map(|c| c.as_os_str().to_string_lossy().to_string())
                                        .collect()
                                })
                                .unwrap_or_else(|_| {
                                    vec![
                                        d.file_name()
                                            .map(|n| n.to_string_lossy().to_string())
                                            .unwrap_or_default(),
                                    ]
                                });
                            format!("{} / {}", self.library.systems[i].name, crumbs.join(" / "))
                        }
                        None => self.library.systems[i].name.clone(),
                    },
                    None => match self.open_collection {
                        Some(ci) => self
                            .library
                            .collections()
                            .get(ci)
                            .map(|(n, _)| n.clone())
                            .unwrap_or_else(|| "Collection".into()),
                        None if self.search_global => "All games".to_string(),
                        None if self.virtual_row == 1 => "Favorites".to_string(),
                        None => "Recent".to_string(),
                    },
                };
                let n = self.games.len();
                if self.flow_view && n > 0 && !self.games[sel.min(n - 1)].game.folder {
                    self.draw_flow(fb, &prompt, sel);
                    return;
                }
                let mut y0 = self.draw_header(fb, &prompt);
                if let Some(q) = self.search.clone() {
                    y0 = self.draw_search_bar(fb, y0, &q, n);
                }
                let page = self.page_rows();
                // Box art of the selected game on the right, once the cursor
                // rests; scrolling fast shows the frame and no downloads.
                let cover_box = 84;
                let with_covers = n > 0
                    && self
                        .games
                        .get(sel)
                        .map(|e| {
                            !e.game.folder
                                && !self.library.systems[e.sys].is_video()
                                && crate::art::system_label(&self.library.systems[e.sys].name)
                                    .is_some()
                        })
                        .unwrap_or(false);
                if with_covers {
                    self.row_shrink = cover_box + 10;
                    let entry = self.games[sel].clone();
                    let system = self.library.systems[entry.sys].name.clone();
                    let bx = w - left - cover_box;
                    let by = y0 + 4;
                    let settled = self.now - self.last_input > 0.12;
                    let img = if settled {
                        self.art
                            .cover(
                                &system,
                                &entry.game.path,
                                cover_box as usize,
                                cover_box as usize,
                            )
                            .cloned()
                    } else {
                        None
                    };
                    let frame = scale(self.theme.dim, 0.6);
                    match img {
                        Some(img) => {
                            let x = bx + (cover_box - img.w as i32) / 2;
                            let y = by + (cover_box - img.h as i32) / 2;
                            fb.rect(x - 1, y - 1, img.w as i32 + 2, img.h as i32 + 2, frame);
                            fb.blit(x, y, &img);
                        }
                        None => {
                            // Dashed frame; a blinking dot while it loads.
                            for i in (0..cover_box).step_by(4) {
                                fb.put(bx + i, by, frame);
                                fb.put(bx + i, by + cover_box - 1, frame);
                                fb.put(bx, by + i, frame);
                                fb.put(bx + cover_box - 1, by + i, frame);
                            }
                            let key = crate::art::Art::cover_key(
                                &system,
                                &entry.game.path,
                                cover_box as usize,
                                cover_box as usize,
                            );
                            if !settled || self.art.loading(&key) {
                                if (self.now * 3.0) as i64 % 2 == 0 {
                                    fb.rect(
                                        bx + cover_box / 2 - 2,
                                        by + cover_box / 2 - 2,
                                        4,
                                        4,
                                        frame,
                                    );
                                }
                            } else {
                                fb.text_centered(
                                    bx + cover_box / 2,
                                    by + cover_box / 2 - 4,
                                    "no art",
                                    frame,
                                    1,
                                );
                            }
                        }
                    }
                    // Tags of the file name under the box: region, revision.
                    let stem = entry
                        .game
                        .path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("");
                    let (_, tags, _, _) = crate::index::parse_name(stem);
                    let mut ty = by + cover_box + 6;
                    for t in tags.iter().take(3) {
                        let t: String = t.chars().take((cover_box / 8) as usize).collect();
                        fb.text(bx, ty, &t, scale(self.theme.dim, 0.9), 1);
                        ty += 10;
                    }
                    if let Some(st) = self.states.latest(&entry.game.path) {
                        let label: String =
                            st.label().chars().take((cover_box / 8) as usize).collect();
                        fb.text(bx, ty + 2, &label, self.theme.green, 1);
                        ty += 10;
                    }
                    if let Some(at) = self.recent_at.get(&entry.game.path) {
                        let when = std::time::UNIX_EPOCH
                            + std::time::Duration::from_secs((*at).max(0) as u64);
                        let label: String = format!("played {}", states::when_label(when))
                            .chars()
                            .take((cover_box / 8) as usize)
                            .collect();
                        fb.text(bx, ty + 2, &label, scale(self.theme.dim, 0.9), 1);
                    }
                }
                if n == 0 && self.yt_query {
                    let msg = if self.yt_search.is_some() {
                        "searching YouTube"
                    } else {
                        "type what to look for, then Enter"
                    };
                    fb.text(left, y0, msg, self.theme.dim, 1);
                } else if n == 0 && self.yt_results {
                    fb.text(left, y0, "no videos found", self.theme.dim, 1);
                } else if n == 0 && self.games_back.is_some() {
                    fb.text(left, y0, "nothing here yet", self.theme.dim, 1);
                } else if n == 0 && self.search.as_deref().is_some_and(|q| !q.is_empty()) {
                    fb.text(left, y0, "no title matches", self.theme.dim, 1);
                } else if n == 0 {
                    match sys {
                        Some(i) => {
                            let dir = crate::library::expand(&self.library.systems[i].dir);
                            fb.text(left, y0, "no games found in", self.theme.dim, 1);
                            fb.text(
                                left,
                                y0 + 12,
                                &cut(&dir.display().to_string(), max_cols),
                                self.theme.paper,
                                1,
                            );
                        }
                        None => fb.text(left, y0, "nothing here yet", self.theme.dim, 1),
                    }
                } else {
                    let end = (top + page).min(n);
                    for (row, i) in (top..end).enumerate() {
                        let y = y0 + row as i32 * row_h;
                        let entry = self.games[i].clone();
                        let mut right = String::new();
                        if sys.is_none() {
                            right.push_str(&self.library.systems[entry.sys].name);
                        }
                        if let Some(c) = &self.conversion {
                            if c.src == entry.game.path {
                                right = format!("{}%", c.percent());
                            }
                        } else if entry.game.crt_path.is_some() {
                            right.push_str(" CRT");
                        }
                        let fav = !entry.game.folder && self.is_favorite(&entry);
                        if entry.game.folder {
                            self.draw_row(fb, y, &entry.game.title, "", i == sel, self.theme.paper);
                            fb.bitmap(
                                left + self.slide() + 4,
                                y + 1,
                                &icons::FOLDER,
                                if i == sel {
                                    self.theme.accent
                                } else {
                                    self.theme.dim
                                },
                                1,
                                8,
                            );
                            continue;
                        }
                        self.draw_row(fb, y, &entry.game.title, &right, i == sel, self.theme.paper);
                        if fav {
                            fb.bitmap(
                                left + self.slide() + 4,
                                y + 1,
                                &icons::STAR,
                                self.theme.yellow,
                                1,
                                8,
                            );
                        } else if !entry.game.folder
                            && !self.library.systems[entry.sys].is_video()
                            && !self.states.get(&entry.game.path).is_empty()
                        {
                            // A game with a save state: it resumes where it was left.
                            fb.bitmap(
                                left + self.slide() + 6,
                                y + 1,
                                &icons::RESUME,
                                if i == sel {
                                    self.theme.accent
                                } else {
                                    self.theme.green
                                },
                                1,
                                8,
                            );
                        }
                    }
                    let pos = format!("{}/{}", sel + 1, n);
                    fb.text(
                        w - left - Framebuffer::text_width(&pos, 1),
                        h - 28,
                        &pos,
                        self.theme.dim,
                        1,
                    );
                }
                let is_video = sys
                    .map(|i| self.library.systems[i].is_video())
                    .unwrap_or(false);
                if self.osk.is_some() {
                    self.draw_osk(fb);
                }
                let keyboard = self.pad == PadKind::Keyboard;
                let hint = if self.osk.is_some() {
                    self.hint(&[("A", "type"), ("X", "del"), ("Y", "space"), ("B", "done")])
                } else if is_video && matches!(self.games_back, Some(Screen::YouTube { .. })) {
                    self.hint(&[("A", "play"), ("Y", "later"), ("B", "back")])
                } else if is_video {
                    self.hint(&[("A", "play"), ("X", "convert"), ("Y", "fav"), ("B", "back")])
                } else if keyboard {
                    self.hint(&[("A", "run"), ("X", "covers"), ("Y", "fav"), ("/", "find")])
                } else {
                    self.hint(&[("A", "run"), ("X", "covers"), ("Y", "fav"), ("LT", "find")])
                };
                fb.text(left, h - 16, &hint, scale(self.theme.dim, 0.7), 1);
            }
            Screen::Profile { sel } => {
                let y0 = self.draw_header(fb, "TV");
                let p = self.profile.clone();
                let rows: [(&str, String); 7] = [
                    ("monitor", p.monitor.clone()),
                    ("h shift", format!("{:+}", p.h_shift)),
                    ("v shift", format!("{:+}", p.v_shift)),
                    ("h size", format!("{:.2}", p.h_size)),
                    (
                        "invert sync",
                        if p.invert_sync {
                            "on".into()
                        } else {
                            "off".into()
                        },
                    ),
                    ("test pattern", "240p suite".into()),
                    ("save", String::new()),
                ];
                for (i, (label, value)) in rows.iter().enumerate() {
                    let y = y0 + i as i32 * row_h;
                    let right = if i < 5 {
                        format!("< {value} >")
                    } else {
                        value.clone()
                    };
                    self.draw_row(fb, y, label, &right, i == sel, self.theme.paper);
                }
                let idx = format!("preset {}/{}", p.preset_index() + 1, PRESETS.len());
                fb.text(left, h - 40, &idx, scale(self.theme.dim, 0.7), 1);
                fb.text(
                    left,
                    h - 28,
                    &cut("saved to profile.toml + switchres.ini", max_cols),
                    scale(self.theme.dim, 0.7),
                    1,
                );
                fb.text(
                    left,
                    h - 16,
                    "<> change  A select  B back saves",
                    scale(self.theme.dim, 0.7),
                    1,
                );
            }
            Screen::Pair { sel } => {
                let y0 = self.draw_header(fb, "Pads");
                let devices = self.bt.devices.clone();
                if devices.is_empty() {
                    fb.text(left, y0, "no devices yet", self.theme.dim, 1);
                }
                for (i, (mac, name)) in devices.iter().enumerate().take(Self::ROWS_PER_PAGE) {
                    let y = y0 + i as i32 * row_h;
                    self.draw_row(fb, y, name, &mac[9..], i == sel, self.theme.paper);
                }
                let dots = ((self.now * 2.0) as usize) % 4;
                let status = if self.bt.busy {
                    format!("{}{}", self.bt.status, ".".repeat(dots))
                } else {
                    self.bt.status.clone()
                };
                fb.text(left, h - 28, &cut(&status, max_cols), self.theme.cyan, 1);
                let hint = self.hint(&[("A", "pair/scan"), ("X", "remap pad"), ("B", "back")]);
                fb.text(left, h - 16, &hint, scale(self.theme.dim, 0.7), 1);
            }
            Screen::Settings { sel } => {
                self.draw_settings_menu(fb, sel);
                return;
            }
            Screen::Power { sel } => {
                self.draw_menu_screen(fb, "Power", &POWER_ITEMS, sel);
                return;
            }
            Screen::Saver { sel } => {
                self.draw_saver_settings(fb, sel);
                return;
            }
            Screen::Diag { top } => {
                self.draw_diag(fb, top);
                return;
            }
            Screen::About { top } => {
                self.draw_about(fb, top);
                return;
            }
            Screen::Style { sel } => {
                self.draw_style(fb, sel);
                return;
            }
            Screen::VideoFit { sel } => {
                self.draw_video_fit(fb, sel);
                return;
            }
            Screen::MusicSettings { sel } => {
                self.draw_music_settings(fb, sel);
                return;
            }
            Screen::VideoSettings { sel } => {
                self.draw_video_settings(fb, sel);
                return;
            }
            Screen::FrameSettings { sel } => {
                self.draw_frame_settings(fb, sel);
                return;
            }
            Screen::AmbientSettings { sel } => {
                self.draw_ambient_settings(fb, sel);
                return;
            }
            Screen::SoundSettings { sel } => {
                self.draw_sound_settings(fb, sel);
                return;
            }
            Screen::Music { sel, top } => {
                self.draw_music(fb, sel, top);
                return;
            }
            Screen::MusicList { sel, top } => {
                self.draw_music_list(fb, sel, top);
                return;
            }
            Screen::NowPlaying => {
                self.draw_now_playing(fb);
                return;
            }
            Screen::Equalizer { band } => {
                self.draw_equalizer(fb, band);
                return;
            }
            Screen::Resume { sel } => {
                self.draw_resume(fb, sel);
                return;
            }
            Screen::PadWizard => {
                self.draw_pad_wizard(fb);
                return;
            }
            Screen::Videos { sel } => {
                self.draw_menu_screen(fb, "Videos", &VIDEOS_ITEMS, sel);
                return;
            }
            Screen::YouTube { sel } => {
                self.draw_menu_screen(fb, "YouTube", &YOUTUBE_ITEMS, sel);
                return;
            }
            Screen::Monitor { page } => {
                self.draw_monitor(fb, page);
                return;
            }
            Screen::Frame => {
                self.draw_frame(fb);
                return;
            }
            Screen::AmbientHub { sel } => {
                self.draw_menu_screen(fb, "Ambient", &AMBIENT_ITEMS, sel);
                return;
            }
            Screen::Ambient => {
                self.draw_ambient(fb);
                return;
            }
            Screen::Menu => {}
        }
        if let Some((msg, _)) = &self.message
            && !matches!(self.screen, Screen::Pair { .. })
        {
            fb.text(left, h - 28, &cut(msg, max_cols - 8), self.theme.cyan, 1);
        }
    }

    /// The question asked before a game that was left in the middle: carry
    /// on from the state, or start again. The state is never deleted; a new
    /// session simply does not read it, and overwrites it on exit.
    fn draw_resume(&mut self, fb: &mut Framebuffer, sel: usize) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32;
        let width = w - 2 * left;
        let (title, label) = match &self.pending_entry {
            Some((entry, _)) => (
                entry.game.title.clone(),
                self.states
                    .latest(&entry.game.path)
                    .map(|st| st.label())
                    .unwrap_or_default(),
            ),
            None => (String::new(), String::new()),
        };
        let max_cols = (width / 8) as usize;
        let head: String = title.chars().take(max_cols.saturating_sub(9)).collect();
        let y0 = self.draw_header(fb, &format!("Resume  {head}"));
        let items = [
            (icons::RESUME, "Carry on where you left off", false),
            (icons::GAMEPAD, "Start a new session", false),
        ];
        let row_h = 14;
        let band_y = self.band(y0 + sel as i32 * row_h);
        fb.rect(left, band_y, width, row_h - 1, self.theme.selection);
        for (i, (icon, text, sub)) in items.iter().enumerate() {
            let y = y0 + i as i32 * row_h;
            self.draw_menu_row(fb, left, y, width, icon, text, *sub, i == sel, 1.0);
        }
        if !label.is_empty() {
            fb.text(left + 4, y0 + 2 * row_h + 8, &label, self.theme.green, 1);
        }
        fb.text(
            left + 4,
            y0 + 2 * row_h + 20,
            "the state on disk is kept either way",
            scale(self.theme.dim, 0.9),
            1,
        );
        let hint = self.hint(&[("A", "choose"), ("B", "back")]);
        fb.text(left, h - 14, &hint, scale(self.theme.dim, 0.7), 1);
    }

    /// The cover flow: the selected game's box art large in the middle, the
    /// neighbours receding to both sides at an angle, everything mirrored on
    /// a dark floor, stars behind. Left and right slide the row with inertia.
    fn draw_flow(&mut self, fb: &mut Framebuffer, prompt: &str, sel: usize) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32;
        let n = self.games.len();
        let now = self.now;
        // Ease toward the selection; snap when close.
        let target = sel as f32;
        self.flow_pos += (target - self.flow_pos) * 0.22;
        if (self.flow_pos - target).abs() < 0.01 {
            self.flow_pos = target;
        }
        let floor_y = 162;
        // Sky: a quiet gradient and a few stars that twinkle.
        for y in 0..floor_y {
            let f = y as f32 / floor_y as f32;
            fb.rect(
                0,
                y,
                w,
                1,
                lerp_color(self.theme.bg, self.theme.selection, 0.25 * (1.0 - f)),
            );
        }
        for i in 0..70u32 {
            let x = (i * 97 + 13) as i32 % w;
            let y = (i * 53 + 7) as i32 % (floor_y - 10);
            let tw = 0.5 + 0.5 * ((now * 1.3 + i as f64 * 0.7).sin() as f32);
            fb.put(
                x,
                y,
                lerp_color(self.theme.bg, self.theme.paper, 0.2 + 0.5 * tw),
            );
        }
        // Floor: darker toward the bottom with a horizon line.
        for y in floor_y..h {
            let f = (y - floor_y) as f32 / (h - floor_y) as f32;
            fb.rect(
                0,
                y,
                w,
                1,
                lerp_color(self.theme.selection, self.theme.bg, 0.4 + 0.6 * f),
            );
        }
        fb.rect(0, floor_y, w, 1, scale(self.theme.dim, 0.7));
        // Covers, far ones first.
        let cw = 100;
        let ch = 126;
        // Covers stand on a shelf just above the floor; the reflection
        // hangs from the same edge.
        let bottom = floor_y - 6;
        let cy = bottom - ch / 2;
        let lo = (self.flow_pos.floor() as i64 - 4).max(0) as usize;
        let hi = ((self.flow_pos.ceil() as usize) + 4).min(n.saturating_sub(1));
        let mut order: Vec<usize> = (lo..=hi).collect();
        order.sort_by(|a, b| {
            let da = (*a as f32 - self.flow_pos).abs();
            let db = (*b as f32 - self.flow_pos).abs();
            db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
        });
        let entries: Vec<Entry> = order.iter().map(|i| self.games[*i].clone()).collect();
        for (k, i) in order.iter().enumerate() {
            let entry = &entries[k];
            let d = *i as f32 - self.flow_pos;
            let ad = d.abs();
            let sc = 0.78f32.powf(ad).max(0.25);
            let off = d.signum() * (86.0 + 38.0 * (ad - 1.0).max(0.0)) * ad.min(1.0);
            let x_c = w as f32 / 2.0 + off;
            let ww = (cw as f32 * sc * (1.0 - 0.25 * ad.min(1.0))) as i32;
            let hh = (ch as f32 * sc) as i32;
            // The far edge is shorter: a cover turned toward the middle.
            let tilt = 0.82 + 0.18 * (1.0 - ad.min(1.0));
            let (hl, hr) = if d < 0.0 {
                ((hh as f32 * tilt) as i32, hh)
            } else {
                (hh, (hh as f32 * tilt) as i32)
            };
            let x0 = (x_c - ww as f32 / 2.0) as i32;
            let y0 = bottom - hh;
            let shade = 1.0 - 0.45 * ad.min(1.0);
            let system = self.library.systems[entry.sys].name.clone();
            let img = self
                .art
                .cover(&system, &entry.game.path, cw as usize, ch as usize)
                .cloned();
            match img {
                Some(img) => {
                    // Keep the cover's own proportions inside the box.
                    let f = (ww as f32 / img.w as f32).min(hh as f32 / img.h as f32);
                    let iw = (img.w as f32 * f) as i32;
                    let ih = (img.h as f32 * f) as i32;
                    let ix = (x_c - iw as f32 / 2.0) as i32;
                    let iy = bottom - ih;
                    let (il, ir) = (
                        (hl as f32 * ih as f32 / hh as f32) as i32,
                        (hr as f32 * ih as f32 / hh as f32) as i32,
                    );
                    fb.blit_trapezoid(&img, ix, iy, iw, il, ir, shade, 1.0, false, floor_y, 0);
                    // Reflection: the same cover upside down under the floor,
                    // fading out within a few rows.
                    let ry = floor_y + 1 + (floor_y - bottom);
                    fb.blit_trapezoid(
                        &img,
                        ix,
                        ry,
                        iw,
                        il,
                        ir,
                        shade * 0.6,
                        0.35,
                        true,
                        floor_y + 30,
                        30,
                    );
                    let _ = (x0, y0);
                }
                None => {
                    let frame = scale(self.theme.dim, 0.6);
                    for k in (0..ww).step_by(4) {
                        fb.put(x0 + k, y0, frame);
                        fb.put(x0 + k, y0 + hh - 1, frame);
                    }
                    for k in (0..hh).step_by(4) {
                        fb.put(x0, y0 + k, frame);
                        fb.put(x0 + ww - 1, y0 + k, frame);
                    }
                    if let Some((logo, c)) = icons::system_logo(&system) {
                        let s = if ad < 0.5 { 2 } else { 1 };
                        fb.bitmap(
                            x_c as i32 - 5 * s,
                            bottom - hh / 2 - 5 * s,
                            logo,
                            scale(c, shade),
                            s,
                            10,
                        );
                    }
                    let _ = cy;
                }
            }
        }
        // Title and details of the selection.
        let entry = self.games[sel.min(n - 1)].clone();
        let max_cols = ((w - 2 * left) / 8) as usize;
        // The save state label sits on the left of the same line, so the
        // centred title keeps clear of it on both sides.
        let state = self.states.latest(&entry.game.path).map(|st| st.label());
        let reserve = state.as_ref().map(|l| l.chars().count() + 2).unwrap_or(0);
        let room = max_cols.saturating_sub(reserve).max(8);
        // The title centres in what is left of the line, not in the frame.
        let centre = w / 2 + (reserve as i32 * 8) / 2;
        let title: String = entry.game.title.chars().take(room).collect();
        fb.text_centered(centre, floor_y + 34, &title, self.theme.bright_green, 1);
        let stem = entry
            .game
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let (_, tags, _, _) = crate::index::parse_name(stem);
        let system_label = crate::index::catalog(&self.library.systems[entry.sys].name)
            .map(|(l, _, _)| l.to_string())
            .unwrap_or_else(|| self.library.systems[entry.sys].name.clone());
        let mut detail = system_label;
        for t in tags.iter().take(2) {
            detail.push_str("  ");
            detail.push_str(t);
        }
        let detail: String = detail.chars().take(max_cols).collect();
        fb.text_centered(w / 2, floor_y + 46, &detail, self.theme.dim, 1);
        if let Some(label) = &state {
            fb.text(left, floor_y + 34, label, self.theme.green, 1);
        }
        if self.is_favorite(&entry) {
            fb.bitmap(
                w - left - 8,
                floor_y + 35,
                &icons::STAR,
                self.theme.yellow,
                1,
                8,
            );
        }
        let pos = format!("{}/{}", sel + 1, n);
        fb.text(
            w - left - Framebuffer::text_width(&pos, 1),
            4,
            &pos,
            scale(self.theme.dim, 0.8),
            1,
        );
        let p: String = prompt.chars().take(24).collect();
        fb.text(left, 4, &p, scale(self.theme.dim, 0.8), 1);
        let hint = self.hint(&[("A", "run"), ("X", "list"), ("Y", "fav"), ("B", "back")]);
        fb.text(left, h - 14, &hint, scale(self.theme.dim, 0.7), 1);
    }

    /// The search bar under the header: the query with a blinking cursor and
    /// the number of matches. Returns the y the list starts at.
    pub(super) fn draw_search_bar(
        &mut self,
        fb: &mut Framebuffer,
        y0: i32,
        q: &str,
        hits: usize,
    ) -> i32 {
        let w = fb.w as i32;
        let left = (w as f32 * 0.05) as i32 + self.slide();
        let width = w - 2 * (w as f32 * 0.05) as i32 - self.row_shrink;
        fb.rect(left, y0 - 2, width, 12, scale(self.theme.selection, 0.7));
        let max_cols = ((width - 8) / 8) as usize;
        let shown: String = if q.chars().count() + 2 > max_cols {
            q.chars().skip(q.chars().count() + 2 - max_cols).collect()
        } else {
            q.to_string()
        };
        let text = format!("/ {shown}");
        fb.text(left + 4, y0, &text, self.theme.accent, 1);
        if (self.now * 2.0).floor() as i64 % 2 == 0 {
            let cx = left + 4 + Framebuffer::text_width(&text, 1) + 1;
            fb.rect(cx, y0, 6, 8, self.theme.accent);
        }
        let count = if self.yt_query {
            if self.yt_search.is_some() {
                "searching".to_string()
            } else {
                "Enter searches YouTube".to_string()
            }
        } else if self.music_query.is_some() {
            "Enter searches".to_string()
        } else if q.trim().is_empty() {
            "type to filter".to_string()
        } else {
            format!("{hits} found")
        };
        fb.text(
            left + width - 4 - Framebuffer::text_width(&count, 1),
            y0,
            &count,
            scale(self.theme.dim, 0.9),
            1,
        );
        y0 + 14
    }

    /// Four rows of keys above the hints; the pad's cursor sits on a band.
    pub(super) fn draw_osk(&mut self, fb: &mut Framebuffer) {
        let Some((cr, cc)) = self.osk else {
            return;
        };
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32;
        let key_w = 20;
        let row_h = 11;
        let top = h - 20 - OSK_ROWS.len() as i32 * row_h;
        fb.rect(left, top - 3, key_w * 10 + 2, 1, scale(self.theme.dim, 0.5));
        for (r, row) in OSK_ROWS.iter().enumerate() {
            for (c, ch) in row.chars().enumerate() {
                let x = left + c as i32 * key_w;
                let y = top + r as i32 * row_h;
                let on = (r as i32, c as i32) == (cr, cc);
                if on {
                    fb.rect(x, y - 1, key_w - 2, row_h - 1, self.theme.selection);
                }
                let label = if ch == ' ' {
                    "sp".to_string()
                } else {
                    ch.to_string()
                };
                fb.text_centered(
                    x + (key_w - 2) / 2,
                    y + 1,
                    &label,
                    if on {
                        self.theme.accent
                    } else {
                        self.theme.paper
                    },
                    1,
                );
            }
        }
    }
}
