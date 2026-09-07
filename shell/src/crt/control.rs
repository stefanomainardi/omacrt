//! Live control channel of the launcher.
//!
//! A named pipe in the state directory takes one input name per line and the
//! launcher feeds each line through the same path as a key press or a pad
//! button. `omarchy-crt shell key right fire` writes to it, so a script, the
//! bar plugin or a test can drive the menu without a virtual keyboard, whose
//! events reach SDL only some of the time under Wayland.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

/// Canonical input names, in the order the help text lists them.
pub const INPUTS: &[&str] = &[
    "up", "down", "left", "right", "fire", "back", "fav", "alt", "start", "home", "menu",
    "search", "osk", "del", "next", "prev", "first", "last",
];

pub fn path() -> PathBuf {
    super::state_dir().join("shell.ctl")
}

/// Map a key or button name to a canonical input name.
pub fn normalize(name: &str) -> Option<&'static str> {
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "up" | "k" => "up",
        "down" | "j" => "down",
        "left" | "h" => "left",
        "right" | "l" => "right",
        "fire" | "a" | "enter" | "return" | "space" | "ok" => "fire",
        "back" | "b" | "esc" | "escape" => "back",
        "fav" | "y" | "f" => "fav",
        "alt" | "x" => "alt",
        "start" => "start",
        "home" => "home",
        "menu" | "pause" => "menu",
        "search" | "find" | "/" => "search",
        "osk" | "keyboard" | "lt" => "osk",
        "del" | "delete" | "backspace" => "del",
        "next" | "pagedown" | "rb" => "next",
        "prev" | "pageup" | "lb" => "prev",
        "first" => "first",
        "last" | "end" => "last",
        _ => return None,
    })
}

/// Create the pipe and read it on a thread, one line per message.
pub fn listen() -> std::io::Result<Receiver<String>> {
    let p = path();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let _ = std::fs::remove_file(&p);
    let c = std::ffi::CString::new(p.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::other("bad pipe path"))?;
    if unsafe { libc::mkfifo(c.as_ptr(), 0o600) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        loop {
            // Blocks until a writer opens the pipe; EOF once it closes, then wait
            // for the next one.
            let Ok(f) = std::fs::File::open(&p) else {
                std::thread::sleep(Duration::from_millis(200));
                continue;
            };
            for line in BufReader::new(f).lines().map_while(Result::ok) {
                let line = line.trim().to_string();
                if !line.is_empty() && tx.send(line).is_err() {
                    return;
                }
            }
        }
    });
    Ok(rx)
}

/// Type text into the launcher's search bar (opened first when closed).
pub fn send_text(text: &str) -> std::io::Result<()> {
    let line = format!("type {}", text.replace(['\n', '\r'], " "));
    send(&[line.as_str()])
}

/// Send inputs to the running launcher. Fails when nothing listens.
pub fn send(inputs: &[&str]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path())
        .map_err(|e| match e.raw_os_error() {
            Some(libc::ENXIO) | Some(libc::ENOENT) => {
                std::io::Error::other("the launcher is not running")
            }
            _ => e,
        })?;
    for i in inputs {
        writeln!(f, "{i}")?;
    }
    Ok(())
}
