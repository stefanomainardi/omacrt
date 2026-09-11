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

/// What a log has to say about the run that wrote it.
///
/// Nobody reads a log in a state directory. This is how the self test reads
/// it for them: a launcher that stopped, and a line repeated so many times
/// that it is a condition rather than an event. The crash that this was
/// written for was in the log for a morning before anybody looked.
#[derive(Debug, Default, PartialEq)]
pub struct Trouble {
    /// Every panic, as `file:line: message`, oldest first.
    pub panics: Vec<String>,
    /// The most repeated line and how many times, when that is a flood.
    pub flood: Option<(String, usize)>,
}

/// A line repeated at least this many times is a condition, not an event.
const FLOOD: usize = 50;

/// How much of the end of a log to read. A log is capped at eight megabytes
/// and the interesting part is always the end, so the whole file is never
/// worth the wait.
const TAIL_BYTES: u64 = 512 * 1024;

/// Read the tail of a log and say what went wrong in it.
pub fn trouble(path: &Path) -> Trouble {
    let Some(text) = tail(path, TAIL_BYTES) else {
        return Trouble::default();
    };
    let mut out = Trouble::default();
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        // `thread 'main' (123) panicked at src/sky.rs:1444:37:` and the
        // reason on the line after it.
        if let Some(at) = line.find("panicked at ") {
            let where_ = line[at + "panicked at ".len()..].trim_end_matches(':');
            let why = lines.get(i + 1).map(|l| l.trim()).unwrap_or("");
            out.panics.push(if why.is_empty() {
                where_.to_string()
            } else {
                format!("{where_}: {why}")
            });
        }
    }
    // The most repeated line, with the numbers in it flattened so that one
    // condition does not read as a thousand different lines.
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for line in &lines {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        *counts.entry(flatten(t)).or_default() += 1;
    }
    if let Some((line, n)) = counts.into_iter().max_by_key(|(_, n)| *n)
        && n >= FLOOD
    {
        out.flood = Some((line, n));
    }
    out
}

/// Digits become `N`, so that two timestamps are one line.
fn flatten(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_number = false;
    for c in line.chars() {
        if c.is_ascii_digit() {
            if !in_number {
                out.push('N');
                in_number = true;
            }
        } else {
            in_number = false;
            out.push(c);
        }
    }
    out
}

/// The last `bytes` of a file, from the first whole line.
pub fn tail(path: &Path, bytes: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let from = len.saturating_sub(bytes);
    f.seek(SeekFrom::Start(from)).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf).into_owned();
    Some(if from == 0 {
        text
    } else {
        // The first line is half a line.
        text.split_once('\n')
            .map(|(_, r)| r.to_string())
            .unwrap_or(text)
    })
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

    #[test]
    fn a_log_says_what_went_wrong_in_it() {
        let dir = std::env::temp_dir().join(format!("omacrt-trouble-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("shell.log");
        let mut text = String::new();
        for i in 0..120 {
            text.push_str(&format!(
                "queue_frame: Page flip commit failed on device `Some(\"/dev/dri/card1\")` ({i})\n"
            ));
        }
        text.push_str("thread 'main' (3088371) panicked at src/sky.rs:1444:37:\n");
        text.push_str("attempt to multiply with overflow\n");
        std::fs::write(&path, &text).unwrap();

        let t = trouble(&path);
        assert_eq!(
            t.panics,
            vec!["src/sky.rs:1444:37: attempt to multiply with overflow".to_string()]
        );
        let (line, n) = t
            .flood
            .expect("a hundred and twenty of one line is a flood");
        assert_eq!(n, 120);
        assert!(line.contains("Page flip commit failed"), "{line}");
        // The numbers are flattened, so one condition is one line.
        assert!(!line.contains("119"), "{line}");

        // A log with nothing wrong in it says nothing.
        std::fs::write(&path, "compositor up\nmode: 3520x240\n").unwrap();
        assert_eq!(trouble(&path), Trouble::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
