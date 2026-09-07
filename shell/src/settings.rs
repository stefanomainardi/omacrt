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
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Music {
    /// ISO country code whose radio stations come first; empty follows the locale.
    #[serde(default)]
    pub country: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub screensaver: Screensaver,
    /// Theme name from ~/.local/share/omarchy/themes, or `system` to follow Omarchy.
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub video: VideoFit,
    #[serde(default)]
    pub music: Music,
}

fn default_theme() -> String {
    "system".into()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            screensaver: Screensaver {
                enabled: true,
                idle_secs: 60,
                effect: "random".into(),
            },
            theme: "system".into(),
            video: VideoFit::default(),
            music: Music::default(),
        }
    }
}

impl Settings {
    pub fn path(config_dir: &Path) -> PathBuf {
        config_dir.join("settings.toml")
    }

    pub fn load(config_dir: &Path) -> Self {
        std::fs::read_to_string(Self::path(config_dir))
            .ok()
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, config_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(config_dir)?;
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(Self::path(config_dir), text)
    }
}
