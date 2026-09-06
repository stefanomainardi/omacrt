//! ROM folders: scan the library and adopt an existing collection laid out
//! the RePlayOS or Batocera way without copying anything.

use crate::library::{Library, System, expand};
use std::path::Path;

/// Folder names used by other front ends, mapped to our system names and
/// the core that runs them here. Only systems with a core installed on Arch
/// are listed.
pub const ALIASES: &[(&str, &str, &str, &[&str])] = &[
    // (foreign folder, our system, core, extensions)
    (
        "nintendo_nes",
        "nes",
        "mesen",
        &["nes", "fds", "unf", "unif", "zip"],
    ),
    ("nes", "nes", "mesen", &["nes", "fds", "unf", "unif", "zip"]),
    (
        "nintendo_snes",
        "snes",
        "snes9x",
        &["sfc", "smc", "swc", "fig", "zip"],
    ),
    (
        "snes",
        "snes",
        "snes9x",
        &["sfc", "smc", "swc", "fig", "zip"],
    ),
    (
        "sega_md",
        "megadrive",
        "genesis_plus_gx",
        &["md", "smd", "gen", "bin", "zip"],
    ),
    (
        "megadrive",
        "megadrive",
        "genesis_plus_gx",
        &["md", "smd", "gen", "bin", "zip"],
    ),
    (
        "sega_ms",
        "mastersystem",
        "genesis_plus_gx",
        &["sms", "zip"],
    ),
    (
        "mastersystem",
        "mastersystem",
        "genesis_plus_gx",
        &["sms", "zip"],
    ),
    ("sega_gg", "gamegear", "genesis_plus_gx", &["gg", "zip"]),
    (
        "sega_cd",
        "segacd",
        "genesis_plus_gx",
        &["cue", "chd", "iso", "m3u"],
    ),
    (
        "segacd",
        "segacd",
        "genesis_plus_gx",
        &["cue", "chd", "iso", "m3u"],
    ),
    (
        "sega_32x",
        "sega32x",
        "picodrive",
        &["32x", "chd", "m3u", "zip"],
    ),
    (
        "sega_st",
        "saturn",
        "kronos",
        &["cue", "ccd", "chd", "toc", "m3u"],
    ),
    (
        "saturn",
        "saturn",
        "kronos",
        &["cue", "ccd", "chd", "toc", "m3u"],
    ),
    (
        "sega_dc",
        "dreamcast",
        "flycast",
        &["chd", "cdi", "gdi", "cue", "m3u"],
    ),
    (
        "dreamcast",
        "dreamcast",
        "flycast",
        &["chd", "cdi", "gdi", "cue", "m3u"],
    ),
    (
        "nec_pce",
        "pcengine",
        "mednafen_pce_fast",
        &["pce", "sgx", "zip"],
    ),
    (
        "pcengine",
        "pcengine",
        "mednafen_pce_fast",
        &["pce", "sgx", "zip"],
    ),
    (
        "nec_pcecd",
        "pcenginecd",
        "mednafen_pce",
        &["cue", "ccd", "chd", "m3u"],
    ),
    (
        "nintendo_n64",
        "n64",
        "mupen64plus_next",
        &["n64", "v64", "z64", "zip"],
    ),
    (
        "n64",
        "n64",
        "mupen64plus_next",
        &["n64", "v64", "z64", "zip"],
    ),
    ("nintendo_gb", "gb", "mgba", &["gb", "sgb", "zip"]),
    ("gb", "gb", "mgba", &["gb", "sgb", "zip"]),
    ("nintendo_gbc", "gbc", "mgba", &["gbc", "zip"]),
    ("gbc", "gbc", "mgba", &["gbc", "zip"]),
    ("nintendo_gba", "gba", "mgba", &["gba", "zip"]),
    ("gba", "gba", "mgba", &["gba", "zip"]),
    ("nintendo_ds", "nds", "melonds", &["nds", "zip"]),
    (
        "sony_psx",
        "psx",
        "mednafen_psx_hw",
        &["cue", "chd", "pbp", "m3u", "img", "iso"],
    ),
    (
        "psx",
        "psx",
        "mednafen_psx_hw",
        &["cue", "chd", "pbp", "m3u", "img", "iso"],
    ),
    ("sony_psp", "psp", "ppsspp", &["iso", "cso", "pbp", "chd"]),
    ("snk_ng", "neogeo", "fbneo", &["zip"]),
    ("neogeo", "neogeo", "fbneo", &["zip"]),
    ("arcade_fbneo", "arcade", "fbneo", &["zip"]),
    ("fbneo", "arcade", "fbneo", &["zip"]),
    ("arcade_mame", "mame", "mame", &["zip", "chd"]),
    ("mame", "mame", "mame", &["zip", "chd"]),
    (
        "commodore_c64",
        "c64",
        "vice_x64sc",
        &["d64", "t64", "tap", "prg", "crt", "g64", "m3u", "zip"],
    ),
    (
        "c64",
        "c64",
        "vice_x64sc",
        &["d64", "t64", "tap", "prg", "crt", "g64", "m3u", "zip"],
    ),
    (
        "commodore_ami",
        "amiga",
        "puae",
        &[
            "adf", "adz", "dms", "ipf", "hdf", "lha", "uae", "m3u", "zip",
        ],
    ),
    (
        "amiga",
        "amiga",
        "puae",
        &[
            "adf", "adz", "dms", "ipf", "hdf", "lha", "uae", "m3u", "zip",
        ],
    ),
    (
        "amstrad_cpc",
        "amstradcpc",
        "cap32",
        &["dsk", "sna", "tap", "cdt", "cpr", "m3u", "zip"],
    ),
    ("scummvm", "scummvm", "scummvm", &["scummvm", "svm"]),
];

pub struct Scan {
    pub name: String,
    pub dir: String,
    pub exists: bool,
    pub games: usize,
    pub unknown: usize,
    pub core_present: bool,
}

/// One row per system: folder, game count, files with unknown extensions,
/// whether the core is installed.
pub fn scan(lib: &Library) -> Vec<Scan> {
    lib.systems
        .iter()
        .map(|s| {
            let dir = expand(&s.dir);
            let exists = dir.is_dir();
            let games = lib.games(s).len();
            let unknown = if exists && !s.extensions.is_empty() {
                std::fs::read_dir(&dir)
                    .map(|rd| {
                        rd.filter_map(|e| e.ok())
                            .filter(|e| e.path().is_file())
                            .filter(|e| {
                                let p = e.path();
                                let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
                                !s.extensions.iter().any(|x| x.eq_ignore_ascii_case(ext))
                                    && !ext.eq_ignore_ascii_case("m3u")
                            })
                            .count()
                    })
                    .unwrap_or(0)
            } else {
                0
            };
            let core_present = s.is_video() || lib.core_path(s).exists();
            Scan {
                name: s.name.clone(),
                dir: dir.display().to_string(),
                exists,
                games,
                unknown,
                core_present,
            }
        })
        .collect()
}

pub struct Linked {
    pub folder: String,
    pub system: String,
    pub files: usize,
}

/// Systems for a foreign ROM collection: every known folder under `root`
/// that holds files becomes a system pointing at it. Systems already
/// defined keep their tuning and only change directory.
pub fn link(root: &Path, current: &[System]) -> (Vec<System>, Vec<Linked>) {
    let mut systems: Vec<System> = current.to_vec();
    let mut linked = Vec::new();
    for (folder, name, core, exts) in ALIASES {
        let dir = root.join(folder);
        if !dir.is_dir() {
            continue;
        }
        let files = std::fs::read_dir(&dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.path().is_file())
                    .count()
            })
            .unwrap_or(0);
        if files == 0 || linked.iter().any(|l: &Linked| l.system == *name) {
            continue;
        }
        let dir_text = dir.display().to_string();
        if let Some(existing) = systems.iter_mut().find(|s| s.name == *name) {
            existing.dir = dir_text;
        } else {
            systems.push(System {
                name: name.to_string(),
                dir: dir_text,
                core: core.to_string(),
                extensions: exts.iter().map(|e| e.to_string()).collect(),
                video: "super".into(),
                options: Default::default(),
                devices: Vec::new(),
                runahead: 0,
                rewind: false,
                analog_dpad: None,
                player: String::new(),
            });
        }
        linked.push(Linked {
            folder: folder.to_string(),
            system: name.to_string(),
            files,
        });
    }
    (systems, linked)
}

fn toml_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `systems.toml` text for a list of systems.
pub fn systems_toml(systems: &[System], switching: bool) -> String {
    let mut out =
        String::from("# Written by omarchy-crt. Each [[system]] maps a ROM folder to a core.\n");
    out.push_str(&format!("switching = {switching}\n"));
    for s in systems {
        out.push_str("\n[[system]]\n");
        out.push_str(&format!("name = {}\n", toml_str(&s.name)));
        out.push_str(&format!("dir = {}\n", toml_str(&s.dir)));
        out.push_str(&format!("core = {}\n", toml_str(&s.core)));
        let exts: Vec<String> = s.extensions.iter().map(|e| toml_str(e)).collect();
        out.push_str(&format!("extensions = [{}]\n", exts.join(", ")));
        if !s.video.is_empty() {
            out.push_str(&format!("video = {}\n", toml_str(&s.video)));
        }
        if s.runahead > 0 {
            out.push_str(&format!("runahead = {}\n", s.runahead));
        }
        if s.rewind {
            out.push_str("rewind = true\n");
        }
        if let Some(a) = s.analog_dpad {
            out.push_str(&format!("analog_dpad = {a}\n"));
        }
        if !s.player.is_empty() {
            out.push_str(&format!("player = {}\n", toml_str(&s.player)));
        }
        if !s.devices.is_empty() {
            let d: Vec<String> = s.devices.iter().map(|x| toml_str(x)).collect();
            out.push_str(&format!("devices = [{}]\n", d.join(", ")));
        }
        if !s.options.is_empty() {
            out.push_str("\n[system.options]\n");
            for (k, v) in &s.options {
                out.push_str(&format!("{k} = {}\n", toml_str(v)));
            }
        }
    }
    out
}
