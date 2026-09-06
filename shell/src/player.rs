//! Video playback through mpv, controlled over its JSON IPC socket so the
//! shell keeps the pad and draws its own 240p overlay while mpv paints the
//! picture. Interlaced sources are left interlaced: on a CRT that is the point.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

pub const EXTENSIONS: [&str; 10] = [
    "mp4", "mkv", "avi", "mpg", "mpeg", "ts", "m4v", "webm", "mov", "vob",
];

/// Key bindings mpv applies itself while it has keyboard focus: the same
/// actions the shell sends over IPC for the pad.
pub const INPUT_CONF: &str = "# Written by omarchy-crt-shell
ENTER cycle pause
SPACE cycle pause
RIGHT seek 10
LEFT seek -10
UP add volume 5
DOWN add volume -5
ESC quit
BS quit
q quit
";

/// On screen display drawn by mpv itself, in the shell's language: a dark
/// bar at the bottom with the progress, times, state and the command hints,
/// shown for a few seconds at start and after every command, always while
/// paused. Colors arrive through script-opts.
pub const OSD_LUA: &str = r#"-- Written by omarchy-crt-shell
local mp = require "mp"
local options = require "mp.options"
local o = { accent = "7aa2f7", dim = "565f89", paper = "c0caf5", selection = "292e42", hints = "Enter pause   < > seek 10s   ^ v volume   Esc stop" }
options.read_options(o, "omarchycrt")

local overlay = mp.create_osd_overlay("ass-events")
local hide_timer = nil
local visible = false

local function ass_color(hex)
  -- RRGGBB to ASS &HBBGGRR&
  return "&H" .. hex:sub(5, 6) .. hex:sub(3, 4) .. hex:sub(1, 2) .. "&"
end

local function clock(s)
  if not s or s < 0 then s = 0 end
  s = math.floor(s)
  local h = math.floor(s / 3600)
  local m = math.floor((s % 3600) / 60)
  local sec = s % 60
  if h > 0 then return string.format("%d:%02d:%02d", h, m, sec) end
  return string.format("%d:%02d", m, sec)
end

local function render()
  local w, h = mp.get_osd_size()
  if not w or w == 0 then w, h = 640, 480 end
  local pos = mp.get_property_number("time-pos", 0)
  local dur = mp.get_property_number("duration", 0)
  local paused = mp.get_property_native("pause", false)
  local vol = mp.get_property_number("volume", 100)
  local title = mp.get_property("media-title", ""):gsub("%.[%w]+$", "")
  if #title > 40 then title = title:sub(1, 38) .. ".." end
  local accent, dim, paper, sel = ass_color(o.accent), ass_color(o.dim), ass_color(o.paper), ass_color(o.selection)

  -- Geometry in OSD pixels; sizes scale with height so 240 and 480 lines both read.
  local unit = h / 240
  local margin = math.floor(16 * unit)
  local bar_h = math.floor(4 * unit)
  local box_h = math.floor(52 * unit)
  local box_y = h - margin - box_h
  local fs_big = math.floor(9 * unit) * 2
  local fs_small = math.floor(7 * unit) * 2
  local font = "\\fnmonospace"

  local a = {}
  -- Box.
  a[#a+1] = string.format("{\\an7\\pos(0,0)\\bord0\\shad0\\1c&H000000&\\1a&H28&\\p1}m %d %d l %d %d l %d %d l %d %d{\\p0}",
    margin - 6 * unit, box_y - 4 * unit, w - margin + 6 * unit, box_y - 4 * unit, w - margin + 6 * unit, h - margin + 4 * unit, margin - 6 * unit, h - margin + 4 * unit)
  -- Title.
  a[#a+1] = string.format("{\\an7\\pos(%d,%d)\\bord0\\shad0%s\\fs%d\\1c%s}%s",
    margin + 8 * unit, box_y + 5 * unit, font, fs_big, paper, title:gsub("[{}]", ""))
  -- Progress track and fill.
  local track_y = box_y + 22 * unit
  local track_x0, track_x1 = margin + 8 * unit, w - margin - 8 * unit
  a[#a+1] = string.format("{\\an7\\pos(0,0)\\bord0\\shad0\\1c%s\\p1}m %d %d l %d %d l %d %d l %d %d{\\p0}",
    sel, track_x0, track_y, track_x1, track_y, track_x1, track_y + bar_h, track_x0, track_y + bar_h)
  if dur > 0 then
    local fx = track_x0 + (track_x1 - track_x0) * math.max(0, math.min(1, pos / dur))
    a[#a+1] = string.format("{\\an7\\pos(0,0)\\bord0\\shad0\\1c%s\\p1}m %d %d l %d %d l %d %d l %d %d{\\p0}",
      accent, track_x0, track_y, fx, track_y, fx, track_y + bar_h, track_x0, track_y + bar_h)
    -- Knob.
    a[#a+1] = string.format("{\\an7\\pos(0,0)\\bord0\\shad0\\1c%s\\p1}m %d %d l %d %d l %d %d l %d %d{\\p0}",
      paper, fx - unit, track_y - unit, fx + unit, track_y - unit, fx + unit, track_y + bar_h + unit, fx - unit, track_y + bar_h + unit)
  end
  -- Times, state, volume.
  local line_y = track_y + bar_h + 5 * unit
  a[#a+1] = string.format("{\\an7\\pos(%d,%d)\\bord0\\shad0%s\\fs%d\\1c%s}%s / %s",
    track_x0, line_y, font, fs_small, paper, clock(pos), clock(dur))
  local state = paused and "PAUSED" or "PLAYING"
  a[#a+1] = string.format("{\\an9\\pos(%d,%d)\\bord0\\shad0%s\\fs%d\\1c%s}%s   VOL %d%%",
    track_x1, line_y, font, fs_small, paused and accent or dim, state, math.floor(vol))
  -- Hints.
  a[#a+1] = string.format("{\\an7\\pos(%d,%d)\\bord0\\shad0%s\\fs%d\\1c%s}%s",
    track_x0, line_y + 10 * unit, font, fs_small, dim, o.hints)

  overlay.res_x, overlay.res_y = w, h
  overlay.data = table.concat(a, "\n")
  overlay:update()
end

local function hide()
  if mp.get_property_native("pause", false) then return end
  overlay:remove()
  visible = false
end

local function show(seconds)
  visible = true
  render()
  if hide_timer then hide_timer:kill() end
  hide_timer = mp.add_timeout(seconds or 3, hide)
end

mp.observe_property("pause", "native", function() show(3) end)
mp.observe_property("volume", "number", function() show(2) end)
mp.register_event("seek", function() show(3) end)
mp.register_event("file-loaded", function() show(4) end)
mp.observe_property("time-pos", "number", function() if visible then render() end end)
"#;

/// Build the mpv command for one file. `socket` is the IPC path,
/// `input_conf` the key bindings, `osd` the Lua overlay, `colors` the
/// theme as `accent,dim,paper,selection` hex values.
pub fn command(
    mpv: &str,
    file: &Path,
    socket: &Path,
    input_conf: &Path,
    osd: &Path,
    colors: [&str; 4],
) -> std::process::Command {
    let mut cmd = std::process::Command::new(mpv);
    // Our own Wayland app id, so window rules pin this player to the tube
    // and leave a desktop mpv alone.
    cmd.arg("--wayland-app-id=omarchy-crt-player").arg("--fs")
        .arg("--no-terminal")
        .arg("--really-quiet")
        .arg("--no-osc")
        .arg("--osd-level=0")
        .arg("--no-input-default-bindings")
        .arg(format!("--input-conf={}", input_conf.display()))
        .arg(format!("--script={}", osd.display()))
        .arg(format!(
            "--script-opts=omarchycrt-accent={},omarchycrt-dim={},omarchycrt-paper={},omarchycrt-selection={}",
            colors[0], colors[1], colors[2], colors[3]
        ))
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
