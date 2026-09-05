//! Video playback through mpv, controlled over its JSON IPC socket so the
//! shell keeps the pad and draws its own 240p overlay while mpv paints the
//! picture. Interlaced sources are left interlaced: on a CRT that is the point.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

pub const EXTENSIONS: [&str; 10] = [
    "mp4", "mkv", "avi", "mpg", "mpeg", "ts", "m4v", "webm", "mov", "vob",
];

/// Build the mpv command for one file. `socket` is the IPC path.
pub fn command(mpv: &str, file: &Path, socket: &Path) -> std::process::Command {
    let mut cmd = std::process::Command::new(mpv);
    cmd.arg("--fs")
        .arg("--no-terminal")
        .arg("--really-quiet")
        .arg("--no-osc")
        .arg("--osd-level=0")
        .arg("--no-input-default-bindings")
        .arg("--keep-open=no")
        .arg("--deinterlace=no")
        .arg("--save-position-on-quit")
        .arg("--hwdec=auto-safe")
        .arg(format!("--input-ipc-server={}", socket.display()))
        .arg(file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    cmd
}

/// State mirrored from mpv while a video plays.
pub struct Player {
    socket: PathBuf,
    stream: Option<UnixStream>,
    buf: Vec<u8>,
    next_connect: f64,
    next_poll: f64,
    pub time: f64,
    pub duration: f64,
    pub paused: bool,
    pub title: String,
    pub connected: bool,
}

impl Player {
    pub fn new(socket: PathBuf, title: &str) -> Self {
        let _ = std::fs::remove_file(&socket);
        Self {
            socket,
            stream: None,
            buf: Vec::new(),
            next_connect: 0.0,
            next_poll: 0.0,
            time: 0.0,
            duration: 0.0,
            paused: false,
            title: title.to_string(),
            connected: false,
        }
    }

    fn send(&mut self, json: &str) {
        if let Some(s) = self.stream.as_mut() {
            let mut line = json.to_string();
            line.push('\n');
            if s.write_all(line.as_bytes()).is_err() {
                self.stream = None;
                self.connected = false;
            }
        }
    }

    /// Fire-and-forget mpv command, e.g. `["cycle", "pause"]`.
    pub fn command(&mut self, args: &[&str]) {
        let list: Vec<String> = args
            .iter()
            .map(|a| format!("\"{}\"", a.replace('"', "\\\"")))
            .collect();
        self.send(&format!("{{\"command\":[{}]}}", list.join(",")));
    }

    pub fn seek(&mut self, secs: i32) {
        let v = secs.to_string();
        self.command(&["seek", &v, "relative"]);
    }

    pub fn volume(&mut self, delta: i32) {
        let v = delta.to_string();
        self.command(&["add", "volume", &v]);
    }

    pub fn toggle_pause(&mut self) {
        self.command(&["cycle", "pause"]);
        self.paused = !self.paused;
    }

    pub fn quit(&mut self) {
        self.command(&["quit"]);
    }

    /// Connect when the socket appears, ask for the properties we show, read
    /// whatever mpv answered. Non blocking; call once per frame.
    pub fn poll(&mut self, now: f64) {
        if self.stream.is_none() && now >= self.next_connect {
            self.next_connect = now + 0.25;
            if let Ok(s) = UnixStream::connect(&self.socket) {
                let _ = s.set_nonblocking(true);
                self.stream = Some(s);
                self.connected = true;
            }
        }
        if self.stream.is_none() {
            return;
        }
        if now >= self.next_poll {
            self.next_poll = now + 0.5;
            self.send("{\"command\":[\"get_property\",\"time-pos\"],\"request_id\":1}");
            self.send("{\"command\":[\"get_property\",\"duration\"],\"request_id\":2}");
            self.send("{\"command\":[\"get_property\",\"pause\"],\"request_id\":3}");
            self.send("{\"command\":[\"get_property\",\"media-title\"],\"request_id\":4}");
        }
        let mut chunk = [0u8; 4096];
        loop {
            let n = match self.stream.as_mut().unwrap().read(&mut chunk) {
                Ok(0) => {
                    self.stream = None;
                    self.connected = false;
                    break;
                }
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => {
                    self.stream = None;
                    self.connected = false;
                    break;
                }
            };
            self.buf.extend_from_slice(&chunk[..n]);
            if n < chunk.len() {
                break;
            }
        }
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=pos).collect();
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&line[..line.len() - 1]) {
                self.apply(&v);
            }
        }
    }

    fn apply(&mut self, v: &serde_json::Value) {
        let Some(id) = v.get("request_id").and_then(|x| x.as_u64()) else {
            return;
        };
        let data = v.get("data");
        match id {
            1 => {
                if let Some(t) = data.and_then(|d| d.as_f64()) {
                    self.time = t;
                }
            }
            2 => {
                if let Some(d) = data.and_then(|d| d.as_f64()) {
                    self.duration = d;
                }
            }
            3 => {
                if let Some(p) = data.and_then(|d| d.as_bool()) {
                    self.paused = p;
                }
            }
            4 => {
                if let Some(t) = data.and_then(|d| d.as_str()) {
                    if !t.is_empty() {
                        self.title = t.to_string();
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn clock(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}
