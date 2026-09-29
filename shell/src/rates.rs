//! What field rate a core runs at, remembered between launches.
//!
//! A core says its own rate in its log a second or two after it starts, and
//! acting on it then costs a second mode change: 280 ms with nothing for the
//! converter to lock to, in the middle of a title screen. Remembering it
//! means the next launch of the same core and standard builds the rate into
//! the timing before the emulator opens, and there is one mode change instead
//! of two.
//!
//! Keyed two ways. By core and standard, because for a console the rate
//! belongs to the console and its region: Genesis Plus GX says 49.70 for
//! every European cartridge and 59.92 for every American one, so a game never
//! played before still starts at the right rate. And by game, because an
//! arcade board runs at whatever that board ran at: Mortal Kombat at 54.7,
//! R-Type at 55, most of the rest at 57.5 or 60. One line for MAME would be
//! the last game's rate for every game, and a set name says no region, so the
//! core's line was never even consulted for one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn path() -> PathBuf {
    crate::crt::state_dir().join("rates.tsv")
}

/// `core\tstandard` for the key, so the file is readable and sorts sensibly.
fn key(core: &str, standard: &str) -> String {
    // The same core is written three ways: `genesis_plus_gx` in a system's
    // configuration, and `/usr/lib/libretro/genesis_plus_gx_libretro.so` on
    // the command line the launcher builds. The reader and the writer see
    // different ones, so both are reduced to the bare name.
    let core = core.rsplit('/').next().unwrap_or(core);
    let core = core.strip_suffix(".so").unwrap_or(core);
    let core = core.strip_suffix("_libretro").unwrap_or(core);
    format!("{core}\t{standard}")
}

fn load() -> BTreeMap<String, f32> {
    load_from(&path())
}

fn load_from(file: &Path) -> BTreeMap<String, f32> {
    let mut out = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(file) else {
        return out;
    };
    for line in text.lines() {
        let mut parts = line.rsplitn(2, '\t');
        let (Some(hz), Some(k)) = (parts.next(), parts.next()) else {
            continue;
        };
        if let Ok(hz) = hz.trim().parse::<f32>()
            && (40.0..=90.0).contains(&hz)
        {
            out.insert(k.to_string(), hz);
        }
    }
    out
}

/// The rate this core ran at last time in this standard, if it has.
pub fn known(core: &str, standard: &str) -> Option<f32> {
    load().get(&key(core, standard)).copied()
}

fn games_path() -> PathBuf {
    crate::crt::state_dir().join("game-rates.tsv")
}

/// The rate this game ran at last time, if it has run before.
pub fn known_game(game: &Path) -> Option<f32> {
    known_game_in(&games_path(), game)
}

fn known_game_in(file: &Path, game: &Path) -> Option<f32> {
    load_from(file)
        .get(game.to_string_lossy().as_ref())
        .copied()
}

/// Remember the rate a game has just reported.
pub fn remember_game(game: &Path, hz: f32) {
    let key = game.to_string_lossy().replace(['\t', '\n', '\r'], " ");
    if !key.is_empty() {
        store(&games_path(), key, hz);
    }
}

/// Remember a rate a core has just reported. Writes nothing when it is the
/// same as what is already there, so a game a night does not rewrite a file a
/// night.
pub fn remember(core: &str, standard: &str, hz: f32) {
    store(&path(), key(core, standard), hz);
}

fn store(file: &Path, k: String, hz: f32) {
    if !(40.0..=90.0).contains(&hz) {
        return;
    }
    let mut all = load_from(file);
    if all.get(&k).is_some_and(|v| (v - hz).abs() < 0.001) {
        return;
    }
    all.insert(k, hz);
    let text: String = all.iter().map(|(k, v)| format!("{k}\t{v:.4}\n")).collect();
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = file.with_extension("tsv.new");
    if std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, file).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_core_given_as_a_path_and_by_name_are_the_same_core() {
        // The launcher writes the rate under the path it launched, and reads
        // it back under the name in the system's configuration. Both have to
        // land on the same line or the rate is never found and every game
        // costs a second mode change.
        let want = key("genesis_plus_gx", "pal");
        assert_eq!(
            key("/usr/lib/libretro/genesis_plus_gx_libretro.so", "pal"),
            want
        );
        assert_eq!(key("genesis_plus_gx_libretro.so", "pal"), want);
        assert_eq!(key("genesis_plus_gx_libretro", "pal"), want);
    }

    /// A file of this test's own, so the tests can run beside each other.
    fn file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("omacrt-rates-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join(format!("{name}.tsv"));
        let _ = std::fs::remove_file(&p);
        p
    }

    fn remember_game_in(file: &Path, game: &Path, hz: f32) {
        store(file, game.to_string_lossy().into_owned(), hz);
    }

    #[test]
    fn two_boards_on_one_core_keep_their_own_rates() {
        let f = file("boards");
        let mk = Path::new("/roms/arcade_mame/mk.zip");
        let outrun = Path::new("/roms/arcade_mame/outrun.zip");
        assert_eq!(known_game_in(&f, mk), None);
        remember_game_in(&f, mk, 54.706);
        remember_game_in(&f, outrun, 60.056);
        assert_eq!(known_game_in(&f, mk), Some(54.706));
        assert_eq!(known_game_in(&f, outrun), Some(60.056));
        let _ = std::fs::remove_file(&f);
    }

    #[test]
    fn a_rate_no_television_runs_at_is_not_remembered() {
        let f = file("refused");
        let p = Path::new("/roms/x.zip");
        remember_game_in(&f, p, 0.0);
        remember_game_in(&f, p, 120.0);
        assert_eq!(known_game_in(&f, p), None);
        let _ = std::fs::remove_file(&f);
    }

    #[test]
    fn the_standard_is_part_of_the_key() {
        assert_ne!(
            key("genesis_plus_gx", "pal"),
            key("genesis_plus_gx", "ntsc")
        );
    }
}
