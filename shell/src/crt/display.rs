//! Flyback, the compositor that owns the tube, as the rest of the program
//! talks to it: start it, stop it, ask whether it is up, send it a line.
//!
//! When the DAC's connector is marked non-desktop, the desktop compositor
//! leaves it alone and our process leases it: modeline, page flips and a
//! small Wayland compositor for the launcher, the emulator and the player.
//! Everything here talks to that process: is the connector leaseable, is the
//! process running, start and stop it, and its control pipe (stacking order
//! and live mode changes).

use std::io::Write;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Wayland socket the display process listens on; clients of the tube get
/// it as `WAYLAND_DISPLAY`.
pub const SOCKET: &str = "wayland-crt";

pub fn ctl_path() -> PathBuf {
    super::state_dir().join("display.ctl")
}

pub fn pid_path() -> PathBuf {
    super::state_dir().join("display.pid")
}

/// Written by the display process while the desktop preview window is open.
/// The window belongs to that process, so it is the one that knows; asking
/// the compositor instead means a subprocess at the moment a game starts.
pub fn monitor_path() -> PathBuf {
    super::state_dir().join("monitor.open")
}

/// True while the desktop preview window is open.
pub fn monitor_open() -> bool {
    monitor_path().exists()
}

/// Written by the display process every few seconds while the tube is up:
/// how long a program's picture takes to get from its commit to the start of
/// scanout, which is very nearly to the phosphor on a set with no panel and
/// no scaler. Nothing else on the machine can know it: the two ends are a
/// client's commit and the kernel's own vblank timestamp, and only the
/// compositor sees both.
pub fn latency_path() -> PathBuf {
    super::state_dir().join("display.latency")
}

/// The last figure the display process wrote: milliseconds, frames, and how
/// many frames it was measured over. `None` when the tube is not up, or has
/// not shown anything yet.
pub fn latency() -> Option<(f64, f64, usize)> {
    latency_in(&std::fs::read_to_string(latency_path()).ok()?)
}

/// Half a line of three numbers, and nothing shown unless all three are
/// there: a truncated file is what a reader finds while the writer is part
/// way through, and half a measurement is not a measurement.
fn latency_in(text: &str) -> Option<(f64, f64, usize)> {
    let mut parts = text.split_whitespace();
    let ms = parts.next()?.parse().ok()?;
    let frames = parts.next()?.parse().ok()?;
    let samples = parts.next()?.parse().ok()?;
    Some((ms, frames, samples))
}

pub fn log_path() -> PathBuf {
    super::state_dir().join("display.log")
}

struct Card(OwnedFd);
impl AsFd for Card {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}
impl drm::Device for Card {}
impl drm::control::Device for Card {}

/// True when the kernel marks the connector non-desktop, which is what makes
/// the compositor offer it for leasing (scripts/crt-lease-setup.sh).
pub fn leaseable(connector: &str) -> bool {
    use drm::control::Device as _;
    let want = connector
        .trim_start_matches("card")
        .trim_start_matches(|c: char| c.is_ascii_digit() || c == '-');
    let (kind, num) = match want.rsplit_once('-') {
        Some((k, n)) => (k.replace('-', ""), n.to_string()),
        None => return false,
    };
    for card in ["/dev/dri/card1", "/dev/dri/card0", "/dev/dri/card2"] {
        let Ok(f) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(card)
        else {
            continue;
        };
        let dev = Card(OwnedFd::from(f));
        let Ok(res) = dev.resource_handles() else {
            continue;
        };
        for h in res.connectors() {
            let Ok(info) = dev.get_connector(*h, false) else {
                continue;
            };
            let name = format!("{:?}", info.interface()).replace('-', "");
            if name != kind || info.interface_id().to_string() != num {
                continue;
            }
            let Ok(props) = dev.get_properties(*h) else {
                continue;
            };
            for (pid, val) in props.iter() {
                if let Ok(pi) = dev.get_property(*pid)
                    && pi.name().to_str().unwrap_or("") == "non-desktop"
                {
                    return *val == 1;
                }
            }
        }
    }
    false
}

pub fn running() -> bool {
    let Ok(pid) = std::fs::read_to_string(pid_path()) else {
        return false;
    };
    let Ok(pid) = pid.trim().parse::<i32>() else {
        return false;
    };
    // Alive is not enough: the number in the file can have been handed to
    // something else entirely since it was written.
    super::pid_runs(pid, "flyback")
}

fn binary() -> PathBuf {
    let name = "flyback";
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(name))
}

/// Start the display process on `connector` and wait for its socket.
pub fn start(connector: &str) -> Result<String, String> {
    start_with_sink(connector, None)
}

/// Start the display process; `sink` is the PipeWire sink whose monitor
/// becomes the audio track of recordings.
pub fn start_with_sink(connector: &str, sink: Option<&str>) -> Result<String, String> {
    if running() {
        return Ok("already running".into());
    }
    let _ = std::fs::create_dir_all(super::state_dir());
    let log = crate::logfile::open(&log_path()).map_err(|e| e.to_string())?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(binary());
    if let Some(s) = sink {
        cmd.env("OMACRT_SINK", s);
    }
    cmd.arg("run")
        .arg(connector)
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
    let child = cmd.spawn().map_err(|e| e.to_string())?;
    let _ = std::fs::write(pid_path(), child.id().to_string());
    let socket = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(SOCKET);
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(6) {
        if socket.exists() && ctl_path().exists() {
            return Ok(format!("up, {SOCKET}"));
        }
        if !running() {
            let tail = std::fs::read_to_string(log_path())
                .ok()
                .and_then(|t| t.lines().last().map(str::to_string))
                .unwrap_or_default();
            return Err(format!("display process exited: {tail}"));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("display process did not come up".into())
}

pub fn stop() -> String {
    if !running() {
        let _ = std::fs::remove_file(pid_path());
        return "not running".into();
    }
    let _ = send("quit");
    let pid: i32 = std::fs::read_to_string(pid_path())
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    for _ in 0..30 {
        std::thread::sleep(Duration::from_millis(100));
        if !running() {
            let _ = std::fs::remove_file(pid_path());
            return "stopped".into();
        }
    }
    if pid > 0 && super::pid_runs(pid, "flyback") {
        unsafe { libc::kill(pid, libc::SIGTERM) };
    }
    std::thread::sleep(Duration::from_millis(300));
    let _ = std::fs::remove_file(pid_path());
    "stopped (terminated)".into()
}

/// One line to the display process: `top <app_id>`, `mode <modeline>`, `quit`.
pub fn send(line: &str) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(ctl_path())?;
    writeln!(f, "{line}")
}

/// Bring a client's window to the front of the tube.
pub fn raise(app_id: &str) -> bool {
    send(&format!("top {app_id}")).is_ok()
}

/// Switch the tube to another modeline, live.
pub fn mode(modeline: &str) -> bool {
    send(&format!("mode {modeline}")).is_ok()
}

/// Environment for a program that should appear on the tube.
pub fn env(cmd: &mut Command) {
    cmd.env("WAYLAND_DISPLAY", SOCKET)
        .env("SDL_VIDEODRIVER", "wayland")
        .env_remove("DISPLAY");
}

/// True inside a process that was started for the tube.
pub fn on_tube() -> bool {
    std::env::var("WAYLAND_DISPLAY")
        .map(|v| v == SOCKET)
        .unwrap_or(false)
}

/// Open the desktop monitor window (preview of the tube, keyboard to the
/// tube) and give it focus, so typing goes to the program on the tube.
pub fn monitor_focus() -> String {
    if send("monitor on").is_err() {
        return "display process not reachable".into();
    }
    for _ in 0..30 {
        std::thread::sleep(Duration::from_millis(100));
        if monitor_open() {
            super::output::focus_class("omacrt-monitor");
            return "keyboard on the tube: the OmaCRT window has focus (close it to stop)".into();
        }
    }
    "monitor window did not appear".into()
}

/// Record the tube (picture and its audio) to an mp4 until `record_stop`.
pub fn record_start(path: &str, sink: Option<&str>) -> std::io::Result<()> {
    match sink {
        Some(s) => send(&format!("record start {path} {s}")),
        None => send(&format!("record start {path}")),
    }
}

pub fn record_stop() -> std::io::Result<()> {
    send("record stop")
}

#[cfg(test)]
mod latency_tests {
    use super::latency_in;

    #[test]
    fn the_three_numbers_the_display_writes_come_back() {
        assert_eq!(latency_in("16.52 0.99 300\n"), Some((16.52, 0.99, 300)));
    }

    #[test]
    fn a_half_written_file_is_no_measurement() {
        for half in ["", "16.52", "16.52 0.99", "16.52 0.99 \n", "x y z"] {
            assert_eq!(latency_in(half), None, "{half:?}");
        }
    }
}
