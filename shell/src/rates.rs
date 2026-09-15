//! What field rate a core runs at, remembered between launches.
//!
//! A core says its own rate in its log a second or two after it starts, and
//! acting on it then costs a second mode change: 280 ms with nothing for the
//! converter to lock to, in the middle of a title screen. Remembering it
//! means the next launch of the same core and standard builds the rate into
//! the timing before the emulator opens, and there is one mode change instead
//! of two.
//!
//! Keyed by core and standard rather than by game, because the rate belongs
//! to the console and its region: Genesis Plus GX says 49.70 for every
//! European cartridge and 59.92 for every American one. Two lines a core
//! instead of one a game, for a collection of twenty thousand.

use std::collections::BTreeMap;
use std::path::PathBuf;

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
    let mut out = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(path()) else {
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

/// Remember a rate a core has just reported. Writes nothing when it is the
/// same as what is already there, so a game a night does not rewrite a file a
/// night.
pub fn remember(core: &str, standard: &str, hz: f32) {
    if !(40.0..=90.0).contains(&hz) {
        return;
    }
    let k = key(core, standard);
    let mut all = load();
    if all.get(&k).is_some_and(|v| (v - hz).abs() < 0.001) {
        return;
    }
    all.insert(k, hz);
    let text: String = all.iter().map(|(k, v)| format!("{k}\t{v:.4}\n")).collect();
    let p = path();
    let tmp = p.with_extension("tsv.new");
    if std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, &p).is_err() {
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

    #[test]
    fn the_standard_is_part_of_the_key() {
        assert_ne!(
            key("genesis_plus_gx", "pal"),
            key("genesis_plus_gx", "ntsc")
        );
    }
}
