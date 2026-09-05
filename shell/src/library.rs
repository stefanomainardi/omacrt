//! Game library: systems and ROM folders, RGB-Pi style. Each system maps a
//! directory of ROMs to a libretro core and a preferred 15 kHz video mode.
//! Configured in `~/.config/omarchy-crt/systems.toml`; RetroArch runs with a
//! dedicated config so its own menu never shows up.

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Deserialize)]
pub struct System {
    /// Short name shown in the listing, e.g. `snes`.
    pub name: String,
    /// Directory holding the ROMs. `~` is expanded.
    pub dir: String,
    /// Core name (`snes9x`) or full path to a `*_libretro.so`.
    pub core: String,
    /// Accepted extensions, lowercase, without the dot.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Video policy: `super` (default, wide frame, height follows the core),
    /// `native` (exact core resolution), or a pinned frame such as `512x224`.
    #[serde(default)]
    pub video: String,
}

/// How the display mode follows the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoPolicy {
    /// Fixed wide width (2560 by default), vertical resolution and refresh follow the core.
    Super(u32),
    /// Width, height and refresh all follow the core.
    Native,
    /// One fixed frame for the whole session; the core is scaled into it.
    Fixed(u32, u32),
}

impl VideoPolicy {
    pub fn parse(s: &str) -> Self {
        let s = s.trim().to_lowercase();
        if s.is_empty() || s == "super" {
            return VideoPolicy::Super(2560);
        }
        if let Some(w) = s.strip_prefix("super:") {
            return VideoPolicy::Super(w.parse().unwrap_or(2560));
        }
        if s == "native" {
            return VideoPolicy::Native;
        }
        if let Some((w, h)) = s.split_once('x') {
            if let (Ok(w), Ok(h)) = (w.parse(), h.parse()) {
                return VideoPolicy::Fixed(w, h);
            }
        }
        VideoPolicy::Super(2560)
    }

    pub fn label(&self) -> String {
        match self {
            VideoPolicy::Super(2560) => "super".into(),
            VideoPolicy::Super(w) => format!("super:{w}"),
            VideoPolicy::Native => "native".into(),
            VideoPolicy::Fixed(w, h) => format!("{w}x{h}"),
        }
    }

    /// RetroArch keys for this policy. `switching` enables CRT SwitchRes
    /// (needs the KMS or X11 video driver and the 15 kHz kernel); without it
    /// only pinned frames change anything, which is what a desktop test needs.
    pub fn retroarch_keys(&self, switching: bool) -> String {
        let mut out = String::new();
        let mut kv = |k: &str, v: &str| out.push_str(&format!("{k} = \"{v}\"\n"));
        match self {
            VideoPolicy::Super(w) => {
                kv("crt_switch_resolution", if switching { "1" } else { "0" });
                kv("crt_switch_resolution_super", &w.to_string());
                kv("aspect_ratio_index", "22");
                kv("video_scale_integer", "false");
            }
            VideoPolicy::Native => {
                kv("crt_switch_resolution", if switching { "1" } else { "0" });
                kv("crt_switch_resolution_super", "0");
                kv("aspect_ratio_index", "22");
                kv("video_scale_integer", "true");
            }
            VideoPolicy::Fixed(w, h) => {
                kv("crt_switch_resolution", "0");
                kv("video_fullscreen_x", &w.to_string());
                kv("video_fullscreen_y", &h.to_string());
                kv("aspect_ratio_index", "23");
                kv("custom_viewport_x", "0");
                kv("custom_viewport_y", "0");
                kv("custom_viewport_width", &w.to_string());
                kv("custom_viewport_height", &h.to_string());
                kv("video_scale_integer", "false");
            }
        }
        out
    }
}

#[derive(Debug, Deserialize)]
struct File {
    #[serde(default)]
    system: Vec<System>,
    #[serde(default)]
    retroarch: Option<String>,
    #[serde(default)]
    core_dir: Option<String>,
    /// Enable mode switching in RetroArch (CRT SwitchRes). Off until the
    /// 15 kHz stack is in place; pinned frames work regardless.
    #[serde(default)]
    switching: bool,
}

#[derive(Clone, Debug)]
pub struct Game {
    pub title: String,
    pub path: PathBuf,
}

pub struct Library {
    pub systems: Vec<System>,
    pub retroarch: String,
    pub core_dir: PathBuf,
    pub config_dir: PathBuf,
    pub switching: bool,
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn expand(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        home().join(rest)
    } else if p == "~" {
        home()
    } else {
        PathBuf::from(p)
    }
}

pub fn default_path() -> PathBuf {
    home().join(".config/omarchy-crt/systems.toml")
}

fn default_systems() -> Vec<System> {
    let sys = |name: &str, dir: &str, core: &str, ext: &[&str], video: &str| System {
        name: name.into(),
        dir: dir.into(),
        core: core.into(),
        extensions: ext.iter().map(|e| e.to_string()).collect(),
        video: video.into(),
    };
    vec![
        sys("nes", "~/Games/roms/nes", "mesen", &["nes", "zip"], "super"),
        sys(
            "snes",
            "~/Games/roms/snes",
            "snes9x",
            &["sfc", "smc", "zip"],
            "super",
        ),
        sys(
            "megadrive",
            "~/Games/roms/megadrive",
            "genesis_plus_gx",
            &["md", "bin", "gen", "zip"],
            "super",
        ),
        sys("arcade", "~/Games/roms/arcade", "fbneo", &["zip"], "super"),
        sys(
            "psx",
            "~/Games/roms/psx",
            "swanstation",
            &["cue", "chd", "pbp"],
            "super",
        ),
    ]
}

impl Library {
    pub fn load(path: &Path) -> Self {
        let config_dir = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| home().join(".config/omarchy-crt"));
        let file = std::fs::read_to_string(path)
            .ok()
            .and_then(|t| toml::from_str::<File>(&t).ok());
        let (systems, retroarch, core_dir, switching) = match file {
            Some(f) => (
                if f.system.is_empty() {
                    default_systems()
                } else {
                    f.system
                },
                f.retroarch.unwrap_or_else(|| "retroarch".into()),
                f.core_dir
                    .map(|d| expand(&d))
                    .unwrap_or_else(|| PathBuf::from("/usr/lib/libretro")),
                f.switching,
            ),
            None => (
                default_systems(),
                "retroarch".into(),
                PathBuf::from("/usr/lib/libretro"),
                false,
            ),
        };
        Self {
            systems,
            retroarch,
            core_dir,
            config_dir,
            switching,
        }
    }

    /// ROMs of a system, sorted by title. Missing directories yield an empty list.
    pub fn games(&self, system: &System) -> Vec<Game> {
        let dir = expand(&system.dir);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut games: Vec<Game> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .filter(|p| {
                if system.extensions.is_empty() {
                    return true;
                }
                p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| system.extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
                    .unwrap_or(false)
            })
            .map(|path| Game {
                title: clean_title(&path),
                path,
            })
            .collect();
        games.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        games
    }

    pub fn core_path(&self, system: &System) -> PathBuf {
        if system.core.contains('/') || system.core.ends_with(".so") {
            expand(&system.core)
        } else {
            self.core_dir.join(format!("{}_libretro.so", system.core))
        }
    }

    /// Path of the RetroArch config we launch with, creating it on first use.
    pub fn retroarch_config(&self) -> std::io::Result<PathBuf> {
        let path = self.config_dir.join("retroarch.cfg");
        if !path.exists() {
            std::fs::create_dir_all(&self.config_dir)?;
            std::fs::write(&path, DEFAULT_RETROARCH_CFG)?;
        }
        Ok(path)
    }

    /// Build the RetroArch command for one game. The shell keeps running and
    /// waits for the process; RetroArch's menu is never shown.
    pub fn command(&self, system: &System, game: &Game) -> std::io::Result<std::process::Command> {
        let cfg = self.retroarch_config()?;
        // Per launch overrides: the video policy of this system.
        let policy = VideoPolicy::parse(&system.video);
        let launch_cfg = self.config_dir.join("launch.cfg");
        std::fs::write(&launch_cfg, policy.retroarch_keys(self.switching))?;
        let mut cmd = std::process::Command::new(&self.retroarch);
        cmd.arg("--config")
            .arg(cfg)
            .arg("--appendconfig")
            .arg(launch_cfg)
            .arg("--fullscreen")
            .arg("-L")
            .arg(self.core_path(system))
            .arg(&game.path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        Ok(cmd)
    }
}

/// `Super Metroid (USA).sfc` -> `Super Metroid`.
pub fn clean_title(path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
    let mut out = String::new();
    let mut depth = 0;
    for ch in stem.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        stem.to_string()
    } else {
        trimmed.to_string()
    }
}

/// RetroArch settings for a console-like experience: no menu, no on-screen
/// text, fullscreen, hotkeys to leave, save state on exit and resume on start.
pub const DEFAULT_RETROARCH_CFG: &str = r#"# Written by omarchy-crt-shell. Edit freely; it is only created when missing.
video_fullscreen = "true"
video_windowed_fullscreen = "true"
video_font_enable = "false"
menu_show_load_content_animation = "false"
notification_show_autoconfig = "false"
notification_show_config_override_load = "false"
notification_show_remap_load = "false"
notification_show_set_initial_disk = "false"
notification_show_fast_forward = "false"
notification_show_screenshot = "false"
pause_nonactive = "false"
quit_press_twice = "false"
input_menu_toggle = "nul"
input_menu_toggle_gamepad_combo = "0"
input_exit_emulator = "escape"
input_quit_gamepad_combo = "4"
savestate_auto_save = "true"
savestate_auto_load = "true"
video_smooth = "false"
"#;
