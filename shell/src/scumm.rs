//! ScummVM games are folders, and RetroArch launches files.
//!
//! The libretro core wants a `.scummvm` file: a one line text file holding the
//! game's ScummVM identifier, sitting in the folder with the game data. Nobody
//! ships those, so a copied Monkey Island is a folder full of `MONKEY2.000`
//! and `MONKEY2.EXE` that the launcher can see and cannot start.
//!
//! This works the identifier out from the data itself and writes the file. For
//! the LucasArts games the identifier is the name of the data file: `comi.la0`
//! is `comi`, `monkey2.000` is `monkey2`. The older ones, whose data is all
//! called `000.LFL`, are recognised by the executable next to it. Everything
//! written is checked against the list of identifiers the core knows, so a
//! folder that cannot be placed is left alone rather than given a file that
//! fails at launch.

use std::path::{Path, PathBuf};

/// Identifiers the ScummVM core answers to, for the games most likely to be
/// on a disk next to a television. A folder that does not resolve to one of
/// these is left for the player to name.
pub const KNOWN: &[&str] = &[
    // LucasArts
    "maniac", "zak", "indy3", "loom", "monkey", "monkey2", "atlantis", "tentacle", "samnmax", "ft",
    "dig", "comi", "indy4", "fate",
    // Revolution, Adventure Soft, Westwood and the other regulars
    "sky", "queen", "sword1", "sword2", "simon1", "simon2", "kyra1", "kyra2", "kyra3", "lure",
    "bass", "toon", "gob", "drascula", "touche", "sfinx",
];

/// Games whose data files are all called `000.LFL`, told apart by the
/// executable that shipped with them.
const BY_EXECUTABLE: &[(&str, &str)] = &[
    ("MONKEY.EXE", "monkey"),
    ("MONKEY.000", "monkey"),
    ("LOOM.EXE", "loom"),
    ("INDY3.EXE", "indy3"),
    ("ZAK.EXE", "zak"),
    ("MANIAC.EXE", "maniac"),
    ("SKY.DSK", "sky"),
    ("QUEEN.1", "queen"),
    ("SIMON.GME", "simon1"),
    ("GAME32.DAT", "simon2"),
];

/// Folder names that name the game plainly enough, as a last resort.
const BY_NAME: &[(&str, &str)] = &[
    ("secret of monkey island", "monkey"),
    ("monkey island 2", "monkey2"),
    ("lechuck", "monkey2"),
    ("curse of monkey island", "comi"),
    ("monkey island 3", "comi"),
    ("day of the tentacle", "tentacle"),
    ("sam and max", "samnmax"),
    ("sam & max", "samnmax"),
    ("full throttle", "ft"),
    ("fate of atlantis", "atlantis"),
    ("last crusade", "indy3"),
    ("beneath a steel sky", "sky"),
    ("broken sword", "sword1"),
    ("flight of the amazon queen", "queen"),
];

fn is_known(id: &str) -> bool {
    KNOWN.contains(&id)
}

/// The ScummVM identifier of the game in `dir`, and the folder its data
/// actually lives in. These arrive as a folder holding one more folder, and
/// the launcher file has to sit with the data, not above it.
fn detect_in(dir: &Path) -> Option<(String, PathBuf)> {
    if let Some(id) = detect_here(dir) {
        return Some((id, dir.to_path_buf()));
    }
    let mut subs = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir());
    let first = subs.next()?;
    if subs.next().is_some() {
        // More than one folder: this is a collection, not a game.
        return None;
    }
    detect_here(&first).map(|id| (id, first))
}

/// The identifier alone.
pub fn detect(dir: &Path) -> Option<String> {
    detect_in(dir).map(|(id, _)| id)
}

/// Extensions that are a game still in its packaging: ScummVM reads files,
/// not disc images, so a folder holding only these has nothing to run yet.
const PACKAGED: &[&str] = &["iso", "bin", "cue", "img", "mdf", "nrg", "7z", "zip", "rar"];

/// True when the folder holds nothing but disc images or archives.
fn only_packaged(dir: &Path) -> bool {
    let mut saw_file = false;
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    for e in rd.filter_map(|e| e.ok()) {
        let path = e.path();
        if !path.is_file() {
            continue;
        }
        saw_file = true;
        let ext = path
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !PACKAGED.contains(&ext.as_str()) {
            return false;
        }
    }
    saw_file
}

fn detect_here(dir: &Path) -> Option<String> {
    let names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .map(|n| n.to_ascii_uppercase())
        .collect();
    if names.is_empty() {
        return None;
    }
    // The LucasArts rule: the data file is named after the game.
    for n in &names {
        let Some((stem, ext)) = n.rsplit_once('.') else {
            continue;
        };
        if !matches!(ext, "000" | "LA0") {
            continue;
        }
        let id = stem.to_ascii_lowercase();
        if is_known(&id) {
            return Some(id);
        }
    }
    for (file, id) in BY_EXECUTABLE {
        if names.iter().any(|n| n == file) {
            return Some((*id).to_string());
        }
    }
    // The folder's own name, for a game whose files say nothing.
    let label = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    for (needle, id) in BY_NAME {
        if label.contains(needle) {
            return Some((*id).to_string());
        }
    }
    None
}

/// What preparing one folder did.
pub enum Prepared {
    /// A launcher file was written, naming this game.
    Wrote(PathBuf, String),
    /// The game is still a disc image or an archive: it has to be unpacked
    /// before ScummVM can read it.
    Packaged(PathBuf),
}

/// Write the launcher file for the game in `dir`, if it does not have one and
/// the game can be placed. The file goes next to the data, since that is where
/// the core looks for it.
fn ensure_launcher(dir: &Path) -> Option<Prepared> {
    let (id, data) = detect_in(dir)?;
    let has_one = std::fs::read_dir(&data)
        .ok()?
        .filter_map(|e| e.ok())
        .any(|e| e.path().extension().is_some_and(|x| x == "scummvm"));
    if has_one {
        return None;
    }
    if only_packaged(&data) {
        return Some(Prepared::Packaged(dir.to_path_buf()));
    }
    let title = dir.file_name().and_then(|n| n.to_str()).unwrap_or(&id);
    let file = data.join(format!("{title}.scummvm"));
    std::fs::write(&file, format!("{id}\n")).ok()?;
    Some(Prepared::Wrote(file, id))
}

/// Prepare every game folder directly under `root`.
pub fn prepare_all(root: &Path) -> Vec<Prepared> {
    let Ok(rd) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    rd.filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter_map(|dir| ensure_launcher(&dir))
        .collect()
}

/// Unpack the disc images in a game folder into the folder itself.
///
/// A ScummVM game bought on CD arrives as one or two disc images, and ScummVM
/// reads files rather than images. The contents of both discs belong in the
/// same folder: that is how the manual says to install a two disc game, and
/// how the second disc's music and speech end up next to the first disc's.
///
/// The images are left where they are. They are somebody's backup, and the
/// unpacked copy costs disk rather than replacing anything.
pub fn unpack(dir: &Path) -> Result<String, String> {
    let mut discs: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|x| x.to_str())
                    .map(|x| x.to_ascii_lowercase())
                    .is_some_and(|x| matches!(x.as_str(), "iso" | "img" | "bin" | "mdf" | "nrg"))
        })
        .collect();
    if discs.is_empty() {
        return Err(format!("{}: no disc image to unpack", dir.display()));
    }
    // Disc one first, so that a file on both discs is written by the disc the
    // game expects it from and the later one is skipped.
    discs.sort();
    let mut done = 0;
    for disc in &discs {
        let ok = std::process::Command::new("7z")
            .arg("x")
            .arg("-y")
            .arg("-aos") // never overwrite: the first disc wins
            .arg("-bso0")
            .arg("-bsp0")
            .arg(disc)
            .arg(format!("-o{}", dir.display()))
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            done += 1;
        }
    }
    if done == 0 {
        return Err(format!(
            "{}: could not be unpacked (is 7z installed?)",
            dir.display()
        ));
    }
    // The data usually arrives in a subfolder of the disc; the launcher file
    // goes wherever the game was actually found.
    match ensure_launcher(dir) {
        Some(Prepared::Wrote(file, id)) => {
            Ok(format!("{done} disc(s) unpacked, {id}: {}", file.display()))
        }
        Some(Prepared::Packaged(_)) | None => Ok(format!(
            "{done} disc(s) unpacked into {}, but the game could not be placed",
            dir.display()
        )),
    }
}

/// Game folders under `root` that hold nothing but disc images.
pub fn packaged_under(root: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    rd.filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter(|dir| matches!(ensure_launcher(dir), Some(Prepared::Packaged(_))))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(files: &[&str]) -> PathBuf {
        // One directory per set of files: the tests run at the same time.
        let tag: String = files
            .concat()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect();
        let dir = std::env::temp_dir().join(format!("omacrt-scumm-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in files {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        dir
    }

    #[test]
    fn a_lucasarts_game_is_named_by_its_own_data_file() {
        let dir = folder(&["MONKEY2.000", "MONKEY2.001", "MONKEY2.EXE"]);
        assert_eq!(detect(&dir).as_deref(), Some("monkey2"));
        let _ = std::fs::remove_dir_all(&dir);

        let dir = folder(&["COMI.LA0", "COMI.LA1"]);
        assert_eq!(detect(&dir).as_deref(), Some("comi"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_older_game_is_named_by_the_executable_beside_its_lfl_files() {
        let dir = folder(&["000.LFL", "901.LFL", "MONKEY.EXE", "Disk01.lec"]);
        assert_eq!(detect(&dir).as_deref(), Some("monkey"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_that_cannot_be_placed_is_left_alone() {
        let dir = folder(&["readme.txt", "something.dat"]);
        assert_eq!(detect(&dir), None);
        assert!(ensure_launcher(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_launcher_file_holds_the_identifier_and_is_written_once() {
        let dir = folder(&["ATLANTIS.000", "ATLANTIS.001"]);
        let Some(Prepared::Wrote(file, id)) = ensure_launcher(&dir) else {
            panic!("a launcher file should be written");
        };
        assert_eq!(id, "atlantis");
        assert_eq!(std::fs::read_to_string(&file).unwrap().trim(), "atlantis");
        assert!(
            ensure_launcher(&dir).is_none(),
            "the second time changes nothing"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_game_still_inside_a_disc_image_is_reported_not_written() {
        let dir = folder(&["Monkey Island 3 CD1.iso", "Monkey Island 3 CD2.iso"]);
        // The folder name is what places this one, and it has nothing to read.
        let named = dir.parent().unwrap().join("curse of monkey island");
        let _ = std::fs::remove_dir_all(&named);
        std::fs::rename(&dir, &named).unwrap();
        assert!(matches!(
            ensure_launcher(&named),
            Some(Prepared::Packaged(_))
        ));
        assert!(
            !named.join("curse of monkey island.scummvm").exists(),
            "nothing is written for a game that cannot be read yet"
        );
        let _ = std::fs::remove_dir_all(&named);
    }

    #[test]
    fn the_launcher_file_lands_beside_the_data_not_above_it() {
        let outer = folder(&[]);
        let inner = outer.join("MONKEY2");
        std::fs::create_dir_all(&inner).unwrap();
        for f in ["MONKEY2.000", "MONKEY2.001"] {
            std::fs::write(inner.join(f), b"x").unwrap();
        }
        let Some(Prepared::Wrote(file, id)) = ensure_launcher(&outer) else {
            panic!("a launcher file should be written");
        };
        assert_eq!(id, "monkey2");
        assert_eq!(file.parent(), Some(inner.as_path()), "{}", file.display());
        let _ = std::fs::remove_dir_all(&outer);
    }
}
