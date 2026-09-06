//! CRT output control shared by the `omarchy-crt` CLI and the launcher:
//! configuration, persistent state, the RGB-Pi 2 DAC, the Hyprland output,
//! audio routing, the launcher process, BIOS files and ROM folders.

pub mod audio;
pub mod bios;
pub mod dac;
pub mod launcher;
pub mod output;
pub mod roms;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

pub const SHELL_CLASS: &str = "omarchy-crt-shell";

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn config_dir() -> PathBuf {
    std::env::var_os("OMARCHY_CRT_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config/omarchy-crt"))
}

pub fn state_dir() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/state"))
        .join("omarchy-crt")
}

/// `~/.config/omarchy-crt/crt.toml`.
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub output: Output,
    pub modelines: Modelines,
    pub shell: Shell,
    pub audio: Audio,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Output {
    /// DRM connector (`HDMI-A-1` or `card1-HDMI-A-1`); empty picks the first
    /// connected HDMI output whose EDID names a Mortaca (RGB-Pi 2) device.
    pub connector: String,
    /// Hyprland position of the CRT output.
    pub position: String,
    /// Composite sync mode of the RGB-Pi 2: `and`, `xor`, `separate`.
    pub csync: String,
    /// Standard used by `on` without an argument: `ntsc` or `pal`.
    pub standard: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Modelines {
    pub ntsc: String,
    pub pal: String,
    /// 240p at 60.00 Hz for filming the tube with a 60 fps camera: no
    /// beat between the 60.04 Hz standard timing and the shutter.
    pub film: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Shell {
    /// Launcher binary; a bare name is looked up in PATH, then next to this
    /// program, then in the source tree.
    pub bin: String,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Audio {
    /// Enable the DAC's HDMI audio profile while on and send the launcher,
    /// RetroArch and mpv there. Desktop audio stays where it is.
    pub route: bool,
    /// Also make the CRT the system default sink while on (everything plays
    /// on the television). Off by default.
    pub system_default: bool,
    /// Sink volume applied to the CRT output, percent. PipeWire allows up to
    /// 150; 125 is about +6 dB, enough for a DAC whose line level sits low.
    pub volume: u32,
}

impl Default for Output {
    fn default() -> Self {
        Self {
            connector: String::new(),
            position: "auto".into(),
            csync: "xor".into(),
            standard: "ntsc".into(),
        }
    }
}

impl Default for Modelines {
    fn default() -> Self {
        Self {
            ntsc: "72 3520 3695 4033 4577 240 242 245 262 -hsync -vsync".into(),
            pal: "72 3840 3948 4290 4608 288 291 294 312 -hsync -vsync".into(),
            film: "72 3520 3695 4033 4580 240 242 245 262 -hsync -vsync".into(),
        }
    }
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            bin: SHELL_CLASS.into(),
            args: vec![
                "--fullscreen".into(),
                "--stretch".into(),
                "--auto-boot".into(),
            ],
        }
    }
}

impl Default for Audio {
    fn default() -> Self {
        Self {
            route: true,
            system_default: false,
            volume: 125,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            output: Output::default(),
            modelines: Modelines::default(),
            shell: Shell::default(),
            audio: Audio::default(),
        }
    }
}

pub const DEFAULT_CONFIG: &str = r#"# omarchy-crt configuration. Every key is optional.

[output]
# DRM connector of the CRT DAC (HDMI-A-1 or card1-HDMI-A-1). Empty = the first
# connected HDMI output whose EDID names a Mortaca (RGB-Pi 2) device.
connector = ""
# Hyprland position of the CRT output.
position = "auto"
# Composite sync of the RGB-Pi 2: "and", "xor" or "separate". TVs differ.
csync = "xor"
# Standard used by `omarchy-crt on` without an argument: "ntsc" or "pal".
standard = "ntsc"

[modelines]
# Hyprland modelines. Clocks must be whole MHz, Hyprland truncates them.
ntsc = "72 3520 3695 4033 4577 240 242 245 262 -hsync -vsync"
pal = "72 3840 3948 4290 4608 288 291 294 312 -hsync -vsync"
# 240p at exactly 60.00 Hz, for filming the tube with a 60 fps camera.
film = "72 3520 3695 4033 4580 240 242 245 262 -hsync -vsync"

[shell]
bin = "omarchy-crt-shell"
args = ["--fullscreen", "--stretch", "--auto-boot"]

[audio]
# Enable the DAC's HDMI audio while on and send the launcher, RetroArch and
# mpv there. Desktop sounds stay on the desktop.
route = true
# Also make the CRT the system default sink while on (everything on the TV).
system_default = false
# Sink volume for the CRT, percent (up to 150). 125 is about +6 dB, which the
# RGB-Pi 2 needs to reach a normal television volume.
volume = 125
"#;

impl Config {
    pub fn path() -> PathBuf {
        config_dir().join("crt.toml")
    }

    /// Load the config, writing the commented default file when missing.
    pub fn load() -> Self {
        let path = Self::path();
        if !path.exists() {
            let _ = std::fs::create_dir_all(config_dir());
            let _ = std::fs::write(&path, DEFAULT_CONFIG);
        }
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn modeline(&self, standard: &str) -> Option<&str> {
        match standard {
            "ntsc" => Some(&self.modelines.ntsc),
            "pal" => Some(&self.modelines.pal),
            "film" => Some(&self.modelines.film),
            _ => None,
        }
    }
}

/// `~/.local/state/omarchy-crt/state.json`: what `on` changed, so `off`
/// can undo it.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct State {
    pub on: bool,
    pub standard: String,
    pub lines: u32,
    pub shift_x: i32,
    pub shift_y: i32,
    pub previous_profile: String,
    pub previous_sink: String,
    pub audio_card: String,
}

impl State {
    pub fn path() -> PathBuf {
        state_dir().join("state.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all(state_dir());
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(Self::path(), text);
        }
    }
}

/// Run a command and return stdout when it succeeded.
pub fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        None
    }
}

/// Run a command; return (success, combined output).
pub fn run_loose(cmd: &str, args: &[&str]) -> (bool, String) {
    match Command::new(cmd).args(args).output() {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            (out.status.success(), text.trim().to_string())
        }
        Err(e) => (false, format!("{cmd}: {e}")),
    }
}
