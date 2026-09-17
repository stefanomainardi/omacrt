//! How many lines a game draws, remembered between launches.
//!
//! The launcher asks the tube for a frame before the emulator opens, and the
//! only number it has then is the system's own default: 224 for `mame`, 240
//! for `nes`. A console draws what its system draws, but an arcade board
//! draws whatever that board drew, and one default cannot be right for both
//! Double Dragon at 240 lines and Out Run at 224.
//!
//! What happened without this: the tube was put in a 224 line frame and the
//! emulator was laid out for it, the core said 240 a dozen seconds later, the
//! launcher followed it into a 240 line frame, and the emulator went on
//! drawing 224 lines at the top of it. Sixteen black lines at the bottom and
//! the top of the game pushed up under the overscan. Measured on ddragon on
//! 2026-09-17: `Geometry: 256x240` from the core against
//! `custom_viewport_height = "224"` in the launch config.
//!
//! Keyed by the game rather than by the system, because that is the thing
//! that has a height. One line per game actually played, written only when
//! the number changes.

use std::path::{Path, PathBuf};

fn path() -> PathBuf {
    crate::crt::state_dir().join("lines.tsv")
}

/// Lines a game may draw. Below the first a frame is not a frame, and above
/// the last no 15 kHz set will take it.
const RANGE: std::ops::RangeInclusive<u32> = 144..=1080;

fn load(file: &Path) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    let Ok(text) = std::fs::read_to_string(file) else {
        return out;
    };
    for line in text.lines() {
        let Some((key, lines)) = line.rsplit_once('\t') else {
            continue;
        };
        if let Ok(n) = lines.trim().parse::<u32>()
            && RANGE.contains(&n)
            && !key.is_empty()
        {
            out.push((key.to_string(), n));
        }
    }
    out
}

/// What this game drew last time, if it has run before.
pub fn known(game: &Path) -> Option<u32> {
    known_in(&path(), game)
}

fn known_in(file: &Path, game: &Path) -> Option<u32> {
    let key = game.to_string_lossy();
    load(file)
        .into_iter()
        .find(|(k, _)| *k == key)
        .map(|(_, n)| n)
}

/// Remember what a core reported for this game. Writes nothing when it is
/// already what is on file.
pub fn remember(game: &Path, lines: u32) {
    remember_in(&path(), game, lines);
}

fn remember_in(file: &Path, game: &Path, lines: u32) {
    if !RANGE.contains(&lines) {
        return;
    }
    let key = game.to_string_lossy().replace(['\t', '\n', '\r'], " ");
    if key.is_empty() {
        return;
    }
    let mut all = load(file);
    match all.iter_mut().find(|(k, _)| *k == key) {
        Some(slot) if slot.1 == lines => return,
        Some(slot) => slot.1 = lines,
        None => all.push((key, lines)),
    }
    all.sort_by(|a, b| a.0.cmp(&b.0));
    let text: String = all.iter().map(|(k, n)| format!("{k}\t{n}\n")).collect();
    let p = file;
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = p.with_extension("tsv.new");
    if std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, p).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file of this test's own, so nothing here depends on the process
    /// environment and the tests can run beside each other.
    fn file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("omacrt-shapes-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join(format!("{name}.tsv"));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn a_game_is_remembered_by_its_own_path() {
        let f = file("one");
        let dd = Path::new("/roms/arcade_mame/ddragon.zip");
        let outrun = Path::new("/roms/arcade_mame/outrun.zip");
        assert_eq!(known_in(&f, dd), None, "nothing known before it has run");
        remember_in(&f, dd, 240);
        remember_in(&f, outrun, 224);
        // Two games of one system, two heights. A default for the system
        // cannot be right for both, which is the whole reason for this.
        assert_eq!(known_in(&f, dd), Some(240));
        assert_eq!(known_in(&f, outrun), Some(224));
        // Written again with the same number changes nothing.
        remember_in(&f, dd, 240);
        assert_eq!(known_in(&f, dd), Some(240));
        // And a core that changes its mind is believed.
        remember_in(&f, dd, 256);
        assert_eq!(known_in(&f, dd), Some(256));
        let _ = std::fs::remove_file(&f);
    }

    #[test]
    fn a_number_that_cannot_be_a_frame_is_refused() {
        let f = file("two");
        let p = Path::new("/roms/x.zip");
        remember_in(&f, p, 0);
        remember_in(&f, p, 4000);
        assert_eq!(known_in(&f, p), None);
        remember_in(&f, p, 240);
        assert_eq!(known_in(&f, p), Some(240));
        let _ = std::fs::remove_file(&f);
    }

    #[test]
    fn a_path_with_a_tab_in_it_cannot_break_the_file() {
        let f = file("three");
        remember_in(&f, Path::new("/roms/a\tb.zip"), 240);
        // The tab became a space on the way in, so the file still has one
        // separator a line and the entry reads back under that name.
        assert_eq!(known_in(&f, Path::new("/roms/a b.zip")), Some(240));
        let _ = std::fs::remove_file(&f);
    }
}
