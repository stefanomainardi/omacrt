//! The boot sequence, the menu and the screensaver, drawn frame by frame.
//!
//! Timings follow crt.omarchy.org: power surge and roll, BIOS POST lines,
//! logo revealed band by band, systems-online chime, wordmark laser-etched
//! (TerminalTextEffects port), then everything settles into an `ls` listing.
//! On top of that: a "CRT" cartridge badge slams in like an arcade title
//! card, and an idle screensaver cycles text effects on the wordmark.

use crate::assets::ICON_24;
use crate::audio::Sound;
use crate::effects::{self, Effect, Kind, Palette};
use crate::etch::LaserEtch;
use crate::fb::{Color, Framebuffer, scale};
use crate::library::{Game, Library};
use crate::menu::Item;
use crate::theme::Theme;

fn clamp(v: f32, a: f32, b: f32) -> f32 {
    v.max(a).min(b)
}
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn ease(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

pub enum Action {
    None,
    Quit,
    Launch(String),
    /// Run a game: the shell waits for the process and shows a "now playing" screen.
    Run(std::process::Command, String),
}

/// Which screen the menu is on after boot.
enum Screen {
    Menu,
    Systems { sel: usize },
    Games { sys: usize, sel: usize, top: usize },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Nav {
    Up,
    Down,
    Left,
    Right,
    Back,
}

pub struct SysInfo {
    pub host: String,
    pub kernel: String,
    pub mode: String,
}

impl SysInfo {
    pub fn probe(w: usize, h: usize, hz: u32) -> Self {
        let read = |p: &str| {
            std::fs::read_to_string(p)
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        let host = read("/etc/hostname");
        let kernel = read("/proc/sys/kernel/osrelease");
        Self {
            host: if host.is_empty() {
                "omarchy".into()
            } else {
                host
            },
            kernel: if kernel.is_empty() {
                "linux".into()
            } else {
                kernel
            },
            mode: format!("{w}x{h}@{hz}"),
        }
    }
}

/// Wordmark pixel size and geometry shared by boot and screensaver.
const MARK_SCALE: i32 = 3;
const ETCH_START: f32 = 4.15;
const TAG_SCALE: i32 = 2;
const TAG_START: f32 = 6.9;
const TAG_TOTAL: f32 = 1.4;

/// "CRT" in a 5x7 pixel font, each letter one pixel row lower than the last:
/// a stair-step diagonal, the 8-bit way to slant a title.
fn crt_tag_art() -> String {
    const C: [&str; 7] = [
        ".###.", "#...#", "#....", "#....", "#....", "#...#", ".###.",
    ];
    const R: [&str; 7] = [
        "####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#",
    ];
    const T: [&str; 7] = [
        "#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#..",
    ];
    let letters = [C, R, T];
    let (w, h) = (17usize, 9usize);
    let mut canvas = vec![vec![b'.'; w]; h];
    for (i, glyph) in letters.iter().enumerate() {
        for (ry, row) in glyph.iter().enumerate() {
            for (rx, ch) in row.bytes().enumerate() {
                if ch == b'#' {
                    canvas[ry + i][i * 6 + rx] = b'#';
                }
            }
        }
    }
    let rows: Vec<String> = canvas
        .into_iter()
        .map(|r| String::from_utf8(r).unwrap())
        .collect();
    let refs: Vec<&str> = rows.iter().map(|r| r.as_str()).collect();
    effects::art_to_blocks(&refs)
}

struct Saver {
    effect: Effect,
    kind: Kind,
    started: f64,
}

pub struct Scene {
    theme: Theme,
    info: SysInfo,
    items: Vec<Item>,
    mark_cols: i32,
    mark_rows: i32,
    boot_started: bool,
    t0: f64,
    now: f64,
    mem: u32,
    last_post_line: i32,
    crunches: u32,
    chime_played: bool,
    menu_live: bool,
    sel: usize,
    message: Option<(String, f64)>,
    pending: Vec<Sound>,
    etch: Option<LaserEtch>,
    tag: effects::Grid,
    tag_seed: u32,
    tag_sounds: [bool; 2],
    saver: Option<Saver>,
    last_input: f64,
    idle_secs: f32,
    rng: u32,
    library: Library,
    screen: Screen,
    games: Vec<Game>,
    running: Option<(String, String)>,
    mark_small: effects::Grid,
}

impl Scene {
    pub fn new(
        theme: Theme,
        info: SysInfo,
        items: Vec<Item>,
        idle_secs: f32,
        library: Library,
    ) -> Self {
        let stops = [theme.magenta, theme.cyan, theme.paper];
        let grid = effects::Grid::wordmark(stops);
        let (mark_cols, mark_rows) = (grid.cols, grid.rows);
        let tag = effects::Grid::from_text(&crt_tag_art(), stops);
        Self {
            theme,
            info,
            items,
            mark_cols,
            mark_rows,
            boot_started: false,
            t0: 0.0,
            now: 0.0,
            mem: 0,
            last_post_line: -1,
            crunches: 0,
            chime_played: false,
            menu_live: false,
            sel: 0,
            message: None,
            pending: Vec::new(),
            etch: None,
            tag,
            tag_seed: 0x1234_5678,
            tag_sounds: [false; 2],
            saver: None,
            last_input: 0.0,
            idle_secs,
            rng: 0x2545_f491,
            library,
            screen: Screen::Menu,
            games: Vec::new(),
            running: None,
            mark_small: grid,
        }
    }

    fn rand(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    fn stops(&self) -> [Color; 3] {
        [self.theme.magenta, self.theme.cyan, self.theme.paper]
    }

    fn palette(&self) -> Palette {
        Palette {
            green: self.theme.green,
            cyan: self.theme.cyan,
            orange: self.theme.orange,
            yellow: self.theme.yellow,
            red: self.theme.red,
            dim: self.theme.dim,
        }
    }

    /// Sounds queued during the last `draw`; the caller plays them.
    pub fn take_sounds(&mut self) -> Vec<Sound> {
        std::mem::take(&mut self.pending)
    }

    pub fn boot_started(&self) -> bool {
        self.boot_started
    }

    pub fn t0(&self) -> f64 {
        self.t0
    }

    fn t(&self) -> f32 {
        if self.boot_started {
            (self.now - self.t0) as f32
        } else {
            -1.0
        }
    }

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

    /// Any user input: wakes the screensaver (returns true if it did).
    pub fn touch(&mut self, now: f64) -> bool {
        self.last_input = now;
        if self.saver.take().is_some() {
            self.pending.push(Sound::Move);
            return true;
        }
        false
    }

    pub fn start_screensaver(&mut self, now: f64, kind: Option<Kind>) {
        let kind = kind
            .unwrap_or_else(|| effects::ALL[(self.rand() % effects::ALL.len() as u32) as usize]);
        let seed = (now * 997.0) as u32 ^ self.rand();
        let effect = Effect::new(kind, seed, self.stops(), self.palette());
        self.saver = Some(Saver {
            effect,
            kind,
            started: now,
        });
    }

    pub fn navigate(&mut self, nav: Nav) {
        if !self.menu_live || self.running.is_some() {
            return;
        }
        if !matches!(self.screen, Screen::Menu) {
            self.navigate_browser(nav);
            return;
        }
        if self.items.is_empty() || nav == Nav::Back {
            return;
        }
        let n = self.items.len();
        let rows = n.div_ceil(2);
        let (col, row) = (self.sel / rows, self.sel % rows);
        let next = match nav {
            Nav::Up => {
                if row == 0 {
                    self.sel
                } else {
                    self.sel - 1
                }
            }
            Nav::Down => {
                if row + 1 >= rows || self.sel + 1 >= n {
                    self.sel
                } else {
                    self.sel + 1
                }
            }
            Nav::Left => {
                if col == 0 {
                    self.sel
                } else {
                    self.sel - rows
                }
            }
            Nav::Right => {
                if col + 1 >= 2 {
                    self.sel
                } else {
                    (self.sel + rows).min(n - 1)
                }
            }
            Nav::Back => self.sel,
        };
        if next != self.sel {
            self.sel = next;
            self.pending.push(Sound::Move);
        }
    }

    pub fn activate(&mut self) -> Action {
        if !self.menu_live || self.running.is_some() {
            return Action::None;
        }
        if !matches!(self.screen, Screen::Menu) {
            return self.activate_browser();
        }
        let Some(item) = self.items.get(self.sel).cloned() else {
            return Action::None;
        };
        self.pending.push(Sound::Select);
        if item.quit {
            return Action::Quit;
        }
        if !item.command.trim().is_empty() {
            self.message = Some((
                format!("launching {}", item.name.trim_end_matches('/')),
                self.now + 4.0,
            ));
            return Action::Launch(item.command);
        }
        let name = item.name.trim_end_matches('/');
        let text = match name {
            "about" => format!(
                "omarchy-crt {} on {}",
                env!("CARGO_PKG_VERSION"),
                self.info.kernel
            ),
            "tv-profile" => "tv profile: ntsc 60Hz (coming soon)".to_string(),
            "screensaver" => {
                let now = self.now;
                self.start_screensaver(now, None);
                return Action::None;
            }
            "games" | "retroarch" => {
                if self.library.systems.is_empty() {
                    "no systems configured in systems.toml".to_string()
                } else {
                    self.screen = Screen::Systems { sel: 0 };
                    return Action::None;
                }
            }
            other => format!("{other}: nothing to do yet"),
        };
        self.message = Some((text, self.now + 4.0));
        Action::None
    }

    // -- game browser (RGB-Pi style: systems, then games) --------------------

    const ROWS_PER_PAGE: usize = 13;

    fn open_games(&mut self, sys: usize) {
        let system = self.library.systems[sys].clone();
        self.games = self.library.games(&system);
        self.screen = Screen::Games {
            sys,
            sel: 0,
            top: 0,
        };
    }

    fn navigate_browser(&mut self, nav: Nav) {
        let mut moved = false;
        match &mut self.screen {
            Screen::Systems { sel } => match nav {
                Nav::Up if *sel > 0 => {
                    *sel -= 1;
                    moved = true;
                }
                Nav::Down if *sel + 1 < self.library.systems.len() => {
                    *sel += 1;
                    moved = true;
                }
                Nav::Back | Nav::Left => {
                    self.screen = Screen::Menu;
                    moved = true;
                }
                _ => {}
            },
            Screen::Games { sys, sel, top } => {
                let n = self.games.len();
                let page = Self::ROWS_PER_PAGE;
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
                        let s = *sys;
                        self.screen = Screen::Systems { sel: s };
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
            Screen::Menu => {}
        }
        if moved {
            self.pending.push(Sound::Move);
        }
    }

    fn activate_browser(&mut self) -> Action {
        match self.screen {
            Screen::Systems { sel } => {
                self.pending.push(Sound::Select);
                self.open_games(sel);
                Action::None
            }
            Screen::Games { sys, sel, .. } => {
                let Some(game) = self.games.get(sel).cloned() else {
                    return Action::None;
                };
                let system = self.library.systems[sys].clone();
                match self.library.command(&system, &game) {
                    Ok(cmd) => {
                        self.pending.push(Sound::Select);
                        self.running = Some((game.title.clone(), system.name.clone()));
                        Action::Run(cmd, game.title)
                    }
                    Err(e) => {
                        self.message = Some((format!("cannot launch: {e}"), self.now + 4.0));
                        Action::None
                    }
                }
            }
            Screen::Menu => Action::None,
        }
    }

    /// The game process ended; back to the list, cursor where it was.
    pub fn game_finished(&mut self, ok: bool) {
        self.running = None;
        self.last_input = self.now;
        self.pending
            .push(if ok { Sound::Lock } else { Sound::Crunch });
        if !ok {
            self.message = Some(("retroarch exited with an error".into(), self.now + 4.0));
        }
    }

    /// Jump straight to the browser (for testing and frame dumps).
    pub fn debug_browse(&mut self, system: Option<&str>) {
        self.menu_live = true;
        self.chime_played = true;
        match system.and_then(|n| self.library.systems.iter().position(|s| s.name == n)) {
            Some(i) => self.open_games(i),
            None => self.screen = Screen::Systems { sel: 0 },
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// Compact header used by the browser and the running screen: small icon,
    /// the wordmark at 1 px per cell, a prompt line underneath.
    fn draw_header(&self, fb: &mut Framebuffer, prompt: &str) -> i32 {
        let left = (fb.w as f32 * 0.05) as i32;
        fb.bitmap(left, 8, &ICON_24, self.theme.green, 1, 24);
        let mx = fb.w as i32 - left - self.mark_small.cols;
        for cell in &self.mark_small.cells {
            effects::draw_cell(fb, mx, 10, 1, cell, cell.final_color);
        }
        fb.text(left, 40, prompt, self.theme.dim, 1);
        52
    }

    fn draw_browser(&mut self, fb: &mut Framebuffer) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32;
        let max_cols = ((w - 2 * left) / 8) as usize;
        let cut = |s: &str, n: usize| -> String { s.chars().take(n).collect() };
        let row_h = 12;
        match self.screen {
            Screen::Systems { sel } => {
                let y0 = self.draw_header(fb, "omarchy $ ls games/");
                let systems = self.library.systems.clone();
                for (i, sys) in systems.iter().enumerate() {
                    let y = y0 + i as i32 * row_h;
                    let count = self.library.games(sys).len();
                    let on = i == sel;
                    let label = format!("{}{}/", if on { "> " } else { "  " }, sys.name);
                    if on {
                        fb.rect(
                            left - 4,
                            y - 2,
                            w - 2 * left + 8,
                            row_h,
                            scale(self.theme.green, 0.12),
                        );
                    }
                    fb.text(
                        left,
                        y,
                        &cut(&label, max_cols - 10),
                        if on {
                            self.theme.bright_green
                        } else {
                            self.theme.green
                        },
                        1,
                    );
                    let right = format!("{count:>4}  {}", sys.video);
                    fb.text(
                        w - left - Framebuffer::text_width(&right, 1),
                        y,
                        &right,
                        self.theme.dim,
                        1,
                    );
                }
                if let Some(sys) = systems.get(sel) {
                    let core = self.library.core_path(sys);
                    let core_ok = core.exists();
                    let info = format!(
                        "core {}{}",
                        sys.core,
                        if core_ok { "" } else { " (missing)" }
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
                fb.text(
                    left,
                    h - 16,
                    "A open   B back",
                    scale(self.theme.dim, 0.7),
                    1,
                );
            }
            Screen::Games { sys, sel, top } => {
                let system = self.library.systems[sys].clone();
                let n = self.games.len();
                let y0 = self.draw_header(fb, &format!("omarchy $ ls games/{}/", system.name));
                if n == 0 {
                    let dir = crate::library::expand(&system.dir);
                    fb.text(left, y0, "no games found in", self.theme.dim, 1);
                    fb.text(
                        left,
                        y0 + 12,
                        &cut(&dir.display().to_string(), max_cols),
                        self.theme.paper,
                        1,
                    );
                } else {
                    let end = (top + Self::ROWS_PER_PAGE).min(n);
                    for (row, i) in (top..end).enumerate() {
                        let y = y0 + row as i32 * row_h;
                        let on = i == sel;
                        if on {
                            fb.rect(
                                left - 4,
                                y - 2,
                                w - 2 * left + 8,
                                row_h,
                                scale(self.theme.green, 0.12),
                            );
                        }
                        let label =
                            format!("{}{}", if on { "> " } else { "  " }, self.games[i].title);
                        fb.text(
                            left,
                            y,
                            &cut(&label, max_cols),
                            if on {
                                self.theme.bright_green
                            } else {
                                self.theme.paper
                            },
                            1,
                        );
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
                fb.text(
                    left,
                    h - 16,
                    "A run   B back   <> page",
                    scale(self.theme.dim, 0.7),
                    1,
                );
            }
            Screen::Menu => {}
        }
        if let Some((msg, _)) = &self.message {
            fb.text(left, h - 28, &cut(msg, max_cols - 8), self.theme.red, 1);
        }
    }

    fn draw_running(&mut self, fb: &mut Framebuffer) {
        let Some((title, system)) = self.running.clone() else {
            return;
        };
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32;
        let y0 = self.draw_header(fb, &format!("omarchy $ play {system}/"));
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

    // -- power and roll ------------------------------------------------------

    pub fn power(&self) -> f32 {
        if self.saver.is_some() {
            return 1.0;
        }
        let t = self.t();
        if !self.boot_started {
            return 0.82;
        }
        if t < 0.08 {
            t / 0.08 * 0.15
        } else if t < 0.22 {
            2.1
        } else if t < 0.55 {
            lerp(2.1, 1.0, (t - 0.22) / 0.33)
        } else {
            1.0
        }
    }

    pub fn roll(&self) -> f32 {
        let t = self.t();
        if !self.boot_started || self.saver.is_some() {
            0.0
        } else if t < 0.7 {
            1.0 - t / 0.7
        } else if (1.92..2.18).contains(&t) {
            1.0 - (t - 1.92) / 0.26
        } else {
            0.0
        }
    }

    // -- drawing -------------------------------------------------------------

    pub fn draw(&mut self, fb: &mut Framebuffer, now: f64) {
        self.now = now;
        fb.clear(self.theme.bg);
        if self.saver.is_some() {
            self.draw_saver(fb);
            return;
        }
        if !self.boot_started {
            self.draw_gate(fb);
            return;
        }
        let t = self.t();
        if self.running.is_some() {
            self.draw_running(fb);
            return;
        }
        if self.menu_live && !matches!(self.screen, Screen::Menu) {
            self.draw_browser(fb);
            if let Some((_, until)) = &self.message {
                if self.now > *until {
                    self.message = None;
                }
            }
            if self.idle_secs > 0.0 && (now - self.last_input) as f32 > self.idle_secs {
                self.start_screensaver(now, None);
            }
            return;
        }
        self.draw_post(fb, t);
        self.draw_logo(fb, t);
        self.draw_etch(fb, t);
        self.draw_crt_tag(fb, t);
        self.draw_listing(fb, t);
        if t >= 4.0 && !self.chime_played {
            self.chime_played = true;
            self.pending.push(Sound::Chime);
        }
        if t > 6.9 && !self.menu_live {
            self.menu_live = true;
            self.last_input = now;
        }
        if let Some((_, until)) = &self.message {
            if self.now > *until {
                self.message = None;
            }
        }
        if self.menu_live && self.idle_secs > 0.0 && (now - self.last_input) as f32 > self.idle_secs
        {
            self.start_screensaver(now, None);
        }
    }

    fn draw_gate(&mut self, fb: &mut Framebuffer) {
        let (w, h) = (fb.w as i32, fb.h as i32);
        let s = 3;
        fb.bitmap(
            (w - 24 * s) / 2,
            (h as f32 * 0.26) as i32,
            &ICON_24,
            self.theme.green,
            s,
            24,
        );
        fb.text_centered(
            w / 2,
            (h as f32 * 0.58) as i32,
            "OMARCHY",
            self.theme.green,
            2,
        );
        let msg = "PRESS START";
        let y = (h as f32 * 0.70) as i32;
        fb.text_centered(w / 2, y, msg, self.theme.dim, 1);
        if (self.now * 2.0).floor() as i64 % 2 == 0 {
            let x = w / 2 + Framebuffer::text_width(msg, 1) / 2 + 4;
            fb.rect(x, y, 6, 8, self.theme.green);
        }
        fb.text_centered(
            w / 2,
            h - 16,
            "(C) 2026 OMACOM  15KHZ EDITION",
            scale(self.theme.dim, 0.6),
            1,
        );
    }

    fn post_lines(&self) -> Vec<(String, Color, bool)> {
        let th = &self.theme;
        vec![
            ("OmarchyBIOS 4.01 / Omacom".into(), th.green, false),
            ("(C) 2026 Omacom Foundation".into(), th.dim, false),
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
                format!("DSK  omarchy-crt {}          OK", env!("CARGO_PKG_VERSION")),
                th.paper,
                false,
            ),
        ]
    }

    fn draw_post(&mut self, fb: &mut Framebuffer, t: f32) {
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
            self.mem = ((t - start - 0.18 * 4.0) * 90_000.0).max(0.0).min(65_536.0) as u32;
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
        for (text, color, is_mem) in lines.iter().take(shown as usize) {
            let mut s = text.clone();
            if *is_mem {
                s = if self.mem >= 65_536 {
                    "MEM  65536K OK".into()
                } else {
                    format!("MEM  {:05}K", self.mem)
                };
            }
            let s: String = s.chars().take(max_cols as usize).collect();
            fb.text(x, y, &s, scale(*color, fade), 1);
            last_end = (x + Framebuffer::text_width(&s, 1) + 4, y);
            y += row_h;
        }
        // Block cursor after the last POST line, blinking fast like a BIOS.
        if shown > 0 && (self.now * 6.0).floor() as i64 % 2 == 0 {
            fb.rect(last_end.0, last_end.1, 6, 8, scale(self.theme.paper, fade));
        }
    }

    fn logo_final(&self, fb: &Framebuffer) -> (i32, i32, i32) {
        let size = 24;
        ((fb.w as i32 - size) / 2, (fb.h as f32 * 0.035) as i32, size)
    }

    fn mark_final_y(&self, fb: &Framebuffer) -> i32 {
        let (_, ly, lsize) = self.logo_final(fb);
        ly + lsize + 8
    }

    fn draw_logo(&mut self, fb: &mut Framebuffer, t: f32) {
        let appear = clamp((t - 2.2) / 1.45, 0.0, 1.0);
        if appear <= 0.0 {
            return;
        }
        let (w, h) = (fb.w as f32, fb.h as f32);
        let up = ease(clamp((t - 4.0) / 0.5, 0.0, 1.0));
        let settle = ease(clamp((t - 6.9) / 0.55, 0.0, 1.0));
        let (_, fy, fsize) = self.logo_final(fb);
        let big = (h * 0.40).min(96.0);
        let size = lerp(lerp(big, 32.0, up), fsize as f32, settle);
        let cy = lerp(lerp(h * 0.46, h * 0.22, up), fy as f32 + size * 0.5, settle);
        let x = (w * 0.5 - size * 0.5).round() as i32;
        let y = (cy - size * 0.5).round() as i32;
        let px = (size / 24.0).max(1.0);
        let band = 4;
        let bands = (size / band as f32).ceil() as i32;
        let revealed = (bands as f32 * ease(appear)).ceil() as i32;
        let max_y = y + revealed * band;
        let green = self.theme.green;
        for ry in 0..24 {
            for rx in 0..24 {
                if ICON_24[ry].as_bytes()[rx] != b'#' {
                    continue;
                }
                let x0 = x + (rx as f32 * px).round() as i32;
                let x1 = x + ((rx + 1) as f32 * px).round() as i32;
                let y0 = y + (ry as f32 * px).round() as i32;
                let y1 = (y + ((ry + 1) as f32 * px).round() as i32).min(max_y);
                if y1 > y0 {
                    fb.rect(x0, y0, x1 - x0, y1 - y0, green);
                }
            }
        }
        if appear < 1.0 {
            let beam_y = max_y;
            let sw = size.round() as i32;
            fb.rect_add(x, beam_y - 5, sw, 8, scale(self.theme.cyan, 0.22));
            fb.rect(x, beam_y - 2, sw, 2, scale(self.theme.green, 0.9));
        }
    }

    fn draw_etch(&mut self, fb: &mut Framebuffer, t: f32) {
        if t < ETCH_START {
            return;
        }
        let fade = ease(clamp((t - 4.2) / 0.35, 0.0, 1.0));
        let (w, h) = (fb.w as f32, fb.h as f32);
        let mw = self.mark_cols * MARK_SCALE;
        let settle = ease(clamp((t - 6.9) / 0.55, 0.0, 1.0));
        let final_y = self.mark_final_y(fb);
        let x = ((w - mw as f32) * 0.5).round() as i32;
        let y = lerp(h * 0.36, final_y as f32, settle).round() as i32;
        if let Some(etch) = self.etch.as_mut() {
            etch.advance_to(t - ETCH_START);
            etch.draw(fb, x, y, MARK_SCALE, fade);
        }
    }

    fn tag_geometry(&self, fb: &Framebuffer) -> (i32, i32, i32, i32) {
        // Right-aligned under the wordmark's last letters.
        let mw = self.mark_cols * MARK_SCALE;
        let mark_x = (fb.w as i32 - mw) / 2;
        let mark_bottom = self.mark_final_y(fb) + self.mark_rows * 2 * MARK_SCALE;
        let tw = self.tag.cols * TAG_SCALE;
        let th = self.tag.rows * 2 * TAG_SCALE;
        (mark_x + mw - tw - 2, mark_bottom + 2, tw, th)
    }

    /// "CRT" appears like a tape hunting for sync (TTE `vhstape`): torn lines,
    /// a tracking wave, snow, then a clean redraw with a lock click.
    fn draw_crt_tag(&mut self, fb: &mut Framebuffer, t: f32) {
        if t < TAG_START {
            return;
        }
        let local = t - TAG_START;
        let (x, y, _, _) = self.tag_geometry(fb);
        if !self.tag_sounds[0] {
            self.tag_sounds[0] = true;
            self.pending.push(Sound::Vhs);
        }
        if local >= TAG_TOTAL {
            if !self.tag_sounds[1] {
                self.tag_sounds[1] = true;
                self.pending.push(Sound::Lock);
            }
            for cell in &self.tag.cells {
                effects::draw_cell(fb, x, y, TAG_SCALE, cell, cell.final_color);
            }
            return;
        }
        effects::draw_vhs(
            fb,
            &self.tag,
            x,
            y,
            TAG_SCALE,
            local,
            TAG_TOTAL,
            self.tag_seed,
            1.0,
        );
    }

    fn draw_listing(&mut self, fb: &mut Framebuffer, t: f32) {
        let fade = ease(clamp((t - 6.95) / 0.45, 0.0, 1.0));
        if fade <= 0.0 {
            return;
        }
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32;
        let max_cols = ((w - 2 * left) / 8) as usize;
        let cut = |s: &str| -> String { s.chars().take(max_cols).collect() };

        let (_, tag_y, _, tag_h) = self.tag_geometry(fb);
        let mut y = tag_y + tag_h + 6;
        fb.text(
            left,
            y,
            &cut("Beautiful, Fun & Opinionated Linux"),
            scale(self.theme.paper, fade),
            1,
        );
        y += 12;
        // The prompt is typed out, one character every 40 ms, then the listing follows.
        let prompt = "omarchy $ ls";
        let typed = (((t - 7.1) / 0.04).floor().max(0.0) as usize).min(prompt.len());
        fb.text(left, y, &prompt[..typed], scale(self.theme.dim, fade), 1);
        if typed < prompt.len() {
            if (self.now * 4.0).floor() as i64 % 2 == 0 {
                fb.rect(
                    left + Framebuffer::text_width(&prompt[..typed], 1),
                    y,
                    6,
                    8,
                    scale(self.theme.dim, fade),
                );
            }
            return;
        }
        y += 14;

        let n = self.items.len();
        let rows = n.div_ceil(2);
        let col_w = (w - 2 * left) / 2;
        let row_h = 12;
        for (i, item) in self.items.iter().enumerate() {
            let col = (i / rows) as i32;
            let row = (i % rows) as i32;
            let ix = left + col * col_w;
            let iy = y + row * row_h;
            let on = self.menu_live && i == self.sel;
            if on {
                fb.rect(
                    ix - 4,
                    iy - 2,
                    col_w - 8,
                    row_h,
                    scale(self.theme.green, 0.12 * fade),
                );
                fb.text(
                    ix,
                    iy,
                    &format!("> {}", item.name),
                    scale(self.theme.bright_green, fade),
                    1,
                );
            } else {
                fb.text(
                    ix,
                    iy,
                    &format!("  {}", item.name),
                    scale(self.theme.green, fade),
                    1,
                );
            }
        }

        if let Some((msg, _)) = &self.message {
            let my = h - 32;
            fb.text(left, my, &cut(msg), scale(self.theme.cyan, fade), 1);
            let cursor_x = left + Framebuffer::text_width(&cut(msg), 1) + 3;
            if (self.now * 2.0).floor() as i64 % 2 == 0 {
                fb.rect(cursor_x, my, 6, 8, scale(self.theme.cyan, fade));
            }
        }

        let footer = format!(
            "{} {} {}",
            self.info.kernel, self.info.mode, self.theme.name
        );
        fb.text(
            left,
            h - 16,
            &cut(&footer),
            scale(self.theme.dim, 0.7 * fade),
            1,
        );
    }

    fn draw_saver(&mut self, fb: &mut Framebuffer) {
        let (w, h) = (fb.w as i32, fb.h as i32);
        let now = self.now;
        let dim = self.theme.dim;
        let (mw, mh) = (self.mark_cols * MARK_SCALE, self.mark_rows * 2 * MARK_SCALE);
        let mut restart = false;
        if let Some(saver) = self.saver.as_mut() {
            let t = (now - saver.started) as f32;
            saver.effect.advance_to(t);
            let x = (w - mw) / 2;
            let y = (h - mh) / 2 - 8;
            saver.effect.draw(fb, x, y, MARK_SCALE, t, 1.0);
            let caption = format!("tte {}", saver.kind.name());
            fb.text(
                (w as f32 * 0.05) as i32,
                h - 16,
                &caption,
                scale(dim, 0.6),
                1,
            );
            // Hold the finished picture for a while, then move on to another effect.
            restart = t > saver.effect.length() + 3.0;
        }
        if restart {
            self.start_screensaver(now, None);
        }
    }
}
