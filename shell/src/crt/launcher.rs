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
    // `-U` so this is the user's own launchers and nobody else's, and the
    // pattern has to be the first word: without that anchor it matched any
    // command line mentioning the binary, including the installer's own
    // `install -m 755 … omacrt-shell`, which `shell stop` would then signal.
    let pattern = format!("^(\\S*/)?{SHELL_CLASS}( |$)");
    let uid = users_own_uid();
    let Some(text) = run("pgrep", &["-U", &uid, "-f", &pattern]) else {
        return Vec::new();
    };
    let me = std::process::id();
    text.split_whitespace()
        .filter_map(|s| s.parse().ok())
        .filter(|p| *p != me)
        .collect()
}

/// This user's id, for scoping a process search to their own session.
fn users_own_uid() -> String {
    // SAFETY: `getuid` reads a field of the calling process and cannot fail.
    unsafe { libc::getuid() }.to_string()
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
    // Anything of ours still running belongs to a launcher that is gone.
    super::tidy::sweep_orphans();
    output::window_rules(output_name);
    let _ = std::fs::create_dir_all(state_dir());
    let log = crate::logfile::open(&state_dir().join("shell.log")).map_err(|e| e.to_string())?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(bin);
    if let Some(s) = sink {
        cmd.env("PULSE_SINK", s).env("PIPEWIRE_NODE", s);
    }
    // With the display process up the launcher (and everything it starts)
    // is a client of our own compositor on the tube.
    if super::display::running() {
        super::display::env(&mut cmd);
    }
    // SDL's PipeWire backend loads module-rt into the launcher, which sets a
    // 200 ms RLIMIT_RTTIME on the process so rtkit grants it realtime. Every
    // game inherits that limit (an unprivileged process cannot raise it
    // back), rtkit then grants realtime to RetroArch's PulseAudio thread
    // too, and the kernel kills the game with SIGKILL the first time that
    // thread runs 200 ms without sleeping. libpulse leaves the limits alone,
    // rtkit refuses it, and the games run as they do from a terminal.
    cmd.env("SDL_AUDIODRIVER", "pulseaudio");
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

/// Stop the launcher and wait until it is gone. SDL turns SIGTERM into a
/// quit event, which the launcher only sees once its frame loop runs again,
/// so a launcher busy in a mode change can take a moment; one that never
/// answers gets SIGKILL, otherwise a restart would end with two launchers
/// fighting over the tube.
pub fn stop() -> String {
    // The launcher is cliamp's face: no launcher, no music.
    crate::music::stop_now();
    crate::player::stop_all();
    let list = pids();
    if list.is_empty() {
        return "not running".into();
    }
    for pid in &list {
        unsafe { libc::kill(*pid as libc::pid_t, libc::SIGTERM) };
    }
    for _ in 0..30 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if pids().is_empty() {
            let swept = super::tidy::sweep_orphans();
            return if swept > 0 {
                format!("stopped {} ({swept} left behind)", list.len())
            } else {
                format!("stopped {}", list.len())
            };
        }
    }
    let left = pids();
    for pid in &left {
        unsafe { libc::kill(*pid as libc::pid_t, libc::SIGKILL) };
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
    // An emulator does not die with the launcher that started it: the signal
    // goes to the launcher alone, the child is reparented to systemd and
    // keeps running, holding the audio and answering "something is playing"
    // for as long as the machine is up. With the launcher gone it is an
    // orphan by definition, so this is where it goes.
    let swept = super::tidy::sweep_orphans();
    let mut out = format!("stopped {} ({} killed)", list.len(), left.len());
    if swept > 0 {
        out.push_str(&format!(", {swept} left behind"));
    }
    out
}

/// Bring the tube's current program to the front: the game while one runs,
/// else the launcher. Focusing the launcher over a running game would hide
/// the game's workspace, and a hidden fullscreen client blocks on its next
/// frame until the compositor calls it unresponsive.
pub fn focus() -> (bool, String) {
    if super::display::running() {
        let who = match playing() {
            Some("retroarch") => "com.libretro.RetroArch",
            Some("mpv") => "omacrt-player",
            _ => SHELL_CLASS,
        };
        return (
            super::display::raise(who),
            format!("raised {who} on the tube"),
        );
    }
    match playing() {
        Some("retroarch") => output::focus_class("com.libretro.RetroArch"),
        Some("mpv") => output::focus_class("omacrt-player"),
        _ => output::focus_class(SHELL_CLASS),
    }
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
