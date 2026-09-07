//! BIOS files the libretro cores expect in RetroArch's system directory,
//! keyed by the ROM folder they belong to.

use super::{config_dir, home};
use std::path::{Path, PathBuf};

pub struct Entry {
    pub system: &'static str,
    pub file: &'static str,
    pub required: bool,
    pub description: &'static str,
}

const E: fn(&'static str, &'static str, bool, &'static str) -> Entry =
    |system, file, required, description| Entry {
        system,
        file,
        required,
        description,
    };

/// Minimum files per system. Paths are relative to the system directory,
/// which mirrors the layout RetroArch cores document.
pub fn table() -> Vec<Entry> {
    vec![
        E("psx", "scph5501.bin", true, "PlayStation US BIOS"),
        E("psx", "scph5500.bin", true, "PlayStation JP BIOS"),
        E("psx", "scph5502.bin", true, "PlayStation EU BIOS"),
        E("segacd", "bios_CD_U.bin", true, "Mega CD US"),
        E("segacd", "bios_CD_E.bin", true, "Mega CD EU"),
        E("segacd", "bios_CD_J.bin", true, "Mega CD JP"),
        E("saturn", "sega_101.bin", true, "Saturn JP BIOS"),
        E("saturn", "mpr-17933.bin", true, "Saturn US and EU BIOS"),
        E(
            "pcenginecd",
            "syscard3.pce",
            true,
            "PC Engine CD System Card 3",
        ),
        E(
            "pcenginecd",
            "syscard1.pce",
            false,
            "PC Engine CD System Card 1",
        ),
        E(
            "pcenginecd",
            "syscard2.pce",
            false,
            "PC Engine CD System Card 2",
        ),
        E(
            "pcenginecd",
            "gexpress.pce",
            false,
            "PC Engine Games Express card",
        ),
        E("dreamcast", "dc/dc_boot.bin", true, "Dreamcast boot ROM"),
        E("dreamcast", "dc/dc_flash.bin", false, "Dreamcast flash"),
        E("dreamcast", "dc/naomi.zip", false, "Naomi BIOS (arcade)"),
        E(
            "neogeo",
            "fbneo/neogeo.zip",
            true,
            "Neo Geo BIOS set for FBNeo",
        ),
        E("neogeocd", "neocd/neocd_z.rom", true, "Neo Geo CD BIOS"),
        E("gba", "gba_bios.bin", false, "Game Boy Advance BIOS"),
        E("gb", "gb_bios.bin", false, "Game Boy boot ROM"),
        E("gbc", "gbc_bios.bin", false, "Game Boy Color boot ROM"),
        E("nes", "disksys.rom", false, "Famicom Disk System"),
        E("mastersystem", "bios.sms", false, "Master System BIOS"),
        E("gamegear", "bios.gg", false, "Game Gear BIOS"),
        E("atari5200", "5200.rom", true, "Atari 5200 BIOS"),
        E("atari7800", "7800 BIOS (U).rom", true, "Atari 7800 BIOS"),
        E("lynx", "lynxboot.img", true, "Atari Lynx boot ROM"),
        E("3do", "panafz10.bin", true, "Panasonic FZ-10 BIOS"),
        E("amiga", "kick34005.A500", true, "Kickstart 1.3 (A500)"),
        E("amiga", "kick40068.A1200", true, "Kickstart 3.1 (A1200)"),
        E("amiga", "kick40060.CD32", false, "Kickstart 3.1 (CD32)"),
        E("amiga", "kick40060.CD32.ext", false, "CD32 extended ROM"),
        E(
            "msx",
            "Machines/Shared Roms/MSX2.ROM",
            true,
            "MSX2 main ROM (blueMSX)",
        ),
        E("x68000", "keropi/iplrom.dat", true, "Sharp X68000 IPL ROM"),
        E("x68000", "keropi/cgrom.dat", true, "Sharp X68000 font ROM"),
    ]
}

/// Whole folders worth copying alongside the table when importing a
/// complete BIOS collection.
pub const FOLDERS: &[&str] = &[
    "dc",
    "fbneo",
    "neocd",
    "keropi",
    "Machines",
    "same_cdi",
    "melonDS DS",
    "fuse",
    "scummvm",
    "PPSSPP",
    "mame",
    "mame2003-plus",
    "Mupen64plus",
    "hatari",
];

/// RetroArch's `system_directory`, read from our config first.
pub fn system_dir() -> PathBuf {
    for cfg in [
        config_dir().join("retroarch.cfg"),
        home().join(".config/retroarch/retroarch.cfg"),
    ] {
        if let Ok(text) = std::fs::read_to_string(&cfg) {
            for line in text.lines() {
                if let Some(rest) = line.strip_prefix("system_directory") {
                    let v = rest.trim_start_matches([' ', '=']).trim().trim_matches('"');
                    if let Some(r) = v.strip_prefix("~/") {
                        return home().join(r);
                    }
                    return PathBuf::from(v);
                }
            }
        }
    }
    home().join(".config/retroarch/system")
}

pub struct Item {
    pub system: String,
    pub file: String,
    pub required: bool,
    pub description: String,
    pub present: bool,
    /// The ROM folder of this system exists, so the file matters.
    pub relevant: bool,
}

pub struct Report {
    pub system_dir: PathBuf,
    pub items: Vec<Item>,
}

impl Report {
    pub fn missing(&self) -> Vec<&Item> {
        self.items
            .iter()
            .filter(|i| i.relevant && i.required && !i.present)
            .collect()
    }

    pub fn relevant_required(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.relevant && i.required)
            .count()
    }
}

/// Check the table against the system directory. `systems` are the ROM
/// folder names that exist in the library.
pub fn report(systems: &[String]) -> Report {
    let dir = system_dir();
    let items = table()
        .into_iter()
        .map(|e| Item {
            present: dir.join(e.file).exists(),
            relevant: systems.iter().any(|s| s == e.system),
            system: e.system.into(),
            file: e.file.into(),
            required: e.required,
            description: e.description.into(),
        })
        .collect();
    Report {
        system_dir: dir,
        items,
    }
}

fn copy_file(src: &Path, dst: &Path) -> std::io::Result<bool> {
    if let (Ok(a), Ok(b)) = (src.metadata(), dst.metadata()) {
        if a.len() == b.len() {
            return Ok(false);
        }
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(src, dst)?;
    Ok(true)
}

fn copy_tree(src: &Path, dst: &Path, copied: &mut usize) -> std::io::Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target, copied)?;
        } else if copy_file(&entry.path(), &target)? {
            *copied += 1;
        }
    }
    Ok(())
}

/// Copy BIOS files from another collection (a RePlayOS or Batocera `bios`
/// folder) into the system directory. Table files always; with `all` the
/// known folders too. Returns (copied, skipped as already present).
pub fn import(src: &Path, all: bool) -> std::io::Result<(usize, usize)> {
    let dir = system_dir();
    let mut copied = 0;
    let mut skipped = 0;
    for e in table() {
        let from = src.join(e.file);
        if from.is_file() {
            if copy_file(&from, &dir.join(e.file))? {
                copied += 1;
            } else {
                skipped += 1;
            }
        }
    }
    if all {
        for folder in FOLDERS {
            let from = src.join(folder);
            if from.is_dir() {
                copy_tree(&from, &dir.join(folder), &mut copied)?;
            }
        }
        for entry in std::fs::read_dir(src)?.flatten() {
            if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                let name = entry.file_name();
                if copy_file(&entry.path(), &dir.join(&name))? {
                    copied += 1;
                }
            }
        }
    }
    Ok((copied, skipped))
}

/// Folders that look like a BIOS collection: `bios`, `BIOS` or `system`
/// under the given places (library roots, discovered disks), holding at
/// least one file the table knows. `(folder, known files in it)`.
pub fn discover(places: &[PathBuf]) -> Vec<(PathBuf, usize)> {
    let known: Vec<String> = table()
        .iter()
        .map(|e| e.file.rsplit('/').next().unwrap_or(e.file).to_ascii_lowercase())
        .collect();
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for place in places {
        let mut cands = vec![place.clone()];
        for name in ["bios", "BIOS", "Bios", "system", "System", "systems"] {
            cands.push(place.join(name));
            cands.push(place.join("roms").join(name));
        }
        if let Some(parent) = place.parent() {
            for name in ["bios", "BIOS", "system"] {
                cands.push(parent.join(name));
            }
        }
        for c in cands {
            let Ok(canon) = std::fs::canonicalize(&c) else {
                continue;
            };
            // Case insensitive disks answer to `bios` and `BIOS` alike: one
            // folder, counted once, by device and inode.
            let Ok(meta) = std::fs::metadata(&canon) else {
                continue;
            };
            let id = {
                use std::os::unix::fs::MetadataExt;
                (meta.dev(), meta.ino())
            };
            if canon == system_dir() || !seen.insert(id) {
                continue;
            }
            let Ok(rd) = std::fs::read_dir(&canon) else {
                continue;
            };
            let hits = rd
                .flatten()
                .take(2000)
                .filter(|e| {
                    let n = e.file_name().to_string_lossy().to_ascii_lowercase();
                    known.contains(&n)
                })
                .count();
            if hits > 0 {
                out.push((canon, hits));
            }
        }
    }
    out.sort_by(|a, b| b.1.cmp(&a.1));
    out
}
