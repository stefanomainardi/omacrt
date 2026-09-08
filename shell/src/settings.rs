//! User settings saved in `~/.config/omarchy-crt/settings.toml`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Screensaver {
    pub enabled: bool,
    /// Seconds of inactivity before the screensaver starts.
    pub idle_secs: u32,
    /// Which text effect the wordmark page uses: a name from `effects::ALL`,
    /// or `random` for any of them. This says nothing about which pages an
    /// idle television shows; that is `pages`.
    pub effect: String,
    /// Seconds a page stays before the next one. 0, or one page on its own,
    /// keeps that page up.
    #[serde(default = "default_cycle_secs")]
    pub cycle_secs: u32,
    /// The pages an idle television shows. One of them stays up; several
    /// take turns, in the order of `PAGES`. Empty in a file written by an
    /// older build, which `migrate` reads as its old `effect` value.
    #[serde(default)]
    pub pages: Vec<String>,
}

fn default_cycle_secs() -> u32 {
    240
}

/// Everything an idle television can show, in the order pages take turns.
///
/// The launcher draws each of these and knows what to call them; this is the
/// list itself, because the settings file, its migration and the screen all
/// have to agree on the names.
pub const PAGES: [&str; 4] = ["effects", "photos", "ambient", "system"];

impl Screensaver {
    /// Is this page in the rotation?
    pub fn shows(&self, page: &str) -> bool {
        self.pages.iter().any(|p| p == page)
    }

    /// The pages in the rotation, in the order they take turns.
    pub fn rotation(&self) -> Vec<&'static str> {
        PAGES.iter().copied().filter(|p| self.shows(p)).collect()
    }

    /// Add or remove a page, keeping the order of `PAGES`. The last page
    /// cannot be removed: an idle television has to show something.
    pub fn toggle(&mut self, page: &str) -> bool {
        if self.shows(page) {
            if self.pages.len() < 2 {
                return false;
            }
            self.pages.retain(|p| p != page);
        } else {
            self.pages.push(page.to_string());
            self.pages
                .sort_by_key(|p| PAGES.iter().position(|q| q == p).unwrap_or(usize::MAX));
        }
        true
    }

    /// Read a file written before pages existed.
    ///
    /// The old `effect` said three different things at once: a text effect,
    /// `random` for any of them, the name of a whole page, or `mix` for all
    /// of them taking turns. Each of those becomes a rotation, and `effect`
    /// goes back to meaning one thing.
    fn migrate(&mut self) {
        if !self.pages.is_empty() {
            return;
        }
        match self.effect.as_str() {
            "mix" => {
                self.pages = PAGES.iter().map(|p| p.to_string()).collect();
                self.effect = "random".into();
            }
            name if PAGES.contains(&name) && name != "effects" => {
                self.pages = vec![name.to_string()];
                self.effect = "random".into();
            }
            _ => self.pages = vec!["effects".into()],
        }
    }
}

/// How modern video is fitted to the tube.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VideoFit {
    /// `auto`, `ntsc` (480i 59.94) or `pal` (576i 50).
    pub standard: String,
    /// 24 fps film: `pulldown` (3:2 to 59.94) or `speedup` (25/24, PAL style).
    pub film24: String,
    /// 16:9 into 4:3: `letterbox`, `crop` or `anamorphic`.
    pub aspect: String,
    /// Keep a 5% margin so nothing hides in the overscan.
    pub overscan: bool,
    /// Downscale 4:3 sources to 320x240 progressive (retro gameplay captures).
    pub retro_240p: bool,
    /// Keep the desktop preview window open while a game runs. Off by
    /// default: the preview is for driving the launcher from the desk, and
    /// nobody wants a second copy of the game on the monitor behind them.
    #[serde(default)]
    pub monitor_in_games: bool,
}

impl Default for VideoFit {
    fn default() -> Self {
        Self {
            standard: "auto".into(),
            film24: "pulldown".into(),
            aspect: "letterbox".into(),
            overscan: true,
            retro_240p: false,
            monitor_in_games: false,
        }
    }
}

/// The photo frame and the ambient screen.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Frame {
    /// How much furniture goes over the picture: `photos` for none, `clock`
    /// for the time and the caption, `panel` for the whole ambient page.
    #[serde(default = "default_frame_style")]
    pub style: String,
    /// Seconds a photograph stays up.
    #[serde(default = "default_frame_seconds")]
    pub seconds: u32,
    /// Where the pictures come from: `memories`, `album`, `favorites`, `all`.
    #[serde(default = "default_frame_source")]
    pub source: String,
    /// The album's name, when the source is an album.
    #[serde(default)]
    pub album: String,
    /// Let a photograph that fills the screen drift while it is up.
    #[serde(default = "default_true")]
    pub pan: bool,
    /// Read from a file written before the clock and weather page had
    /// settings of its own, and never written again: `Settings::migrate`
    /// moves them to `[ambient]` and `[sound]`.
    #[serde(default, skip_serializing)]
    pub weather: String,
    #[serde(default, skip_serializing)]
    pub calendar: String,
    #[serde(default, skip_serializing)]
    pub weather_sound: bool,
}

fn default_frame_style() -> String {
    "clock".into()
}
fn default_frame_seconds() -> u32 {
    25
}
fn default_frame_source() -> String {
    "memories".into()
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            style: default_frame_style(),
            seconds: default_frame_seconds(),
            source: default_frame_source(),
            album: String::new(),
            pan: true,
            weather: String::new(),
            calendar: String::new(),
            weather_sound: false,
        }
    }
}

/// The music screen (cliamp).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Music {
    /// ISO country code whose radio stations come first; empty follows the locale.
    #[serde(default)]
    pub country: String,
    /// Pad rumble on the beat while the music screens are up.
    #[serde(default)]
    pub rumble: bool,
    /// Idle seconds before the deck gives way to the visualizer; 0 never.
    #[serde(default = "default_idle")]
    pub idle_secs: u32,
    /// Seconds a visualizer mode stays before the next; 0 keeps one.
    #[serde(default = "default_cycle")]
    pub cycle_secs: u32,
    /// The visualizer stands in for the screensaver while music plays.
    #[serde(default = "default_true")]
    pub saver: bool,
    /// Synced lyrics on the deck and over the visualizer.
    #[serde(default = "default_true")]
    pub lyrics: bool,
    /// Deck look: `auto` (turntable for albums and Spotify), `cassette`, `turntable`.
    #[serde(default = "default_look")]
    pub look: String,
    /// Visualizer modes switched off, by name.
    #[serde(default)]
    pub disabled_visualizers: Vec<String>,
    /// The needle on a track change. Read from a file written before the
    /// sounds had a section of their own, and never written again;
    /// `Settings::migrate` moves it to `[sound] deck`.
    #[serde(default, skip_serializing)]
    pub change_sound: bool,
}

impl Default for Music {
    fn default() -> Self {
        Self {
            country: String::new(),
            rumble: false,
            idle_secs: 180,
            cycle_secs: 45,
            saver: true,
            lyrics: true,
            look: "auto".into(),
            disabled_visualizers: Vec::new(),
            change_sound: false,
        }
    }
}

fn default_idle() -> u32 {
    180
}
fn default_cycle() -> u32 {
    45
}
fn default_true() -> bool {
    true
}
fn default_look() -> String {
    "auto".into()
}

/// Video settings beyond the fit pipeline.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Videos {
    /// Tallest YouTube stream to fetch, in lines (a 240 line tube needs no more than 480).
    #[serde(default = "default_yt_quality")]
    pub yt_quality: u32,
    /// Hits a YouTube search lists.
    #[serde(default = "default_yt_results")]
    pub yt_results: u32,
}

fn default_yt_quality() -> u32 {
    480
}
fn default_yt_results() -> u32 {
    20
}

impl Default for Videos {
    fn default() -> Self {
        Self {
            yt_quality: 480,
            yt_results: 20,
        }
    }
}

/// The clock and weather page: where the weather is from, and what is next.
///
/// These lived under `[frame]` while the ambient page had no settings of its
/// own, because one supply thread fetches the photographs, the weather and
/// the calendar together. They are the ambient page's, so they say so.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Ambient {
    /// Place for the weather; empty asks about the city in the machine's own
    /// timezone.
    #[serde(default)]
    pub place: String,
    /// A calendar to read the next appointment from, as an `.ics` address.
    #[serde(default)]
    pub calendar: String,
}

/// Which sounds the launcher makes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sound {
    /// Moving about: the beep on a move, the click of a select, the whoosh
    /// of a page. On by default, because they are how the launcher answers
    /// a button on a television nobody is sitting close to. The boot show
    /// keeps its own sounds either way: it is a show.
    #[serde(default = "default_true")]
    pub menu: bool,
    /// The needle set down when the record deck changes track. Off by
    /// default: it interrupts the music it sits on.
    #[serde(default)]
    pub deck: bool,
    /// The weather's own sound while the clock and weather page is up. Off by
    /// default: that page comes up on its own when the set is left alone.
    #[serde(default)]
    pub weather: bool,
}

impl Default for Sound {
    fn default() -> Self {
        Self {
            menu: true,
            deck: false,
            weather: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    /// The shape of this file, so a build can tell an older one from a newer
    /// one. See `crate::config`.
    #[serde(default)]
    pub version: u32,
    pub screensaver: Screensaver,
    /// Theme name from ~/.local/share/omarchy/themes, or `system` to follow Omarchy.
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub video: VideoFit,
    #[serde(default)]
    pub music: Music,
    #[serde(default)]
    pub videos: Videos,
    #[serde(default)]
    pub frame: Frame,
    #[serde(default)]
    pub ambient: Ambient,
    #[serde(default)]
    pub sound: Sound,
}

fn default_theme() -> String {
    "system".into()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: crate::config::VERSION,
            screensaver: Screensaver {
                enabled: true,
                idle_secs: 60,
                effect: "random".into(),
                cycle_secs: default_cycle_secs(),
                // A new machine shows all of it, which is the point of
                // having four pages. A file that already exists keeps
                // whatever it asked for; see `Screensaver::migrate`.
                pages: PAGES.iter().map(|p| p.to_string()).collect(),
            },
            theme: "system".into(),
            video: VideoFit::default(),
            music: Music::default(),
            videos: Videos::default(),
            ambient: Ambient::default(),
            sound: Sound::default(),
            frame: Frame::default(),
        }
    }
}

impl Settings {
    pub fn path(config_dir: &Path) -> PathBuf {
        config_dir.join("settings.toml")
    }

    pub fn load(config_dir: &Path) -> Self {
        let mut out: Self = crate::config::read(&Self::path(config_dir))
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default();
        out.screensaver.migrate();
        out.migrate();
        out
    }

    /// A file written before the sounds and the clock and weather page had
    /// sections of their own keeps what it asked for.
    fn migrate(&mut self) {
        if self.ambient.place.is_empty() {
            self.ambient.place = std::mem::take(&mut self.frame.weather);
        }
        if self.ambient.calendar.is_empty() {
            self.ambient.calendar = std::mem::take(&mut self.frame.calendar);
        }
        // Both of these were off by default, so an older file can only ever
        // turn one on.
        self.sound.weather |= self.frame.weather_sound;
        self.sound.deck |= self.music.change_sound;
        // The visualizer used to arrive after six seconds, which is sooner
        // than it takes to choose a station. Nobody picked six: it was the
        // default, and it is written into every file the launcher has saved,
        // so the old default becomes the new one. Any other number was
        // chosen and is left alone.
        if self.music.idle_secs == 6 {
            self.music.idle_secs = default_idle();
        }
    }

    pub fn save(&self, config_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(config_dir)?;
        let mut stamped = self.clone();
        stamped.version = crate::config::VERSION;
        let text = toml::to_string_pretty(&stamped).map_err(std::io::Error::other)?;
        crate::store::save(&Self::path(config_dir), text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_visualizer_no_longer_arrives_after_six_seconds() {
        // The old default is replaced; a number somebody chose is kept.
        let mut moved = Settings::default();
        moved.music.idle_secs = 6;
        moved.migrate();
        assert_eq!(moved.music.idle_secs, 180);

        let mut chosen = Settings::default();
        chosen.music.idle_secs = 30;
        chosen.migrate();
        assert_eq!(chosen.music.idle_secs, 30);

        let mut never = Settings::default();
        never.music.idle_secs = 0;
        never.migrate();
        assert_eq!(never.music.idle_secs, 0, "never stays never");
    }

    #[test]
    fn an_older_file_keeps_its_place_its_calendar_and_its_sounds() {
        // What a file written before the clock and weather page and the
        // sounds had sections of their own looks like.
        let text = r#"
version = 1
theme = "system"

[screensaver]
enabled = true
idle_secs = 60
effect = "random"
pages = ["ambient"]

[music]
change_sound = true

[frame]
style = "clock"
seconds = 25
source = "memories"
weather = "Brussels"
calendar = "https://example.invalid/cal.ics"
weather_sound = true
"#;
        let mut settings: Settings = toml::from_str(text).expect("an older settings file loads");
        settings.migrate();
        assert_eq!(settings.ambient.place, "Brussels");
        assert_eq!(settings.ambient.calendar, "https://example.invalid/cal.ics");
        assert!(settings.sound.weather, "the weather sound was on");
        assert!(settings.sound.deck, "the track change sound was on");
        // And the menu sounds, which the older file never mentioned, are on:
        // they are how the launcher answers a button.
        assert!(settings.sound.menu);
        // Written again, the file says it in the new places and not the old.
        let out = toml::to_string_pretty(&settings).expect("it writes");
        assert!(out.contains("[ambient]"), "{out}");
        assert!(out.contains("[sound]"), "{out}");
        assert!(!out.contains("weather_sound"), "{out}");
        assert!(!out.contains("change_sound"), "{out}");
    }

    #[test]
    fn a_new_file_makes_a_noise_only_where_it_should() {
        let fresh = Settings::default();
        assert!(fresh.sound.menu, "moving about answers");
        assert!(!fresh.sound.deck, "the deck is quiet until asked");
        assert!(!fresh.sound.weather, "the weather is quiet until asked");
    }

    fn saver(effect: &str, pages: &[&str]) -> Screensaver {
        Screensaver {
            enabled: true,
            idle_secs: 60,
            effect: effect.into(),
            cycle_secs: 240,
            pages: pages.iter().map(|p| p.to_string()).collect(),
        }
    }

    #[test]
    fn a_file_from_before_pages_keeps_what_it_asked_for() {
        // A text effect, or any of them, was the wordmark page all along.
        let mut s = saver("random", &[]);
        s.migrate();
        assert_eq!(s.pages, vec!["effects"]);
        assert_eq!(s.effect, "random");

        let mut s = saver("vhstape", &[]);
        s.migrate();
        assert_eq!(s.pages, vec!["effects"]);
        // The effect it named is still the effect the page uses.
        assert_eq!(s.effect, "vhstape");

        // A page name meant that page and nothing else.
        let mut s = saver("photos", &[]);
        s.migrate();
        assert_eq!(s.pages, vec!["photos"]);
        assert_eq!(s.effect, "random");

        // `mix` meant all of them.
        let mut s = saver("mix", &[]);
        s.migrate();
        assert_eq!(s.pages, PAGES.to_vec());
        assert_eq!(s.effect, "random");
    }

    #[test]
    fn a_file_that_already_has_pages_is_left_alone() {
        let mut s = saver("burn", &["photos", "system"]);
        s.migrate();
        assert_eq!(s.pages, vec!["photos", "system"]);
        assert_eq!(s.effect, "burn");
    }

    #[test]
    fn pages_take_turns_in_one_order_however_they_were_switched_on() {
        let mut s = saver("random", &["system"]);
        assert!(s.toggle("photos"));
        assert!(s.toggle("effects"));
        // Not the order they were added: the order they are drawn in.
        assert_eq!(s.rotation(), vec!["effects", "photos", "system"]);
    }

    #[test]
    fn the_last_page_cannot_be_switched_off() {
        let mut s = saver("random", &["photos"]);
        assert!(!s.toggle("photos"));
        assert_eq!(s.rotation(), vec!["photos"]);
        // With two on, either can go.
        assert!(s.toggle("ambient"));
        assert!(s.toggle("photos"));
        assert_eq!(s.rotation(), vec!["ambient"]);
    }
}
