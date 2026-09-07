//! Game library: systems, ROM folders and per system launch policy.
//!
//! `~/.config/omarchy-crt/systems.toml` maps a directory of ROMs to a libretro
//! core plus everything that makes a game "right" on first launch: the video
//! policy, libretro core options, RetroArch input device types, run-ahead and
//! rewind. RetroArch runs with a dedicated base config and a per launch
//! override so its own menu never shows up.

use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
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
    /// The shape of this file. See `crate::config`.
    #[serde(default)]
    version: u32,
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
        if let Some((w, h)) = s.split_once('x')
            && let (Ok(w), Ok(h)) = (w.parse(), h.parse())
        {
            return VideoPolicy::Fixed(w, h);
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
                // Integer scaling only means something when the emulator is
                // picking the mode. When the host picks it the picture lands
                // in a frame thousands of pixels wide that the tube squeezes
                // back to 4:3, and asking for whole multiples there leaves the
                // game in a strip in the middle of the screen with its top and
                // bottom cut off.
                kv(
                    "video_scale_integer",
                    if switching { "true" } else { "false" },
                );
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
    /// Named lists from `collections/*.txt`, resolved once at load time:
    /// the launcher reads them every frame on two screens.
    collections: Vec<(String, Vec<(usize, PathBuf)>)>,
    /// System of every indexed file, for constant time `system_of`.
    path_system: HashMap<PathBuf, usize>,
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

/// The active lines a console draws, as a starting point for the tube.
///
/// It only has to be right often enough that the first frame of a game is
/// already the right shape: the launcher reads the core's own reports from
/// the emulator's log while it runs and follows them from there, so a game
/// that disagrees costs one mode change rather than a squashed picture.
///
/// 480 is a real interlaced frame, not half a progressive one: a console that
/// drew 480 lines on a television gets 480 lines on this one.
pub fn default_lines(system: &str) -> Option<u32> {
    Some(match system {
        "nes" | "pcengine" | "pcenginecd" | "psx" | "n64" | "mastersystem" | "gamegear" => 240,
        "snes" | "megadrive" | "megacd" | "32x" | "neogeo" | "arcade" | "saturn" | "mame" => 224,
        "dreamcast" | "naomi" | "ps2" | "gamecube" | "wii" | "xbox" => 480,
        "gb" | "gbc" | "gba" | "nds" | "psp" | "ngp" | "wonderswan" | "lynx" => 240,
        _ => return None,
    })
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
        sys(
            "scummvm",
            "scummvm",
            &["scummvm", "svm"],
            // These games were drawn 320x200 and shown on 4:3 monitors, which
            // stretched them to 240 lines: that is what they are supposed to
            // look like. Pinning the frame at 240 also keeps the launcher's
            // own screens, which are 240 lines, sharp over the game.
            "320x240",
            0,
            false,
            // Point and click on a sofa: the left stick is the pointer, with
            // enough acceleration to cross the screen and a response curve
            // that still lets it stop on a door handle. The interface stays at
            // the resolution the games were drawn for, since that is what the
            // tube shows, and hardware acceleration stays off because the core
            // draws these games in software anyway.
            opts(&[
                ("scummvm_pointer_device", "Left Analog"),
                ("scummvm_gamepad_cursor_speed", "1.0"),
                ("scummvm_gamepad_cursor_acceleration_time", "0.2"),
                ("scummvm_analog_response", "quadratic"),
                ("scummvm_analog_deadzone", "15"),
                ("scummvm_mouse_speed", "1.0"),
                ("scummvm_mouse_fine_control_speed_reduction", "4"),
                ("scummvm_gui_aspect_ratio", "4/3"),
                ("scummvm_gui_h_res", "320x200"),
                ("scummvm_video_hw_acceleration", "disabled"),
                ("scummvm_autosave", "enabled"),
                ("scummvm_samplerate", "48000"),
                // A point and click game on a pad needs the two mouse buttons
                // and the handful of keys these games were written around:
                // Enter for a dialogue box, Escape to skip a cutscene, the
                // full stop to skip a line, F5 for the save menu, and the
                // virtual keyboard for the one game that asks you to type.
                ("scummvm_mapper_a", "RETROKE_LEFT_BUTTON"),
                ("scummvm_mapper_b", "RETROKE_RIGHT_BUTTON"),
                ("scummvm_mapper_x", "RETROK_ESCAPE"),
                ("scummvm_mapper_y", "RETROK_PERIOD"),
                ("scummvm_mapper_start", "RETROK_RETURN"),
                ("scummvm_mapper_select", "RETROKE_SCUMMVM_GUI"),
                ("scummvm_mapper_l", "RETROKE_VKBD"),
                ("scummvm_mapper_r", "RETROKE_FINE_CONTROL"),
                ("scummvm_mapper_l2", "RETROK_F5"),
                ("scummvm_mapper_r2", "RETROK_SPACE"),
                ("scummvm_mapper_l3", "RETROK_BACKSPACE"),
                ("scummvm_mapper_r3", "RETROK_RETURN"),
            ]),
        ),
        sys(
            "gamecube",
            "dolphin",
            &[
                "iso", "gcm", "rvz", "gcz", "ciso", "wia", "dol", "elf", "m3u",
            ],
            "native",
            0,
            false,
            // A GameCube drew 640x480 interlaced on a television, and that is
            // what it gets here: the internal resolution stays native, the
            // widescreen hacks stay off, and progressive scan stays off, which
            // is what a 15 kHz set can show. The boot animation is skipped
            // because it needs an IPL dump nobody has by default.
            opts(&[
                ("dolphin_efb_scale", "1x Native (640x528)"),
                ("dolphin_widescreen", "disabled"),
                ("dolphin_widescreen_hack", "disabled"),
                ("dolphin_progressive_scan", "disabled"),
                ("dolphin_force_progressive", "disabled"),
                // A GameCube drew 640x480; the extra lines are the frame
                // buffer's, not the picture's. Cropping them is what makes the
                // core report the 480 lines the television is set to.
                ("dolphin_crop_overscan", "enabled"),
                ("dolphin_skip_gc_bios", "enabled"),
                ("dolphin_osd_enabled", "disabled"),
                ("dolphin_shader_compilation_mode", "sync"),
                ("dolphin_wait_for_shaders", "enabled"),
                ("dolphin_cpu_core", "JIT Recompiler"),
                ("dolphin_dsp_hle", "enabled"),
                ("dolphin_fastmem", "enabled"),
            ]),
        ),
    ]
    .into_iter()
    .map(|mut s| {
        if matches!(s.name.as_str(), "n64" | "dreamcast" | "psx" | "gamecube") {
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
        let file = crate::config::read(path).and_then(|t| toml::from_str::<File>(&t).ok());
        if let Some(f) = &file
            && f.version > crate::config::VERSION
        {
            eprintln!(
                "omarchy-crt: {} comes from a newer version and may hold \
                 settings this build does not know about",
                path.display()
            );
        }
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
            // A system the scan found but `systems.toml` does not mention.
            // It gets the built-in definition when there is one, which is
            // where the tuning lives: the picture a console drew, the core
            // options that make it look right, the buttons a game needs. Only
            // a system nobody has tuned falls back to the catalogue, which
            // knows a core and a list of extensions and nothing else.
            let builtin: std::collections::HashMap<String, System> = default_systems()
                .into_iter()
                .map(|s| (s.name.clone(), s))
                .collect();
            for (name, _) in ix.systems() {
                if systems.iter().any(|s| s.name == name) {
                    continue;
                }
                if let Some(mut tuned) = builtin.get(&name).cloned() {
                    // The folder comes from the index, not from the built-in
                    // guess at where a collection lives.
                    tuned.dir = String::new();
                    systems.push(tuned);
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
        // A system that names no line count gets the one its console drew.
        for s in &mut systems {
            if s.lines.is_none() && !s.is_video() {
                s.lines = default_lines(&s.name);
            }
        }
        let mut lib = Self {
            systems,
            retroarch,
            core_dir,
            config_dir,
            switching,
            index,
            collections: Vec::new(),
            path_system: HashMap::new(),
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
        lib.path_system = match &lib.index {
            Some(ix) => ix
                .items
                .iter()
                .filter_map(|it| {
                    let sys = lib.systems.iter().position(|s| s.name == it.system)?;
                    Some((it.path.clone(), sys))
                })
                .collect(),
            None => HashMap::new(),
        };
        lib.collections = lib.load_collections();
        lib
    }

    /// The named collections, sorted by name, each with its games resolved
    /// to a system.
    pub fn collections(&self) -> &[(String, Vec<(usize, PathBuf)>)] {
        &self.collections
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
        // Arcade collections name their files after the emulated set, so the
        // index carries `mslug` where the game is Metal Slug. The databases
        // RetroArch ships pair the two; the title is what the lists show,
        // sort by and search.
        let title_of = |it: &crate::index::Item| crate::covers::title_for(&system.name, &it.title);
        let mut best: BTreeMap<String, (&crate::index::Item, String)> = BTreeMap::new();
        for it in items {
            if it.disc.map(|d| d > 1).unwrap_or(false) {
                continue;
            }
            let title = title_of(it);
            let key = title.to_lowercase();
            match best.get(&key) {
                Some((cur, _)) if rank(cur) <= rank(it) => {}
                _ => {
                    best.insert(key, (it, title));
                }
            }
        }
        let mut games: Vec<Game> = best
            .into_values()
            .map(|(it, title)| Game {
                title,
                path: it.path.clone(),
                crt_path: None,
                folder: false,
            })
            .collect();
        games.sort_by_key(|a| a.title.to_lowercase());
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

    /// Curated lists: `~/.config/omarchy-crt/collections/<Name>.txt`, one
    /// absolute game path per line. Returns (name, [(system index, path)]).
    fn load_collections(&self) -> Vec<(String, Vec<(usize, PathBuf)>)> {
        let dir = self.config_dir.join("collections");
        let Ok(rd) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for e in rd.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("txt") {
                continue;
            }
            let name = path
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let items: Vec<(usize, PathBuf)> = text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .filter_map(|l| {
                    let p = PathBuf::from(l);
                    self.system_of(&p).map(|i| (i, p))
                })
                .collect();
            out.push((name, items));
        }
        out.sort_by_key(|a| a.0.to_lowercase());
        out
    }

    /// Index of the system a game file belongs to: from the index when it
    /// knows the file, else from the folder the file sits in.
    pub fn system_of(&self, path: &Path) -> Option<usize> {
        if let Some(&i) = self.path_system.get(path) {
            return Some(i);
        }
        self.systems
            .iter()
            .position(|s| !s.dir.is_empty() && path.starts_with(expand(&s.dir)))
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
        folders.sort_by_key(|a| a.title.to_lowercase());
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
            if counts[&g.title.to_lowercase()] > 1
                && let Some(tag) = first_tag(&g.path)
            {
                g.title = format!("{} ({tag})", g.title);
            }
        }
        games.sort_by_key(|a| a.title.to_lowercase());
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
    fn launch_keys(&self, system: &System, cores_cfg: &Path, extra: &str, resume: bool) -> String {
        let mut out = VideoPolicy::parse(&system.video).retroarch_keys(self.switching);
        {
            let mut kv = |k: &str, v: &str| out.push_str(&format!("{k} = \"{v}\"\n"));
            // Pad profiles come from the directory the launcher keeps filled,
            // written here rather than in `retroarch.cfg` so that a config
            // from an older version cannot point somewhere empty. RetroArch
            // adds the driver's own subdirectory to this path.
            if let Some(dir) = crate::padmap::autoconfig_dir().parent() {
                kv("joypad_autoconfig_dir", &dir.display().to_string());
                kv("input_autodetect_enable", "true");
            }
            // The emulator's own rumble volume: nothing to feel without it,
            // and nothing to gain from it when the pad cannot shake.
            kv(
                "input_rumble_gain",
                if crate::rumble::pad_can_rumble() {
                    "100"
                } else {
                    "0"
                },
            );
            if system.runahead > 0 {
                kv("run_ahead_enabled", "true");
                kv("run_ahead_frames", &system.runahead.to_string());
                // One core instance: the secondary one is torn down at exit in
                // a way that crashes several cores (SIGSEGV after "Unloading
                // core"), and run-ahead works without it.
                kv("run_ahead_secondary_instance", "false");
            } else {
                kv("run_ahead_enabled", "false");
            }
            // No network command interface: processing a datagram crashes
            // RetroArch 1.22 (SIGSEGV in the input poll). The launcher
            // presses hotkeys through the tube's compositor instead.
            kv("network_cmd_enable", "false");
            // Auto load and save of the per game state: a game resumes where
            // it was left, and the pause menu's save is an explicit copy.
            // `resume` is false when the player asked for a fresh start: the
            // state on disk is left alone, this run just does not read it.
            kv("savestate_auto_save", "true");
            kv("savestate_auto_load", if resume { "true" } else { "false" });
            kv("quit_on_close_content", "1");
            // A plain window the compositor floats and pins over the tube, not
            // a fullscreen one (see crt::output::window_rules); its size
            // comes with the geometry keys the launcher appends.
            kv("video_fullscreen", "false");
            kv("video_windowed_fullscreen", "false");
            kv("video_window_custom_size_enable", "true");
            kv("video_window_save_positions", "false");
            kv("video_window_show_decorations", "false");
            // A hotkey and a pad combo pause the game and could open an
            // in-game overlay; RGUI does not render on this gl/wayland
            // super-resolution path (it opens and pauses but draws nothing),
            // so the launcher will draw its own pause screen. Meanwhile the
            // network command interface above drives pause, save, load and
            // quit, and the pad combo is reserved so nothing quits by
            // accident.
            kv("menu_toggle_gamepad_combo", "0");
            kv("input_menu_toggle", "nul");
            kv("input_quit_gamepad_combo", "0");
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
        self.command_resuming(system, game, extra, true)
    }

    /// The same, saying whether the run should pick up the state left behind.
    pub fn command_resuming(
        &self,
        system: &System,
        game: &Game,
        extra: &str,
        resume: bool,
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
                crate::player::command("mpv", &self.mpv_socket(), &input_conf, &osd, colors);
            cmd.args(fit_args);
            crate::player::add_target(&mut cmd, file);
            return Ok(cmd);
        }
        // A core that needs files nobody ships with it gets them now, once.
        if let Some(note) = crate::coredata::ensure(&system.core, &crate::coredata::system_dir()) {
            eprintln!("{note}");
        }
        let cfg = self.retroarch_config()?;
        let cores_cfg = self.config_dir.join("cores.cfg");
        let mut options = String::new();
        // Vibration first, so anything written by hand in systems.toml has the
        // last word: the emulator keeps the value it reads last.
        let rumbles = crate::rumble::pad_can_rumble();
        if rumbles {
            for (k, v) in crate::rumble::options(&system.name) {
                options.push_str(&format!("{k} = \"{v}\"\n"));
            }
        }
        for (k, v) in &system.options {
            options.push_str(&format!("{k} = \"{v}\"\n"));
        }
        std::fs::write(&cores_cfg, options)?;
        let launch_cfg = self.config_dir.join("launch.cfg");
        std::fs::write(
            &launch_cfg,
            self.launch_keys(system, &cores_cfg, extra, resume),
        )?;
        let mut cmd = std::process::Command::new(&self.retroarch);
        cmd.arg("--config")
            .arg(cfg)
            .arg("--appendconfig")
            .arg(launch_cfg);
        for d in &system.devices {
            cmd.arg(format!("--device={d}"));
        }
        // A PlayStation only shakes when the pad it is given has the motors:
        // a device type, not a core option. A system that names its own
        // devices has already said what it wants.
        if rumbles && system.devices.is_empty() {
            for d in crate::rumble::devices(&system.name) {
                cmd.arg(format!("--device={d}"));
            }
        }
        cmd.arg("-L")
            .arg(self.core_path(system))
            .arg(&game.path)
            .arg("--verbose")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(game_log());
        Ok(cmd)
    }
}

/// True when the emulator's log shows it had already torn the game down
/// when it died: an exit-time crash, not a game crash.
pub fn exited_after_unload() -> bool {
    std::fs::read_to_string(game_log_path())
        .map(|t| t.contains("Unloading core"))
        .unwrap_or(false)
}

/// Where the emulator's own output goes, one file per run, so a crash on the
/// tube leaves something to read: `~/.local/state/omarchy-crt/game.log`.
pub fn game_log_path() -> PathBuf {
    crate::crt::state_dir().join("game.log")
}

fn game_log() -> std::process::Stdio {
    let path = game_log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match std::fs::File::create(&path) {
        Ok(f) => std::process::Stdio::from(f),
        Err(_) => std::process::Stdio::null(),
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
joypad_autoconfig_dir = "/usr/share/libretro/autoconfig"
input_max_users = "4"
"#;

/// Change one key (`core` or `dir`) of a system in `systems.toml`, adding the
/// `[[system]]` block from the catalogue when the system only lives in the
/// index. The file is ours (the header says so), so it is rewritten whole;
/// per system option tables survive the round trip.
pub fn set_system_field(system: &str, key: &str, value: &str) -> Result<(), String> {
    if !matches!(key, "core" | "dir") {
        return Err(format!("{key}: only core and dir can be set"));
    }
    let path = default_path();
    let text = crate::config::read(&path).unwrap_or_default();
    let mut root: toml::Value = if text.trim().is_empty() {
        toml::Value::Table(Default::default())
    } else {
        toml::from_str(&text).map_err(|e| format!("systems.toml: {e}"))?
    };
    let table = root.as_table_mut().ok_or("systems.toml: not a table")?;
    let list = table
        .entry("system")
        .or_insert_with(|| toml::Value::Array(Vec::new()));
    let arr = list
        .as_array_mut()
        .ok_or("systems.toml: system is not a list")?;
    let found = arr
        .iter_mut()
        .find(|v| v.get("name").and_then(toml::Value::as_str) == Some(system));
    let entry = match found {
        Some(e) => e,
        None => {
            let (_, core, exts) = crate::index::catalog(system)
                .ok_or_else(|| format!("{system} is not in the catalogue"))?;
            let mut t = toml::map::Map::new();
            t.insert("name".into(), toml::Value::String(system.into()));
            t.insert("dir".into(), toml::Value::String(String::new()));
            t.insert("core".into(), toml::Value::String(core.into()));
            t.insert(
                "extensions".into(),
                toml::Value::Array(
                    exts.iter()
                        .map(|e| toml::Value::String(e.to_string()))
                        .collect(),
                ),
            );
            t.insert("video".into(), toml::Value::String("super".into()));
            arr.push(toml::Value::Table(t));
            arr.last_mut().unwrap()
        }
    };
    let t = entry
        .as_table_mut()
        .ok_or("systems.toml: bad system entry")?;
    t.insert(key.into(), toml::Value::String(value.into()));
    // The version is written by hand at the top: a plain key has to come
    // before the `[[system]]` tables, which is not the order the map would
    // serialize it in.
    if let Some(t) = root.as_table_mut() {
        t.remove("version");
    }
    let body = toml::to_string_pretty(&root).map_err(|e| e.to_string())?;
    let out = format!(
        "# Written by omarchy-crt. Each [[system]] maps a ROM folder to a core.\n\
         version = {}\n{body}",
        crate::config::VERSION
    );
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    crate::store::save(&path, out).map_err(|e| e.to_string())
}

/// Cores installed in the core directory, by their short names.
pub fn installed_cores(core_dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(core_dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let n = e.file_name().to_string_lossy().to_string();
                    n.strip_suffix("_libretro.so").map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// The picture the running core is drawing, read from the emulator's own log.
///
/// RetroArch with `--verbose` announces the geometry it was given when a game
/// starts, and again whenever the core changes it: a PlayStation menu going to
/// 480 lines, a Saturn game switching between 224 and 240. Both lines carry
/// the same shape, and the last one in the file is the truth:
///
/// ```text
/// [INFO] [Core] Geometry: 640x480, Aspect: 1.333, FPS: 59.95, ...
/// [INFO] [Environ] SET_GEOMETRY: 640x480, Aspect: 1.333.
/// ```
///
/// This is how the tube follows a game rather than a table: the launcher polls
/// it while a game runs and asks for that many lines. Nothing else in
/// RetroArch will say it without a network command, and those crash the
/// emulator often enough to be worth avoiding.
pub fn core_geometry(log: &str) -> Option<(u32, u32)> {
    let mut found = None;
    for line in log.lines() {
        let Some(rest) = line
            .split_once("SET_GEOMETRY:")
            .or_else(|| line.split_once("Geometry:"))
            .map(|(_, r)| r.trim())
        else {
            continue;
        };
        let size = rest.split(',').next().unwrap_or("").trim();
        let Some((w, h)) = size.split_once('x') else {
            continue;
        };
        if let (Ok(w), Ok(h)) = (w.trim().parse::<u32>(), h.trim().parse::<u32>())
            && (1..=4096).contains(&w)
            && (1..=1200).contains(&h)
        {
            found = Some((w, h));
        }
    }
    found
}
