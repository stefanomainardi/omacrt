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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub screensaver: Screensaver,
    /// Theme name from ~/.local/share/omarchy/themes, or `system` to follow Omarchy.
    #[serde(default = "default_theme")]
    pub theme: String,
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
