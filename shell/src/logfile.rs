//! Log files that cannot grow without end.
//!
//! The launcher and the display process write their output to a file in the
//! state directory, and both of them run for as long as the television is on.
//! Left alone that file grows for ever: on the machine this was written on the
//! display log had reached 367 MB of one line repeated, which is a disk filling
//! up slowly enough that nobody notices until it matters.
//!
//! [`open`] hands back an appending handle to a file that was rotated first if
//! it had grown past the cap, keeping exactly one older generation as
//! `<name>.1`. [`rotate_if_big`] does the same check while a process is
//! running, for the one that writes continuously.

use std::fs::{File, OpenOptions};
use std::path::Path;

/// Files are rotated once they pass this. Two generations of it is the most
/// the state directory will ever hold per log.
pub const CAP_BYTES: u64 = 8 * 1024 * 1024;

/// Whether the log has anything to say beyond warnings and errors: set
/// `OMACRT_LOG=debug` for the per second bookkeeping.
pub fn debug_enabled() -> bool {
    matches!(
        std::env::var("OMACRT_LOG").as_deref(),
        Ok("debug") | Ok("trace")
    )
}

/// An appending handle to `path`, rotating it first when it is already big.
pub fn open(path: &Path) -> std::io::Result<File> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    rotate_if_big(path, CAP_BYTES);
    OpenOptions::new().create(true).append(true).open(path)
}

/// Move `path` aside when it has passed `cap`, keeping one older generation.
/// Returns true when a rotation happened.
pub fn rotate_if_big(path: &Path, cap: u64) -> bool {
    let big = std::fs::metadata(path)
        .map(|m| m.len() > cap)
        .unwrap_or(false);
    if !big {
        return false;
    }
    let mut old = path.as_os_str().to_os_string();
    old.push(".1");
    let _ = std::fs::rename(path, Path::new(&old));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_big_log_is_rotated_and_one_generation_is_kept() {
        let dir = std::env::temp_dir().join(format!("omacrt-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("display.log");
        std::fs::write(&path, vec![b'x'; 32]).unwrap();
        assert!(!rotate_if_big(&path, 1024), "a small log stays put");
        std::fs::write(&path, vec![b'x'; 2048]).unwrap();
        assert!(rotate_if_big(&path, 1024), "a big log is rotated");
        assert!(!path.exists(), "the current log starts again");
        assert!(
            dir.join("display.log.1").is_file(),
            "one generation is kept"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn opening_gives_an_appending_handle() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("omacrt-log2-{}", std::process::id()));
        let path = dir.join("shell.log");
        let mut f = open(&path).unwrap();
        writeln!(f, "first").unwrap();
        let mut g = open(&path).unwrap();
        writeln!(g, "second").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("first") && text.contains("second"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
