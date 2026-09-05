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
use crate::profile::{PRESETS, Profile};
use crate::settings::Settings;
use crate::theme::Theme;
use std::path::PathBuf;

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
    Systems {
        sel: usize,
        top: usize,
    },
    /// `sys` is None for the virtual lists (recent, favorites).
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
const HOME: [(icons::Icon, &str, bool); 6] = [
    (icons::GAMEPAD, "Games", true),
    (icons::STAR, "Favorites", true),
    (icons::CLOCK, "Recent", true),
    (icons::GEAR, "Settings", true),
    (icons::INFO, "About", true),
    (icons::POWER, "Power", true),
];

/// Settings submenu entries.
const SETTINGS_ITEMS: [(icons::Icon, &str, bool); 4] = [
    (icons::TV, "TV profile", true),
    (icons::PAD, "Pads", true),
    (icons::SAVER, "Screensaver", true),
    (icons::PULSE, "Diagnostics", true),
];

/// Power submenu entries.
const POWER_ITEMS: [(icons::Icon, &str, bool); 2] = [
    (icons::DESKTOP, "Back to desktop", false),
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
    info: SysInfo,
    band_y: f32,
    screen_since: f64,
    settings: Settings,
    diag: Vec<(String, String)>,
    /// A game list opened from the home menu goes back to it, not to Games.
    list_from_home: bool,
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
        let stops = [theme.magenta, theme.cyan, theme.paper];
        let grid = effects::Grid::wordmark(stops);
        let (mark_cols, mark_rows) = (grid.cols, grid.rows);
        Self {
            theme,
            info,
            band_y: -1.0,
            screen_since: 0.0,
            settings: Settings::load(&library.config_dir),
            diag: Vec::new(),
            list_from_home: false,
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
            pad: PadKind::Generic,
            bt: Bluetooth::new(),
            profile: Profile::load(&library.config_dir),
            recent: load_list(&library.config_dir.join("recent.txt"), &library),
            favorites: load_list(&library.config_dir.join("favorites.txt"), &library),
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
            1 => {
                let list = self.favorites.clone();
                self.list_from_home = true;
                self.open_virtual(&list);
            }
            2 => {
                let list = self.recent.clone();
                self.list_from_home = true;
                self.open_virtual(&list);
            }
            3 => self.go(Screen::Settings { sel: 0 }),
            4 => self.go(Screen::About { top: 0 }),
            _ => self.go(Screen::Power { sel: 0 }),
        }
        Action::None
    }

    fn activate_settings(&mut self, sel: usize) -> Action {
        self.pending.push(Sound::Select);
        match sel {
            0 => self.go(Screen::Profile { sel: 0 }),
            1 => {
                if Bluetooth::available() {
                    self.go(Screen::Pair { sel: 0 });
                    if self.bt.devices.is_empty() {
                        self.bt.start_scan();
                    }
                } else {
                    self.message = Some(("bluetoothctl not found".into(), self.now + 4.0));
                }
            }
            2 => self.go(Screen::Saver { sel: 0 }),
            _ => {
                self.diag = self.gather_diagnostics();
                self.go(Screen::Diag { top: 0 });
            }
        }
        Action::None
    }

    /// The Power submenu: back to the desktop, power off with confirmation.
    fn activate_power(&mut self, sel: usize) -> Action {
        self.pending.push(Sound::Select);
        match sel {
            0 => Action::Quit,
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
    const VIRTUAL: usize = 2; // recent/, favorites/

    fn open_games(&mut self, sys: Option<usize>) {
        self.games = match sys {
            Some(i) => {
                let system = self.library.systems[i].clone();
                self.library
                    .games(&system)
                    .into_iter()
                    .map(|game| Entry { game, sys: i })
                    .collect()
            }
            None => Vec::new(),
        };
        self.screen = Screen::Games {
            sys,
            sel: 0,
            top: 0,
        };
    }

    fn open_virtual(&mut self, list: &[(usize, PathBuf)]) {
        self.games = list
            .iter()
            .filter(|(i, p)| *i < self.library.systems.len() && p.exists())
            .map(|(i, p)| Entry {
                game: Game {
                    title: crate::library::clean_title(p),
                    path: p.clone(),
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
                        if self.list_from_home {
                            self.screen = Screen::Menu;
                            self.pending.push(Sound::Move);
                            return;
                        }
                        let row = match self.screen {
                            Screen::Games { sys: Some(i), .. } => i + Self::VIRTUAL,
                            _ => 0,
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
                    self.screen = Screen::Settings { sel: 2 };
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
                    self.screen = Screen::Settings { sel: 3 };
                    moved = true;
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
            1 => self.profile.h_shift = (self.profile.h_shift + dir).clamp(-16, 16),
            2 => self.profile.v_shift = (self.profile.v_shift + dir).clamp(-16, 16),
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
                match sel {
                    0 => {
                        let list = self.recent.clone();
                        self.open_virtual(&list);
                    }
                    1 => {
                        let list = self.favorites.clone();
                        self.open_virtual(&list);
                    }
                    i => self.open_games(Some(i - Self::VIRTUAL)),
                }
                Action::None
            }
            Screen::Games { sel, .. } => {
                let Some(entry) = self.games.get(sel).cloned() else {
                    return Action::None;
                };
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
            Screen::Diag { .. } | Screen::About { .. } => Action::None,
            Screen::Menu => Action::None,
        }
    }

    fn run_entry(&mut self, entry: &Entry) -> Action {
        let system = self.library.systems[entry.sys].clone();
        match self
            .library
            .command(&system, &entry.game, &self.profile.retroarch_keys())
        {
            Ok(cmd) => {
                self.pending.push(Sound::Select);
                self.running = Some((entry.game.title.clone(), system.name.clone()));
                self.remember(entry);
                Action::Run(cmd, entry.game.title.clone())
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

    /// Remember which pad family is connected, for on-screen button labels.
    pub fn set_pad(&mut self, name: Option<&str>) {
        self.pad = name.map(PadKind::from_name).unwrap_or(PadKind::Generic);
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
        let w = fb.w as i32;
        let margin = (w as f32 * 0.05) as i32;
        let left = margin + self.slide();
        let max_cols = ((w - 2 * margin) / 8) as usize;
        if on {
            fb.rect(left, y - 2, w - 2 * margin, 12, self.theme.selection);
        }
        let room = max_cols.saturating_sub(right.chars().count() + 1);
        let text: String = format!("  {label}").chars().take(room).collect();
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
                        _ => {
                            let sys = &systems[i - Self::VIRTUAL];
                            let count = self.library.games(sys).len();
                            let right = format!(
                                "{count:>4}  {}",
                                crate::library::VideoPolicy::parse(&sys.video).label()
                            );
                            self.draw_row(fb, y, &sys.name, &right, on, self.theme.green);
                            fb.bitmap(left + ox + 4, y + 1, &icons::CONSOLE, icon_c, 1, 8);
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
            Screen::Games { sys, sel, top } => {
                let prompt = match sys {
                    Some(i) => self.library.systems[i].name.clone(),
                    None => "Recent and favorites".to_string(),
                };
                let n = self.games.len();
                let y0 = self.draw_header(fb, &prompt);
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
                        let fav = self.is_favorite(&entry);
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
                fb.text(
                    left,
                    h - 16,
                    "A run  B back  Y fav  <> page",
                    scale(self.theme.dim, 0.7),
                    1,
                );
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
            Screen::Menu => {}
        }
        if let Some((msg, _)) = &self.message {
            if !matches!(self.screen, Screen::Pair { .. }) {
                fb.text(left, h - 28, &cut(msg, max_cols - 8), self.theme.cyan, 1);
            }
        }
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
        let footer = format!(
            "{} {} {}",
            self.info.kernel, self.info.mode, self.theme.name
        );
        fb.text(
            left,
            h - 14,
            &cut(&footer),
            scale(self.theme.dim, 0.7 * fade),
            1,
        );
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
