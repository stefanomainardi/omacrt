//! Where a game's picture sits on the tube, when it is not where its system's
//! pictures sit.
//!
//! The picture is placed at three levels, each a correction on the one
//! before. The television's own centring is the TV profile, a property of the
//! set in the room. A system's is `shift_x` and `shift_y` in `systems.toml`,
//! because a console's video timing puts its picture in its own place. And a
//! game's is here, for the few that draw off centre on hardware too or carry
//! a border of their own. A game's entry replaces its system's rather than
//! adding to it, so what was saved for a game is what the game gets, whatever
//! the system is set to later.
//!
//! Shifts are in the launcher's pixels, 320 to the line, and lines down, the
//! units the TV profile and `omacrt mode --shift-x` already use.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// How far a picture may be moved either way, the same as the TV profile.
pub const LIMIT: i32 = 16;

fn path() -> PathBuf {
    crate::crt::config_dir().join("centring.tsv")
}

fn key(game: &Path) -> String {
    game.to_string_lossy().replace(['\t', '\n', '\r'], " ")
}

fn load_from(file: &Path) -> BTreeMap<String, (i32, i32)> {
    let mut out = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(file) else {
        return out;
    };
    for line in text.lines() {
        let mut parts = line.rsplitn(3, '\t');
        let (Some(y), Some(x), Some(k)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        if let (Ok(x), Ok(y)) = (x.trim().parse::<i32>(), y.trim().parse::<i32>()) {
            out.insert(
                k.to_string(),
                (x.clamp(-LIMIT, LIMIT), y.clamp(-LIMIT, LIMIT)),
            );
        }
    }
    out
}

/// The shift saved for this game, if one was.
pub fn game(game: &Path) -> Option<(i32, i32)> {
    game_in(&path(), game)
}

fn game_in(file: &Path, game: &Path) -> Option<(i32, i32)> {
    load_from(file).get(&key(game)).copied()
}

/// Save a game's shift, or forget it with `None` so the game follows its
/// system again.
pub fn set_game(game: &Path, shift: Option<(i32, i32)>) -> std::io::Result<()> {
    set_game_in(&path(), game, shift)
}

fn set_game_in(file: &Path, game: &Path, shift: Option<(i32, i32)>) -> std::io::Result<()> {
    let mut all = load_from(file);
    let k = key(game);
    match shift {
        Some((x, y)) => all.insert(k, (x.clamp(-LIMIT, LIMIT), y.clamp(-LIMIT, LIMIT))),
        None => all.remove(&k),
    };
    let text: String = all
        .iter()
        .map(|(k, (x, y))| format!("{k}\t{x}\t{y}\n"))
        .collect();
    crate::store::save(file, text)
}

/// The shift a game is started with, on top of the television's: the game's
/// own if it has one, else its system's.
pub fn effective(system_shift: (i32, i32), game_path: &Path) -> (i32, i32) {
    game(game_path).unwrap_or(system_shift)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("omacrt-centring-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("centring.tsv")
    }

    #[test]
    fn a_game_keeps_its_shift_and_can_give_it_up() {
        let file = scratch("keep");
        let game = Path::new("/roms/snes/Chrono Trigger (USA).sfc");
        assert_eq!(game_in(&file, game), None);
        set_game_in(&file, game, Some((2, -1))).unwrap();
        assert_eq!(game_in(&file, game), Some((2, -1)));
        set_game_in(&file, game, None).unwrap();
        assert_eq!(game_in(&file, game), None);
    }

    #[test]
    fn a_shift_out_of_range_is_held_at_the_limit() {
        let file = scratch("limit");
        let game = Path::new("/roms/mame/sf2.zip");
        set_game_in(&file, game, Some((40, -99))).unwrap();
        assert_eq!(game_in(&file, game), Some((LIMIT, -LIMIT)));
    }

    #[test]
    fn a_name_with_a_tab_in_it_does_not_break_the_file() {
        let file = scratch("tab");
        let odd = Path::new("/roms/odd\tname.zip");
        let plain = Path::new("/roms/plain.zip");
        set_game_in(&file, odd, Some((1, 1))).unwrap();
        set_game_in(&file, plain, Some((3, 0))).unwrap();
        assert_eq!(game_in(&file, plain), Some((3, 0)));
        assert_eq!(game_in(&file, odd), Some((1, 1)));
    }
}
