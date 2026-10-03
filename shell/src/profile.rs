//! TV profile: monitor preset and picture geometry, saved in
//! `~/.config/omacrt/profile.toml`, written out as a `switchres.ini`
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
    /// Gain on each of the three colour channels, from `GAIN_MIN` to
    /// `GAIN_MAX`, 1.0 for untouched. A converter that drives one gun lower
    /// than the others tints the whole picture, and the RGB-Pi 2 has been
    /// measured with its green more than 100 mV below red and blue. The
    /// display process puts these in the output's gamma table.
    #[serde(default = "unity")]
    pub red: f32,
    #[serde(default = "unity")]
    pub green: f32,
    #[serde(default = "unity")]
    pub blue: f32,
}

fn unity() -> f32 {
    1.0
}

/// How far a channel may be turned down or up.
pub const GAIN_MIN: f32 = 0.70;
pub const GAIN_MAX: f32 = 1.30;

/// A gamma table for one channel: `len` steps from black to `gain` of full
/// scale, straight, held at full scale where a gain above one runs past it.
pub fn ramp(gain: f32, len: usize) -> Vec<u16> {
    let gain = gain.clamp(GAIN_MIN, GAIN_MAX) as f64;
    let last = len.saturating_sub(1).max(1) as f64;
    (0..len)
        .map(|i| ((i as f64 / last * gain).clamp(0.0, 1.0) * 65535.0).round() as u16)
        .collect()
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            monitor: "generic_15".into(),
            h_shift: 0,
            v_shift: 0,
            h_size: 1.0,
            invert_sync: false,
            red: 1.0,
            green: 1.0,
            blue: 1.0,
        }
    }
}

impl Profile {
    pub fn path(config_dir: &Path) -> PathBuf {
        config_dir.join("profile.toml")
    }

    pub fn load(config_dir: &Path) -> Self {
        crate::store::load_parsed(&Self::path(config_dir), |t| toml::from_str(t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, config_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(config_dir)?;
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        crate::store::save(&Self::path(config_dir), text)?;
        crate::store::save(&config_dir.join("switchres.ini"), self.switchres_ini())
    }

    /// The three channel gains, each held inside its range.
    pub fn gains(&self) -> [f32; 3] {
        [self.red, self.green, self.blue].map(|g| {
            if g.is_finite() {
                g.clamp(GAIN_MIN, GAIN_MAX)
            } else {
                1.0
            }
        })
    }

    /// The control line that tells the display process the gains.
    pub fn colour_command(&self) -> String {
        let [r, g, b] = self.gains();
        format!("colour {r:.3} {g:.3} {b:.3}")
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
            "# Written by omacrt-shell from profile.toml\n\
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unity_ramp_runs_from_black_to_full_scale() {
        let r = ramp(1.0, 256);
        assert_eq!((r[0], r[255]), (0, 65535));
        assert!(r.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn a_lowered_channel_tops_out_below_full_and_a_raised_one_clips() {
        assert_eq!(
            *ramp(0.9, 256).last().unwrap(),
            (0.9f32 as f64 * 65535.0).round() as u16
        );
        let up = ramp(1.2, 256);
        assert_eq!(*up.last().unwrap(), 65535);
        assert_eq!(up[230], 65535, "a gain above one holds at full scale");
        assert!(up[200] < 65535, "and runs straight below it");
    }

    #[test]
    fn an_old_profile_without_gains_loads_at_unity() {
        let p: Profile = toml::from_str(
            "monitor = \"generic_15\"\nh_shift = 0\nv_shift = 0\nh_size = 1.0\ninvert_sync = false\n",
        )
        .unwrap();
        assert_eq!(p.gains(), [1.0, 1.0, 1.0]);
        assert_eq!(p.colour_command(), "colour 1.000 1.000 1.000");
    }
}
