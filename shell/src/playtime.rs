//! How long each game has been played, in all.
//!
//! Counted while the emulator runs and is not paused, so an evening left on
//! the pause menu does not count as an evening of play, and a video is not a
//! game. One line per game ever played, `path<TAB>seconds`, in the user's
//! configuration next to `recent.txt`: the recent list keeps twenty games and
//! this keeps them all.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn load(file: &Path) -> HashMap<PathBuf, u64> {
    let Some(text) = crate::store::load_string(file) else {
        return HashMap::new();
    };
    text.lines()
        .filter_map(|l| {
            let (p, secs) = l.rsplit_once('\t')?;
            let secs: u64 = secs.trim().parse().ok()?;
            (!p.is_empty()).then(|| (PathBuf::from(p), secs))
        })
        .collect()
}

/// Add a stretch of play to a game and write the file.
pub fn add(file: &Path, all: &mut HashMap<PathBuf, u64>, game: &Path, secs: u64) {
    if secs == 0 || game.as_os_str().is_empty() {
        return;
    }
    *all.entry(game.to_path_buf()).or_default() += secs;
    let mut lines: Vec<String> = all
        .iter()
        .map(|(p, s)| {
            let p = p.to_string_lossy().replace(['\t', '\n', '\r'], " ");
            format!("{p}\t{s}\n")
        })
        .collect();
    lines.sort();
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = crate::store::save(file, lines.concat()) {
        eprintln!("cannot write {}: {e}", file.display());
    }
}

/// `time 4h12m` under a cover, ten columns at most. Nothing under a minute,
/// which is a launch that did not take rather than a game played.
pub fn label(secs: u64) -> Option<String> {
    let mins = secs / 60;
    match mins {
        0 => None,
        m if m < 60 => Some(format!("time {m}m")),
        m if m < 600 => Some(format!("time {}h{:02}m", m / 60, m % 60)),
        m => Some(format!("time {}h", m / 60)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("omacrt-playtime-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join(format!("{name}.tsv"));
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_file(p.with_extension("tsv.bak"));
        p
    }

    #[test]
    fn stretches_of_play_add_up_and_survive_a_reload() {
        let f = file("add");
        let mk = Path::new("/roms/arcade_mame/mk.zip");
        let mut all = load(&f);
        add(&f, &mut all, mk, 600);
        add(&f, &mut all, mk, 150);
        add(&f, &mut all, Path::new("/roms/a\tb.zip"), 60);
        let again = load(&f);
        assert_eq!(again.get(mk), Some(&750));
        assert_eq!(again.get(Path::new("/roms/a b.zip")), Some(&60));
    }

    #[test]
    fn the_label_fits_under_a_cover() {
        assert_eq!(label(59), None);
        assert_eq!(label(23 * 60), Some("time 23m".into()));
        assert_eq!(label(4 * 3600 + 12 * 60), Some("time 4h12m".into()));
        assert_eq!(label(123 * 3600), Some("time 123h".into()));
        for secs in [60, 3599, 3600, 35_999, 36_000, 3_600_000] {
            assert!(label(secs).unwrap().len() <= 10, "{secs}");
        }
    }
}
