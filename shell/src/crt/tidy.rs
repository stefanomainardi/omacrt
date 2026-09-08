//! What gets left behind, found and cleared.
//!
//! A television driven by several processes leaves things lying about. The one
//! that matters is an emulator that outlived its launcher: killed with the
//! launcher's own signal it keeps running, gets reparented to systemd, holds
//! the audio sink, and answers "something is playing" for as long as the
//! machine is up. One of those from a morning's testing blocked every launch
//! for eleven hours before anybody asked why.
//!
//! Nothing here guesses. An emulator is ours when its command line carries
//! our own configuration file, which nothing else on the machine passes, and
//! it is an orphan when no launcher is running to own it.

use std::path::PathBuf;

/// Something left behind, and what would be done about it.
#[derive(Clone, Debug, PartialEq)]
pub struct Mess {
    /// A short name for the row: `orphan retroarch`.
    pub what: String,
    /// The detail worth printing: a pid, a path, a size.
    pub detail: String,
    /// What clearing it would do, in words.
    pub fix: String,
    pub kind: Kind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// An emulator with no launcher to own it: `pid`.
    Orphan(u32),
    /// A file to delete.
    Stale(PathBuf),
}

/// Every process id whose command line carries our RetroArch configuration
/// or our player's socket. `/proc` is read directly rather than through
/// pgrep, so this can never match the process asking the question.
pub fn ours() -> Vec<(u32, String)> {
    let config = super::config_dir();
    let marks = [
        config.join("retroarch.cfg").to_string_lossy().to_string(),
        config.join("mpv.sock").to_string_lossy().to_string(),
    ];
    let me = std::process::id();
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return out;
    };
    for entry in dir.flatten() {
        let name = entry.file_name();
        let Ok(pid) = name.to_string_lossy().parse::<u32>() else {
            continue;
        };
        if pid == me {
            continue;
        }
        let Ok(raw) = std::fs::read(format!("/proc/{pid}/cmdline")) else {
            continue;
        };
        // The command line is nul separated; spaces make it readable.
        let line = String::from_utf8_lossy(&raw).replace('\0', " ");
        if marks.iter().any(|m| line.contains(m.as_str())) {
            out.push((pid, line.trim().to_string()));
        }
    }
    out
}

/// Is any ancestor of this process a launcher?
fn owned_by_launcher(pid: u32, launchers: &[u32]) -> bool {
    let mut at = pid;
    // Ten steps is more than the tree ever is, and stops a loop dead.
    for _ in 0..10 {
        if launchers.contains(&at) {
            return true;
        }
        let Some(parent) = parent_of(at) else {
            return false;
        };
        if parent <= 1 {
            return false;
        }
        at = parent;
    }
    false
}

/// The parent of a process, out of `/proc/<pid>/stat`.
fn parent_of(pid: u32) -> Option<u32> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The command sits in brackets and may hold anything, so the fields are
    // counted from the last bracket: state, then the parent.
    let after = text.rfind(')')?;
    let mut fields = text.get(after + 2..)?.split_whitespace();
    let _state = fields.next()?;
    fields.next()?.parse().ok()
}

/// The name of the program a command line runs, for a readable row.
fn program(line: &str) -> String {
    line.split_whitespace()
        .next()
        .map(|p| {
            std::path::Path::new(p)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| p.to_string())
        })
        .unwrap_or_else(|| "a process".into())
}

/// Emulators of ours that no launcher owns.
pub fn orphans() -> Vec<Mess> {
    let launchers = super::launcher::pids();
    ours()
        .into_iter()
        .filter(|(pid, _)| !owned_by_launcher(*pid, &launchers))
        .map(|(pid, line)| {
            let name = program(&line);
            Mess {
                what: format!("orphan {name}"),
                detail: format!("pid {pid}"),
                fix: "stop it: nothing is left to own it".into(),
                kind: Kind::Orphan(pid),
            }
        })
        .collect()
}

/// Half written files a fetch left behind, in the caches this owns.
fn leftovers() -> Vec<Mess> {
    let home = super::home();
    let dirs = [
        home.join(".cache/omarchy-crt/art"),
        home.join(".cache/omarchy-crt/frame"),
        home.join(".cache/omarchy-crt/music-art"),
    ];
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if !(name.ends_with(".part") || name.ends_with(".small")) {
                continue;
            }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            out.push(Mess {
                what: "half fetched file".into(),
                detail: format!("{} ({size} bytes)", path.display()),
                fix: "delete it: the next fetch starts again".into(),
                kind: Kind::Stale(path),
            });
        }
    }
    out
}

/// A watchdog pidfile whose process is gone.
fn stale_watchdog() -> Vec<Mess> {
    let path = super::watchdog::pid_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(pid) = text.trim().parse::<u32>() else {
        return vec![Mess {
            what: "watchdog pidfile".into(),
            detail: format!("{} holds no pid", path.display()),
            fix: "delete it".into(),
            kind: Kind::Stale(path),
        }];
    };
    if std::path::Path::new(&format!("/proc/{pid}")).exists() {
        return Vec::new();
    }
    vec![Mess {
        what: "watchdog pidfile".into(),
        detail: format!("pid {pid} is gone"),
        fix: "delete it: `on` starts a new watchdog".into(),
        kind: Kind::Stale(path),
    }]
}

/// Everything worth clearing, in the order it should be cleared.
pub fn survey() -> Vec<Mess> {
    let mut out = orphans();
    out.extend(stale_watchdog());
    out.extend(leftovers());
    out
}

/// Clear one thing. Returns what happened, for a line of output.
pub fn clear(mess: &Mess) -> String {
    match &mess.kind {
        Kind::Orphan(pid) => {
            let pid = *pid as libc::pid_t;
            unsafe { libc::kill(pid, libc::SIGTERM) };
            // A core that has wedged ignores the polite signal; the eleven
            // hour one did.
            for _ in 0..20 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
                    return "stopped".into();
                }
            }
            unsafe { libc::kill(pid, libc::SIGKILL) };
            std::thread::sleep(std::time::Duration::from_millis(200));
            if std::path::Path::new(&format!("/proc/{pid}")).exists() {
                "would not stop".into()
            } else {
                "killed".into()
            }
        }
        Kind::Stale(path) => match std::fs::remove_file(path) {
            Ok(()) => "deleted".into(),
            Err(e) => format!("could not delete: {e}"),
        },
    }
}

/// Clear the emulators nothing owns, and say how many went. Called where a
/// mess would get in the way: starting the tube, stopping the launcher, and
/// on the watchdog's own slow beat.
pub fn sweep_orphans() -> usize {
    let found = orphans();
    for mess in &found {
        clear(mess);
    }
    found.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_parent_of_this_process_is_readable() {
        let me = std::process::id();
        let parent = parent_of(me).expect("this process has a parent");
        assert!(parent > 0);
        // And its own ancestry holds itself, whatever the tree above is.
        assert!(owned_by_launcher(me, &[me]));
        assert!(!owned_by_launcher(me, &[]));
    }

    #[test]
    fn a_command_line_gives_up_the_program_that_runs_it() {
        assert_eq!(program("/usr/bin/retroarch --config x"), "retroarch");
        assert_eq!(program("mpv --idle"), "mpv");
        assert_eq!(program(""), "a process");
    }

    #[test]
    fn nothing_of_ours_is_this_very_process() {
        // Whatever the machine is running, the survey never offers to stop
        // the process doing the surveying.
        let me = std::process::id();
        assert!(ours().iter().all(|(pid, _)| *pid != me));
    }
}
