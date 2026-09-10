//! Writing the user's files without ever leaving one half written.
//!
//! Everything this project owns on disk, the systems list, the settings, the
//! index, the favourites, the pad mapping, is small and precious: losing it
//! means losing work somebody did by hand. A plain `fs::write` truncates the
//! file first, so a crash, a full disk or a machine losing power in the middle
//! leaves an empty or half written file behind. That is not theoretical: the
//! recent list corrupted itself once already by growing a column per save.
//!
//! Every durable write goes through [`save`]: the bytes land in a temporary
//! file next to the target, are flushed to the disk, and only then replace it
//! with a rename, which is atomic on every filesystem we care about. The
//! previous copy is kept as `<name>.bak`, so a bad write is one command away
//! from being undone, and [`load`] falls back to it when the main file is
//! unreadable.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// A file only its owner can read. What this project writes is a person's
/// settings, their appointments, the captions of their photographs and the
/// names of what they play, and the default mode is whatever the umask
/// happens to be, which on a common `022` is readable by everybody on the
/// machine.
const OWNER_ONLY_FILE: u32 = 0o600;
/// The same for a directory, so a file inside one cannot be reached even
/// when its own mode is missed.
const OWNER_ONLY_DIR: u32 = 0o700;

/// Create `dir` and everything above it, and make sure `dir` itself is
/// private. Directories above it are left as they are: they are
/// `~/.config` and its like, and they belong to the person, not to us.
pub fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(OWNER_ONLY_DIR))
}

/// Where the backup of `path` lives.
fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

/// Write `contents` to `path` atomically, keeping the previous copy.
///
/// The temporary file is created in the same directory as the target, since a
/// rename across filesystems is not atomic (and not even possible).
pub fn save(path: &Path, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
    save_with_mode(path, contents, None)
}

/// The same, for a file whose contents are nobody else's business: it is
/// created `0600` and its directory `0700`, rather than inheriting whatever
/// the umask allows.
pub fn save_private(path: &Path, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
    save_with_mode(path, contents, Some(OWNER_ONLY_FILE))
}

fn save_with_mode(
    path: &Path,
    contents: impl AsRef<[u8]>,
    mode: Option<u32>,
) -> std::io::Result<()> {
    let contents = contents.as_ref();
    if let Some(dir) = path.parent() {
        match mode {
            Some(_) => create_private_dir(dir)?,
            None => fs::create_dir_all(dir)?,
        }
    }
    // Keep the copy that is about to be replaced, but never overwrite a good
    // backup with a file we have not managed to replace yet.
    if path.is_file() {
        let _ = fs::copy(path, backup_path(path));
    }
    let tmp = temp_beside(path);
    {
        // The mode goes on at creation, not after: a file that is briefly
        // world readable has already been read by anybody watching.
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        if let Some(m) = mode {
            opts.mode(m);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(contents)?;
        f.flush()?;
        // Ask the filesystem to put the bytes on the disk before the rename,
        // so a power cut cannot leave a renamed but empty file.
        let _ = f.sync_all();
    }
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Read a file, falling back to its backup when the file is missing or
/// unreadable. Returns None when neither can be read.
pub fn load(path: &Path) -> Option<Vec<u8>> {
    match fs::read(path) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => fs::read(backup_path(path)).ok().filter(|v| !v.is_empty()),
    }
}

/// The same as [`load`], for text files.
pub fn load_string(path: &Path) -> Option<String> {
    load(path).and_then(|v| String::from_utf8(v).ok())
}

fn temp_beside(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.tmp", std::process::id()));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of its own per test. The tests run at the same time and
    /// one of them lists the directory looking for temporary files, so a
    /// shared one makes it fail whenever another test is mid-save.
    fn scratch(name: &str) -> PathBuf {
        let stem = name.split('.').next().unwrap_or(name);
        let dir = std::env::temp_dir().join(format!("omacrt-store-{}-{stem}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn a_save_replaces_the_file_and_keeps_the_previous_one() {
        let p = scratch("keep.txt");
        save(&p, "first").unwrap();
        save(&p, "second").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "second");
        assert_eq!(fs::read_to_string(backup_path(&p)).unwrap(), "first");
    }

    #[test]
    fn a_broken_file_reads_from_the_backup() {
        let p = scratch("broken.txt");
        save(&p, "good").unwrap();
        save(&p, "newer").unwrap();
        fs::write(&p, b"").unwrap(); // truncated, as a crash would leave it
        assert_eq!(load_string(&p).as_deref(), Some("good"));
    }

    #[test]
    fn no_temporary_file_is_left_behind() {
        let p = scratch("clean.txt");
        save(&p, "x").unwrap();
        let dir = p.parent().unwrap();
        let leftovers: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temporary files left: {leftovers:?}");
    }

    use std::os::unix::fs::PermissionsExt;

    fn mode_of(p: &Path) -> u32 {
        fs::metadata(p).expect("stat").permissions().mode() & 0o777
    }

    /// The umask decides the mode of a file created without one, and a
    /// common `022` leaves a settings file readable by everybody on the
    /// machine. `save_private` does not ask the umask.
    #[test]
    fn a_private_save_is_owner_only() {
        let dir = std::env::temp_dir().join(format!("omacrt-store-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let file = dir.join("inner").join("secret.toml");
        save_private(&file, b"key = \"nobody else's\"\n").expect("save");
        assert_eq!(mode_of(&file), 0o600, "the file is readable by others");
        assert_eq!(
            mode_of(file.parent().expect("parent")),
            0o700,
            "the directory is traversable by others"
        );
        // A second write keeps the mode, and the backup it makes inherits it.
        save_private(&file, b"key = \"still nobody else's\"\n").expect("save again");
        assert_eq!(mode_of(&file), 0o600);
        assert_eq!(mode_of(&backup_path(&file)), 0o600, "the backup is open");
        let _ = fs::remove_dir_all(&dir);
    }
}
