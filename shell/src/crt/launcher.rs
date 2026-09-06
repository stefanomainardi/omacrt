//! The launcher process: find the binary, start it on the CRT, stop it, and
//! tell which program currently has the tube.

use super::output;
use super::{Config, SHELL_CLASS, run, state_dir};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// The launcher binary: PATH, then next to this executable, then the
/// release build in the source tree.
pub fn binary(cfg: &Config) -> Option<PathBuf> {
    let name = cfg.shell.bin.trim();
    if name.is_empty() {
        return None;
    }
    if name.contains('/') {
        let p = PathBuf::from(shellexpand(name));
        return p.exists().then_some(p);
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let p = dir.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    let exe = std::env::current_exe().ok()?;
    let here = exe.parent()?;
    for cand in [
        here.join(name),
        here.join("..").join("shell/target/release").join(name),
    ] {
        if cand.is_file() {
            return std::fs::canonicalize(cand).ok();
        }
    }
    None
}

fn shellexpand(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        return super::home().join(rest).display().to_string();
    }
    p.to_string()
}

/// Launcher processes. The kernel truncates `comm` to 15 bytes, so match
/// the full command line instead of the process name.
pub fn pids() -> Vec<u32> {
    let pattern = format!("(^|/){SHELL_CLASS}( |$)");
    let Some(text) = run("pgrep", &["-f", &pattern]) else {
        return Vec::new();
    };
    let me = std::process::id();
    text.split_whitespace()
        .filter_map(|s| s.parse().ok())
        .filter(|p| *p != me)
        .collect()
}

/// Start the launcher on the CRT output. `sink` is the PipeWire sink the
/// launcher and everything it spawns should play on; it travels through
/// `PULSE_SINK` and `PIPEWIRE_NODE`, which SDL, RetroArch and mpv honour.
pub fn start(cfg: &Config, output_name: &str, sink: Option<&str>) -> Result<String, String> {
    let bin =
        binary(cfg).ok_or_else(|| format!("launcher binary not found ({})", cfg.shell.bin))?;
    if !pids().is_empty() {
        return Ok("already running".into());
    }
    output::window_rules(output_name);
    let _ = std::fs::create_dir_all(state_dir());
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(state_dir().join("shell.log"))
        .map_err(|e| e.to_string())?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(bin);
    if let Some(s) = sink {
        cmd.env("PULSE_SINK", s).env("PIPEWIRE_NODE", s);
    }
    cmd.args(&cfg.shell.args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err));
    // Own session so the launcher survives the CLI exiting.
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.spawn().map_err(|e| e.to_string())?;
    std::thread::sleep(std::time::Duration::from_millis(1500));
    Ok("started".into())
}

pub fn stop() -> String {
    let list = pids();
    for pid in &list {
        unsafe { libc::kill(*pid as libc::pid_t, libc::SIGTERM) };
    }
    if list.is_empty() {
        "not running".into()
    } else {
        format!("stopped {}", list.len())
    }
}

pub fn focus() -> (bool, String) {
    output::focus_class(SHELL_CLASS)
}

/// Which program is on the tube besides the launcher.
pub fn playing() -> Option<&'static str> {
    if run("pgrep", &["-x", "retroarch"]).is_some() {
        Some("retroarch")
    } else if run("pgrep", &["-x", "mpv"]).is_some() {
        Some("mpv")
    } else {
        None
    }
}
