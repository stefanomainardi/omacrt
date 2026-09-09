//! Keeping the tube alive when the display process is not.
//!
//! The display process owns the leased connector: the launcher, RetroArch,
//! mpv and everything else on the television are its Wayland clients. When it
//! dies they all die with it, the screen goes black in the middle of a game,
//! and nothing brings it back: the only way out is the desktop, a keyboard and
//! `omacrt on` again, which is not a thing to ask of somebody holding a
//! pad on the sofa.
//!
//! So `omacrt on` leaves a small process behind. It watches the display
//! pid a few times a second, and when it goes away with the state still saying
//! the tube is on, it puts the display, the timing and, if the launcher had
//! been up, the launcher back. It gives up after a few restarts in a row: a
//! display that cannot stay up is a bug to read in the log, not something to
//! restart for ever.

use super::state_dir;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// How often the pid is checked.
pub const TICK: std::time::Duration = std::time::Duration::from_millis(500);

/// More restarts than this inside [`WINDOW_SECS`] means the display is broken
/// rather than unlucky, and the watchdog stands down.
pub const MAX_RESTARTS: usize = 3;
pub const WINDOW_SECS: f64 = 120.0;

pub fn pid_path() -> PathBuf {
    state_dir().join("watchdog.pid")
}

/// The pid in the file, when a process by that number is alive.
fn live_pid() -> Option<i32> {
    let pid: i32 = std::fs::read_to_string(pid_path())
        .ok()?
        .trim()
        .parse()
        .ok()?;
    (unsafe { libc::kill(pid, 0) } == 0).then_some(pid)
}

pub fn running() -> bool {
    live_pid().is_some()
}

/// True while this process is still the watchdog the pid file names. A newer
/// `omacrt on` starts its own, and the older one steps aside.
pub fn still_ours() -> bool {
    live_pid() == Some(std::process::id() as i32)
}

pub fn claim() {
    let _ = std::fs::create_dir_all(state_dir());
    let _ = std::fs::write(pid_path(), std::process::id().to_string());
}

/// Start the watchdog as a detached process, replacing one already there.
pub fn start() -> Result<(), String> {
    stop();
    let bin = std::env::current_exe().map_err(|e| e.to_string())?;
    let log = crate::logfile::open(&state_dir().join("watchdog.log")).map_err(|e| e.to_string())?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(bin);
    cmd.arg("watchdog")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err));
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// Stop the watchdog, if one is running. Called before turning the tube off,
/// so that a deliberate shutdown is not read as a crash.
pub fn stop() {
    if let Some(pid) = live_pid() {
        unsafe { libc::kill(pid, libc::SIGTERM) };
        for _ in 0..20 {
            if live_pid().is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    let _ = std::fs::remove_file(pid_path());
}

/// Restart times inside the window, oldest first: the caller pushes the
/// current instant and asks whether it has run out of patience.
#[derive(Default)]
pub struct Budget {
    times: Vec<f64>,
}

impl Budget {
    /// Record a restart at `now` (seconds, any monotonic origin) and say
    /// whether the watchdog should keep going.
    pub fn spend(&mut self, now: f64) -> bool {
        self.times.retain(|t| now - t < WINDOW_SECS);
        self.times.push(now);
        self.times.len() <= MAX_RESTARTS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_of_restarts_runs_out_but_a_slow_trickle_does_not() {
        let mut b = Budget::default();
        for i in 0..MAX_RESTARTS {
            assert!(b.spend(i as f64), "restart {i} is still within budget");
        }
        assert!(!b.spend(MAX_RESTARTS as f64), "the burst runs out");

        let mut b = Budget::default();
        for i in 0..10 {
            assert!(
                b.spend(i as f64 * (WINDOW_SECS + 1.0)),
                "one restart every window is not a loop"
            );
        }
    }
}
