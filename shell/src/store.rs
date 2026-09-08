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
use std::path::{Path, PathBuf};

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
    let contents = contents.as_ref();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    // Keep the copy that is about to be replaced, but never overwrite a good
    // backup with a file we have not managed to replace yet.
    if path.is_file() {
        let _ = fs::copy(path, backup_path(path));
    }
    let tmp = temp_beside(path);
    {
        let mut f = fs::File::create(&tmp)?;
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
        let dir =
            std::env::temp_dir().join(format!("omarchy-crt-store-{}-{stem}", std::process::id()));
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
}
