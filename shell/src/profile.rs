//! TV profile: monitor preset and picture geometry, saved in
//! `~/.config/omarchy-crt/profile.toml`, written out as a `switchres.ini`
//! for the timing calculator and as RetroArch keys for centering.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Switchres monitor presets that matter for a TV or an arcade chassis.
pub const PRESETS: [&str; 7] = [
    "generic_15",
    "ntsc",
    "pal",
    "arcade_15",
    "arcade_15_25",
    "arcade_15_25_31",
    "arcade_31",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    /// Switchres monitor preset.
    pub monitor: String,
    /// Horizontal picture shift in pixels, -16 to 16.
    pub h_shift: i32,
    /// Vertical picture shift in lines, -16 to 16.
    pub v_shift: i32,
    /// Horizontal size, 0.80 to 1.20 (1.0 = untouched).
    pub h_size: f32,
    /// Sync polarity flip for picky sets.
    pub invert_sync: bool,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            monitor: "generic_15".into(),
            h_shift: 0,
            v_shift: 0,
            h_size: 1.0,
            invert_sync: false,
        }
    }
}

impl Profile {
    pub fn path(config_dir: &Path) -> PathBuf {
        config_dir.join("profile.toml")
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
        std::fs::write(Self::path(config_dir), text)?;
        std::fs::write(config_dir.join("switchres.ini"), self.switchres_ini())
    }

    pub fn preset_index(&self) -> usize {
        PRESETS.iter().position(|p| *p == self.monitor).unwrap_or(0)
    }

    pub fn cycle_preset(&mut self, dir: i32) {
        let n = PRESETS.len() as i32;
        let i = (self.preset_index() as i32 + dir).rem_euclid(n);
        self.monitor = PRESETS[i as usize].into();
    }

    /// `switchres.ini` consumed by RetroArch CRT SwitchRes and GroovyMAME.
    pub fn switchres_ini(&self) -> String {
        format!(
            "# Written by omarchy-crt-shell from profile.toml\n\
             monitor            {}\n\
             modeline_generation 1\n\
             dotclock_min       0\n\
             sync_refresh_tolerance 2.0\n\
             super_width        2560\n\
             h_size             {:.3}\n\
             h_shift            {}\n\
             v_shift            {}\n\
             interlace          1\n\
             doublescan         0\n\
             {}\n",
            self.monitor,
            self.h_size,
            self.h_shift,
            self.v_shift,
            if self.invert_sync {
                "sync_polarity     invert"
            } else {
                ""
            }
        )
    }

    /// RetroArch keys that follow the profile: centering and porch tweaks.
    pub fn retroarch_keys(&self) -> String {
        format!(
            "crt_switch_center_adjust = \"{}\"\ncrt_switch_porch_adjust = \"{}\"\n",
            self.h_shift, self.v_shift
        )
    }
}
