//! The boot sequence, the menu and the screensaver, drawn frame by frame.
//!
//! Timings follow crt.omarchy.org: power surge and roll, BIOS POST lines,
//! logo revealed band by band, systems-online chime, wordmark laser-etched
//! (TerminalTextEffects port), then everything settles into an `ls` listing.
//! On top of that: a "CRT" cartridge badge slams in like an arcade title
//! card, and an idle screensaver cycles text effects on the wordmark.

use crate::assets::ICON_24;
use crate::audio::Sound;
use crate::bt::Bluetooth;
use crate::effects::{self, Effect, Kind, Palette};
use crate::etch::LaserEtch;
use crate::fb::{Color, Framebuffer, scale};
use crate::icons;
use crate::library::{Game, Library};
use crate::pad::PadKind;
use crate::player::Player;
use crate::profile::{PRESETS, Profile};
use crate::settings::Settings;
use crate::theme::Theme;
use crate::videofit::{self, Conversion};
use std::path::{Path, PathBuf};

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
    /// Start the launcher again in place (same arguments).
    Restart,
    Launch(String),
}

/// Which screen the menu is on after boot.
enum Screen {
    Menu,
    Systems {
        sel: usize,
        top: usize,
    },
    /// Curated lists imported or written by hand.
    Collections {
        sel: usize,
        top: usize,
    },
    /// `sys` is None for the virtual lists (recent, favorites, collections).
    Games {
        sys: Option<usize>,
        sel: usize,
        top: usize,
    },
    Profile {
        sel: usize,
    },
    Pair {
        sel: usize,
    },
    Settings {
        sel: usize,
    },
    Power {
        sel: usize,
    },
    Saver {
        sel: usize,
    },
    Diag {
        top: usize,
    },
    About {
        top: usize,
    },
    Style {
        sel: usize,
    },
    VideoFit {
        sel: usize,
    },
}

/// One row of a game list: the game and the system that runs it.
#[derive(Clone)]
struct Entry {
    game: Game,
    sys: usize,
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
/// Home menu entries: icon, label, opens a submenu.
const HOME: [(icons::Icon, &str, bool); 7] = [
    (icons::GAMEPAD, "Games", true),
    (icons::FILM, "Videos", true),
    (icons::STAR, "Favorites", true),
    (icons::CLOCK, "Recent", true),
    (icons::GEAR, "Settings", true),
    (icons::INFO, "About", true),
    (icons::POWER, "Power", true),
];

/// Settings submenu entries.
const SETTINGS_ITEMS: [(icons::Icon, &str, bool); 6] = [
    (icons::TV, "TV profile", true),
    (icons::FIT, "Video fit", true),
    (icons::PAD, "Pads", true),
    (icons::SAVER, "Screensaver", true),
    (icons::BRUSH, "Style", true),
    (icons::PULSE, "Diagnostics", true),
];

const FIT_ROWS: usize = 5;

/// A game being launched: the media animation plays, then RetroArch starts.
struct Launch {
    cmd: std::process::Command,
    title: String,
    system: String,
    disc: bool,
    color: Color,
    started: f64,
    spawned: bool,
    /// Geometry the CRT should switch to for this program, when the output
    /// is a wide super resolution the host controls.
    lines: Option<Geometry>,
}

/// Per program picture geometry handed to the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub lines: Option<u32>,
    pub shift_x: i32,
    pub shift_y: i32,
}

const LAUNCH_SECS: f32 = 1.15;

/// Power submenu entries.
const POWER_ITEMS: [(icons::Icon, &str, bool); 3] = [
    (icons::DESKTOP, "Back to desktop", false),
    (icons::PULSE, "Restart launcher", false),
    (icons::POWER, "Power off", false),
];

/// Rows per page in the systems list.
const SYS_PAGE: usize = 11;

/// About page, wrapped for 36 columns.
const ABOUT: &[&str] = &[
    "OMARCHY CRT",
    "",
    "Retro gaming on a real 15 kHz tube,",
    "from your everyday Omarchy machine.",
    "",
    "Goals",
    "- native resolutions and refresh",
    "  rates for every game, 480i too",
    "- no scaler, no fake scanlines:",
    "  the CRT does the work",
    "- games right on first launch:",
    "  cores, options, pads, latency",
    "- one boot entry, the desktop",
    "  stays untouched",
    "- fully open, from kernel to shell",
    "",
    "Made by Stefano Mainardi",
    "",
    "Built on Omarchy by DHH and the",
    "Omacom Foundation (MIT).",
    "Effects after TerminalTextEffects",
    "by ChrisBuilds. Switchres by",
    "Calamity. 15 kHz kernel patches",
    "by D0023R. font8x8 by Daniel",
    "Hepper. RetroArch and the libretro",
    "cores by their authors.",
    "",
    "MIT license.",
];

const TAG_SCALE: i32 = 2;
const TAG_START: f32 = 6.9;

struct Saver {
    effect: Effect,
    kind: Kind,
    started: f64,
}

pub struct Scene {
    theme: Theme,
    /// Pixel size of the output the shell is drawn on. On a wide 15 kHz
    /// super resolution the whole frame is one 4:3 picture, so launched
    /// programs must fill it instead of keeping square pixels.
    output_size: (u32, u32),
    info: SysInfo,
    band_y: f32,
    screen_since: f64,
    settings: Settings,
    diag: Vec<(String, String)>,
    /// A game list opened from the home menu goes back to it, not to Games.
    list_from_home: bool,
    themes: Vec<(String, PathBuf)>,
    /// Theme transition: (from, to, start time).
    theme_blend: Option<(Theme, Theme, f64)>,
    launching: Option<Launch>,
    post_clicks: usize,
    player: Option<Player>,
    conversion: Option<Conversion>,
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
    /// Menu item waiting for a confirming second press, with its deadline.
    armed: Option<(usize, f64)>,
    pending: Vec<Sound>,
    pending_samples: Vec<Vec<f32>>,
    etch: Option<LaserEtch>,
    tag_sound_played: bool,
    etch_sound_played: bool,
    saver: Option<Saver>,
    last_input: f64,
    idle_secs: f32,
    rng: u32,
    library: Library,
    screen: Screen,
    games: Vec<Entry>,
    /// Which virtual list is open: 0 recent, 1 favorites, 2 a collection.
    virtual_row: usize,
    /// The collection open in the game list, if any (index into collections()).
    open_collection: Option<usize>,
    /// Box art and console pictures, fetched and scaled off thread.
    art: crate::art::Art,
    /// Pixels taken from the right of list rows by a picture panel.
    row_shrink: i32,
    /// A shift changed on the TV profile screen and the tube should show it.
    profile_preview: bool,
    /// Subfolder of the current system being browsed, None at its root.
    game_dir: Option<PathBuf>,
    /// Game counts per system, refreshed when the library is (re)read.
    system_counts: Vec<usize>,
    running: Option<(String, String)>,
    mark_small: effects::Grid,
    profile: Profile,
    recent: Vec<(usize, PathBuf)>,
    favorites: Vec<(usize, PathBuf)>,
    pad: PadKind,
    bt: Bluetooth,
}

impl Scene {
    pub fn new(theme: Theme, info: SysInfo, idle_secs: f32, library: Library) -> Self {
        // A theme chosen in Settings overrides the system theme.
        let settings = Settings::load(&library.config_dir);
        let theme = if settings.theme != "system" {
            Theme::installed()
                .into_iter()
                .find(|(n, _)| *n == settings.theme)
                .and_then(|(n, p)| Theme::load_named(&p, &n))
                .unwrap_or(theme)
        } else {
            theme
        };
        let stops = [theme.magenta, theme.cyan, theme.paper];
        let grid = effects::Grid::wordmark(stops);
        let (mark_cols, mark_rows) = (grid.cols, grid.rows);
        let mut scene = Self {
            output_size: (320, 240),
            theme,
            info,
            band_y: -1.0,
            screen_since: 0.0,
            settings: Settings::load(&library.config_dir),
            diag: Vec::new(),
            list_from_home: false,
            virtual_row: 0,
            open_collection: None,
            art: crate::art::Art::new(),
            row_shrink: 0,
            profile_preview: false,
            game_dir: None,
            system_counts: Vec::new(),
            themes: Theme::installed(),
            theme_blend: None,
            launching: None,
            post_clicks: 0,
            player: None,
            conversion: None,
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
            armed: None,
            pending: Vec::new(),
            pending_samples: Vec::new(),
            etch: None,
            tag_sound_played: false,
            etch_sound_played: false,
            saver: None,
            last_input: 0.0,
            idle_secs,
            rng: 0x2545_f491,
            pad: PadKind::Keyboard,
            bt: Bluetooth::new(),
            profile: Profile::load(&library.config_dir),
            recent: load_list(&library.config_dir.join("recent.txt"), &library),
            favorites: load_list(&library.config_dir.join("favorites.txt"), &library),
            library,
            screen: Screen::Menu,
            games: Vec::new(),
            running: None,
            mark_small: grid,
        };
        scene.refresh_counts();
        scene
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

    /// Runtime-generated buffers queued during the last `draw`.
    pub fn take_samples(&mut self) -> Vec<Vec<f32>> {
        std::mem::take(&mut self.pending_samples)
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

    /// Jump to the end of the boot sequence: menu up, sounds that would have
    /// played are marked as done so nothing fires late.
    pub fn skip_boot(&mut self, now: f64) {
        if !self.boot_started || self.menu_live {
            return;
        }
        self.t0 = now - 9.75;
        self.chime_played = true;
        self.tag_sound_played = true;
        self.etch_sound_played = true;
        self.post_clicks = usize::MAX / 2;
        self.pending.push(Sound::Lock);
    }

    pub fn booting(&self) -> bool {
        self.boot_started && !self.menu_live
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
        let n = HOME.len();
        let next = match nav {
            Nav::Up if self.sel > 0 => self.sel - 1,
            Nav::Down if self.sel + 1 < n => self.sel + 1,
            _ => self.sel,
        };
        if next != self.sel {
            self.sel = next;
            self.armed = None;
            self.pending.push(Sound::Move);
        }
    }

    /// Idle seconds before the screensaver, from settings (0 disables).
    fn idle_limit(&self) -> f32 {
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

    /// Switch screen and restart the slide-in transition.
    fn go(&mut self, screen: Screen) {
        self.screen = screen;
        self.screen_since = self.now;
        self.band_y = -1.0;
        self.pending.push(Sound::Whoosh);
    }

    /// Switch theme with a short blend; `name` is a theme directory name or `system`.
    fn apply_theme(&mut self, name: &str) {
        let target = if name == "system" {
            Theme::default_path()
                .and_then(|p| Theme::load(&p))
                .unwrap_or_else(Theme::tokyo_night)
        } else {
            match self.themes.iter().find(|(n, _)| n == name) {
                Some((n, p)) => Theme::load_named(p, n).unwrap_or_else(Theme::tokyo_night),
                None => return,
            }
        };
        self.theme_blend = Some((self.theme.clone(), target, self.now));
    }

    /// Advance the theme blend; rebuilds the small wordmark on every step.
    fn tick_theme(&mut self) {
        let Some((from, to, start)) = self.theme_blend.clone() else {
            return;
        };
        let t = ((self.now - start) / 0.35).clamp(0.0, 1.0) as f32;
        self.theme = Theme::blend(&from, &to, ease(t));
        self.mark_small = effects::Grid::wordmark(self.stops());
        if t >= 1.0 {
            self.theme = to;
            self.mark_small = effects::Grid::wordmark(self.stops());
            self.theme_blend = None;
        }
    }

    /// The RetroArch command once the launch animation has run its course.
    pub fn take_launch(&mut self) -> Option<(std::process::Command, String, Option<Geometry>)> {
        let l = self.launching.as_mut()?;
        if l.spawned || ((self.now - l.started) as f32) < LAUNCH_SECS - 0.2 {
            return None;
        }
        l.spawned = true;
        let cmd = std::mem::replace(&mut l.cmd, std::process::Command::new("true"));
        Some((cmd, l.title.clone(), l.lines))
    }

    pub fn activate(&mut self) -> Action {
        if !self.menu_live || self.running.is_some() {
            return Action::None;
        }
        if !matches!(self.screen, Screen::Menu) {
            return self.activate_browser();
        }
        self.pending.push(Sound::Select);
        match self.sel {
            0 => {
                if self.library.systems.is_empty() {
                    self.message = Some(("no systems in systems.toml".into(), self.now + 4.0));
                } else {
                    self.go(Screen::Systems { sel: 0, top: 0 });
                }
            }
            1 => match self.library.systems.iter().position(|s| s.is_video()) {
                Some(i) => {
                    self.list_from_home = true;
                    self.open_games(Some(i));
                }
                None => {
                    self.message = Some(("no video folder in systems.toml".into(), self.now + 4.0))
                }
            },
            2 => {
                let list = self.favorites.clone();
                self.list_from_home = true;
                self.open_virtual(&list);
            }
            3 => {
                let list = self.recent.clone();
                self.list_from_home = true;
                self.open_virtual(&list);
            }
            4 => self.go(Screen::Settings { sel: 0 }),
            5 => self.go(Screen::About { top: 0 }),
            _ => self.go(Screen::Power { sel: 0 }),
        }
        Action::None
    }

    fn activate_settings(&mut self, sel: usize) -> Action {
        self.pending.push(Sound::Select);
        match sel {
            0 => self.go(Screen::Profile { sel: 0 }),
            1 => self.go(Screen::VideoFit { sel: 0 }),
            2 => {
                if Bluetooth::available() {
                    self.go(Screen::Pair { sel: 0 });
                    if self.bt.devices.is_empty() {
                        self.bt.start_scan();
                    }
                } else {
                    self.message = Some(("bluetoothctl not found".into(), self.now + 4.0));
                }
            }
            3 => self.go(Screen::Saver { sel: 0 }),
            4 => {
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
            _ => {
                self.diag = self.gather_diagnostics();
                self.go(Screen::Diag { top: 0 });
            }
        }
        Action::None
    }

    /// Video fit rows: standard, film 24, aspect, overscan, retro 240p.
    fn adjust_fit(&mut self, row: usize, dir: i32) {
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
            _ => {}
        }
    }

    /// Start converting the selected video for the CRT (X button).
    pub fn convert_selected(&mut self) {
        let Screen::Games { sel, .. } = self.screen else {
            return;
        };
        let Some(entry) = self.games.get(sel).cloned() else {
            return;
        };
        if !self.library.systems[entry.sys].is_video() {
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
    fn tick_conversion(&mut self) {
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
                self.games = self.entries_for(i);
                if let Screen::Games { sel, .. } = &mut self.screen {
                    *sel = sel_keep.min(self.games.len().saturating_sub(1));
                }
            }
        }
    }

    /// The Power submenu: back to the desktop, power off with confirmation.
    fn activate_power(&mut self, sel: usize) -> Action {
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
                Action::Launch("systemctl poweroff".into())
            }
        }
    }

    /// Screensaver settings rows: enabled, idle time, effect, preview.
    fn adjust_saver(&mut self, row: usize, dir: i32) {
        let sv = &mut self.settings.screensaver;
        match row {
            0 => sv.enabled = !sv.enabled,
            1 => {
                let v = sv.idle_secs as i32 + dir * 30;
                sv.idle_secs = v.clamp(30, 900) as u32;
            }
            2 => {
                let names: Vec<&str> = std::iter::once("random")
                    .chain(effects::ALL.iter().map(|k| k.name()))
                    .collect();
                let i = names.iter().position(|n| *n == sv.effect).unwrap_or(0) as i32;
                let next = (i + dir).rem_euclid(names.len() as i32) as usize;
                sv.effect = names[next].to_string();
            }
            _ => {}
        }
    }

    fn save_settings(&mut self) {
        if let Err(e) = self.settings.save(&self.library.config_dir) {
            eprintln!("settings: {e}");
        }
    }

    /// Facts about the machine and the setup, for the Diagnostics screen.
    fn gather_diagnostics(&self) -> Vec<(String, String)> {
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

    // -- game browser (systems, then games; recent and favorites on top) -----

    const ROWS_PER_PAGE: usize = 13;
    const VIRTUAL: usize = 3; // recent/, favorites/, collections/

    fn open_games(&mut self, sys: Option<usize>) {
        self.game_dir = None;
        self.games = match sys {
            Some(i) => self.entries_for(i),
            None => Vec::new(),
        };
        self.screen = Screen::Games {
            sys,
            sel: 0,
            top: 0,
        };
    }

    /// Entries of system `i` in the folder currently browsed.
    fn entries_for(&self, i: usize) -> Vec<Entry> {
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

    /// Counts for the systems screen, computed once per library read so the
    /// screen never rescans folders while drawing.
    fn refresh_counts(&mut self) {
        self.system_counts = self
            .library
            .systems
            .iter()
            .map(|s| self.library.count(s))
            .collect();
    }

    /// Step into a subfolder of the current system.
    fn enter_folder(&mut self, sys: usize, dir: PathBuf) {
        self.game_dir = Some(dir);
        self.games = self.entries_for(sys);
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
        self.games = self.entries_for(sys);
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

    fn open_virtual(&mut self, list: &[(usize, PathBuf)]) {
        self.games = list
            .iter()
            .filter(|(i, p)| *i < self.library.systems.len() && p.exists())
            .map(|(i, p)| Entry {
                game: Game {
                    title: crate::library::clean_title(p),
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
        self.screen = Screen::Games {
            sys: None,
            sel: 0,
            top: 0,
        };
    }

    /// Rows of the systems screen: recent/, favorites/, then every system.
    fn system_rows(&self) -> usize {
        Self::VIRTUAL + self.library.systems.len()
    }

    fn navigate_browser(&mut self, nav: Nav) {
        let mut moved = false;
        let system_rows = self.system_rows();
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
            Screen::Games { sel, top, .. } => {
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
                        if let Screen::Games { sys: Some(i), .. } = self.screen {
                            if self.leave_folder(i) {
                                return;
                            }
                        }
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
                            Screen::Games { sys: Some(i), .. } => i + Self::VIRTUAL,
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
                        self.screen = Screen::Settings { sel: 0 };
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
                    self.screen = Screen::Settings { sel: 1 };
                    moved = true;
                }
                _ => {}
            },
            Screen::Settings { sel } => match nav {
                Nav::Up if *sel > 0 => {
                    *sel -= 1;
                    moved = true;
                }
                Nav::Down if *sel + 1 < SETTINGS_ITEMS.len() => {
                    *sel += 1;
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
                Nav::Down if *sel + 1 < 4 => {
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
                    self.screen = Screen::Settings { sel: 3 };
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
                    self.screen = Screen::Settings { sel: 5 };
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
                        self.screen = Screen::Settings { sel: 4 };
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
                    self.screen = Screen::Settings { sel: 1 };
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
                    self.screen = Screen::Menu;
                    moved = true;
                }
                _ => {}
            },
            Screen::Menu => {}
        }
        if moved {
            self.pending.push(Sound::Move);
        }
    }

    fn adjust_profile(&mut self, row: usize, dir: i32) {
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

    fn activate_browser(&mut self) -> Action {
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
                    i => self.open_games(Some(i - Self::VIRTUAL)),
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
            Screen::Power { sel } => self.activate_power(sel),
            Screen::Saver { sel } => {
                if sel == 3 {
                    self.pending.push(Sound::Select);
                    let kind = self.chosen_effect();
                    let now = self.now;
                    self.start_screensaver(now, kind);
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
            Screen::Diag { .. } | Screen::About { .. } => Action::None,
            Screen::Menu => Action::None,
        }
    }

    /// True once when a TV profile shift changed; the host saves the profile
    /// and moves the picture so the change shows while adjusting.
    pub fn take_profile_preview(&mut self) -> bool {
        std::mem::take(&mut self.profile_preview)
    }

    pub fn save_profile(&self) {
        if let Err(e) = self.profile.save(&self.library.config_dir) {
            eprintln!("profile: {e}");
        }
    }

    /// Called by the host with the size of the drawable output.
    pub fn set_output_size(&mut self, w: u32, h: u32) {
        self.output_size = (w.max(1), h.max(1));
    }

    /// True when the output is a wide super resolution (3520x240 and the
    /// like) where a square pixel picture would show as a thin strip.
    fn wide_output(&self) -> bool {
        self.output_size.0 as f32 / self.output_size.1 as f32 > 3.0
    }

    fn run_entry(&mut self, entry: &Entry) -> Action {
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
            if entry.game.crt_path.is_none() {
                let probe = videofit::probe(&entry.game.path);
                let plan = videofit::plan(&probe, &self.settings.video);
                self.message = Some((format!("fit: {}", plan.label()), self.now + 4.0));
                lines.extend(plan.mpv_args());
            }
            if self.wide_output() {
                lines.push("--keepaspect=no".into());
            }
            lines.join("\n")
        } else {
            let mut keys = self.profile.retroarch_keys();
            if self.wide_output() {
                // Fill the frame (aspect 24 = Full): the tube turns the wide frame back into 4:3.
                let (w, h) = self.output_size;
                keys.push_str(&format!(
                    "aspect_ratio_index = \"24\"\nvideo_aspect_ratio = \"{:.4}\"\nvideo_scale_integer = \"false\"\ncustom_viewport_x = \"0\"\ncustom_viewport_y = \"0\"\ncustom_viewport_width = \"{w}\"\ncustom_viewport_height = \"{h}\"\n",
                    w as f32 / h as f32
                ));
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
                })
            } else {
                None
            }
        } else {
            None
        };
        match self.library.command(&system, &entry.game, &extra) {
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
        save_list(
            &self.library.config_dir.join("recent.txt"),
            &self.recent,
            &self.library,
        );
    }

    /// Toggle the selected game in the favorites list.
    pub fn toggle_favorite(&mut self) {
        let Screen::Games { sel, .. } = self.screen else {
            return;
        };
        let Some(entry) = self.games.get(sel).cloned() else {
            return;
        };
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
        self.launching = None;
        self.player = None;
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
        match system {
            Some("profile") => self.screen = Screen::Profile { sel: 0 },
            Some("pair") => self.screen = Screen::Pair { sel: 0 },
            Some("settings") => self.screen = Screen::Settings { sel: 0 },
            Some("about") => self.screen = Screen::About { top: 0 },
            Some("saver") => self.screen = Screen::Saver { sel: 0 },
            Some("diag") => {
                self.diag = self.gather_diagnostics();
                self.screen = Screen::Diag { top: 0 };
            }
            Some("power") => self.screen = Screen::Power { sel: 0 },
            Some("style") => self.screen = Screen::Style { sel: 0 },
            Some("fit") => self.screen = Screen::VideoFit { sel: 0 },
            Some(n) => match self.library.systems.iter().position(|s| s.name == n) {
                Some(i) => self.open_games(Some(i)),
                None => self.screen = Screen::Systems { sel: 0, top: 0 },
            },
            None => self.screen = Screen::Systems { sel: 0, top: 0 },
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.is_some()
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

    /// Remember which pad family is connected, for on-screen button labels.
    pub fn set_pad(&mut self, name: Option<&str>) {
        self.pad = name.map(PadKind::from_name).unwrap_or(PadKind::Keyboard);
    }

    fn hint(&self, parts: &[(&str, &str)]) -> String {
        let l = self.pad.labels();
        parts
            .iter()
            .map(|(button, what)| {
                let b = match *button {
                    "A" => l.accept,
                    "B" => l.back,
                    "Y" => l.fav,
                    "X" => l.alt,
                    other => other,
                };
                format!("{b} {what}")
            })
            .collect::<Vec<_>>()
            .join("  ")
    }

    /// Compact header used by the browser and the running screen: small icon,
    /// the wordmark at 1 px per cell, a prompt line underneath.
    fn draw_header(&self, fb: &mut Framebuffer, prompt: &str) -> i32 {
        let left = (fb.w as f32 * 0.05) as i32 + self.slide();
        fb.bitmap(left, 8, &ICON_24, self.theme.green, 1, 24);
        let mx = fb.w as i32 - left - self.mark_small.cols;
        for cell in &self.mark_small.cells {
            effects::draw_cell(fb, mx, 10, 1, cell, cell.final_color);
        }
        fb.text(left, 40, prompt, self.theme.dim, 1);
        let clock = chrono::Local::now().format("%H:%M").to_string();
        fb.text(
            fb.w as i32 - left - Framebuffer::text_width(&clock, 1),
            40,
            &clock,
            scale(self.theme.dim, 0.7),
            1,
        );
        52
    }

    fn draw_row(
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
            fb.rect(left, y - 2, w - 2 * margin, 12, self.theme.selection);
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

    fn draw_browser(&mut self, fb: &mut Framebuffer) {
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
                let systems = self.library.systems.clone();
                // The selected console sits on the right; rows make room.
                let panel = 72;
                self.row_shrink = panel + 8;
                if sel >= Self::VIRTUAL {
                    let name = systems[sel - Self::VIRTUAL].name.clone();
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
                            let count = self
                                .system_counts
                                .get(i - Self::VIRTUAL)
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
                if sel >= Self::VIRTUAL {
                    if let Some(sys) = systems.get(sel - Self::VIRTUAL) {
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
                        "omarchy-crt library collections import <folder>",
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
                    Some(i) => match &self.game_dir {
                        Some(d) => format!(
                            "{} / {}",
                            self.library.systems[i].name,
                            d.file_name()
                                .map(|n| n.to_string_lossy())
                                .unwrap_or_default()
                        ),
                        None => self.library.systems[i].name.clone(),
                    },
                    None => match self.open_collection {
                        Some(ci) => self
                            .library
                            .collections()
                            .get(ci)
                            .map(|(n, _)| n.clone())
                            .unwrap_or_else(|| "Collection".into()),
                        None if self.virtual_row == 1 => "Favorites".to_string(),
                        None => "Recent".to_string(),
                    },
                };
                let n = self.games.len();
                let y0 = self.draw_header(fb, &prompt);
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
                            let key = crate::art::Art::cover_key(&system, &entry.game.path);
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
                }
                if n == 0 {
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
                    let end = (top + Self::ROWS_PER_PAGE).min(n);
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
                let hint = if is_video {
                    self.hint(&[("A", "play"), ("X", "convert"), ("Y", "fav"), ("B", "back")])
                } else {
                    self.hint(&[("A", "run"), ("B", "back"), ("Y", "fav"), ("<>", "page")])
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
                let hint = self.hint(&[("A", "pair/scan"), ("B", "back")]);
                fb.text(left, h - 16, &hint, scale(self.theme.dim, 0.7), 1);
            }
            Screen::Settings { sel } => {
                self.draw_menu_screen(fb, "Settings", &SETTINGS_ITEMS, sel);
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
            Screen::Menu => {}
        }
        if let Some((msg, _)) = &self.message {
            if !matches!(self.screen, Screen::Pair { .. }) {
                fb.text(left, h - 28, &cut(msg, max_cols - 8), self.theme.cyan, 1);
            }
        }
    }

    /// Video overlay: title, progress bar, times, pause state. mpv draws the
    /// picture in its own fullscreen window; this is what the shell shows
    /// underneath and on a second output.
    fn draw_player(&mut self, fb: &mut Framebuffer) {
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

    fn draw_running(&mut self, fb: &mut Framebuffer) {
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
        self.art.poll();
        self.row_shrink = 0;
        self.now = now;
        self.tick_theme();
        self.tick_conversion();
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
            if self.player.is_some() {
                self.draw_player(fb);
            } else if self.launching.is_some()
                && ((now - self.launching.as_ref().unwrap().started) as f32) < LAUNCH_SECS
            {
                self.draw_launching(fb);
            } else {
                self.draw_running(fb);
            }
            return;
        }
        if self.menu_live && !matches!(self.screen, Screen::Menu) {
            if matches!(self.screen, Screen::Pair { .. }) && self.bt.poll() {
                self.pending.push(Sound::Lock);
            }
            self.draw_browser(fb);
            if let Some((_, until)) = &self.message {
                if self.now > *until {
                    self.message = None;
                }
            }
            let limit = self.idle_limit();
            if limit > 0.0 && (now - self.last_input) as f32 > limit {
                let kind = self.chosen_effect();
                self.start_screensaver(now, kind);
            }
            return;
        }
        self.draw_post(fb, t);
        self.draw_logo(fb, t);
        self.draw_etch(fb, t);
        self.draw_crt_tag(fb, t);
        self.draw_home(fb, t);
        if t >= 4.0 && !self.chime_played {
            self.chime_played = true;
            self.pending.push(Sound::Chime);
        }
        if t > 9.6 && !self.menu_live {
            self.menu_live = true;
            self.last_input = now;
        }
        if let Some((_, until)) = &self.message {
            if self.now > *until {
                self.message = None;
            }
        }
        let limit = self.idle_limit();
        if self.menu_live && limit > 0.0 && (now - self.last_input) as f32 > limit {
            let kind = self.chosen_effect();
            self.start_screensaver(now, kind);
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
            if self.post_clicks % 2 == 0 {
                self.pending.push(Sound::Click);
            }
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
            if !self.etch_sound_played {
                self.etch_sound_played = true;
                self.pending_samples.push(etch.synth(crate::audio::RATE));
            }
            etch.advance_to(t - ETCH_START);
            etch.draw(fb, x, y, MARK_SCALE, fade);
        }
    }

    fn tag_geometry(&self, fb: &Framebuffer) -> (i32, i32, i32, i32) {
        // Right-aligned under the wordmark's last letters.
        let mw = self.mark_cols * MARK_SCALE;
        let mark_x = (fb.w as i32 - mw) / 2;
        let mark_bottom = self.mark_final_y(fb) + self.mark_rows * 2 * MARK_SCALE;
        let tw = crate::crt_tag::COLS * TAG_SCALE;
        let th = crate::crt_tag::ROWS * TAG_SCALE;
        (mark_x + mw - tw - 2, mark_bottom + 2, tw, th)
    }

    /// "CRT" appears like a tape hunting for sync (TTE `vhstape`): torn lines,
    /// a tracking wave, snow, then a clean redraw with a lock click.
    /// "CRT" traced by an electron beam, letter notes, a stamp and a glint.
    fn draw_crt_tag(&mut self, fb: &mut Framebuffer, t: f32) {
        if t < TAG_START {
            return;
        }
        let local = t - TAG_START;
        let (x, y, _, _) = self.tag_geometry(fb);
        if !self.tag_sound_played {
            self.tag_sound_played = true;
            self.pending.push(Sound::TagReveal);
        }
        let look = crate::crt_tag::Look {
            stops: self.stops(),
            bg: self.theme.bg,
            floor_light: self.theme.dim,
            floor_dark: self.theme.fg_dark_floor(),
            orange: self.theme.orange,
            yellow: self.theme.yellow,
        };
        crate::crt_tag::draw(fb, x, y, TAG_SCALE, local, &look);
    }

    /// Horizontal slide-in offset for a screen that just opened.
    fn slide(&self) -> i32 {
        let p = ((self.now - self.screen_since) / 0.12).clamp(0.0, 1.0) as f32;
        ((1.0 - ease(p)) * 40.0) as i32
    }

    /// One Omarchy style menu row: icon, label, chevron; the selected row sits
    /// on a band in the theme's selection color with accent colored text.
    #[allow(clippy::too_many_arguments)]
    fn draw_menu_row(
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
        let (icon_c, text_c, chev_c) = if on {
            (self.theme.accent, self.theme.accent, self.theme.accent)
        } else {
            (self.theme.dim, self.theme.paper, self.theme.dim)
        };
        fb.bitmap(x + 4, y + 2, icon, scale(icon_c, fade), 1, 8);
        fb.text(x + 18, y + 2, label, scale(text_c, fade), 1);
        if submenu {
            let cx = x + width - 8;
            for i in 0..3 {
                fb.put(cx + i, y + 3 + i, scale(chev_c, fade));
                fb.put(cx + i, y + 9 - i, scale(chev_c, fade));
            }
        }
    }

    /// Animate the selection band toward `target_y`; returns the band's y.
    fn band(&mut self, target_y: i32) -> i32 {
        let target = target_y as f32;
        if self.band_y < 0.0 {
            self.band_y = target;
        } else {
            let k = 0.35;
            self.band_y += (target - self.band_y) * k;
            if (self.band_y - target).abs() < 0.5 {
                self.band_y = target;
            }
        }
        self.band_y.round() as i32
    }

    /// Home menu under the logo: Play..., six entries, footer.
    fn draw_home(&mut self, fb: &mut Framebuffer, t: f32) {
        let fade = ease(clamp((t - 9.65) / 0.45, 0.0, 1.0));
        if fade <= 0.0 {
            return;
        }
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32;
        let width = w - 2 * left;
        let (_, tag_y, _, tag_h) = self.tag_geometry(fb);
        let y0 = tag_y + tag_h + 4;
        fb.text(left + 4, y0, "Play...", scale(self.theme.dim, fade), 1);
        let rows_y = y0 + 12;
        let row_h = 14;
        let band_y = self.band(rows_y + self.sel as i32 * row_h);
        if self.menu_live {
            fb.rect(
                left,
                band_y,
                width,
                row_h - 1,
                scale(self.theme.selection, fade),
            );
        }
        for (i, (icon, label, submenu)) in HOME.iter().enumerate() {
            let y = rows_y + i as i32 * row_h;
            self.draw_menu_row(
                fb,
                left,
                y,
                width,
                icon,
                label,
                *submenu,
                self.menu_live && i == self.sel,
                fade,
            );
        }
        let max_cols = (width / 8) as usize;
        let cut = |s: &str| -> String { s.chars().take(max_cols).collect() };
        if let Some((msg, _)) = &self.message {
            fb.text(left, h - 28, &cut(msg), scale(self.theme.cyan, fade), 1);
        }
    }

    /// A submenu drawn like the home rows under the compact header.
    fn draw_menu_screen(
        &mut self,
        fb: &mut Framebuffer,
        title: &str,
        items: &[(icons::Icon, &str, bool)],
        sel: usize,
    ) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32 + self.slide();
        let width = w - 2 * (w as f32 * 0.05) as i32;
        let y0 = self.draw_header(fb, title);
        let row_h = 14;
        let band_y = self.band(y0 + sel as i32 * row_h);
        fb.rect(left, band_y, width, row_h - 1, self.theme.selection);
        for (i, (icon, label, sub)) in items.iter().enumerate() {
            let y = y0 + i as i32 * row_h;
            self.draw_menu_row(fb, left, y, width, icon, label, *sub, i == sel, 1.0);
        }
        let max_cols = (width / 8) as usize;
        if let Some((msg, _)) = &self.message {
            let m: String = msg.chars().take(max_cols).collect();
            fb.text(left, h - 28, &m, self.theme.cyan, 1);
        }
        let hint = self.hint(&[("A", "select"), ("B", "back")]);
        fb.text(left, h - 14, &hint, scale(self.theme.dim, 0.7), 1);
    }

    /// Screensaver settings: enabled, idle time, effect, preview.
    fn draw_saver_settings(&mut self, fb: &mut Framebuffer, sel: usize) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32 + self.slide();
        let width = w - 2 * (w as f32 * 0.05) as i32;
        let y0 = self.draw_header(fb, "Screensaver");
        let sv = self.settings.screensaver.clone();
        let rows: [(&str, String); 4] = [
            (
                "enabled",
                if sv.enabled {
                    "on".into()
                } else {
                    "off".into()
                },
            ),
            ("after", format!("{} s", sv.idle_secs)),
            ("effect", sv.effect.clone()),
            ("preview", String::new()),
        ];
        let row_h = 14;
        let band_y = self.band(y0 + sel as i32 * row_h);
        fb.rect(left, band_y, width, row_h - 1, self.theme.selection);
        for (i, (label, value)) in rows.iter().enumerate() {
            let y = y0 + i as i32 * row_h;
            let on = i == sel;
            let c = if on {
                self.theme.accent
            } else {
                self.theme.paper
            };
            fb.text(left + 18, y + 2, label, c, 1);
            let right = if i < 3 {
                format!("< {value} >")
            } else {
                value.clone()
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
        let n = effects::ALL.len();
        let note = format!("{n} effects ported so far, random picks one");
        let max_cols = (width / 8) as usize;
        fb.text(
            left,
            h - 28,
            &note.chars().take(max_cols).collect::<String>(),
            scale(self.theme.dim, 0.7),
            1,
        );
        let hint = self.hint(&[("<>", "change"), ("A", "preview"), ("B", "back saves")]);
        fb.text(left, h - 14, &hint, scale(self.theme.dim, 0.7), 1);
    }

    /// Diagnostics: key and value rows, scrollable.
    fn draw_diag(&mut self, fb: &mut Framebuffer, top: usize) {
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
    fn draw_style(&mut self, fb: &mut Framebuffer, sel: usize) {
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

    /// Launch animation: a cartridge slides into its slot (or a disc spins
    /// up), a click, then the picture cuts to black for the emulator.
    fn draw_launching(&mut self, fb: &mut Framebuffer) {
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
        if disc {
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

    /// Video fit settings: how modern video is adapted to the tube.
    fn draw_video_fit(&mut self, fb: &mut Framebuffer, sel: usize) {
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

    /// About: goals and credits, scrollable text.
    fn draw_about(&mut self, fb: &mut Framebuffer, top: usize) {
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

/// Lists of `system_name<TAB>path` lines; system names survive reordering.
fn load_list(path: &std::path::Path, lib: &Library) -> Vec<(usize, PathBuf)> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|l| {
            let (name, p) = l.split_once('\t')?;
            let i = lib.systems.iter().position(|s| s.name == name)?;
            Some((i, PathBuf::from(p)))
        })
        .collect()
}

fn save_list(path: &std::path::Path, list: &[(usize, PathBuf)], lib: &Library) {
    let text: String = list
        .iter()
        .filter_map(|(i, p)| Some(format!("{}\t{}\n", lib.systems.get(*i)?.name, p.display())))
        .collect();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(path, text) {
        eprintln!("cannot write {}: {e}", path.display());
    }
}
