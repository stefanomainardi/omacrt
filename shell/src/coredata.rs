//! Files a core needs that nobody ships with it.
//!
//! A few libretro cores are useless on their own: Dolphin wants its `Sys`
//! folder for shaders, fonts, per game settings and the cheat handler, and the
//! Arch package contains one shared object and nothing else. RetroArch has a
//! menu for this, buried under an online updater, which is not a thing to go
//! looking for from a sofa with a pad in hand.
//!
//! These are not BIOS files. They are freely redistributable parts of the
//! emulator, published by the libretro project at a known address, so the
//! launcher fetches them once when they are missing and never mentions them
//! again. Anything that has to come off the user's own console stays in
//! `crt::bios`.

use std::path::{Path, PathBuf};

pub struct Extra {
    /// Core short name, as in `systems.toml`.
    pub core: &'static str,
    /// Archive on the libretro asset server.
    pub url: &'static str,
    /// A file that exists once the archive is unpacked, relative to
    /// RetroArch's system directory.
    pub marker: &'static str,
    pub what: &'static str,
}

pub const TABLE: &[Extra] = &[Extra {
    core: "dolphin",
    url: "https://buildbot.libretro.com/assets/system/Dolphin.zip",
    marker: "dolphin-emu/Sys/codehandler.bin",
    what: "Dolphin's Sys folder: shaders, fonts and per game settings",
}];

pub fn extra_for(core: &str) -> Option<&'static Extra> {
    TABLE.iter().find(|e| e.core == core)
}

/// True when this core is missing the files it needs.
pub fn missing(core: &str, system_dir: &Path) -> bool {
    extra_for(core).is_some_and(|e| !system_dir.join(e.marker).exists())
}

/// Fetch and unpack what a core needs, if it is not already there. Returns a
/// line worth logging when something was done.
pub fn ensure(core: &str, system_dir: &Path) -> Option<String> {
    let extra = extra_for(core)?;
    if system_dir.join(extra.marker).exists() {
        return None;
    }
    std::fs::create_dir_all(system_dir).ok()?;
    let tmp = std::env::temp_dir().join(format!(
        "omarchy-crt-{}-{}.zip",
        extra.core,
        std::process::id()
    ));
    let ok = std::process::Command::new("curl")
        .args([
            "-fsSL",
            // The same leash as every other fetch: two protocols, a size cap,
            // a timeout, and the URL as an argument rather than a shell word.
            "--proto",
            "=http,https",
            "--proto-redir",
            "=http,https",
            "--max-filesize",
            "134217728",
            "--max-time",
            "300",
            "--retry",
            "1",
            "-o",
        ])
        .arg(&tmp)
        .arg(extra.url)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        let _ = std::fs::remove_file(&tmp);
        return Some(format!("{}: could not be downloaded", extra.what));
    }
    let unpacked = unpack(&tmp, system_dir);
    let _ = std::fs::remove_file(&tmp);
    if !unpacked {
        return Some(format!("{}: could not be unpacked", extra.what));
    }
    Some(format!("{} installed", extra.what))
}

/// Unpack a zip with whichever of the usual tools is on the machine.
fn unpack(archive: &Path, into: &Path) -> bool {
    for (bin, args) in [
        ("bsdtar", vec!["-xf"]),
        ("unzip", vec!["-qo"]),
        ("7z", vec!["x", "-y", "-bso0"]),
    ] {
        let mut cmd = std::process::Command::new(bin);
        cmd.args(&args).arg(archive);
        match bin {
            "bsdtar" => {
                cmd.arg("-C").arg(into);
            }
            "unzip" => {
                cmd.arg("-d").arg(into);
            }
            _ => {
                cmd.arg(format!("-o{}", into.display()));
            }
        }
        if cmd
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return true;
        }
    }
    false
}

/// RetroArch's system directory, where all of this belongs.
pub fn system_dir() -> PathBuf {
    crate::crt::home().join(".config/retroarch/system")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_cores_that_need_something_have_an_entry() {
        assert!(extra_for("dolphin").is_some());
        assert!(extra_for("snes9x").is_none());
    }

    #[test]
    fn a_core_with_its_files_in_place_is_not_missing_them() {
        let dir = std::env::temp_dir().join(format!("omarchy-crt-extra-{}", std::process::id()));
        let marker = dir.join("dolphin-emu/Sys/codehandler.bin");
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        assert!(missing("dolphin", &dir), "nothing there yet");
        std::fs::write(&marker, b"x").unwrap();
        assert!(!missing("dolphin", &dir));
        assert!(ensure("dolphin", &dir).is_none(), "nothing to do");
        assert!(!missing("snes9x", &dir), "a core with no extras never is");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_url_is_https_and_every_marker_is_a_relative_path() {
        for e in TABLE {
            assert!(e.url.starts_with("https://"), "{}", e.url);
            assert!(!e.marker.starts_with('/'), "{}", e.marker);
            assert!(!e.marker.contains(".."), "{}", e.marker);
        }
    }
}
