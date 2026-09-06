//! Game library: systems, ROM folders and per system launch policy.
//!
//! `~/.config/omarchy-crt/systems.toml` maps a directory of ROMs to a libretro
//! core plus everything that makes a game "right" on first launch: the video
//! policy, libretro core options, RetroArch input device types, run-ahead and
//! rewind. RetroArch runs with a dedicated base config and a per launch
//! override so its own menu never shows up.

use serde::Deserialize;
use std::collections::BTreeMap;
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
    /// libretro core options written to `cores.cfg` before launch.
    #[serde(default)]
    pub options: BTreeMap<String, String>,
    /// RetroArch input device types per port, `--device=PORT:TYPE` pairs
    /// such as `"1:1"` (joypad) or `"2:260"`.
    #[serde(default)]
    pub devices: Vec<String>,
    /// Run-ahead frames (0 = off). Only worth it on systems the CPU handles easily.
    #[serde(default)]
    pub runahead: u32,
    /// Allow rewind (costs CPU and memory; off for 3D systems).
    #[serde(default)]
    pub rewind: bool,
    /// Left stick as d-pad in RetroArch: 0 off, 1 on (default), 2 forced.
    /// Off for systems with a real analog stick (Nintendo 64, Dreamcast).
    #[serde(default)]
    pub analog_dpad: Option<u8>,
    /// `retroarch` (default) or `mpv` for video folders.
    #[serde(default)]
    pub player: String,
    /// Active lines the CRT switches to for this system (224 for Super
    /// Nintendo). Unset: the pinned frame height, else the standard's.
    #[serde(default)]
    pub lines: Option<u32>,
    /// Picture shift on the tube for this system, pixels and lines, added to
    /// the TV profile's global shift.
    #[serde(default)]
    pub shift_x: i32,
    #[serde(default)]
    pub shift_y: i32,
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
    /// CRT ready conversion next to the source, when it exists.
    pub crt_path: Option<PathBuf>,
    /// A subfolder to browse into rather than a file to run.
    pub folder: bool,
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

pub struct Library {
    pub systems: Vec<System>,
    pub retroarch: String,
    pub core_dir: PathBuf,
    pub config_dir: PathBuf,
    pub switching: bool,
    /// The scanned game index, when `omarchy-crt library scan` has run.
    /// Systems found there list from it; systems without index entries
    /// fall back to their folder.
    pub index: Option<crate::index::Index>,
}

/// Regions in the order a duplicate title picks its variant.
const REGION_ORDER: &[&str] = &["Europe", "World", "USA", "Japan"];

pub fn home() -> PathBuf {
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

fn opts(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// Built-in systems, tuned for a first launch that looks and plays right on
/// a 15 kHz CRT with the cores Arch ships. Users override any of it in
/// `systems.toml`. Run-ahead only where the CPU has headroom (8 and 16 bit),
/// rewind off on 3D systems.
fn default_systems() -> Vec<System> {
    let sys = |name: &str,
               core: &str,
               ext: &[&str],
               video: &str,
               runahead: u32,
               rewind: bool,
               options: BTreeMap<String, String>| System {
        name: name.into(),
        dir: format!("~/Games/roms/{name}"),
        core: core.into(),
        extensions: ext.iter().map(|e| e.to_string()).collect(),
        video: video.into(),
        options,
        devices: Vec::new(),
        runahead,
        rewind,
        analog_dpad: None,
        player: String::new(),
        lines: None,
        shift_x: 0,
        shift_y: 0,
    };
    vec![
        sys(
            "nes",
            "mesen",
            &["nes", "fds", "unf", "zip"],
            "super",
            1,
            true,
            opts(&[
                ("mesen_aspect_ratio", "No Stretching"),
                ("mesen_overclock", "None"),
            ]),
        ),
        sys(
            "snes",
            "snes9x",
            &["sfc", "smc", "zip"],
            "512x224",
            1,
            true,
            opts(&[
                ("snes9x_overclock_cycles", "disabled"),
                ("snes9x_overclock_superfx", "100%"),
                ("snes9x_superscope_crosshair", "0"),
                ("snes9x_hires_blend", "disabled"),
            ]),
        ),
        sys(
            "megadrive",
            "genesis_plus_gx",
            &["md", "bin", "gen", "smd", "zip"],
            "super",
            1,
            true,
            opts(&[
                ("genesis_plus_gx_overclock", "100%"),
                ("genesis_plus_gx_overscan", "disabled"),
                ("genesis_plus_gx_blargg_ntsc_filter", "disabled"),
                ("genesis_plus_gx_ym2413", "auto"),
            ]),
        ),
        sys(
            "mastersystem",
            "genesis_plus_gx",
            &["sms", "zip"],
            "super",
            1,
            true,
            opts(&[
                ("genesis_plus_gx_overscan", "disabled"),
                ("genesis_plus_gx_ym2413", "enabled"),
            ]),
        ),
        sys(
            "pcengine",
            "mednafen_pce_fast",
            &["pce", "cue", "chd", "zip"],
            "super",
            1,
            true,
            BTreeMap::new(),
        ),
        sys(
            "gb",
            "mgba",
            &["gb", "gbc", "zip"],
            "super",
            1,
            true,
            opts(&[
                ("mgba_gb_model", "Autodetect"),
                ("mgba_gb_colors", "DMG Green"),
            ]),
        ),
        sys(
            "gba",
            "mgba",
            &["gba", "zip"],
            "super",
            1,
            true,
            BTreeMap::new(),
        ),
        sys(
            "neogeo",
            "fbneo",
            &["zip", "7z"],
            "super",
            1,
            false,
            opts(&[
                ("fbneo-neogeo-mode", "MVS_EUR"),
                ("fbneo-force-60hz", "disabled"),
                ("fbneo-allow-patched-romsets", "disabled"),
            ]),
        ),
        sys(
            "arcade",
            "fbneo",
            &["zip", "7z"],
            "super",
            0,
            false,
            opts(&[
                ("fbneo-force-60hz", "disabled"),
                ("fbneo-cpu-speed-adjust", "100%"),
                ("fbneo-allow-patched-romsets", "disabled"),
            ]),
        ),
        sys(
            "psx",
            "mednafen_psx_hw",
            &["cue", "chd", "pbp", "m3u"],
            "super",
            0,
            false,
            opts(&[
                ("beetle_psx_hw_internal_resolution", "1x(native)"),
                ("beetle_psx_hw_crop_overscan", "enabled"),
                ("beetle_psx_hw_dither_mode", "1x(native)"),
                ("beetle_psx_hw_analog_toggle", "enabled"),
            ]),
        ),
        sys(
            "n64",
            "mupen64plus_next",
            &["n64", "z64", "v64", "zip"],
            "super",
            0,
            false,
            // Authentic look: native resolution and hardware-style dithering.
            opts(&[
                ("mupen64plus-43screensize", "320x240"),
                ("mupen64plus-EnableNativeResFactor", "1"),
                ("mupen64plus-DitheringPattern", "True"),
                ("mupen64plus-DitheringQuantization", "True"),
                ("mupen64plus-RDRAMImageDitheringMode", "False"),
                ("mupen64plus-BilinearMode", "3point"),
            ]),
        ),
        sys(
            "dreamcast",
            "flycast",
            &["gdi", "chd", "cdi", "cue", "m3u"],
            "native",
            0,
            false,
            opts(&[
                ("reicast_internal_resolution", "640x480"),
                ("reicast_screen_rotation", "horizontal"),
                ("reicast_widescreen_hack", "disabled"),
            ]),
        ),
    ]
    .into_iter()
    .map(|mut s| {
        if matches!(s.name.as_str(), "n64" | "dreamcast" | "psx") {
            s.analog_dpad = Some(0);
        }
        s
    })
    .chain(std::iter::once(System {
        name: "videos".into(),
        dir: "~/Videos".into(),
        core: String::new(),
        extensions: crate::player::EXTENSIONS
            .iter()
            .map(|e| e.to_string())
            .collect(),
        video: "native".into(),
        options: BTreeMap::new(),
        devices: Vec::new(),
        runahead: 0,
        rewind: false,
        analog_dpad: None,
        player: "mpv".into(),
        lines: None,
        shift_x: 0,
        shift_y: 0,
    }))
    .collect()
}

impl System {
    pub fn is_video(&self) -> bool {
        self.player == "mpv"
    }
}

impl Library {
    /// IPC socket used to talk to mpv.
    pub fn mpv_socket(&self) -> PathBuf {
        self.config_dir.join("mpv.sock")
    }

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
        let index = crate::index::Index::load();
        let mut systems = systems;
        if let Some(ix) = &index {
            for (name, _) in ix.systems() {
                if systems.iter().any(|s| s.name == name) {
                    continue;
                }
                let (core, exts) = crate::index::catalog(&name)
                    .map(|(_, c, e)| (c.to_string(), e.iter().map(|x| x.to_string()).collect()))
                    .unwrap_or_default();
                systems.push(System {
                    name: name.clone(),
                    dir: String::new(),
                    core,
                    extensions: exts,
                    video: "super".into(),
                    options: BTreeMap::new(),
                    devices: Vec::new(),
                    runahead: 0,
                    rewind: false,
                    analog_dpad: None,
                    player: String::new(),
                    lines: None,
                    shift_x: 0,
                    shift_y: 0,
                });
            }
        }
        let mut lib = Self {
            systems,
            retroarch,
            core_dir,
            config_dir,
            switching,
            index,
        };
        // Systems with nothing to show hide, unless that would empty the list.
        let kept: Vec<System> = lib
            .systems
            .iter()
            .filter(|s| s.is_video() || lib.count(s) > 0)
            .cloned()
            .collect();
        if !kept.is_empty() {
            lib.systems = kept;
        }
        lib
    }

    /// Index entries of a system, one per title: regional variants collapse
    /// onto the preferred region and multi disc games onto disc 1.
    fn indexed(&self, system: &System) -> Option<Vec<Game>> {
        let ix = self.index.as_ref()?;
        let items = ix.items_of(&system.name);
        if items.is_empty() {
            return None;
        }
        let rank = |it: &crate::index::Item| -> usize {
            REGION_ORDER
                .iter()
                .position(|r| r.eq_ignore_ascii_case(&it.region))
                .unwrap_or(REGION_ORDER.len())
        };
        let mut best: BTreeMap<String, &crate::index::Item> = BTreeMap::new();
        for it in items {
            if it.disc.map(|d| d > 1).unwrap_or(false) {
                continue;
            }
            let key = it.title.to_lowercase();
            match best.get(&key) {
                Some(cur) if rank(cur) <= rank(it) => {}
                _ => {
                    best.insert(key, it);
                }
            }
        }
        let mut games: Vec<Game> = best
            .into_values()
            .map(|it| Game {
                title: it.title.clone(),
                path: it.path.clone(),
                crt_path: None,
                folder: false,
            })
            .collect();
        games.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        Some(games)
    }

    /// ROMs of a system, sorted by title. Missing directories yield an empty
    /// list. When a folder holds `.m3u` playlists, the disc images they
    /// reference are hidden so a multi disc game shows up once.
    pub fn games(&self, system: &System) -> Vec<Game> {
        if let Some(g) = self.indexed(system) {
            return g;
        }
        if system.dir.is_empty() {
            return Vec::new();
        }
        self.games_in(system, &expand(&system.dir))
    }

    /// True when this system lists from the index rather than a folder.
    pub fn uses_index(&self, system: &System) -> bool {
        self.index
            .as_ref()
            .map(|ix| ix.items.iter().any(|i| i.system == system.name))
            .unwrap_or(false)
    }

    /// Files matching a system's extensions under `dir` (a folder of a
    /// system), counted through subfolders up to three levels deep.
    pub fn count(&self, system: &System) -> usize {
        if let Some(g) = self.indexed(system) {
            return g.len();
        }
        if system.dir.is_empty() {
            return 0;
        }
        fn walk(dir: &Path, system: &System, depth: u32) -> usize {
            let Ok(rd) = std::fs::read_dir(dir) else {
                return 0;
            };
            let mut n = 0;
            for e in rd.flatten() {
                let Ok(ft) = e.file_type() else { continue };
                if ft.is_dir() {
                    if depth > 0 {
                        n += walk(&e.path(), system, depth - 1);
                    }
                } else if ft.is_file() {
                    let p = e.path();
                    let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
                    if system.extensions.is_empty()
                        || system
                            .extensions
                            .iter()
                            .any(|x| x.eq_ignore_ascii_case(ext))
                    {
                        n += 1;
                    }
                }
            }
            n
        }
        walk(&expand(&system.dir), system, 3)
    }

    /// Entries of one folder of a system: subfolders first (browsable), then
    /// the games in it, sorted by title.
    pub fn games_in(&self, system: &System, dir: &Path) -> Vec<Game> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let entries: Vec<std::fs::DirEntry> = entries.filter_map(|e| e.ok()).collect();
        let mut folders: Vec<Game> = entries
            .iter()
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .map(|e| Game {
                title: e.file_name().to_string_lossy().into_owned(),
                path: e.path(),
                crt_path: None,
                folder: true,
            })
            .collect();
        folders.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        let dir = dir.to_path_buf();
        let entries = entries.into_iter();
        // `file_type` comes free with the directory entry; a stat per file
        // is what makes a 8000 ROM folder on a USB disk take seconds.
        let files: Vec<PathBuf> = entries
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|e| e.path())
            .collect();
        let mut hidden: Vec<PathBuf> = Vec::new();
        for m3u in files.iter().filter(|p| has_ext(p, "m3u")) {
            if let Ok(text) = std::fs::read_to_string(m3u) {
                for line in text
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty() && !l.starts_with('#'))
                {
                    hidden.push(dir.join(line));
                }
            }
        }
        let accepted = |p: &Path| -> bool {
            if system.extensions.is_empty() {
                return true;
            }
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| system.extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
                .unwrap_or(false)
        };
        let video = system.is_video();
        let mut games: Vec<Game> = files
            .iter()
            .filter(|p| {
                accepted(p)
                    && !hidden.iter().any(|h| h == *p)
                    && (!video || !crate::videofit::is_crt_file(p))
            })
            .map(|path| {
                // CRT ready siblings only exist for videos; skip the stat elsewhere.
                let crt = if video {
                    let c = crate::videofit::crt_path(path);
                    c.exists().then_some(c)
                } else {
                    None
                };
                Game {
                    title: clean_title(path),
                    crt_path: crt,
                    path: path.to_path_buf(),
                    folder: false,
                }
            })
            .collect();
        // Two files that clean to the same title (regional variants) keep
        // their first bracketed tag so they stay distinguishable.
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for g in &games {
            *counts.entry(g.title.to_lowercase()).or_default() += 1;
        }
        for g in games.iter_mut() {
            if counts[&g.title.to_lowercase()] > 1 {
                if let Some(tag) = first_tag(&g.path) {
                    g.title = format!("{} ({tag})", g.title);
                }
            }
        }
        games.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        folders.extend(games);
        folders
    }

    pub fn core_path(&self, system: &System) -> PathBuf {
        self.resolve_core(&system.core)
    }

    pub fn resolve_core(&self, core: &str) -> PathBuf {
        if core.contains('/') || core.ends_with(".so") {
            expand(core)
        } else {
            self.core_dir.join(format!("{core}_libretro.so"))
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

    /// Per launch overrides: video policy, geometry, run-ahead, rewind, core options.
    fn launch_keys(&self, system: &System, cores_cfg: &Path, extra: &str) -> String {
        let mut out = VideoPolicy::parse(&system.video).retroarch_keys(self.switching);
        {
            let mut kv = |k: &str, v: &str| out.push_str(&format!("{k} = \"{v}\"\n"));
            if system.runahead > 0 {
                kv("run_ahead_enabled", "true");
                kv("run_ahead_frames", &system.runahead.to_string());
                kv("run_ahead_secondary_instance", "true");
            } else {
                kv("run_ahead_enabled", "false");
            }
            kv(
                "rewind_enable",
                if system.rewind { "true" } else { "false" },
            );
            kv("global_core_options", "true");
            kv("core_options_path", &cores_cfg.display().to_string());
        }
        out.push_str(extra);
        dedupe_keys(&out)
    }

    /// Build the RetroArch command for one game. `extra` holds additional
    /// config keys (TV profile geometry). The shell keeps running and waits
    /// for the process; RetroArch's menu is never shown.
    pub fn command(
        &self,
        system: &System,
        game: &Game,
        extra: &str,
    ) -> std::io::Result<std::process::Command> {
        if system.is_video() {
            std::fs::create_dir_all(&self.config_dir)?;
            let input_conf = self.config_dir.join("mpv-input.conf");
            std::fs::write(&input_conf, crate::player::INPUT_CONF)?;
            let osd = self.config_dir.join("mpv-osd.lua");
            std::fs::write(&osd, crate::player::OSD_LUA)?;
            // `extra` carries "accent,dim,paper,selection" then one mpv argument per line.
            let mut lines = extra.lines();
            let parts: Vec<&str> = lines.next().unwrap_or("").split(',').collect();
            let colors = if parts.len() == 4 {
                [parts[0], parts[1], parts[2], parts[3]]
            } else {
                ["7aa2f7", "565f89", "c0caf5", "292e42"]
            };
            let fit_args: Vec<String> = lines.map(str::to_string).collect();
            // A CRT ready file needs no live fitting, but window options
            // (how the picture fills a wide super resolution) still apply.
            let (file, fit_args) = match &game.crt_path {
                Some(c) => (
                    c.as_path(),
                    fit_args
                        .into_iter()
                        .filter(|a| a.starts_with("--keepaspect"))
                        .collect(),
                ),
                None => (game.path.as_path(), fit_args),
            };
            let mut cmd =
                crate::player::command("mpv", file, &self.mpv_socket(), &input_conf, &osd, colors);
            cmd.args(fit_args);
            return Ok(cmd);
        }
        let cfg = self.retroarch_config()?;
        let cores_cfg = self.config_dir.join("cores.cfg");
        let mut options = String::new();
        for (k, v) in &system.options {
            options.push_str(&format!("{k} = \"{v}\"\n"));
        }
        std::fs::write(&cores_cfg, options)?;
        let launch_cfg = self.config_dir.join("launch.cfg");
        std::fs::write(&launch_cfg, self.launch_keys(system, &cores_cfg, extra))?;
        let mut cmd = std::process::Command::new(&self.retroarch);
        cmd.arg("--config")
            .arg(cfg)
            .arg("--appendconfig")
            .arg(launch_cfg)
            .arg("--fullscreen");
        for d in &system.devices {
            cmd.arg(format!("--device={d}"));
        }
        cmd.arg("-L")
            .arg(self.core_path(system))
            .arg(&game.path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        Ok(cmd)
    }
}

/// RetroArch keeps the first occurrence of a duplicated key, so later
/// overrides (the output geometry appended by the shell) must replace the
/// earlier value instead of following it.
fn dedupe_keys(text: &str) -> String {
    let mut order: Vec<String> = Vec::new();
    let mut values: BTreeMap<String, String> = BTreeMap::new();
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let k = k.trim().to_string();
        if !values.contains_key(&k) {
            order.push(k.clone());
        }
        values.insert(k, v.trim().to_string());
    }
    let mut out = String::new();
    for k in order {
        out.push_str(&format!("{k} = {}\n", values[&k]));
    }
    out
}

fn has_ext(p: &Path, ext: &str) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(ext))
        .unwrap_or(false)
}

/// First bracketed tag of a file name: `Game (PAL) [!].sfc` -> `PAL`.
fn first_tag(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let start = stem.find(['(', '['])? + 1;
    let end = stem[start..].find([')', ']'])? + start;
    let tag = stem[start..end].trim();
    (!tag.is_empty()).then(|| tag.to_string())
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
/// text, fullscreen, hotkeys to leave, save state on exit and resume on start,
/// automatic frame delay for latency.
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
video_crop_overscan = "false"
video_frame_delay_auto = "true"
audio_resampler_quality = "3"
config_save_on_exit = "false"
input_driver = "udev"
input_joypad_driver = "udev"
input_autodetect_enable = "true"
joypad_autoconfig_dir = "/usr/share/libretro/autoconfig/udev"
input_max_users = "4"
"#;
