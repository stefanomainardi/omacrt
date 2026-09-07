//! CRT output control shared by the `omarchy-crt` CLI and the launcher:
//! configuration, persistent state, the RGB-Pi 2 DAC, the Hyprland output,
//! audio routing, the launcher process, BIOS files and ROM folders.

pub mod audio;
pub mod bios;
pub mod control;
pub mod dac;
pub mod display;
pub mod launcher;
pub mod output;
pub mod roms;
pub mod watchdog;

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
#[derive(Default)]
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
    /// Whether this machine can put a real interlaced picture on the tube.
    ///
    /// A console that drew 480 lines wants 480i, and that needs a kernel that
    /// can drive an interlaced mode. Stock `amdgpu` cannot: it accepts the
    /// modeline and scans out something the television makes a mess of, which
    /// looks like a narrow strip rather than a picture. The 15 kHz kernel
    /// patches fix it; until they are installed a 480 line game is shown at
    /// 240 progressive, which is soft but right.
    #[serde(default)]
    pub interlace: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Modelines {
    pub ntsc: String,
    pub pal: String,
    /// 240p at 60.00 Hz for filming the tube with a 60 fps camera: no
    /// beat between the 60.04 Hz standard timing and the shutter.
    pub film: String,
    /// Interlaced frames for video: 480 lines at 59.94 Hz and 576 at 50 Hz,
    /// the same line rates as the progressive standards.
    #[serde(default = "default_ntsc_i")]
    pub ntsc_i: String,
    #[serde(default = "default_pal_i")]
    pub pal_i: String,
}

fn default_ntsc_i() -> String {
    "72 3520 3695 4033 4577 480 484 490 525 -hsync -vsync interlace".into()
}
fn default_pal_i() -> String {
    "72 3840 3948 4290 4608 576 582 588 625 -hsync -vsync interlace".into()
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Shell {
    /// Launcher binary; a bare name is looked up in PATH, then next to this
    /// program, then in the source tree.
    pub bin: String,
    pub args: Vec<String>,
    /// Switch the tube on at login when the DAC is connected (`omarchy-crt
    /// boot` does it), so the television is a console from the start.
    pub autostart: bool,
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
            interlace: false,
        }
    }
}

impl Default for Modelines {
    fn default() -> Self {
        Self {
            ntsc: "72 3520 3695 4033 4577 240 242 245 262 -hsync -vsync".into(),
            pal: "72 3840 3948 4290 4608 288 291 294 312 -hsync -vsync".into(),
            film: "72 3520 3695 4033 4580 240 242 245 262 -hsync -vsync".into(),
            ntsc_i: default_ntsc_i(),
            pal_i: default_pal_i(),
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
            autostart: false,
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
# Interlaced frames for video (omarchy-crt mode 480i | 576i).
ntsc_i = "72 3520 3695 4033 4577 480 484 490 525 -hsync -vsync interlace"
pal_i = "72 3840 3948 4290 4608 576 582 588 625 -hsync -vsync interlace"

[shell]
bin = "omarchy-crt-shell"
args = ["--fullscreen", "--stretch", "--auto-boot"]
# Light the tube at login when the DAC is connected (`omarchy-crt boot`).
autostart = false

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
            "480i" | "ntsc_i" => Some(&self.modelines.ntsc_i),
            "576i" | "pal_i" => Some(&self.modelines.pal_i),
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
    /// What `misc:on_focus_under_fullscreen` was before the tube took it,
    /// so that turning the television off puts the desktop back as it was.
    pub previous_focus_under_fullscreen: Option<i64>,
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

/// The standard actually put on the tube, for a base standard and a line
/// count. More lines than a progressive 15 kHz frame holds is an interlaced
/// picture; fewer is a progressive one. A console that drew 480 lines on a
/// television gets 480 lines here, and nobody has to name a mode for it.
///
/// `lines` of 0 means the standard's own line count, so nothing changes.
pub fn applied_standard(standard: &str, lines: u32) -> &str {
    applied_standard_with(standard, lines, true)
}

/// The same, told whether this machine can show an interlaced picture. When
/// it cannot, a line count that would have asked for one stays on the
/// progressive mode and the emulator scales into it.
pub fn applied_standard_with(standard: &str, lines: u32, interlace: bool) -> &str {
    match (standard, lines) {
        (_, 0) => standard,
        ("ntsc", l) if l > 288 && interlace => "480i",
        ("pal", l) if l > 288 && interlace => "576i",
        ("480i" | "ntsc_i", l) if l <= 288 || !interlace => "ntsc",
        ("576i" | "pal_i", l) if l <= 288 || !interlace => "pal",
        _ => standard,
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

/// Change one `section.key` of `crt.toml` in place, keeping the comments.
/// The value is written as TOML: quoted unless it is a number or a bool.
pub fn set_value(key: &str, value: &str) -> Result<(), String> {
    let (section, name) = key
        .split_once('.')
        .ok_or_else(|| format!("{key}: expected section.key, e.g. audio.volume"))?;
    let allowed: &[(&str, &[&str])] = &[
        ("output", &["connector", "position", "csync", "standard"]),
        ("audio", &["route", "system_default", "volume"]),
        ("shell", &["autostart"]),
        ("modelines", &["ntsc", "pal", "film", "ntsc_i", "pal_i"]),
    ];
    let ok = allowed
        .iter()
        .any(|(s, keys)| *s == section && keys.contains(&name));
    if !ok {
        return Err(format!("{key} is not a setting this command changes"));
    }
    let literal = if value.parse::<f64>().is_ok() || value == "true" || value == "false" {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    };
    let path = Config::path();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut out: Vec<String> = Vec::new();
    let mut in_section = false;
    let mut written = false;
    let mut section_end: Option<usize> = None;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            if in_section && !written {
                section_end = Some(out.len());
            }
            in_section = t == format!("[{section}]");
        } else if in_section && !written {
            let head = t.split('=').next().unwrap_or("").trim();
            if head == name {
                // Keep a trailing comment if the line has one after the value.
                let comment = comment_of(line);
                out.push(format!("{name} = {literal}{comment}"));
                written = true;
                continue;
            }
        }
        out.push(line.to_string());
    }
    if !written {
        match section_end {
            Some(i) => out.insert(i, format!("{name} = {literal}")),
            None if in_section => out.push(format!("{name} = {literal}")),
            None => {
                if !out.is_empty() && !out.last().is_some_and(|l| l.trim().is_empty()) {
                    out.push(String::new());
                }
                out.push(format!("[{section}]"));
                out.push(format!("{name} = {literal}"));
            }
        }
    }
    let mut joined = out.join("\n");
    joined.push('\n');
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    crate::store::save(&path, joined).map_err(|e| e.to_string())
}

/// The `  # comment` tail of a TOML line, if any, outside of quotes.
fn comment_of(line: &str) -> String {
    let mut in_str = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_str = !in_str,
            '#' if !in_str => return format!("  {}", &line[i..]),
            _ => {}
        }
    }
    String::new()
}
