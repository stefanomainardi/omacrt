//! The version stamped on the files this project writes.
//!
//! `settings.toml` and `systems.toml` are read by one version of the launcher
//! and written back by whichever version happens to run next. Serde fills in
//! the keys an older file does not have, so reading forwards has always been
//! safe. Reading *backwards* is not: a file written by a newer version carries
//! keys this build has never heard of, and saving would quietly drop them.
//!
//! So every file we write carries `version`, and every file we read is checked
//! against [`VERSION`]. A file from the future is copied aside before we touch
//! it, under the version that wrote it, so nothing a newer build stored is lost
//! for good. A file from the past is simply read and stamped on the next save.

use std::path::Path;

/// The shape of the files this build writes. Raise it when a change cannot be
/// expressed as a new key with a default, and add the step to [`migrate`].
pub const VERSION: u32 = 1;

/// The `version` key of a config file, 0 when it predates versioning.
pub fn version_of(text: &str) -> u32 {
    for line in text.lines() {
        let line = line.trim();
        // Stop at the first table header: `version` is a top level key, and a
        // section of its own could hold something else called version.
        if line.starts_with('[') {
            break;
        }
        if let Some(rest) = line.strip_prefix("version")
            && let Some(v) = rest.trim_start().strip_prefix('=')
        {
            return v.trim().parse().unwrap_or(0);
        }
    }
    0
}

/// Bring the text of a config file up to [`VERSION`], and say whether anything
/// changed. Older files need no rewriting today, since every key added so far
/// has a default; the ladder is here for the first change that does not.
pub fn migrate(text: &mut String, from: u32) -> bool {
    let mut changed = false;
    let mut at = from;
    while at < VERSION {
        match at {
            // 0 to 1: versioning itself. Every key of the old shape is still
            // read, so the file only gains its stamp on the next save.
            0 => {}
            _ => break,
        }
        at += 1;
        changed = true;
    }
    let _ = text;
    changed
}

/// Keep a copy of a file written by a version this build does not understand,
/// so that going back to it loses nothing. Returns the path of the copy.
pub fn keep_newer(path: &Path, found: u32) -> Option<std::path::PathBuf> {
    if found <= VERSION {
        return None;
    }
    let mut aside = path.as_os_str().to_os_string();
    aside.push(format!(".v{found}"));
    let aside = std::path::PathBuf::from(aside);
    if aside.exists() {
        return Some(aside);
    }
    std::fs::copy(path, &aside).ok().map(|_| aside)
}

/// Read a config file through the durable store, run the migration ladder and
/// hand back the text to parse. A file from the future is copied aside first.
pub fn read(path: &Path) -> Option<String> {
    let mut text = crate::store::load_string(path)?;
    let found = version_of(&text);
    if found > VERSION {
        if let Some(aside) = keep_newer(path, found) {
            eprintln!(
                "omarchy-crt: {} was written by a newer version ({found}); \
                 a copy is kept at {}",
                path.display(),
                aside.display()
            );
        }
    } else if migrate(&mut text, found) {
        let _ = crate::store::save(path, &text);
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_key_is_read_from_the_top_of_the_file() {
        assert_eq!(version_of("version = 3\ntheme = \"system\"\n"), 3);
        assert_eq!(version_of("theme = \"system\"\nversion = 2\n"), 2);
        assert_eq!(version_of("theme = \"system\"\n"), 0);
        // A `version` inside a section is not the file's own.
        assert_eq!(version_of("[system.snes]\nversion = 9\n"), 0);
    }

    #[test]
    fn a_file_from_the_future_is_copied_aside_and_left_alone() {
        let dir = std::env::temp_dir().join(format!("omarchy-crt-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.toml");
        std::fs::write(&path, format!("version = {}\n", VERSION + 5)).unwrap();
        let text = read(&path).expect("the file still parses");
        assert!(text.contains("version"));
        assert!(
            dir.join(format!("settings.toml.v{}", VERSION + 5))
                .is_file(),
            "the newer file is kept"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
