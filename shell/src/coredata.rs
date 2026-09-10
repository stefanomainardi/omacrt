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

fn extra_for(core: &str) -> Option<&'static Extra> {
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

    // Everything happens in a directory this project owns. The archive used
    // to land on a predictable name in /tmp, which another user on the same
    // machine can pre-create as a symlink: curl follows it, and whatever was
    // at the other end got unpacked into RetroArch's system directory. A
    // staging directory under the cache is not writable by anybody else, and
    // it also means a half unpacked archive never touches the real one.
    let work = crate::crt::cache_dir().join("cores");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).ok()?;
    let tmp = work.join(format!("{}.zip", extra.core));
    let staged = work.join(extra.core);

    let done = (|| {
        let ok = crate::net::curl_https_redirect(300, 134_217_728)
            .arg("-o")
            .arg(&tmp)
            .arg("--")
            .arg(extra.url)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            return Err(format!("{}: could not be downloaded", extra.what));
        }
        std::fs::create_dir_all(&staged).map_err(|e| format!("{}: {e}", extra.what))?;
        // What the archive says it holds, before anything is written: an
        // entry naming an absolute path or climbing out with `..` would put
        // a file wherever it liked, and this one is unpacked next door to
        // another program's data.
        if !names_stay_inside(&tmp) {
            return Err(format!(
                "{}: the archive names a path outside itself",
                extra.what
            ));
        }
        if !unpack(&tmp, &staged) {
            return Err(format!("{}: could not be unpacked", extra.what));
        }
        // The archive is trusted by where it comes from rather than by a
        // digest: it is rebuilt by libretro's buildbot, so a pinned hash would
        // go stale and turn into a permanent failure. What is checked instead
        // is that the unpacked tree really is the thing that was asked for,
        // before any of it is put where RetroArch will read it.
        if !staged.join(extra.marker).exists() {
            return Err(format!("{}: the archive did not hold it", extra.what));
        }
        move_into(&staged, system_dir).map_err(|e| format!("{}: {e}", extra.what))?;
        Ok(format!("{} installed", extra.what))
    })();

    let _ = std::fs::remove_dir_all(&work);
    Some(done.unwrap_or_else(|e| e))
}

/// Move the staged tree into place, directory by directory, without removing
/// anything already there that the archive does not carry.
fn move_into(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            move_into(&entry.path(), &dest)?;
        } else {
            std::fs::rename(entry.path(), &dest)
                .or_else(|_| std::fs::copy(entry.path(), &dest).map(|_| ()))?;
        }
    }
    Ok(())
}

/// True when every name in the archive stays inside it.
///
/// The unpacking tools mostly refuse a traversing entry themselves, but they
/// are three different programs chosen by whatever is installed, and the
/// destination is beside RetroArch's own files. Reading the list first costs
/// one process and does not depend on which tool wins.
///
/// An archive whose list cannot be read at all is refused: not being able to
/// look is not the same as having looked.
fn names_stay_inside(archive: &Path) -> bool {
    let listing = [
        ("bsdtar", vec!["-tf"]),
        ("unzip", vec!["-Z1"]),
        ("7z", vec!["l", "-ba", "-slt"]),
    ]
    .iter()
    .find_map(|(bin, args)| {
        let out = std::process::Command::new(bin)
            .args(args)
            .arg(archive)
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    });
    let Some(listing) = listing else {
        return false;
    };
    listing.lines().all(|line| {
        let name = line.strip_prefix("Path = ").unwrap_or(line).trim();
        if name.is_empty() {
            return true;
        }
        !name.starts_with('/')
            && !name.starts_with("\\")
            && !name.contains(':')
            && !Path::new(name)
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
    })
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
        let dir = std::env::temp_dir().join(format!("omacrt-extra-{}", std::process::id()));
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

    use std::io::Write;

    /// Write a zip with the given entry names, using the store method so no
    /// compressor is needed.
    fn zip_with(path: &Path, names: &[&str]) {
        let mut central = Vec::new();
        let mut out = Vec::new();
        for name in names {
            let offset = out.len() as u32;
            let n = name.as_bytes();
            out.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]);
            out.extend_from_slice(&[10, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            out.extend_from_slice(&[0, 0, 0, 0]); // crc of nothing
            out.extend_from_slice(&0u32.to_le_bytes()); // compressed
            out.extend_from_slice(&0u32.to_le_bytes()); // uncompressed
            out.extend_from_slice(&(n.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(n);

            central.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]);
            central.extend_from_slice(&[20, 0, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            central.extend_from_slice(&[0, 0, 0, 0]);
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&(n.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(n);
        }
        let central_at = out.len() as u32;
        let count = names.len() as u16;
        out.extend_from_slice(&central);
        out.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&central_at.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        std::fs::File::create(path)
            .expect("create")
            .write_all(&out)
            .expect("write");
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("omacrt-{}-{name}", std::process::id()))
    }

    #[test]
    fn an_ordinary_archive_is_accepted() {
        let p = tmp("good.zip");
        zip_with(
            &p,
            &[
                "dolphin-emu/Sys/codehandler.bin",
                "dolphin-emu/Sys/GC/font.bin",
            ],
        );
        assert!(names_stay_inside(&p));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn an_archive_that_climbs_out_is_refused() {
        for bad in [
            "../../../../tmp/escaped.txt",
            "/etc/absolute.txt",
            "dolphin-emu/../../escaped",
        ] {
            let p = tmp("evil.zip");
            zip_with(&p, &["dolphin-emu/Sys/codehandler.bin", bad]);
            assert!(!names_stay_inside(&p), "{bad} was accepted");
            let _ = std::fs::remove_file(&p);
        }
    }

    /// A file no tool can list is not an empty archive.
    #[test]
    fn an_unreadable_archive_is_refused() {
        let p = tmp("junk.zip");
        std::fs::write(&p, b"not a zip at all").expect("write");
        assert!(!names_stay_inside(&p));
        let _ = std::fs::remove_file(&p);
    }
}
