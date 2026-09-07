//! User settings saved in `~/.config/omarchy-crt/settings.toml`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Screensaver {
    pub enabled: bool,
    /// Seconds of inactivity before the screensaver starts.
    pub idle_secs: u32,
    /// Effect name from `effects::ALL`, or `random`.
    pub effect: String,
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
}

impl Default for VideoFit {
    fn default() -> Self {
        Self {
            standard: "auto".into(),
            film24: "pulldown".into(),
            aspect: "letterbox".into(),
            overscan: true,
            retro_240p: false,
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
}

impl Default for Music {
    fn default() -> Self {
        Self {
            country: String::new(),
            rumble: false,
            idle_secs: 6,
            cycle_secs: 45,
            saver: true,
            lyrics: true,
            look: "auto".into(),
            disabled_visualizers: Vec::new(),
        }
    }
}

fn default_idle() -> u32 {
    6
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
            },
            theme: "system".into(),
            video: VideoFit::default(),
            music: Music::default(),
            videos: Videos::default(),
        }
    }
}

impl Settings {
    pub fn path(config_dir: &Path) -> PathBuf {
        config_dir.join("settings.toml")
    }

    pub fn load(config_dir: &Path) -> Self {
        crate::config::read(&Self::path(config_dir))
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, config_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(config_dir)?;
        let mut stamped = self.clone();
        stamped.version = crate::config::VERSION;
        let text = toml::to_string_pretty(&stamped).map_err(std::io::Error::other)?;
        crate::store::save(&Self::path(config_dir), text)
    }
}
