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
use crate::deck::{self, Deck};
use crate::effects::{self, Effect, Kind, Palette};
use crate::etch::LaserEtch;
use crate::fb::{Color, Framebuffer, lerp_color, scale};
use crate::icons;
use crate::library::{Game, Library};
use crate::music::{self, Item as MusicItem, Music, Source, Track};
use crate::pad::PadKind;
use crate::padmap::{self, Raw, Wizard};
use crate::player::Player;
use crate::profile::{PRESETS, Profile};
use crate::settings::Settings;
use crate::states;
use crate::yt;
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
#[derive(Clone, Copy, PartialEq, Eq)]
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
    MusicSettings {
        sel: usize,
    },
    VideoSettings {
        sel: usize,
    },
    /// Music through cliamp: the list of sources.
    Music {
        sel: usize,
        top: usize,
    },
    /// One list of stations, playlists or tracks; `music_path` says which.
    MusicList {
        sel: usize,
        top: usize,
    },
    /// What plays now, with the visualiser.
    NowPlaying,
    /// The ten band equaliser of the engine, one slider per band.
    Equalizer {
        band: usize,
    },
    /// A game with a state left behind: carry on, or start a new session.
    Resume {
        sel: usize,
    },
    /// Button by button mapping of a pad SDL does not know.
    PadWizard,
    /// Videos hub: local films, YouTube, the clipboard link.
    Videos {
        sel: usize,
    },
    /// YouTube hub: search, watch later, recently watched.
    YouTube {
        sel: usize,
    },
}

/// A row of the music screen.
#[derive(Clone)]
enum MusicRow {
    Now,
    Source(Source),
    Hub(Hub),
    /// A provider's search: the bar asks for the query.
    Search(String),
    /// The ten band equaliser.
    Equalizer,
}

/// The two worlds of cliamp, each with its own screen: the radio directory
/// and every streaming provider the listener set up.
#[derive(Clone, PartialEq, Eq)]
enum Hub {
    Radio,
    Provider(String, String),
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
const HOME: [(icons::Icon, &str, bool); 8] = [
    (icons::GAMEPAD, "Games", true),
    (icons::FILM, "Videos", true),
    (icons::NOTE, "Music", true),
    (icons::STAR, "Favorites", true),
    (icons::CLOCK, "Recent", true),
    (icons::GEAR, "Settings", true),
    (icons::INFO, "About", true),
    (icons::POWER, "Power", true),
];

/// Settings submenu entries.
const SETTINGS_ITEMS: [(icons::Icon, &str, bool); 8] = [
    (icons::TV, "TV profile", true),
    (icons::FIT, "Video fit", true),
    (icons::PAD, "Pads", true),
    (icons::SAVER, "Screensaver", true),
    (icons::BRUSH, "Style", true),
    (icons::PULSE, "Diagnostics", true),
    (icons::NOTE, "Music", true),
    (icons::FILM, "Videos", true),
];

/// Rows of the Music settings page before the one per visualizer.
const MUSIC_ROWS: usize = 7;
/// Rows of the Videos settings page.
const VIDEOS_ROWS: usize = 3;
/// Country codes the radio row cycles through; empty follows the locale.
const COUNTRIES: [&str; 10] = ["", "IT", "US", "GB", "DE", "FR", "ES", "PT", "JP", "BR"];

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

/// Pause menu over a running game.
const PAUSE_ITEMS: [(icons::Icon, &str, bool); 8] = [
    (icons::GAMEPAD, "Resume", false),
    (icons::FOLDER, "Save state", false),
    (icons::FOLDER, "Load state", false),
    (icons::RESUME, "Rewind two seconds", false),
    (icons::RESUME, "Fast forward", false),
    (icons::PULSE, "Slow motion", false),
    (icons::PULSE, "Reset game", false),
    (icons::DESKTOP, "Back to launcher", false),
];

/// What the main loop has to do with the compositor after a pause action.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PauseOutcome {
    None,
    /// The game is paused and the launcher should be brought to the tube.
    Shown,
    /// The game runs again and should be back on the tube.
    Resumed,
    /// The game was told to quit; the child wait takes it from here.
    Quit,
}

/// Rows per page in the systems list.
const SYS_PAGE: usize = 11;
/// Videos hub entries.
const VIDEOS_ITEMS: [(icons::Icon, &str, bool); 3] = [
    (icons::FILM, "Local videos", true),
    (icons::RESUME, "YouTube", true),
    (icons::FOLDER, "Play the link in the clipboard", false),
];

/// YouTube hub entries.
const YOUTUBE_ITEMS: [(icons::Icon, &str, bool); 3] = [
    (icons::NOTE, "Search", true),
    (icons::CLOCK, "Watch later", true),
    (icons::STAR, "Recently watched", true),
];

/// On screen keyboard: four rows of ten keys the pad walks through.
const OSK_ROWS: [&str; 4] = ["1234567890", "QWERTYUIOP", "ASDFGHJKL-", "ZXCVBNM ._"];

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
    /// System name and ROM path of the running game, for its save states.
    running_path: Option<(String, PathBuf)>,
    /// Selected row of the pause menu while the running game is paused.
    paused: Option<usize>,
    mark_small: effects::Grid,
    profile: Profile,
    recent: Vec<(usize, PathBuf)>,
    /// When each recent game was last started, seconds since the epoch.
    recent_at: std::collections::HashMap<PathBuf, i64>,
    favorites: Vec<(usize, PathBuf)>,
    pad: PadKind,
    bt: Bluetooth,
    /// The text typed into the search bar of a game list, when it is open.
    search: Option<String>,
    /// The list before the search filter; `games` is the filtered view.
    games_all: Vec<Entry>,
    /// On screen keyboard cursor (row, column) while a pad types the search.
    osk: Option<(i32, i32)>,
    /// The open list is the whole collection, opened by a search.
    search_global: bool,
    /// Save states RetroArch wrote, looked up per game as rows show.
    states: states::Cache,
    /// Screen a game list returns to when it was opened from a hub.
    games_back: Option<Screen>,
    /// A YouTube search in flight, and the text typed for it.
    yt_search: Option<std::sync::mpsc::Receiver<Result<Vec<yt::Hit>, String>>>,
    yt_query: bool,
    yt_results: bool,
    /// The game list shown as a row of covers instead of rows of text.
    flow_view: bool,
    /// Where the cover row is, in list indices, easing toward the selection.
    flow_pos: f32,
    /// The hi-fi deck and the visualizers of the music screens.
    deck: Deck,
    /// Album art decoded for the cassette label, and the file it came from.
    cover_img: Option<(PathBuf, crate::art::Image)>,
    /// Screen to come back to when the music visualizer stands in for the screensaver.
    music_saver: Option<Screen>,
    /// The visualizer chosen on purpose (X) rather than by idling.
    music_visual: bool,
    /// X also flips the deck between cassette and turntable: None follows the
    /// source, Some forces one look.
    deck_look: Option<bool>,
    /// The list a station was tuned from and the index in it: left and right move along it.
    tuning: Option<(Vec<Track>, usize)>,
    /// Sleep timer: deadline and the volume to restore.
    sleep: Option<(f64, f64)>,
    sleep_set_at: f64,
    rumble_pending: bool,
    wizard: Option<Wizard>,
    /// A pad that arrived while the wizard could not show: name, guid, id.
    pending_wizard: Option<(String, String, u32)>,
    /// The game waiting for an answer on the resume screen, and the screen
    /// the question was asked from.
    pending_entry: Option<(Entry, Box<Screen>)>,
    remap_request: bool,
    music: Music,
    /// Open music lists, innermost last: source, selected row, first shown row.
    music_path: Vec<(Source, usize, usize)>,
    music_root_sel: usize,
    /// The hub open on the music screen, None at the root.
    music_hub: Option<Hub>,
    music_hub_sel: usize,
    /// A provider search waiting for its query (provider key).
    music_query: Option<String>,
    /// Visualiser bars eased toward the last spectrum frame.
    vis: Vec<f32>,
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
            art: crate::art::Art::new(crate::covers::regions_for(&settings.music.country)),
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
            search: None,
            games_all: Vec::new(),
            osk: None,
            search_global: false,
            states: states::Cache::default(),
            games_back: None,
            yt_search: None,
            yt_query: false,
            yt_results: false,
            flow_view: false,
            flow_pos: 0.0,
            deck: Deck::new(),
            cover_img: None,
            music_saver: None,
            music_visual: false,
            deck_look: None,
            tuning: None,
            sleep: None,
            sleep_set_at: 0.0,
            rumble_pending: false,
            wizard: None,
            pending_wizard: None,
            pending_entry: None,
            remap_request: false,
            music: Music::new(std::env::var("PULSE_SINK").ok()),
            music_path: Vec::new(),
            music_root_sel: 0,
            music_hub: None,
            music_hub_sel: 0,
            music_query: None,
            vis: vec![0.0; 10],
            profile: Profile::load(&library.config_dir),
            recent: load_list(&library.config_dir.join("recent.txt"), &library),
            recent_at: load_times(&library.config_dir.join("recent.txt")),
            favorites: load_list(&library.config_dir.join("favorites.txt"), &library),
            library,
            screen: Screen::Menu,
            games: Vec::new(),
            running: None,
            running_path: None,
            paused: None,
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
        if let Some(prev) = self.music_saver.take() {
            self.screen = prev;
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

    /// Back to the top of the main menu, whatever screen is open. Scripts
    /// driving the control pipe start from here.
    pub fn home(&mut self) {
        if self.running.is_some() || self.launching.is_some() {
            return;
        }
        self.screen = Screen::Menu;
        self.sel = 0;
        self.list_from_home = false;
        self.open_collection = None;
        self.flow_view = false;
        self.pending.push(Sound::Move);
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
                Some(_) => self.go(Screen::Videos { sel: 0 }),
                None => {
                    self.message = Some(("no video folder in systems.toml".into(), self.now + 4.0))
                }
            },
            2 => self.open_music(),
            3 => {
                let list = self.favorites.clone();
                self.list_from_home = true;
                self.open_virtual(&list);
            }
            4 => {
                let list = self.recent.clone();
                self.list_from_home = true;
                self.open_virtual(&list);
            }
            5 => self.go(Screen::Settings { sel: 0 }),
            6 => self.go(Screen::About { top: 0 }),
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
            5 => {
                self.diag = self.gather_diagnostics();
                self.go(Screen::Diag { top: 0 });
            }
            6 => self.go(Screen::MusicSettings { sel: 0 }),
            _ => self.go(Screen::VideoSettings { sel: 0 }),
        }
        Action::None
    }

    /// Music settings rows: how the deck and the visualizers behave.
    fn adjust_music(&mut self, row: usize, dir: i32) {
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
                m.country = COUNTRIES[(i + dir).rem_euclid(COUNTRIES.len() as i32) as usize].to_string();
                self.art = crate::art::Art::new(crate::covers::regions_for(&self.settings.music.country));
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

    fn adjust_videos(&mut self, row: usize, dir: i32) {
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

    fn visualizer_enabled(&self, mode: usize) -> bool {
        !self
            .settings
            .music
            .disabled_visualizers
            .iter()
            .any(|d| d == deck::MODE_NAMES[mode])
    }

    /// Next or previous visualizer among the ones switched on.
    fn music_mode_step(&mut self, dir: i32) {
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
                self.set_games(self.entries_for(i));
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

    fn video_system(&self) -> Option<usize> {
        self.library.systems.iter().position(|s| s.is_video())
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
        self.go(Screen::Games { sys, sel: 0, top: 0 });
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

    /// Y on a link: in or out of the watch later list.
    fn toggle_watch_later(&mut self, entry: &Entry) {
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
            if kept { format!("watch later: {}", entry.game.title) } else { format!("removed {}", entry.game.title) },
            self.now + 2.5,
        ));
        self.pending.push(Sound::Select);
    }

    /// Enter on the search bar while it asks for a query: run the search
    /// and show the hits as the list.
    fn yt_submit(&mut self) {
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
    fn yt_poll(&mut self) {
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

    fn open_virtual(&mut self, list: &[(usize, PathBuf)]) {
        self.search_global = false;
        // The names of the systems, to turn an arcade set name into a title
        // the way the per system lists do.
        let names: Vec<String> = self.library.systems.iter().map(|s| s.name.clone()).collect();
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
    fn set_games(&mut self, list: Vec<Entry>) {
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
    fn page_rows(&self) -> usize {
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
                all.sort_by(|a, b| a.game.title.to_lowercase().cmp(&b.game.title.to_lowercase()));
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
            hits.sort_by(|a, b| b.0.cmp(&a.0));
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

    /// The music list as shown: the open source's items through the search.
    fn music_visible(&self) -> Vec<MusicItem> {
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

    fn initial(e: &Entry) -> char {
        e.game
            .title
            .chars()
            .find(|c| c.is_alphanumeric())
            .map(|c| if c.is_ascii_digit() { '#' } else { c.to_ascii_uppercase() })
            .unwrap_or('#')
    }

    /// Shoulder buttons on the deck: cassette or turntable; on the
    /// visualizer: the previous or next mode.
    fn music_view_step(&mut self, dir: i32) {
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

    /// Jump to the first title of the next (or previous) initial letter.
    pub fn jump_letter(&mut self, dir: i32) {
        if matches!(self.screen, Screen::NowPlaying) {
            self.music_view_step(dir);
            return;
        }
        let Screen::Games { sel, .. } = self.screen else {
            return;
        };
        let n = self.games.len();
        if n == 0 {
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
        if let Screen::Games { sel, top, .. } = &mut self.screen {
            if *sel != target {
                *sel = target;
                *top = target.min(n.saturating_sub(page));
                self.pending.push(Sound::Move);
            }
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
    fn system_rows(&self) -> usize {
        Self::VIRTUAL + self.library.systems.len()
    }

    fn navigate_browser(&mut self, nav: Nav) {
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
                        if let Screen::Games { sys: Some(i), .. } = self.screen {
                            if self.leave_folder(i) {
                                return;
                            }
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
                    self.screen = Screen::Settings { sel: 6 };
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
                    self.screen = Screen::Settings { sel: 7 };
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
                    self.screen = Screen::Settings { sel: 2 };
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
    fn watch_later(&self, sys: usize) -> Vec<Entry> {
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

    // ------------------------------------------------------------ music

    fn open_music(&mut self) {
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
    fn music_rows(&self) -> Vec<(icons::Icon, String, String, MusicRow)> {
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
                rows.push((icons::FOLDER, "By country".into(), String::new(), MusicRow::Source(Source::Countries)));
                rows.push((icons::FOLDER, "By genre".into(), String::new(), MusicRow::Source(Source::Tags)));
                rows.push((
                    icons::STAR,
                    "cliamp picks".into(),
                    String::new(),
                    MusicRow::Source(Source::ProviderPlaylists("radio".into(), "cliamp picks".into())),
                ));
                rows.push((
                    icons::STAR,
                    "Favourite stations".into(),
                    if self.music.favorites.is_empty() { String::new() } else { format!("{:>4}", self.music.favorites.len()) },
                    MusicRow::Source(Source::Favorites),
                ));
                return rows;
            }
            Some(Hub::Provider(key, name)) => {
                rows.push((icons::NOTE, "Search".into(), String::new(), MusicRow::Search(key.clone())));
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
        rows.push((icons::PULSE, "Radio".into(), String::new(), MusicRow::Hub(Hub::Radio)));
        for p in &self.music.providers {
            if p.key == "radio" || p.key == "local" {
                continue;
            }
            let icon = match p.key.as_str() {
                "spotify" => icons::SPOTIFY,
                "youtube" | "ytmusic" | "yt" => icons::RESUME,
                _ => icons::FOLDER,
            };
            rows.push((icon, p.name.clone(), String::new(), MusicRow::Hub(Hub::Provider(p.key.clone(), p.name.clone()))));
        }
        rows.push((icons::CLOCK, "Recently played".into(), String::new(), MusicRow::Source(Source::History)));
        rows.push((
            icons::FOLDER,
            "Queue".into(),
            if st.total > 0 { format!("{:>4}", st.total) } else { String::new() },
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

    fn music_enter(&mut self, src: Source) {
        self.search = None;
        self.osk = None;
        self.music_query = None;
        self.pending.push(Sound::Select);
        self.music.open(&src);
        self.music_path.push((src, 0, 0));
        self.go(Screen::MusicList { sel: 0, top: 0 });
    }

    fn music_back(&mut self) {
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
    fn music_selected(&self, sel: usize) -> Option<MusicItem> {
        self.music_visible().get(sel).cloned()
    }

    fn music_play_item(&mut self, sel: usize, item: MusicItem) {
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
                    self.pending.push(Sound::Static);
                } else {
                    self.deck.insert_at = self.now;
                    self.pending.push(Sound::Insert);
                }
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
                        .and_then(|l| l.iter().position(|it| matches!(it, MusicItem::Track(x) if x.path == t.path)))
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
                self.pending.push(Sound::Insert);
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
    fn music_alt(&mut self, sel: Option<usize>) {
        if let Some(sel) = sel {
            if let Some(MusicItem::Source(Source::ProviderPlaylist(provider, id, name), _)) =
                self.music_selected(sel)
            {
                self.pending.push(Sound::Select);
                self.music.load(&provider, &id);
                self.message = Some((format!("playing {name}"), self.now + 3.0));
                return;
            }
        }
        if self.music.status.active() {
            self.music.toggle();
            self.pending.push(Sound::Select);
        } else {
            self.message = Some(("nothing is playing".into(), self.now + 2.0));
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
                            self.message = Some(("no link in the clipboard".into(), self.now + 3.0));
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
                        self.osk = if self.pad == PadKind::Keyboard { None } else { Some((1, 0)) };
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
            Screen::Diag { .. } | Screen::About { .. } => Action::None,
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
                        self.osk = if self.pad == PadKind::Keyboard { None } else { Some((1, 0)) };
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

    /// Launch, asking first when the game was left in the middle: RetroArch
    /// would otherwise pick the state up without a word.
    fn run_entry(&mut self, entry: &Entry) -> Action {
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
                    "aspect_ratio_index = \"24\"\nvideo_aspect_ratio = \"{:.4}\"\nvideo_scale_integer = \"false\"\ncustom_viewport_x = \"0\"\ncustom_viewport_y = \"0\"\ncustom_viewport_width = \"{w}\"\ncustom_viewport_height = \"{h}\"\nvideo_windowed_position_width = \"{w}\"\nvideo_windowed_position_height = \"{h}\"\nvideo_window_auto_width_max = \"{w}\"\nvideo_window_auto_height_max = \"{h}\"\n",
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
                    if starred { format!("favourite: {label}") } else { format!("removed {label}") },
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

    pub fn is_paused(&self) -> bool {
        self.running.is_some() && self.paused.is_some()
    }

    /// Pause the running game and open the pause menu, or resume it.
    pub fn toggle_pause(&mut self) -> PauseOutcome {
        if self.running.is_none() || self.player.is_some() {
            return PauseOutcome::None;
        }
        if let Some(l) = &self.launching {
            if !l.spawned {
                return PauseOutcome::None;
            }
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
                    self.message =
                        Some(("rewind is off for this system".into(), self.now + 3.0));
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

    fn draw_pause(&mut self, fb: &mut Framebuffer) {
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
        // The same glint as the home logo, on its own rhythm.
        self.glint(fb, left, 8, 24, 24, 8.0, 3.0);
        self.glint(fb, mx, 10, self.mark_small.cols, self.mark_small.rows * 2, 8.0, 2.6);
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
                                    vec![d.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()]
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
                            let key = crate::art::Art::cover_key(&system, &entry.game.path, cover_box as usize, cover_box as usize);
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
                        let label: String = st.label().chars().take((cover_box / 8) as usize).collect();
                        fb.text(bx, ty + 2, &label, self.theme.green, 1);
                        ty += 10;
                    }
                    if let Some(at) = self.recent_at.get(&entry.game.path) {
                        let when = std::time::UNIX_EPOCH + std::time::Duration::from_secs((*at).max(0) as u64);
                        let label: String = format!("played {}", states::when_label(when))
                            .chars()
                            .take((cover_box / 8) as usize)
                            .collect();
                        fb.text(bx, ty + 2, &label, scale(self.theme.dim, 0.9), 1);
                    }
                }
                if n == 0 && self.yt_query {
                    let msg = if self.yt_search.is_some() { "searching YouTube" } else { "type what to look for, then Enter" };
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
                                if i == sel { self.theme.accent } else { self.theme.green },
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
            Screen::MusicSettings { sel } => {
                self.draw_music_settings(fb, sel);
                return;
            }
            Screen::VideoSettings { sel } => {
                self.draw_video_settings(fb, sel);
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
        self.tick_music(now);
        self.yt_poll();
        if self.pending_wizard.is_some()
            && self.menu_live
            && self.running.is_none()
            && self.launching.is_none()
            && self.wizard.is_none()
            && self.saver.is_none()
        {
            if let Some((name, guid, which)) = self.pending_wizard.take() {
                self.pad_wizard_start(&name, &guid, which);
            }
        }
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
            if self.paused.is_some() {
                self.draw_pause(fb);
            } else if self.player.is_some() {
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
                if self.music.status.playing() && self.settings.music.saver {
                    self.music_saver_start(now);
                } else {
                    let kind = self.chosen_effect();
                    self.start_screensaver(now, kind);
                }
            }
            return;
        }
        self.draw_post(fb, t);
        self.draw_logo(fb, t);
        self.draw_etch(fb, t);
        self.draw_crt_tag(fb, t);
        if self.menu_live {
            // Icon and wordmark together, one sweep every nine seconds.
            let (lx, ly, lsize) = self.logo_final(fb);
            let mw = self.mark_cols * MARK_SCALE;
            let mx = (fb.w as i32 - mw) / 2;
            let bottom = self.mark_final_y(fb) + self.mark_rows * 2 * MARK_SCALE;
            let x0 = mx.min(lx);
            let x1 = (mx + mw).max(lx + lsize);
            self.glint(fb, x0, ly, x1 - x0, bottom - ly, 6.0, 0.0);
        }
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
            if self.music.status.playing() && self.settings.music.saver {
                self.music_saver_start(now);
            } else {
                let kind = self.chosen_effect();
                self.start_screensaver(now, kind);
            }
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
        let rows_y = y0 + 11;
        let row_h = 12;
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
        // What plays, on the Music row itself: the home list leaves no room
        // for a line of its own.
        if fade > 0.9 && self.music.status.active() {
            let row = HOME
                .iter()
                .position(|(_, label, _)| *label == "Music")
                .unwrap_or(0);
            let y = rows_y + row as i32 * row_h;
            let label = self
                .music
                .status
                .track
                .as_ref()
                .map(|t| t.label())
                .unwrap_or_default();
            let state = if self.music.status.playing() { "" } else { "  paused" };
            let text = format!("{label}{state}");
            let vis_w = 30;
            // Room between the row's own label and the chevron.
            let room = ((width - 18 - vis_w - 30 - Framebuffer::text_width("Music", 1)) / 8).max(0) as usize;
            let text: String = text.chars().take(room).collect::<String>().trim_end().to_string();
            let tw = Framebuffer::text_width(&text, 1);
            let tx = left + width - 16 - tw;
            fb.text(tx, y + 2, &text, scale(self.theme.dim, 1.0), 1);
            self.draw_vis(fb, tx - vis_w - 6, y + 9, vis_w, 7);
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
    /// A settings table: label left, `< value >` right, a note for the
    /// selected row above the hints.
    fn draw_settings_table(
        &mut self,
        fb: &mut Framebuffer,
        title: &str,
        rows: &[(String, String)],
        notes: &[&str],
        sel: usize,
        row_h: i32,
    ) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32 + self.slide();
        let width = w - 2 * (w as f32 * 0.05) as i32;
        let y0 = self.draw_header(fb, title);
        let band_y = self.band(y0 + sel as i32 * row_h);
        fb.rect(left, band_y, width, row_h - 1, self.theme.selection);
        for (i, (label, value)) in rows.iter().enumerate() {
            let y = y0 + i as i32 * row_h;
            let on = i == sel;
            fb.text(left + 18, y + 2, label, if on { self.theme.accent } else { self.theme.paper }, 1);
            let right = format!("< {value} >");
            fb.text(
                left + width - 8 - Framebuffer::text_width(&right, 1),
                y + 2,
                &right,
                if on { self.theme.accent } else { self.theme.dim },
                1,
            );
        }
        let max_cols = (width / 8) as usize;
        if let Some(note) = notes.get(sel) {
            fb.text(left, h - 28, &note.chars().take(max_cols).collect::<String>(), scale(self.theme.dim, 0.8), 1);
        }
        let hint = self.hint(&[("<>", "change"), ("B", "save")]);
        fb.text(left, h - 14, &hint, scale(self.theme.dim, 0.7), 1);
    }

    fn draw_music_settings(&mut self, fb: &mut Framebuffer, sel: usize) {
        let m = self.settings.music.clone();
        let onoff = |b: bool| if b { "on".to_string() } else { "off".to_string() };
        let secs = |s: u32| if s == 0 { "never".to_string() } else { format!("{s} s") };
        let mut rows: Vec<(String, String)> = vec![
            ("visualizer after".into(), secs(m.idle_secs)),
            ("change mode every".into(), if m.cycle_secs == 0 { "keep one".into() } else { format!("{} s", m.cycle_secs) }),
            ("as screensaver".into(), onoff(m.saver)),
            ("lyrics".into(), onoff(m.lyrics)),
            ("deck".into(), m.look.clone()),
            ("radio country".into(), if m.country.is_empty() { "locale".into() } else { m.country.clone() }),
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
        for _ in 0..deck::MODES {
            notes.push("in the rotation, or skipped");
        }
        self.draw_settings_table(fb, "Music", &rows, &notes, sel, 11);
    }

    fn draw_video_settings(&mut self, fb: &mut Framebuffer, sel: usize) {
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

    /// Advance the music mirror and ease the visualiser toward the last frame.
    fn tick_music(&mut self, now: f64) {
        let vis = matches!(
            self.screen,
            Screen::Music { .. } | Screen::MusicList { .. } | Screen::NowPlaying
        );
        self.music.tick(now, vis && self.menu_live && self.running.is_none());
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

    pub fn take_rumble(&mut self) -> bool {
        std::mem::take(&mut self.rumble_pending)
    }

    /// The selection band breathes with the beat while music plays.
    fn band_color(&self) -> Color {
        if self.music.status.playing() {
            crate::fb::lerp_color(self.theme.selection, self.theme.accent, self.deck.kick.hit * 0.3)
        } else {
            self.theme.selection
        }
    }

    /// Idle with music on: the visualizer stands in for the screensaver.
    fn music_saver_start(&mut self, now: f64) {
        if self.music_saver.is_none() {
            self.music_saver = Some(self.screen);
            self.screen = Screen::NowPlaying;
            self.deck.mode_since = now;
        }
    }

    /// Y on the deck: no timer, 15, 30, 60 minutes, none again.
    fn sleep_cycle(&mut self) {
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
        let restore = self.sleep.map(|(_, r)| r).unwrap_or(self.music.status.volume);
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

    /// Left or right on the deck while a station list is tuned in.
    fn tune(&mut self, dir: i32) -> bool {
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
        self.pending.push(Sound::Static);
        self.music.play(&t);
        true
    }

    /// Ten bars of the spectrum, bottom aligned in the given box.
    fn draw_vis(&self, fb: &mut Framebuffer, x: i32, bottom: i32, width: i32, height: i32) {
        let n = self.vis.len().max(1) as i32;
        let gap = if width >= n * 6 { 2 } else { 1 };
        let bar_w = ((width - (n - 1) * gap) / n).max(1);
        for (i, v) in self.vis.iter().enumerate() {
            let v = v.clamp(0.0, 1.0);
            let h = ((v * height as f32) as i32).min(height);
            let bx = x + i as i32 * (bar_w + gap);
            fb.rect(bx, bottom - height, bar_w, height, scale(self.theme.selection, 0.8));
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
        let text: String = text.chars().take(room).collect::<String>().trim_end().to_string();
        fb.text(left + vis_w + 8, y, &text, self.theme.paper, 1);
    }

    fn draw_music(&mut self, fb: &mut Framebuffer, sel: usize, top: usize) {
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
            let c = if on { self.theme.accent } else { self.theme.dim };
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

    /// The ten bands as vertical sliders, the picked one lit, with the
    /// preset's name and the gain in dB of the band under the cursor.
    fn draw_equalizer(&mut self, fb: &mut Framebuffer, band: usize) {
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
            fb.text(tx, top + plot_h + 4, f, if on { th.paper } else { scale(th.dim, 0.9) }, 1);
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

    fn draw_music_list(&mut self, fb: &mut Framebuffer, sel: usize, top: usize) {
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
                    if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > max_cols {
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
                fb.text(left, y0 + 8, "type what to look for, then Enter", self.theme.dim, 1);
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
                        let c = if on { self.theme.accent } else { self.theme.bright_green };
                        fb.bitmap(left + self.slide() + 4, y + 1, &icons::NOTE, c, 1, 8);
                    } else if matches!(item, MusicItem::Track(t) if self.music.is_favorite(t)) {
                        fb.bitmap(left + self.slide() + 4, y + 1, &icons::STAR, self.theme.yellow, 1, 8);
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

    fn draw_now_playing(&mut self, fb: &mut Framebuffer) {
        let h = fb.h as i32;
        let w = fb.w as i32;
        let left = (w as f32 * 0.05) as i32;
        let st = self.music.status.clone();
        let track = st.track.clone().unwrap_or_default();
        let title = if !track.title.is_empty() { track.title.clone() } else { track.label() };
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
        let station = self.tuning.as_ref().filter(|_| track.stream).map(|(l, i)| (*i, l.len()));
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
            turntable: self.deck_look.unwrap_or(track.path.starts_with("spotify:") || (!track.stream && !track.album.is_empty())),
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
            fb.text(w - left - Framebuffer::text_width(&s, 1), y0 - 12, &s, theme.orange, 1);
        }
        // Two lines of hints: the deck has more controls than fit in one.
        let skip = if track.stream { "tune" } else { "track" };
        let hint1 = self.hint(&[("A", "pause"), ("<>", skip), ("^v", "volume")]);
        let deck_key = if self.pad == PadKind::Keyboard { "PgUp" } else { "LB" };
        let hint2 = self.hint(&[("X", "show"), (deck_key, "deck"), ("Y", "timer"), ("B", "back")]);
        fb.text(left, h - 24, &hint1, scale(theme.dim, 0.7), 1);
        fb.text(left, h - 14, &hint2, scale(theme.dim, 0.7), 1);
    }

    // --------------------------------------------------------- pad wizard

    pub fn pad_wizard_active(&self) -> bool {
        self.wizard.is_some()
    }

    /// A pad SDL has no mapping for: ask for its buttons one by one. When the
    /// menu is not up yet (boot, a game) the pad waits its turn.
    pub fn pad_wizard_start(&mut self, name: &str, guid: &str, which: u32) {
        if !self.menu_live
            || self.running.is_some()
            || self.launching.is_some()
            || self.wizard.is_some()
            || self.saver.is_some()
        {
            self.pending_wizard = Some((name.to_string(), guid.to_string(), which));
            return;
        }
        self.wizard = Some(Wizard::new(name, guid, which));
        self.pending.push(Sound::Insert);
        self.go(Screen::PadWizard);
    }

    /// A raw joystick input while the wizard runs. Returns the finished
    /// mapping line once the last control is answered.
    pub fn pad_wizard_raw(&mut self, which: u32, raw: Raw) -> Option<String> {
        let now = self.now;
        let w = self.wizard.as_mut()?;
        if w.which != which {
            return None;
        }
        if w.feed(raw, now) {
            self.pending.push(Sound::Click);
        }
        if self.wizard.as_ref().is_some_and(|w| w.finished) {
            return self.pad_wizard_finish();
        }
        None
    }

    fn pad_wizard_finish(&mut self) -> Option<String> {
        let w = self.wizard.take()?;
        self.screen = Screen::Settings { sel: 2 };
        if !w.usable() {
            self.pending.push(Sound::Crunch);
            self.message = Some((
                "pad not mapped: A, B and a way to move are needed".into(),
                self.now + 5.0,
            ));
            return None;
        }
        let mapping = w.mapping();
        match padmap::save(&mapping) {
            Ok(()) => {
                self.pending.push(Sound::Lock);
                self.message = Some((format!("pad mapped: {}", w.name), self.now + 5.0));
            }
            Err(e) => {
                self.pending.push(Sound::Crunch);
                self.message = Some((format!("pad mapping not saved: {e}"), self.now + 5.0));
            }
        }
        Some(mapping)
    }

    pub fn take_remap_request(&mut self) -> bool {
        std::mem::take(&mut self.remap_request)
    }

    fn draw_pad_wizard(&mut self, fb: &mut Framebuffer) {
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32 + self.slide();
        let width = w - 2 * (w as f32 * 0.05) as i32;
        let y0 = self.draw_header(fb, "New pad");
        let Some(wiz) = self.wizard.as_ref() else {
            return;
        };
        let max_cols = (width / 8) as usize;
        let name: String = wiz.name.chars().take(max_cols).collect();
        fb.text(left, y0 + 2, &name, self.theme.paper, 1);
        fb.text(
            left,
            y0 + 14,
            &format!("step {} of {}", wiz.step + 1, padmap::STEPS.len()),
            self.theme.dim,
            1,
        );
        if let Some((_, what)) = wiz.current() {
            fb.text(left, y0 + 34, "Press", self.theme.dim, 1);
            let what: String = what.chars().take(max_cols).collect();
            fb.text(left, y0 + 46, &what, self.theme.bright_green, 2.min(1 + (what.len() * 16 <= width as usize) as i32));
        }
        // What is set so far, newest last.
        let mut y = y0 + 74;
        let start = wiz.binds.len().saturating_sub(6);
        for (field, bind) in &wiz.binds[start..] {
            fb.text(left, y, &format!("{field:<14} {bind}"), scale(self.theme.dim, 0.9), 1);
            y += 10;
        }
        fb.text(
            left,
            h - 40,
            "press A again to skip a control the pad lacks",
            scale(self.theme.dim, 0.8),
            1,
        );
        let hint = self.hint(&[("Enter", "skip"), ("Esc", "cancel")]);
        fb.text(left, h - 14, &hint, scale(self.theme.dim, 0.7), 1);
    }

    /// A glint: every `period` seconds a narrow diagonal highlight sweeps
    /// across the box in under a second, brightening only what is drawn
    /// there. The small movement that keeps a logo alive.
    fn glint(&self, fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, period: f64, offset: f64) {
        const SWEEP: f64 = 1.1;
        if w <= 0 || h <= 0 {
            return;
        }
        let bg = self.theme.bg;
        let paper = self.theme.paper;
        // The slow breath: the whole mark brightens and dims a little,
        // the way phosphor never quite sits still.
        let breath = 0.05 + 0.05 * ((self.now * 1.4 + offset).sin() as f32);
        // A scan line that drifts down the mark every other cycle.
        let scan_phase = ((self.now + offset) / (period * 0.5)).fract() as f32;
        let scan_y = y as f32 + scan_phase * (h as f32 + 4.0) - 2.0;
        let phase = (self.now + offset).rem_euclid(period);
        let sweep = phase <= SWEEP;
        let p = (phase / SWEEP) as f32;
        let centre = -0.25 + 1.5 * p;
        for py in y.max(0)..(y + h).min(fb.h as i32) {
            let scan = 1.0 - ((py as f32 - scan_y).abs() / 2.0).min(1.0);
            for px in x.max(0)..(x + w).min(fb.w as i32) {
                let c = fb.px[py as usize * fb.w + px as usize];
                if c == bg {
                    continue;
                }
                let mut k = breath + scan * 0.18;
                if sweep {
                    let u = (px - x) as f32 / w as f32 + 0.4 * (py - y) as f32 / h as f32;
                    let d = (u - centre).abs();
                    if d < 0.13 {
                        k += (1.0 - d / 0.13).powi(2) * 0.95;
                    }
                }
                if k > 0.0 {
                    fb.put(px, py, lerp_color(c, paper, k.min(1.0)));
                }
            }
        }
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
            fb.rect(0, y, w, 1, lerp_color(self.theme.bg, self.theme.selection, 0.25 * (1.0 - f)));
        }
        for i in 0..70u32 {
            let x = (i * 97 + 13) as i32 % w;
            let y = (i * 53 + 7) as i32 % (floor_y - 10);
            let tw = 0.5 + 0.5 * ((now * 1.3 + i as f64 * 0.7).sin() as f32);
            fb.put(x, y, lerp_color(self.theme.bg, self.theme.paper, 0.2 + 0.5 * tw));
        }
        // Floor: darker toward the bottom with a horizon line.
        for y in floor_y..h {
            let f = (y - floor_y) as f32 / (h - floor_y) as f32;
            fb.rect(0, y, w, 1, lerp_color(self.theme.selection, self.theme.bg, 0.4 + 0.6 * f));
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
            let (hl, hr) = if d < 0.0 { ((hh as f32 * tilt) as i32, hh) } else { (hh, (hh as f32 * tilt) as i32) };
            let x0 = (x_c - ww as f32 / 2.0) as i32;
            let y0 = bottom - hh;
            let shade = 1.0 - 0.45 * ad.min(1.0);
            let system = self.library.systems[entry.sys].name.clone();
            let img = self.art.cover(&system, &entry.game.path, cw as usize, ch as usize).cloned();
            match img {
                Some(img) => {
                    // Keep the cover's own proportions inside the box.
                    let f = (ww as f32 / img.w as f32).min(hh as f32 / img.h as f32);
                    let iw = (img.w as f32 * f) as i32;
                    let ih = (img.h as f32 * f) as i32;
                    let ix = (x_c - iw as f32 / 2.0) as i32;
                    let iy = bottom - ih;
                    let (il, ir) = ((hl as f32 * ih as f32 / hh as f32) as i32, (hr as f32 * ih as f32 / hh as f32) as i32);
                    fb.blit_trapezoid(&img, ix, iy, iw, il, ir, shade, 1.0, false, floor_y, 0);
                    // Reflection: the same cover upside down under the floor,
                    // fading out within a few rows.
                    let ry = floor_y + 1 + (floor_y - bottom);
                    fb.blit_trapezoid(&img, ix, ry, iw, il, ir, shade * 0.6, 0.35, true, floor_y + 30, 30);
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
                        fb.bitmap(x_c as i32 - 5 * s, bottom - hh / 2 - 5 * s, logo, scale(c, shade), s, 10);
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
        let stem = entry.game.path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
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
            fb.bitmap(w - left - 8, floor_y + 35, &icons::STAR, self.theme.yellow, 1, 8);
        }
        let pos = format!("{}/{}", sel + 1, n);
        fb.text(w - left - Framebuffer::text_width(&pos, 1), 4, &pos, scale(self.theme.dim, 0.8), 1);
        let p: String = prompt.chars().take(24).collect();
        fb.text(left, 4, &p, scale(self.theme.dim, 0.8), 1);
        let hint = self.hint(&[("A", "run"), ("X", "list"), ("Y", "fav"), ("B", "back")]);
        fb.text(left, h - 14, &hint, scale(self.theme.dim, 0.7), 1);
    }

    /// The search bar under the header: the query with a blinking cursor and
    /// the number of matches. Returns the y the list starts at.
    fn draw_search_bar(&mut self, fb: &mut Framebuffer, y0: i32, q: &str, hits: usize) -> i32 {
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
            if self.yt_search.is_some() { "searching".to_string() } else { "Enter searches YouTube".to_string() }
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
    fn draw_osk(&mut self, fb: &mut Framebuffer) {
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
                let label = if ch == ' ' { "sp".to_string() } else { ch.to_string() };
                fb.text_centered(
                    x + (key_w - 2) / 2,
                    y + 1,
                    &label,
                    if on { self.theme.accent } else { self.theme.paper },
                    1,
                );
            }
        }
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
    // Through the store, so a list truncated by a crash falls back to the
    // copy taken before the last save instead of coming back empty.
    let Some(text) = omarchy_crt_shell::store::load_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|l| {
            // system<TAB>path[<TAB>when]: take the path field alone, or a
            // saved timestamp ends up glued to it.
            let mut parts = l.split('\t');
            let name = parts.next()?;
            let p = parts.next()?;
            let i = lib.systems.iter().position(|s| s.name == name)?;
            Some((i, PathBuf::from(p)))
        })
        .collect()
}

/// Third column of recent.txt: when the game was last started.
fn load_times(path: &std::path::Path) -> std::collections::HashMap<PathBuf, i64> {
    let Some(text) = omarchy_crt_shell::store::load_string(path) else {
        return Default::default();
    };
    text.lines()
        .filter_map(|l| {
            let mut parts = l.split('\t');
            let _name = parts.next()?;
            let p = parts.next()?;
            let at: i64 = parts.next()?.trim().parse().ok()?;
            Some((PathBuf::from(p), at))
        })
        .collect()
}

fn save_recent(
    path: &std::path::Path,
    list: &[(usize, PathBuf)],
    at: &std::collections::HashMap<PathBuf, i64>,
    lib: &Library,
) {
    let text: String = list
        .iter()
        .filter_map(|(i, p)| {
            Some(format!(
                "{}\t{}\t{}\n",
                lib.systems.get(*i)?.name,
                p.display(),
                at.get(p).copied().unwrap_or(0)
            ))
        })
        .collect();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = omarchy_crt_shell::store::save(path, text) {
        eprintln!("cannot write {}: {e}", path.display());
    }
}

fn save_list(path: &std::path::Path, list: &[(usize, PathBuf)], lib: &Library) {
    let text: String = list
        .iter()
        .filter_map(|(i, p)| Some(format!("{}\t{}\n", lib.systems.get(*i)?.name, p.display())))
        .collect();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = omarchy_crt_shell::store::save(path, text) {
        eprintln!("cannot write {}: {e}", path.display());
    }
}

/// A readable title for a watch target: the given one, else a YouTube id or
/// the file name.
fn watch_title(target: &str, given: &str) -> String {
    if !given.is_empty() {
        return given.to_string();
    }
    if let Some(rest) = target.strip_prefix("http://").or_else(|| target.strip_prefix("https://")) {
        let host = rest.split('/').next().unwrap_or(rest);
        if let Some(v) = rest.split("v=").nth(1) {
            let id: String = v.chars().take_while(|c| *c != '&').collect();
            return format!("YouTube {id}");
        }
        if let Some(id) = rest.strip_prefix("youtu.be/") {
            return format!("YouTube {}", id.split(['?', '/']).next().unwrap_or(id));
        }
        return host.to_string();
    }
    crate::library::clean_title(Path::new(target))
}
