//! CRT output control shared by the `omacrt` CLI and the launcher:
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
pub mod tidy;
pub mod watchdog;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

pub const SHELL_CLASS: &str = "omacrt-shell";

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn config_dir() -> PathBuf {
    std::env::var_os("OMACRT_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config/omacrt"))
}

/// Where anything the project can fetch again belongs.
///
/// Four other places work this out for themselves and two of them ignore
/// `XDG_CACHE_HOME`, which is why the tidy sweep can look in the wrong
/// directory; bringing them here is follow-up work. New code uses this one.
/// Does this process id still belong to the program we think it does?
///
/// A pid read out of a file is a fact about the past. The process it names can
/// be gone and its number handed to something else, and every `kill` here is
/// one line after a read, so the check goes immediately before the signal.
/// Reading `/proc/<pid>/cmdline` costs nothing next to being wrong.
pub fn pid_runs(pid: i32, program: &str) -> bool {
    let Ok(raw) = std::fs::read(format!("/proc/{pid}/cmdline")) else {
        return false;
    };
    let line = String::from_utf8_lossy(&raw).replace('\0', " ");
    let argv0 = line.split_whitespace().next().unwrap_or("");
    std::path::Path::new(argv0)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .is_some_and(|name| name == program)
}

pub fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".cache"))
        .join("omacrt")
}

pub fn state_dir() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/state"))
        .join("omacrt")
}

/// The four folders a user's own things live in, each with the name the
/// project used to carry beside it.
///
/// The project was called `omacrt` before it became OmaCRT, and
/// everything a user had - the configuration, the library index, the cached
/// art and covers, the logs - sat under that name. On the first run of a
/// renamed build what the old folder holds is moved into the new one, and
/// nothing is deleted: a name already taken on the new side is left alone on
/// both sides, and the old folder itself stays where it is.
fn legacy_moves() -> Vec<(PathBuf, PathBuf)> {
    /// The name the project carried before it became OmaCRT.
    const LEGACY: &str = "omarchy-crt";

    let base = |var: &str, fallback: &str| {
        std::env::var_os(var)
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(fallback))
    };
    let config = std::env::var_os("OMACRT_CONFIG")
        .map(PathBuf::from)
        .map(|new| (new.with_file_name(LEGACY), new))
        .unwrap_or_else(|| {
            let dir = base("XDG_CONFIG_HOME", ".config");
            (dir.join(LEGACY), dir.join("omacrt"))
        });
    let mut moves = vec![config];
    for (var, fallback) in [
        ("XDG_CACHE_HOME", ".cache"),
        ("XDG_DATA_HOME", ".local/share"),
        ("XDG_STATE_HOME", ".local/state"),
    ] {
        let dir = base(var, fallback);
        moves.push((dir.join(LEGACY), dir.join("omacrt")));
    }
    moves
}

/// Move whatever the old name still holds under the new one, once.
///
/// Called at the start of both the launcher and the `omacrt` tool, before
/// anything reads a configuration file, so a machine that installed the
/// project under its old name keeps its settings, its library and its cache.
///
/// The move is per file rather than per folder: the new folder often exists
/// already, because a default configuration or a log was written into it
/// before the old one was noticed. A name that exists on both sides is left
/// alone on both sides, and the old folder itself is never removed.
pub fn migrate_legacy_dirs() {
    for (old, new) in legacy_moves() {
        if !old.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&old) else {
            continue;
        };
        if std::fs::create_dir_all(&new).is_err() {
            continue;
        }
        let mut moved = 0usize;
        let mut kept = 0usize;
        for entry in entries.flatten() {
            let to = new.join(entry.file_name());
            if to.exists() {
                kept += 1;
                continue;
            }
            match std::fs::rename(entry.path(), &to) {
                Ok(()) => moved += 1,
                Err(e) => {
                    kept += 1;
                    eprintln!("omacrt: {} stays where it is ({e})", entry.path().display());
                }
            }
        }
        // Only a move is worth a line; a folder that has nothing left to
        // give is passed over in silence at every later start.
        if moved > 0 {
            eprintln!(
                "omacrt: {} moved from {} to {}{}",
                moved,
                old.display(),
                new.display(),
                if kept > 0 {
                    format!(", {kept} left behind (a file of that name was there already)")
                } else {
                    String::new()
                }
            );
        }
    }
}

/// `~/.config/omacrt/crt.toml`.
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
    /// connected HDMI output the desktop is not using as a monitor, which
    /// is what the boot time override makes of a DAC's connector whoever
    /// made it.
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
    /// Switch the tube on at login when the DAC is connected (`omacrt
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

pub const DEFAULT_CONFIG: &str = r#"# omacrt configuration. Every key is optional.

[output]
# DRM connector of the CRT DAC (HDMI-A-1 or card1-HDMI-A-1). Empty = the first
# connected HDMI output the desktop is not using as a monitor. That is what
# the boot time override makes of the DAC's connector, so a DAC of any make
# is found this way; the name is only needed when two spare outputs are
# connected and the wrong one is chosen.
connector = ""
# Hyprland position of the CRT output.
position = "auto"
# Composite sync of the RGB-Pi 2: "and", "xor" or "separate". TVs differ.
csync = "xor"
# Standard used by `omacrt on` without an argument: "ntsc" or "pal".
standard = "ntsc"

[modelines]
# Hyprland modelines. Clocks must be whole MHz, Hyprland truncates them.
ntsc = "72 3520 3695 4033 4577 240 242 245 262 -hsync -vsync"
pal = "72 3840 3948 4290 4608 288 291 294 312 -hsync -vsync"
# 240p at exactly 60.00 Hz, for filming the tube with a 60 fps camera.
film = "72 3520 3695 4033 4580 240 242 245 262 -hsync -vsync"
# Interlaced frames for video (omacrt mode 480i | 576i).
ntsc_i = "72 3520 3695 4033 4577 480 484 490 525 -hsync -vsync interlace"
pal_i = "72 3840 3948 4290 4608 576 582 588 625 -hsync -vsync interlace"

[shell]
bin = "omacrt-shell"
args = ["--fullscreen", "--stretch", "--auto-boot"]
# Light the tube at login when the DAC is connected (`omacrt boot`).
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

/// `~/.local/state/omacrt/state.json`: what `on` changed, so `off`
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
    /// The CRT sink's volume before `on` raised it, as a percentage. `off`
    /// puts it back: the DAC needs about 125 % to reach a normal television
    /// volume, and leaving that on a sink the desktop also uses is a loud
    /// surprise later.
    pub previous_volume: String,
    /// Which sink that volume belongs to, so `off` does not have to work out
    /// the connector again to put it back.
    pub crt_sink: String,
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

    /// Through `store::save`, like everything else the project keeps.
    ///
    /// This is the file `off` reads to put the audio profile, the default sink
    /// and the compositor's setting back where it found them. Written straight
    /// with `fs::write`, a crash in the middle left it empty and the undo was
    /// gone; written through the store there is a copy beside it.
    pub fn save(&self) {
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = crate::store::save(&Self::path(), text);
        }
    }
}

/// The standard actually put on the tube, for a base standard and a line
/// count. More lines than a progressive 15 kHz frame holds is an interlaced
/// picture; fewer is a progressive one, so a console that drew 480 lines on a
/// television gets 480 lines here and nobody has to name a mode for it.
///
/// `interlace` says whether this machine can show one. When it cannot, a line
/// count that would have asked for an interlaced mode stays on the
/// progressive one and the emulator scales into it.
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
