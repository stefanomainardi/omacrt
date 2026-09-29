//! The boot sequence, the menu and the screensaver, drawn frame by frame.
//!
//! Timings follow crt.omarchy.org: power surge and roll, BIOS POST lines,
//! logo revealed band by band, systems-online chime, wordmark laser-etched
//! (TerminalTextEffects port), then everything settles into an `ls` listing.
//! On top of that: a "CRT" cartridge badge slams in like an arcade title
//! card, and an idle screensaver cycles text effects on the wordmark.

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
use crate::videofit::{self, Conversion};
use crate::yt;
use omacrt_shell::theme::Theme;
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
    /// A program to run and forget, as a program and its arguments. Not a
    /// command line: nothing here goes through a shell, so nothing a file or
    /// a server chose could ever be read as one.
    Launch(Vec<String>),
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
    /// The four ports, as sockets, and the search for a new pad over the two
    /// lower ones.
    Pads {
        sel: usize,
        scan: bool,
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
    /// What the machine is doing, drawn the way a 16 bit game drew a status
    /// screen. `page` is 0 for the whole machine and 1 for the processes.
    Monitor {
        page: usize,
    },
    /// Photographs from the house's own server, with as much or as little
    /// over them as the settings ask for.
    Frame,
    /// How the photo frame behaves.
    FrameSettings {
        sel: usize,
    },
    /// Where the weather comes from, and what is next.
    AmbientSettings {
        sel: usize,
    },
    /// Which sounds the launcher makes.
    SoundSettings {
        sel: usize,
    },
    /// The time, the day, the weather and what is next, on nothing.
    Ambient,
    /// The three pages for a television with nothing playing on it.
    AmbientHub {
        sel: usize,
    },
    /// Paintings on a wall, one after another; `at` is the one up first.
    Gallery {
        at: usize,
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
        // The machine's name is somebody's, and a picture for a public page
        // shows the fallback instead.
        let host = if shots() {
            String::new()
        } else {
            read("/etc/hostname")
        };
        let kernel = read("/proc/sys/kernel/osrelease");
        Self {
            host: if host.is_empty() {
                "linux".into()
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
const ETCH_START: f32 = 3.55;
/// Home menu entries: icon, label, opens a submenu.
///
/// Eight rows, and eight is the limit: the wordmark and the CRT tag take the
/// top half of a 240 line screen, so a ninth row runs off the bottom. About
/// lives in Settings for that reason.
const HOME: [(icons::Icon, &str, bool); 8] = [
    (icons::GAMEPAD, "Games", true),
    (icons::FILM, "Videos", true),
    (icons::NOTE, "Music", true),
    (icons::STAR, "Favorites", true),
    (icons::CLOCK, "Recent", true),
    (icons::PHOTO, "Ambient", true),
    (icons::GEAR, "Settings", true),
    (icons::POWER, "Power", true),
];

/// The Ambient submenu: what the television shows when nothing is playing.
/// All three are also screensaver pages, and this is where they are found on
/// purpose rather than by leaving the set alone.
const AMBIENT_ITEMS: [(icons::Icon, &str, bool); 4] = [
    (icons::PHOTO, "Photo frame", true),
    (icons::CLOCK, "Clock and weather", true),
    (icons::CHART, "System monitor", true),
    (icons::BRUSH, "Art gallery", true),
];

/// Whether this run is taking pictures for a public page (`OMACRT_SHOTS`
/// set): then the screens leave out the figures that describe somebody's
/// library, the counts of games, films and favourites.
fn shots() -> bool {
    std::env::var_os("OMACRT_SHOTS").is_some()
}

/// How long a painting stays on the wall before the next is hung.
const GALLERY_TURN: f64 = 40.0;

/// The three ambient pages in a few words, for the home row that opens them.
/// A test counts these against `AMBIENT_ITEMS`, so a fourth page cannot be
/// added without saying so here.
const AMBIENT_SUMMARY: &str = "photos, weather, monitor, art";

/// Settings submenu entries.
/// Where a row of the settings page goes. Anything that comes back to
/// Settings names the row it came from rather than counting to it, which is
/// how two rows ended up pointing at the wrong page when the list grew.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Page {
    Profile,
    Fit,
    Music,
    Videos,
    Frame,
    Ambient,
    Sound,
    Saver,
    Style,
    Pads,
    Diag,
    About,
}

/// One line of the settings page: a heading the cursor skips over, or a row
/// that opens a page.
enum SettingsLine {
    Heading(&'static str),
    Row(icons::Icon, &'static str, Page),
}

/// The settings page as it is drawn, in two columns.
///
/// Grouped, because twelve pages in one flat list is a list nobody reads,
/// and in two columns because one column of twelve rows with headings
/// between them leaves no room to breathe on a 240 line screen. The left
/// column is the picture and what the television can be showing; the right
/// is the set itself and the machine under it.
const SETTINGS_LEFT: [SettingsLine; 8] = [
    SettingsLine::Heading("THE PICTURE"),
    SettingsLine::Row(icons::TV, "TV profile", Page::Profile),
    SettingsLine::Row(icons::FIT, "Video fit", Page::Fit),
    SettingsLine::Heading("CHANNELS"),
    SettingsLine::Row(icons::NOTE, "Music", Page::Music),
    SettingsLine::Row(icons::FILM, "Videos", Page::Videos),
    SettingsLine::Row(icons::PHOTO, "Photo frame", Page::Frame),
    SettingsLine::Row(icons::CLOCK, "Clock & weather", Page::Ambient),
];

const SETTINGS_RIGHT: [SettingsLine; 8] = [
    SettingsLine::Heading("THE SET"),
    SettingsLine::Row(icons::SPEAKER, "Sound", Page::Sound),
    SettingsLine::Row(icons::SAVER, "Screensaver", Page::Saver),
    SettingsLine::Row(icons::BRUSH, "Style", Page::Style),
    SettingsLine::Row(icons::PAD, "Pads", Page::Pads),
    SettingsLine::Heading("THIS MACHINE"),
    SettingsLine::Row(icons::PULSE, "Diagnostics", Page::Diag),
    SettingsLine::Row(icons::INFO, "About", Page::About),
];

/// How many rows a column of the settings page has.
const fn settings_count(lines: &[SettingsLine]) -> usize {
    let mut n = 0;
    let mut i = 0;
    while i < lines.len() {
        match &lines[i] {
            SettingsLine::Row(..) => n += 1,
            SettingsLine::Heading(_) => {}
        }
        i += 1;
    }
    n
}

/// Rows in the left column, which is also where the right column's rows
/// start counting from.
const SETTINGS_HALF: usize = settings_count(&SETTINGS_LEFT);
/// Every row of the settings page, left column first.
const SETTINGS_ROWS: usize = SETTINGS_HALF + settings_count(&SETTINGS_RIGHT);

/// The pages a column opens, top to bottom.
fn settings_column(lines: &[SettingsLine]) -> Vec<Page> {
    lines
        .iter()
        .filter_map(|line| match line {
            SettingsLine::Row(_, _, p) => Some(*p),
            SettingsLine::Heading(_) => None,
        })
        .collect()
}

/// The row a page sits on, for anything coming back to Settings.
pub(super) fn settings_row(page: Page) -> usize {
    let mut rows = settings_column(&SETTINGS_LEFT);
    rows.extend(settings_column(&SETTINGS_RIGHT));
    rows.iter().position(|p| *p == page).unwrap_or(0)
}

/// The page a row opens.
fn settings_page(row: usize) -> Page {
    let mut rows = settings_column(&SETTINGS_LEFT);
    rows.extend(settings_column(&SETTINGS_RIGHT));
    rows.get(row).copied().unwrap_or(Page::About)
}

/// Fifteen pixels a row, and twenty for a heading, which spends twelve of
/// them on the gap that separates one group from the group above it. Eight
/// lines a column comes to 130 of the 160 a 240 line screen has between the
/// header and the line a message uses: the room the two columns bought, spent
/// on air rather than on more rows.
const SETTINGS_ROW_H: i32 = 15;
const SETTINGS_HEAD_H: i32 = 20;

/// Rows of the clock and weather page, and of the sound page.
const AMBIENT_ROWS: usize = 3;
const SOUND_ROWS: usize = 4;

/// Rows of the Music settings page before the one per visualizer.
const MUSIC_ROWS: usize = 7;
/// Rows of the Videos settings page.
const VIDEOS_ROWS: usize = 3;
/// Country codes the radio row cycles through; empty follows the locale.
const COUNTRIES: [&str; 10] = ["", "IT", "US", "GB", "DE", "FR", "ES", "PT", "JP", "BR"];

const FIT_ROWS: usize = 6;

/// A game being launched: the media animation plays, then RetroArch starts.
struct Launch {
    cmd: std::process::Command,
    title: String,
    system: String,
    disc: bool,
    color: Color,
    started: f64,
    spawned: bool,
    /// Which of the launch's sounds have been played: 1 the lid, 2 the click.
    cues: u8,
    /// Geometry the CRT should switch to for this program, when the output
    /// is a wide super resolution the host controls.
    lines: Option<Geometry>,
}

/// Per program picture geometry handed to the host.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub lines: Option<u32>,
    /// The picture shift this program wants, or `None` for "nothing to say".
    ///
    /// It was two plain numbers, so every caller had to name a shift whether
    /// it had one or not, and the ones that had none named zero. That zero
    /// went to the CLI, which saved it, and it then became the default for
    /// every later call: a console's own centring survived exactly until the
    /// first time its core reported a resolution.
    pub shift: Option<(i32, i32)>,
    /// Whether to follow the resolution the core reports while it runs. A
    /// pinned frame is a deliberate choice for the whole session, so a core
    /// that changes its mind about its own size is scaled into it instead.
    pub follow: bool,
    /// The television standard to put the tube in, when something knows it.
    /// `None` leaves whatever is already there.
    pub standard: Option<&'static str>,
    /// The field rate the program itself runs at, when its core has said so.
    /// The timing is built to match it, so the two do not slip a whole field
    /// apart every few seconds. `None` keeps the standard's own rate.
    pub hz: Option<f32>,
}

const LAUNCH_SECS: f32 = 1.15;
/// How long a game takes to come back out of its console.
const EJECT_SECS: f32 = 0.9;
/// How long the console stands still with the game in it first. Leaving a
/// game usually changes the television's mode back, and a set takes most of
/// a second to lock again: without the wait the game came out while the
/// picture was still rolling, and nobody saw it.
const EJECT_HOLD: f64 = 0.7;

/// Power submenu entries.
const POWER_ITEMS: [(icons::Icon, &str, bool); 3] = [
    (icons::DESKTOP, "Back to desktop", false),
    (icons::RESET, "Restart launcher", false),
    (icons::POWER, "Power off", false),
];

/// A row of the pause menu. The two choices are not actions: they cycle with
/// left and right and take effect the next time the game starts, because
/// RetroArch reads both from its config and nothing reaches inside a running
/// core to change them. Nothing is lost by restarting: the state is saved and
/// picked up again on its own.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum PauseRow {
    Resume,
    Save,
    Load,
    Rewind,
    FastForward,
    SlowMotion,
    Aspect,
    Shader,
    Reset,
    Quit,
}

/// Pause menu over a running game.
const PAUSE_ROWS: [(PauseRow, icons::Icon, &str); 10] = [
    (PauseRow::Resume, icons::GAMEPAD, "Resume"),
    (PauseRow::Save, icons::SAVE, "Save state"),
    (PauseRow::Load, icons::LOAD, "Load state"),
    (PauseRow::Rewind, icons::REWIND, "Rewind two seconds"),
    (PauseRow::FastForward, icons::FORWARD, "Fast forward"),
    (PauseRow::SlowMotion, icons::SLOW, "Slow motion"),
    (PauseRow::Aspect, icons::FIT, "Picture"),
    (PauseRow::Shader, icons::BRUSH, "Shader"),
    (PauseRow::Reset, icons::RESET, "Reset game"),
    (PauseRow::Quit, icons::LAUNCHER, "Back to launcher"),
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
/// Pages of the system monitor: the machine, then the processes.
const MONITOR_PAGES: usize = 2;
/// Rows of the photo frame settings page.
const FRAME_ROWS: usize = 5;

/// What each page an idle television can show is called on screen, and the
/// line that says what it is. The list itself is `settings::PAGES`, because
/// the file, its migration and this screen have to agree on the names.
///
/// The music visualizer is not among them: it stands in whenever music is
/// playing, which is a stronger claim on the screen than a rotation.
fn saver_page_label(page: &str) -> (&'static str, &'static str) {
    match page {
        "photos" => ("the photographs", "the photo frame"),
        "ambient" => ("the clock and weather", "the weather, drawn, and the time"),
        "system" => ("the system monitor", "what the machine is doing"),
        "gallery" => ("the art gallery", "three paintings, after Magritte"),
        _ => ("the wordmark", "a text effect on the wordmark"),
    }
}

/// Rows of the screensaver settings page: five, then one per page.
const SAVER_ROWS: usize = 5 + omacrt_shell::settings::PAGES.len();
/// Videos hub entries.
const VIDEOS_ITEMS: [(icons::Icon, &str, bool); 3] = [
    (icons::FILM, "Local videos", true),
    (icons::RESUME, "YouTube", true),
    (icons::FOLDER, "Play copied link", false),
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
    "OMACRT",
    "",
    "Retro gaming on a real 15 kHz tube,",
    "from your everyday Hyprland desktop.",
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
const TAG_START: f32 = 5.75;
/// The bridge into the hall: after the copper bar, the laser climbs the
/// screen and the hall stands up behind it, then the word flies to the far
/// wall, then the hall powers on.
const HALL_FROM: f32 = 8.1;
const HALL_CLIMB: f32 = 1.0;
const HALL_FLY: f32 = 0.6;
/// When the word lands and the power-on starts.
const HALL_LANDS: f32 = HALL_FROM + HALL_CLIMB + HALL_FLY;
/// When the menu's window starts to come up, and when it takes input.
const MENU_FROM: f32 = HALL_LANDS + 1.4;
const MENU_LIVE: f32 = MENU_FROM + 0.2;
/// Everything lit and settled: where a skipped boot jumps to.
const BOOT_DONE: f32 = MENU_FROM + 1.2;

/// The project's own name, in one place, so a rename is one edit.
pub const NAME: &str = "OmaCRT";
/// The same name where a filename or a package name is being shown.
pub const NAME_LOWER: &str = "omacrt";

struct Saver {
    effect: Effect,
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
    themes: Vec<omacrt_shell::theme::Installed>,
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
    /// The arcade hall the home stands in, and what the boot's bridge into
    /// it keeps between frames.
    hall: crate::hall::Hall,
    hall_mask: (Vec<bool>, usize, usize),
    hall_scratch: Option<Framebuffer>,
    hall_cues: u8,
    crowd: crate::crowd::Crowd,
    saver: Option<Saver>,
    last_input: f64,
    idle_secs: f32,
    rng: u32,
    library: Library,
    screen: Screen,
    games: Vec<Entry>,
    /// Whether the visualizer had the screen on the previous frame, so the
    /// caption can be shown again the moment it takes it.
    visual_on: bool,
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
    /// Seconds each game has been played, in all.
    playtime: std::collections::HashMap<PathBuf, u64>,
    /// When the running game last started counting: unpaused, not a video.
    play_since: Option<f64>,
    /// Which console stands on the systems stage, and since when, so a new
    /// one arrives rather than appearing.
    stage_name: String,
    stage_since: f64,
    /// The same for the box on the games stage.
    box_path: std::path::PathBuf,
    box_since: f64,
    /// A game coming back out of its console after it ends: the system,
    /// the game and since when.
    ejecting: Option<(String, PathBuf, f64)>,
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
    /// The pads the launcher remembers, and which of them are switched on.
    pad_list: omacrt_shell::pads::Pads,
    pads_here: Vec<omacrt_shell::pads::Pad>,
    /// A pad the screen has asked to be shaken, and how long the drawing of
    /// it shakes for.
    identify: Option<omacrt_shell::pads::Pad>,
    identify_until: f64,
    /// A pad on its way between two ports: where from, where to, and when it
    /// set off. Two sockets swap, so both tiles slide past each other.
    pad_move: Option<(usize, usize, f64)>,
    /// Pads that arrived while the wizard could not show: name, guid, id.
    /// A queue rather than one slot, so two unknown pads at boot both get
    /// their turn instead of the second cancelling the first.
    pending_wizards: Vec<(String, String, u32)>,
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
    /// Kernel counters for the system monitor, sampled while it is on screen.
    sysmon: crate::sysmon::Monitor,
    /// When the monitor last read the counters.
    sysmon_at: f64,
    /// Core bars eased toward the sample, so the bank moves like a meter.
    sysmon_bars: Vec<f32>,
    /// Photographs for the frame, prepared on a thread.
    photos: crate::photos::Feed,
    /// What is on the frame now, and what it is fading up from.
    frame_now: Option<crate::photos::Shown>,
    frame_previous: Option<crate::photos::Shown>,
    /// When the picture on the frame went up.
    frame_since: f64,
    /// The weather, drawn: clouds, rain and the sun's place in its arc.
    sky: crate::sky::Sky,
    /// The screensaver page up now: which of `settings::PAGES` the idle timer
    /// started, when it went up, and the screen it interrupted. The next
    /// input puts that screen back, and the mix knows when to turn the page.
    saver_run: Option<(usize, f64, Screen)>,
}

mod boot;
mod browse;
mod frame;
mod hifi;
mod idle;
mod monitor;
mod padscreen;
mod pause;
mod settings;
mod video;

impl Scene {
    pub fn new(theme: Theme, info: SysInfo, idle_secs: f32, library: Library) -> Self {
        // A theme chosen in Settings overrides the system theme.
        let settings = Settings::load(&library.config_dir);
        let theme = if settings.theme != "system" {
            // A palette compiled in has no file to read, so it is looked for
            // by name first: without this a machine set to one of them fell
            // back to the desktop's theme at every start, and the choice
            // only held until the launcher was restarted.
            Theme::by_name(&settings.theme)
                .or_else(|| {
                    Theme::installed()
                        .into_iter()
                        .find(|(n, _)| *n == settings.theme)
                        .and_then(|(n, p)| Theme::load_named(&p, &n))
                })
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
            themes: Theme::installed_with_swatches(),
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
            hall: crate::hall::Hall::default(),
            hall_mask: crate::hall::sign_mask(),
            hall_scratch: None,
            hall_cues: 0,
            crowd: crate::crowd::Crowd::default(),
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
            pad_list: omacrt_shell::pads::Pads::load(),
            pads_here: Vec::new(),
            identify: None,
            identify_until: 0.0,
            pad_move: None,
            pending_wizards: Vec::new(),
            pending_entry: None,
            remap_request: false,
            music: Music::new(std::env::var("PULSE_SINK").ok()),
            music_path: Vec::new(),
            music_root_sel: 0,
            music_hub: None,
            music_hub_sel: 0,
            music_query: None,
            vis: vec![0.0; 10],
            sysmon: crate::sysmon::Monitor::new(),
            sysmon_at: 0.0,
            sysmon_bars: Vec::new(),
            photos: crate::photos::Feed::new(),
            frame_now: None,
            frame_previous: None,
            frame_since: 0.0,
            sky: crate::sky::Sky::new(),
            saver_run: None,
            profile: Profile::load(&library.config_dir),
            visual_on: false,
            recent: load_list(&library.config_dir.join("recent.txt"), &library),
            recent_at: load_times(&library.config_dir.join("recent.txt")),
            playtime: omacrt_shell::playtime::load(&library.config_dir.join("playtime.tsv")),
            play_since: None,
            stage_name: String::new(),
            stage_since: 0.0,
            box_path: std::path::PathBuf::new(),
            box_since: 0.0,
            ejecting: None,
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
    ///
    /// With the menu sounds switched off the ones that answer a button go no
    /// further. What stays is the boot show, which is a show and has to make
    /// its noise, and the deck's own sounds, which have a switch of their
    /// own.
    pub fn take_sounds(&mut self) -> Vec<Sound> {
        let queued = std::mem::take(&mut self.pending);
        if self.settings.sound.menu {
            return queued;
        }
        queued
            .into_iter()
            .filter(|s| {
                !matches!(
                    s,
                    Sound::Move | Sound::Select | Sound::Lock | Sound::Click | Sound::Whoosh
                )
            })
            .collect()
    }

    /// What the weather should sound like right now, or nothing.
    ///
    /// Only the ambient page has a sound, and only when the setting is on:
    /// the page is the one screen that is all weather, and it is the one the
    /// idle television puts up by itself. A game or a film is playing over
    /// everything and gets the silence it is owed.
    pub fn ambience(&self) -> Option<crate::weather_sound::Ambience> {
        if self.running.is_some() || self.launching.is_some() || self.saver.is_some() {
            return None;
        }
        // The home stands in the arcade hall, and the hall has its sound.
        if matches!(self.screen, Screen::Menu) && self.menu_live && self.settings.sound.hall {
            return Some(crate::weather_sound::Ambience::Arcade);
        }
        if !self.settings.sound.weather || !matches!(self.screen, Screen::Ambient) {
            return None;
        }
        let reading = &self.photos.info.sky;
        if !reading.known {
            return None;
        }
        let minutes = crate::clock::minutes(self.now);
        crate::weather_sound::Ambience::of(reading.kind, reading.daylight(minutes))
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

    /// Any user input: wakes the screensaver (returns true if it did).
    pub fn touch(&mut self, now: f64) -> bool {
        self.last_input = now;
        // A page the idle timer put up is a screensaver, whatever else it
        // can do: the first key press gives the menu back rather than
        // driving the frame or turning the monitor's page.
        if let Some((_, _, back)) = self.saver_run.take() {
            // The effects page draws over whatever screen was up, so there
            // is nothing to put back for it.
            if self.saver.take().is_none() {
                self.screen = back;
                self.screen_since = now;
                self.band_y = -1.0;
            }
            self.pending.push(Sound::Move);
            return true;
        }
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

    /// Put away whatever the idle timer put up, without consuming an input
    /// the way `touch` does. Anything that changes the screen from outside
    /// the television calls this first.
    fn wake(&mut self) {
        // The screen the idle timer covered comes back, as it does for a key
        // press, so a game started from the desktop returns to the menu and
        // not to the screensaver page.
        if let Some((_, _, back)) = self.saver_run.take()
            && self.saver.is_none()
        {
            self.screen = back;
            self.screen_since = self.now;
            self.band_y = -1.0;
        }
        if let Some(prev) = self.music_saver.take() {
            self.screen = prev;
        }
        self.saver = None;
        self.last_input = self.now;
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

    /// Switch screen and restart the slide-in transition.
    /// Which painting is on the gallery wall: the one the page came up
    /// with, moved on by every turn it has taken on its own since.
    fn gallery_on(&self, at: usize) -> usize {
        let turns = ((self.now - self.screen_since).max(0.0) / GALLERY_TURN) as usize;
        (at + turns) % crate::gallery::PAINTINGS.len()
    }

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
            match self.themes.iter().find(|t| t.name == name) {
                // A built in palette is compiled in and has no file.
                Some(t) if t.built_in() => match Theme::by_name(&t.name) {
                    Some(theme) => theme,
                    None => return,
                },
                Some(t) => Theme::load_named(&t.path, &t.name).unwrap_or_else(Theme::tokyo_night),
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
    /// Whether the desktop preview window should stay up while a game runs.
    pub fn keep_preview_in_games(&self) -> bool {
        self.settings.video.monitor_in_games
    }

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
        self.wake();
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
            5 => self.go(Screen::AmbientHub { sel: 0 }),
            6 => self.go(Screen::Settings { sel: 0 }),
            _ => self.go(Screen::Power { sel: 0 }),
        }
        Action::None
    }

    fn save_settings(&mut self) {
        if let Err(e) = self.settings.save(&self.library.config_dir) {
            eprintln!("settings: {e}");
        }
    }

    /// Take a freshly read library, after a scan has changed what is on disk.
    ///
    /// The library and its index are read once at start, because reading them
    /// every frame would mean touching a disk sixty times a second. That
    /// leaves the launcher showing yesterday's collection when a scan runs
    /// from the desktop overlay, which is exactly when somebody is looking at
    /// the numbers to see whether it worked.
    pub fn replace_library(&mut self, library: Library) {
        self.library = library;
        self.refresh_counts();
        // Whatever list is open was built from the old library.
        if let Screen::Games { sys, .. } = self.screen {
            // A rescan can find fewer systems than the open list was built
            // from. Emptying the entries is not enough: the screen keeps the
            // index it had, and the next frame reads `systems[i]` past the end
            // and takes the launcher down while somebody is browsing. The
            // screen goes back to the whole collection, which is what it shows
            // when no system is chosen, so the reader lands somewhere real.
            let sys = match sys {
                Some(i) if i >= self.library.systems.len() => {
                    self.screen = Screen::Games {
                        sys: None,
                        sel: 0,
                        top: 0,
                    };
                    None
                }
                other => other,
            };
            let all = match sys {
                Some(i) if i < self.library.systems.len() => self.entries_for(i),
                Some(_) => Vec::new(),
                None => (0..self.library.systems.len())
                    .filter(|&i| !self.library.systems[i].is_video())
                    .flat_map(|i| self.entries_for(i))
                    .collect(),
            };
            self.set_games(all);
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

    /// True once when a TV profile shift changed; the host saves the profile
    /// and moves the picture so the change shows while adjusting.
    pub fn take_profile_preview(&mut self) -> bool {
        std::mem::take(&mut self.profile_preview)
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

    /// Jump straight to the browser (for testing and frame dumps).
    pub fn debug_browse(&mut self, system: Option<&str>) {
        self.menu_live = true;
        self.chime_played = true;
        match system {
            Some("profile") => self.screen = Screen::Profile { sel: 0 },
            Some("pair") => {
                self.screen = Screen::Pads {
                    sel: 0,
                    scan: false,
                }
            }
            Some("settings") => self.screen = Screen::Settings { sel: 0 },
            // The return from a game, headlessly: the first game of the
            // named system coming back out of its console, a second from now.
            Some(s) if s.starts_with("eject:") => {
                let system = s.trim_start_matches("eject:").to_string();
                let path = self
                    .library
                    .index
                    .as_ref()
                    .and_then(|i| i.items.iter().find(|it| it.system == system))
                    .map(|it| it.path.clone())
                    .unwrap_or_default();
                self.ejecting = Some((system, path, f64::NAN));
            }
            // A launch, for rendering the animation headlessly: the first
            // game of the named system, going into its console. Nothing is
            // run; the command is `true`.
            Some(s) if s.starts_with("launch:") => {
                let system = s.trim_start_matches("launch:").to_string();
                let game = self
                    .library
                    .index
                    .as_ref()
                    .and_then(|i| i.items.iter().find(|it| it.system == system))
                    .map(|it| (it.title.clone(), it.path.clone()));
                let (title, path) = game.unwrap_or_else(|| (system.clone(), PathBuf::new()));
                self.running = Some((title.clone(), system.clone()));
                self.running_path = Some((system.clone(), path));
                self.launching = Some(Launch {
                    cmd: std::process::Command::new("true"),
                    title,
                    system,
                    disc: false,
                    color: self.theme.accent,
                    started: self.now + 1.0,
                    spawned: false,
                    cues: 0,
                    lines: None,
                });
            }
            // The systems list, with a console already selected when one is
            // named after a colon, so its stage can be looked at.
            Some(s) if s == "systems" || s.starts_with("systems:") => {
                let list = self.browse_systems();
                let sel = s
                    .strip_prefix("systems:")
                    .and_then(|want| {
                        list.iter()
                            .position(|&i| self.library.systems[i].name == want)
                    })
                    .map(|p| p + Self::VIRTUAL)
                    .unwrap_or(0);
                self.screen = Screen::Systems {
                    sel,
                    top: sel.saturating_sub(SYS_PAGE - 1),
                };
            }
            Some("about") => self.screen = Screen::About { top: 0 },
            Some("saver") => self.screen = Screen::Saver { sel: 0 },
            Some("diag") => {
                self.diag = self.gather_diagnostics();
                self.screen = Screen::Diag { top: 0 };
            }
            Some("power") => self.screen = Screen::Power { sel: 0 },
            Some("frame") => self.open_frame(),
            Some("framesettings") => self.screen = Screen::FrameSettings { sel: 0 },
            Some("ambientsettings") => self.screen = Screen::AmbientSettings { sel: 0 },
            Some("soundsettings") => self.screen = Screen::SoundSettings { sel: 0 },
            Some("videoshub") => self.screen = Screen::Videos { sel: 0 },
            Some("musicsettings") => self.screen = Screen::MusicSettings { sel: 7 },
            Some("music") => self.open_music(),
            // The pause menu needs a game to be over: for a headless render
            // it gets a made up one, which is enough for every row it draws.
            Some("pause") => {
                self.running = Some(("Chrono Trigger".into(), "snes".into()));
                self.running_path = Some(("snes".into(), std::path::PathBuf::from("ct.sfc")));
                self.paused = Some(6);
            }
            Some("ambienthub") => self.screen = Screen::AmbientHub { sel: 0 },
            Some("saversettings") => self.screen = Screen::Saver { sel: 0 },
            Some("ambient") => self.screen = Screen::Ambient,
            Some(name) if name.starts_with("ambient:") => {
                self.screen = Screen::Ambient;
                self.sky.force_moment(&name["ambient:".len()..]);
            }
            Some(name) if name.starts_with("gallery") => {
                let at = name
                    .strip_prefix("gallery:")
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
                self.screen = Screen::Gallery { at };
                self.screen_since = self.now;
            }
            Some("monitor") => {
                self.sysmon.sample();
                self.screen = Screen::Monitor { page: 0 };
            }
            Some("processes") => {
                self.sysmon.sample();
                self.screen = Screen::Monitor { page: 1 };
            }
            Some("style") => self.screen = Screen::Style { sel: 0 },
            Some("fit") => self.screen = Screen::VideoFit { sel: 0 },
            Some(n) => match self.library.systems.iter().position(|s| s.name == n) {
                Some(i) => self.open_games(Some(i)),
                None => self.screen = Screen::Systems { sel: 0, top: 0 },
            },
            None => self.screen = Screen::Systems { sel: 0, top: 0 },
        }
    }

    /// Open a screen by name, for the control pipe: the desktop menu, the
    /// bar widget and a script all reach the launcher through this.
    ///
    /// Nothing happens while a game or a film is on: a menu entry pressed by
    /// accident must not take the television away from what it is doing.
    pub fn open_screen(&mut self, name: &str) -> bool {
        if self.running.is_some() || self.launching.is_some() || self.player.is_some() {
            return false;
        }
        self.menu_live = true;
        self.chime_played = true;
        // A screen asked for from outside is input, so whatever the idle
        // timer put up has to come down first. Without this the effects
        // saver keeps drawing over the screen that was just opened, and the
        // channel looks like it did not change.
        self.wake();
        match name.trim().to_ascii_lowercase().as_str() {
            "home" | "menu" => self.home(),
            "games" | "systems" => self.go(Screen::Systems { sel: 0, top: 0 }),
            "videos" | "video" => self.go(Screen::Videos { sel: 0 }),
            "youtube" => self.go(Screen::YouTube { sel: 0 }),
            "music" => self.open_music(),
            "favorites" | "favourites" => {
                let list = self.favorites.clone();
                self.list_from_home = true;
                self.open_virtual(&list);
            }
            "recent" => {
                let list = self.recent.clone();
                self.list_from_home = true;
                self.open_virtual(&list);
            }
            "frame" | "photos" => self.open_frame(),
            "ambient" | "clock" | "weather" => self.go(Screen::Ambient),
            "gallery" | "art" | "paintings" => self.go(Screen::Gallery { at: 0 }),
            "idle" | "ambienthub" => self.go(Screen::AmbientHub { sel: 0 }),
            "monitor" | "system" => {
                self.sysmon.sample();
                self.go(Screen::Monitor { page: 0 });
            }
            "processes" => {
                self.sysmon.sample();
                self.go(Screen::Monitor { page: 1 });
            }
            "settings" => self.go(Screen::Settings { sel: 0 }),
            "profile" | "picture" => self.go(Screen::Profile { sel: 0 }),
            "style" | "theme" => self.go(Screen::Style { sel: 0 }),
            "pads" | "pair" => self.go(Screen::Pads {
                sel: 0,
                scan: false,
            }),
            "diagnostics" | "diag" => {
                self.diag = self.gather_diagnostics();
                self.go(Screen::Diag { top: 0 });
            }
            "about" => self.go(Screen::About { top: 0 }),
            "power" => self.go(Screen::Power { sel: 0 }),
            _ => return false,
        }
        true
    }

    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    pub fn is_paused(&self) -> bool {
        self.running.is_some() && self.paused.is_some()
    }

    /// Remember which pad family is connected, for on-screen button labels.
    pub fn set_pad(&mut self, name: Option<&str>) {
        self.pad = name.map(PadKind::from_name).unwrap_or(PadKind::Keyboard);
    }

    /// What a button is called on the connected pad, and the colour its role
    /// is drawn in. The colour follows the role, not the letter, so accepting
    /// is green on every pad even where the pad calls that button X.
    fn hint_key<'a>(&self, button: &'a str) -> (&'a str, Option<Color>) {
        let l = self.pad.labels();
        let th = &self.theme;
        match button {
            "A" => (l.accept, Some(th.green)),
            "B" => (l.back, Some(th.red)),
            "Y" => (l.fav, Some(th.yellow)),
            "X" => (l.alt, Some(th.blue)),
            other => (other, None),
        }
    }

    fn hint_width(&self, parts: &[(&str, &str)]) -> i32 {
        let mut w = 0;
        for (button, what) in parts {
            let (label, role) = self.hint_key(button);
            let cap = if role.is_some() && label.chars().count() == 1 {
                9
            } else {
                Framebuffer::text_width(label, 1) + 4
            };
            w += cap + 3 + Framebuffer::text_width(what, 1) + 10;
        }
        (w - 10).max(0)
    }

    /// The buttons along the bottom, drawn as buttons: a pad's face buttons
    /// as round caps in the colour of what they do, anything else (a key,
    /// a direction, a shoulder) as a small sunken plate with its name.
    fn draw_hint(&self, fb: &mut Framebuffer, x: i32, y: i32, parts: &[(&str, &str)]) -> i32 {
        let th = &self.theme;
        let tones = crate::paint::Tones::of(th);
        let mut cx = x;
        for (button, what) in parts {
            let (label, role) = self.hint_key(button);
            let mut chars = label.chars();
            match (role, chars.next(), chars.next()) {
                (Some(c), Some(k), None) => {
                    cx += crate::paint::keycap(fb, cx, y - 1, k, c, th.bg);
                }
                _ => {
                    let w = Framebuffer::text_width(label, 1) + 4;
                    fb.rect(cx, y - 1, w, 10, tones.lo);
                    fb.rect(
                        cx + 1,
                        y + 8,
                        w - 1,
                        1,
                        lerp_color(th.selection, th.paper, 0.2),
                    );
                    fb.text(cx + 2, y, label, th.fg, 1);
                    cx += w;
                }
            }
            cx += 3;
            fb.text(cx, y, what, scale(th.dim, 0.9), 1);
            cx += Framebuffer::text_width(what, 1) + 10;
        }
        cx
    }

    /// Compact header used by the browser and the running screen: small icon,
    /// the wordmark at 1 px per cell, a prompt line underneath.
    fn draw_header(&self, fb: &mut Framebuffer, prompt: &str) -> i32 {
        let left = (fb.w as f32 * 0.05) as i32 + self.slide();
        // The same mark as everywhere else, at the smallest size its shape
        // holds, and on the same return.
        let th = &self.theme;
        let tones = crate::paint::Tones::of(th);
        let w = fb.w as i32;
        // A little light from above: the room the menu stands in, in the
        // dithered steps a fixed palette would have painted it with.
        crate::paint::gradient_v(
            fb,
            0,
            0,
            w,
            120,
            &[lerp_color(th.bg, th.selection, 0.55), th.bg],
        );
        let (base, hot) = self.retrace_now();
        self.draw_retrace(fb, left, 8, 24.0, base, 1.0, hot);
        let mx = w - left - self.mark_small.cols;
        for cell in &self.mark_small.cells {
            effects::draw_cell(fb, mx + 1, 11, 1, cell, tones.shadow);
        }
        for cell in &self.mark_small.cells {
            effects::draw_cell(fb, mx, 10, 1, cell, cell.final_color);
        }
        // Niente glint sul marchio: il suo movimento e' il ritorno.
        self.glint(
            fb,
            mx,
            10,
            self.mark_small.cols,
            self.mark_small.rows * 2,
            8.0,
            2.6,
        );
        // The title on a shelf label: a band that fades out to the right, a
        // tab of accent in front of it, the words standing on a shadow.
        let span = w - 2 * left;
        crate::paint::gradient_h(
            fb,
            left,
            36,
            span,
            13,
            lerp_color(th.selection, th.accent, 0.22),
            th.bg,
        );
        fb.rect(left, 37, 3, 11, th.accent);
        crate::paint::text_shadow(fb, left + 8, 39, prompt, th.paper, tones.shadow);
        // The clock in a small sunken window of its own.
        let clock = crate::clock::now(self.now).format("%H:%M").to_string();
        let cw = Framebuffer::text_width(&clock, 1) + 8;
        let cx = w - left - cw;
        let well = lerp_color(th.bg, tones.lo, 0.5);
        crate::paint::bevel(fb, &tones, cx, 36, cw, 13, well, th.bg, false);
        fb.text(cx + 4, 39, &clock, th.fg, 1);
        crate::paint::gradient_h(
            fb,
            left,
            50,
            span,
            1,
            lerp_color(th.accent, th.bg, 0.3),
            th.bg,
        );
        52
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
        self.tick_playtime();
        if !self.pending_wizards.is_empty()
            && self.menu_live
            && self.running.is_none()
            && self.launching.is_none()
            && self.wizard.is_none()
            && self.saver.is_none()
        {
            let (name, guid, which) = self.pending_wizards.remove(0);
            self.pad_wizard_start(&name, &guid, which);
        }
        fb.clear(self.theme.bg);
        // Turning the page of the mix happens here, above every branch: the
        // effects page returns from the next one and never reaches the rest
        // of this function.
        if self.running.is_none() && self.launching.is_none() {
            self.cycle_saver_page(now);
        }
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
            } else if self
                .launching
                .as_ref()
                .is_some_and(|l| ((now - l.started) as f32) < LAUNCH_SECS)
            {
                self.draw_launching(fb);
            } else {
                self.draw_running(fb);
            }
            return;
        }
        if let Some((_, _, since)) = self.ejecting.as_mut() {
            // Timed from the first picture after the game, not from its end:
            // the mode change in between blocks for up to a second.
            if since.is_nan() {
                *since = now + EJECT_HOLD;
            }
            if ((now - *since) as f32) < EJECT_SECS {
                self.draw_ejecting(fb);
                return;
            }
            self.ejecting = None;
        }
        if self.menu_live && !matches!(self.screen, Screen::Menu) {
            if self.bt.poll() {
                self.pending.push(Sound::Lock);
            }
            self.draw_browser(fb);
            if let Some((_, until)) = &self.message
                && self.now > *until
            {
                self.message = None;
            }
            let limit = self.idle_limit();
            if limit > 0.0 && (now - self.last_input) as f32 > limit {
                self.idle_reached(now);
            }
            return;
        }
        if t < HALL_FROM + HALL_CLIMB {
            self.draw_post(fb, t);
            self.draw_logo(fb, t);
            self.draw_crt_tag(fb, t, false);
            self.draw_etch(fb, t);
            self.draw_crt_tag(fb, t, true);
        }
        if t >= HALL_FROM {
            self.draw_hall_bridge(fb, t);
        }
        self.draw_home(fb, t);
        if t >= 2.2 && !self.chime_played {
            self.chime_played = true;
            self.pending.push(Sound::Chime);
        }
        if t > MENU_LIVE && !self.menu_live {
            self.menu_live = true;
            self.last_input = now;
        }
        if let Some((_, until)) = &self.message
            && self.now > *until
        {
            self.message = None;
        }
        let limit = self.idle_limit();
        if self.menu_live && limit > 0.0 && (now - self.last_input) as f32 > limit {
            self.idle_reached(now);
        }
    }

    /// Horizontal slide-in offset for a screen that just opened.
    fn slide(&self) -> i32 {
        let p = ((self.now - self.screen_since) / 0.12).clamp(0.0, 1.0) as f32;
        ((1.0 - ease(p)) * 40.0) as i32
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

    /// The game running now, by the path it was launched from.
    pub fn running_game_path(&self) -> Option<PathBuf> {
        self.running_path.as_ref().map(|(_, p)| p.clone())
    }

    pub fn take_rumble(&mut self) -> bool {
        std::mem::take(&mut self.rumble_pending)
    }

    /// A passing message, in a small raised window of its own rather than
    /// loose on the ground. `y` is where the text sits, as it always was.
    fn draw_message(&self, fb: &mut Framebuffer, x: i32, y: i32, text: &str) {
        let th = &self.theme;
        let tones = crate::paint::Tones::of(th);
        let w = Framebuffer::text_width(text, 1) + 12;
        crate::paint::bevel(fb, &tones, x, y - 3, w, 13, tones.panel, tones.panel2, true);
        fb.rect(x + 2, y - 1, 2, 9, th.cyan);
        fb.text(x + 7, y, text, th.cyan, 1);
    }

    /// The selection, drawn as a lit key. A sheen crosses it every four
    /// seconds, the small movement that says which row is alive.
    fn select_bar(&self, fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32) {
        let tones = crate::paint::Tones::of(&self.theme);
        let shine = ((self.now % 4.0) / 1.2) as f32;
        crate::paint::select_bar(fb, &self.theme, &tones, x, y, w, h, shine);
        if self.music.status.playing() && self.deck.kick.hit > 0.05 {
            // The beat still lands on it, as a flash of the accent edge.
            let k = self.deck.kick.hit * 0.6;
            let edge = crate::fb::lerp_color(self.theme.accent, self.theme.paper, k);
            fb.rect(x + 1, y, w - 2, 1, edge);
        }
    }

    /// A glint: every `period` seconds a narrow diagonal highlight sweeps
    /// across the box in under a second, brightening only what is drawn
    /// there. The small movement that keeps a logo alive.
    // A box, a period and a phase. The argument list is the geometry.
    #[allow(clippy::too_many_arguments)]
    fn glint(
        &self,
        fb: &mut Framebuffer,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        period: f64,
        offset: f64,
    ) {
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
}

/// Lists of `system_name<TAB>path` lines; system names survive reordering.
fn load_list(path: &std::path::Path, lib: &Library) -> Vec<(usize, PathBuf)> {
    // Through the store, so a list truncated by a crash falls back to the
    // copy taken before the last save instead of coming back empty.
    let Some(text) = omacrt_shell::store::load_string(path) else {
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
    let Some(text) = omacrt_shell::store::load_string(path) else {
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
    if let Err(e) = omacrt_shell::store::save(path, text) {
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
    if let Err(e) = omacrt_shell::store::save(path, text) {
        eprintln!("cannot write {}: {e}", path.display());
    }
}

/// A readable title for a watch target: the given one, else a YouTube id or
/// the file name.
fn watch_title(target: &str, given: &str) -> String {
    if !given.is_empty() {
        return given.to_string();
    }
    if let Some(rest) = target
        .strip_prefix("http://")
        .or_else(|| target.strip_prefix("https://"))
    {
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
