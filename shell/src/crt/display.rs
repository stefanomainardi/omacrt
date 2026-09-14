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

/// Written by the display process every time it programs a timing, and
/// removed when it stops.
///
/// The compositor is the only thing that knows what the television is being
/// given: it holds the lease, it makes the atomic commit, and the kernel
/// tells it whether the commit landed. Everything else was guessing, and
/// guessing wrong - `status` used to rebuild a timing from the configuration
/// and the saved state and once reported a 251 line mode that had never been
/// programmed, which cost ten minutes of a diagnosis.
pub fn mode_path() -> PathBuf {
    super::state_dir().join("display.mode")
}

/// The timing the tube is actually running, as the compositor last wrote it.
///
/// `None` when the file is missing, which means the display process is not
/// up or is older than this: a caller should say it does not know rather
/// than fall back on arithmetic.
pub fn current_mode() -> Option<super::output::Modeline> {
    let text = std::fs::read_to_string(mode_path()).ok()?;
    super::output::Modeline::parse(text.trim())
}

/// What the display process last measured about itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Latency {
    /// From the commit that drew a frame to the start of its scanout.
    pub ms: f64,
    /// The same, as a fraction of the frame the television is being given.
    pub frames: f64,
    /// How many frames it was measured over.
    pub samples: usize,
    /// What the television is actually being given, which under a variable
    /// refresh rate is not the mode's own rate.
    pub hz: f64,
    /// The tail, written since the median on its own turned out to hide the
    /// thing people actually see. `None` when reading an older file.
    ///
    /// A frame that arrives late once every few seconds is three samples in
    /// three hundred: it does not move a median at all, and it is exactly
    /// what somebody watching calls a glitch.
    pub tail: Option<Tail>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tail {
    /// Commit to scanout at the 95th percentile, and at its worst.
    pub p95: f64,
    pub worst: f64,
    /// The shortest and longest frame the tube was actually given. Under a
    /// variable refresh rate these two apart is what a television reacts to.
    pub frame_min: f64,
    pub frame_max: f64,
    /// The same frames counted in lines, which is the unit a television's own
    /// vertical circuit works in: how many of them ran more than two lines
    /// past the mode's vertical total, and how long the longest one was.
    ///
    /// A set's vertical countdown accepts sync inside a narrow window once it
    /// has locked, and a field that leaves it is retraced at the edge of the
    /// window instead of on the sync that arrived. Two lines is where that
    /// begins on a 60 Hz standard. `None` when reading a file written by a
    /// display process that predates this.
    pub window: Option<Window>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    /// Frames that ran more than two lines past the mode's vertical total.
    pub over: usize,
    /// The longest frame, in lines.
    pub longest_lines: f64,
}

/// The last figure the display process wrote. `None` when the tube is not up,
/// or has not shown anything yet.
pub fn latency() -> Option<Latency> {
    latency_in(&std::fs::read_to_string(latency_path()).ok()?)
}

/// Half a line of four numbers, and nothing shown unless all four are there:
/// a truncated file is what a reader finds while the writer is part way
/// through, and half a measurement is not a measurement.
fn latency_in(text: &str) -> Option<Latency> {
    let mut parts = text.split_whitespace();
    let mut out = Latency {
        ms: parts.next()?.parse().ok()?,
        frames: parts.next()?.parse().ok()?,
        samples: parts.next()?.parse().ok()?,
        hz: parts.next()?.parse().ok()?,
        tail: None,
    };
    // The tail is all four or none of it: a file caught part way through a
    // write is the case this whole function exists for. Six is the same tail
    // with the two line counts after it, which a display process older than
    // they are does not write: the pair is taken together or not at all, for
    // the same reason.
    let rest: Vec<f64> = parts.filter_map(|p| p.parse().ok()).collect();
    let tail = |p95, worst, frame_min, frame_max, window| {
        Some(Tail {
            p95,
            worst,
            frame_min,
            frame_max,
            window,
        })
    };
    out.tail = match rest[..] {
        [p95, worst, frame_min, frame_max] => tail(p95, worst, frame_min, frame_max, None),
        [p95, worst, frame_min, frame_max, over, longest_lines] => tail(
            p95,
            worst,
            frame_min,
            frame_max,
            Some(Window {
                over: over.max(0.0) as usize,
                longest_lines,
            }),
        ),
        _ => None,
    };
    Some(out)
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
    pid().is_some()
}

/// The compositor's process id while it is up.
pub fn pid() -> Option<i32> {
    let pid: i32 = std::fs::read_to_string(pid_path())
        .ok()?
        .trim()
        .parse()
        .ok()?;
    // Alive is not enough: the number in the file can have been handed to
    // something else entirely since it was written.
    super::pid_runs(pid, BINARY).then_some(pid)
}

/// Where the compositor's binary is.
///
/// Beside this program first, because a build run out of a checkout must use
/// its own compositor and not an older installed one. Then along PATH, which
/// is the case this order exists for: the bar plugin runs its own copy of
/// this program from the plugin folder, where nothing else of the install
/// sits. Then the workspace's release directory, for a run straight out of
/// `cargo build`.
pub fn binary() -> PathBuf {
    let exe = std::env::current_exe().ok();
    resolve(BINARY, exe.as_deref(), std::env::var_os("PATH").as_deref())
}

/// The compositor's process and file name. It is what a reader of the bar or
/// the report is being told is driving the tube, so it is public.
pub const BINARY: &str = "flyback";

/// The search, with its two inputs passed in so it can be tested: `exe` is
/// this program's own path and `path` the PATH to walk.
///
/// Returns the bare name when nothing is found, so that the caller's error
/// still says which binary was missing rather than only that something was.
fn resolve(name: &str, exe: Option<&std::path::Path>, path: Option<&std::ffi::OsStr>) -> PathBuf {
    let here = exe.and_then(|p| p.parent());
    if let Some(d) = here {
        let beside = d.join(name);
        if beside.is_file() {
            return beside;
        }
    }
    if let Some(path) = path {
        for dir in std::env::split_paths(path) {
            let p = dir.join(name);
            if p.is_file() {
                return p;
            }
        }
    }
    // A checkout: `shell/target/release` beside the binary's own directory.
    if let Some(d) = here {
        let built = d.join("../shell/target/release").join(name);
        if built.is_file() {
            return built;
        }
    }
    PathBuf::from(name)
}

/// The compositor's binary: while it is up, the file the running process was
/// started from, and otherwise the one a start would use.
///
/// The two are not always the same file. A report that named the second
/// while the first was on the air would be describing a program nobody is
/// running, which is the sort of thing a status report exists to rule out.
pub fn binary_in_use() -> PathBuf {
    pid()
        .and_then(|p| std::fs::read_link(format!("/proc/{p}/exe")).ok())
        .unwrap_or_else(binary)
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
    let bin = binary();
    let mut cmd = Command::new(&bin);
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
    // Named, because the bare error says only that something was not there.
    // The case that produced it: the bar plugin runs its own copy of this
    // program, and an old copy there looks for a compositor under the name
    // it had when that copy was built.
    let child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            let whose = std::env::current_exe()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "this program".into());
            format!("{BINARY}: not found beside {whose} nor on PATH")
        } else {
            format!("{}: {e}", bin.display())
        }
    })?;
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
    if pid > 0 && super::pid_runs(pid, BINARY) {
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

/// Switch the tube to another modeline, live, and say whether it happened.
///
/// The pipe carries no reply, so the answer comes from the file the
/// compositor writes after a modeset lands. Waiting for it is the difference
/// between reporting a request and reporting an outcome: this used to return
/// whether the *write to the pipe* succeeded, which is true even when the
/// compositor refuses the timing, and the caller then saved the new mode to
/// the state file and printed a confirmation for something that never
/// happened.
///
/// A compositor that already has the timing writes nothing, so an unchanged
/// mode is a success as soon as the file says what was asked for.
pub fn mode(modeline: &str) -> bool {
    let Some(want) = super::output::Modeline::parse(modeline) else {
        return false;
    };
    if send(&format!("mode {modeline}")).is_err() {
        return false;
    }
    // A modeset on this chain takes 180 to 230 ms inside the kernel's own
    // call, so half a second is several times what it needs and still short
    // enough not to be felt by anything waiting on it.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    while std::time::Instant::now() < deadline {
        if current_mode().as_ref() == Some(&want) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // An older display process writes no such file, and refusing to work
    // with one would be worse than trusting it: the pipe was written, so
    // report what the pipe can report.
    !mode_path().exists()
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
mod resolve_tests {
    use super::{BINARY, resolve};
    use std::path::{Path, PathBuf};

    /// A directory holding an executable file of the given name.
    fn with_binary(root: &Path, sub: &str) -> PathBuf {
        let dir = root.join(sub);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(BINARY), b"#!/bin/true\n").unwrap();
        dir
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("omacrt-resolve-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_one_beside_this_program_wins() {
        let root = tmp("beside");
        let here = with_binary(&root, "install");
        let elsewhere = with_binary(&root, "onpath");
        let got = resolve(
            BINARY,
            Some(&here.join("omacrt")),
            Some(elsewhere.as_os_str()),
        );
        assert_eq!(got, here.join(BINARY));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The bar plugin: its folder holds the helper and nothing else, so the
    /// compositor has to be found along PATH.
    #[test]
    fn a_lone_helper_finds_it_on_path() {
        let root = tmp("path");
        let plugin = root.join("plugin/bin");
        std::fs::create_dir_all(&plugin).unwrap();
        let installed = with_binary(&root, "local-bin");
        let got = resolve(
            BINARY,
            Some(&plugin.join("omacrt")),
            Some(installed.as_os_str()),
        );
        assert_eq!(got, installed.join(BINARY));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Nothing found is the bare name, so the caller can say what was missing.
    #[test]
    fn nothing_found_still_names_it() {
        let root = tmp("none");
        let empty = root.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let got = resolve(BINARY, Some(&empty.join("omacrt")), Some(empty.as_os_str()));
        assert_eq!(got, PathBuf::from(BINARY));
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod latency_tests {
    use super::latency_in;

    #[test]
    fn the_numbers_the_display_writes_come_back() {
        let l = latency_in("16.52 0.99 300 60.041\n").expect("a measurement");
        assert_eq!(
            (l.ms, l.frames, l.samples, l.hz),
            (16.52, 0.99, 300, 60.041)
        );
    }

    /// The pair of line counts is what a display process writes now, and a
    /// reader has to keep working against one that does not write them.
    #[test]
    fn the_line_counts_are_read_when_they_are_there_and_missed_when_they_are_not() {
        let old = latency_in("16.52 0.99 300 60.041 17.0 18.0 16.50 16.80\n").expect("a tail");
        let tail = old.tail.expect("four numbers of tail");
        assert_eq!(tail.frame_max, 16.80);
        assert_eq!(
            tail.window, None,
            "an older display process writes no lines"
        );

        let now =
            latency_in("16.52 0.99 300 60.041 17.0 18.0 16.50 16.80 3 291.0\n").expect("a tail");
        let w = now.tail.expect("a tail").window.expect("the line counts");
        assert_eq!((w.over, w.longest_lines), (3, 291.0));
    }

    /// Caught between the eighth number and the tenth, the pair is not a
    /// measurement, and neither is the tail it would have belonged to.
    #[test]
    fn half_of_the_line_counts_is_none_of_the_tail() {
        let half = latency_in("16.52 0.99 300 60.041 17.0 18.0 16.50 16.80 3\n").expect("the four");
        assert_eq!(half.ms, 16.52, "the four numbers still read");
        assert_eq!(half.tail, None, "and the rest of the line does not");
    }

    #[test]
    fn a_half_written_file_is_no_measurement() {
        for half in [
            "",
            "16.52",
            "16.52 0.99",
            "16.52 0.99 300",
            "16.52 0.99 300 \n",
            "w x y z",
        ] {
            assert_eq!(latency_in(half), None, "{half:?}");
        }
    }
}
