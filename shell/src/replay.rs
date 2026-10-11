//! Instant replay: the last half minute of a game, framed for a social feed.
//!
//! While a game is on top the display process keeps a ring of two second
//! pieces of the tube under the cache, with the television's sound (see
//! `replay start` in flyback). The launcher stops the ring when the pause
//! menu opens, so the newest piece ends on the play rather than on the menu,
//! and starts it again on the way back.
//!
//! A replay is made from the ring after the fact: the newest pieces joined,
//! the last thirty seconds cut from them, and the game put into a frame the
//! launcher draws itself with its own font and the project's mark. Nothing of
//! this happens while the game runs; the frame, the opening and the closing
//! card are drawn when somebody asks for a replay, and ffmpeg does the rest
//! in the background.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{Receiver, channel};

use crate::fb::{Color, Framebuffer, lerp_color};
use crate::paint::BAYER;

/// How much play a replay holds.
const CLIP_SECS: f64 = 30.0;
/// The ring's frame rate and the replay's.
const FPS: u32 = 30;
/// Pieces taken from the ring: two seconds each, a little over the clip so a
/// piece still being written at the newest end does not leave it short.
const PIECES: usize = 17;

const GROUND_A: Color = 0x0b0d14;
const GROUND_B: Color = 0x141826;
const LINE: Color = 0x2a2e45;
const INK: Color = 0xc0caf5;
const DIM: Color = 0x8b93b8;
const ACCENT: Color = 0x9ece6a;
const AMBER: Color = 0xe0af68;
const LINK: Color = 0x7aa2f7;

/// The project's mark as published, the same file the README shows.
const LOGO_PNG: &[u8] = include_bytes!("../../docs/logo.png");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// 4:5, the size a phone's feed shows largest. The default.
    Feed,
    /// 9:16 for Reels, TikTok and Shorts.
    Vertical,
    /// 16:9 for YouTube and the desktop.
    Wide,
}

impl Format {
    pub fn parse(s: &str) -> Option<Format> {
        match s {
            "" | "feed" | "4:5" => Some(Format::Feed),
            "vertical" | "9:16" => Some(Format::Vertical),
            "wide" | "16:9" => Some(Format::Wide),
            _ => None,
        }
    }

    fn size(self) -> (usize, usize) {
        match self {
            Format::Feed => (1080, 1350),
            Format::Vertical => (1080, 1920),
            Format::Wide => (1920, 1080),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Format::Feed => "feed",
            Format::Vertical => "vertical",
            Format::Wide => "wide",
        }
    }
}

/// Where the ring lives: the session's runtime folder, which is memory. The
/// ring writes about a megabyte a second for as long as a game runs, which on
/// a disk would be four gigabytes for every hour of play; in memory it is
/// never more than the seventy or so megabytes the ring holds.
pub fn ring_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && p.is_dir())
        .map(|p| p.join("omacrt-replay"))
        .unwrap_or_else(|| omacrt_shell::crt::cache_dir().join("replay"))
}

/// Where finished replays go: the desktop's videos folder.
pub fn out_dir() -> PathBuf {
    let videos = std::env::var_os("XDG_VIDEOS_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| omacrt_shell::crt::home().join("Videos"));
    videos.join("OmaCRT")
}

/// When the ring started for the game being played. A replay is cut only
/// from pieces written since then: the ring folder outlives games and keeps
/// whatever an earlier one left, and a replay of somebody else's game is
/// worse than none. `None` while no ring runs for the current game.
static SINCE: std::sync::Mutex<Option<std::time::SystemTime>> = std::sync::Mutex::new(None);

fn since() -> Option<std::time::SystemTime> {
    SINCE.lock().ok().and_then(|g| *g)
}

/// Start keeping the ring, with the television's sound when the audio is
/// routed to it. Called when a game starts and again when it resumes from
/// the pause menu; only the first call for a game sets where it begins.
pub fn start() {
    use omacrt_shell::crt::{Config, audio, display, output};
    // Off until the capture is understood: on the first evening with the
    // ring, the launcher crashed inside SDL's Wayland dispatch moments after
    // every game started it, and the ring's ffmpeg outlived every stop.
    // `OMACRT_REPLAY_RING=1` turns it on for testing.
    if std::env::var_os("OMACRT_REPLAY_RING").is_none() {
        return;
    }
    if !display::running() {
        return;
    }
    let dir = ring_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("replay: {}: {e}", dir.display());
        return;
    }
    let cfg = Config::load();
    let sink = if cfg.audio.route {
        output::pick(&cfg)
            .and_then(|c| audio::target(&c))
            .map(|t| t.sink)
    } else {
        None
    };
    match display::replay_start(&dir.to_string_lossy(), sink.as_deref()) {
        Ok(()) => {
            if let Ok(mut g) = SINCE.lock()
                && g.is_none()
            {
                *g = Some(std::time::SystemTime::now());
            }
        }
        Err(e) => eprintln!("replay: {e}"),
    }
}

pub fn stop() {
    if omacrt_shell::crt::display::running() {
        let _ = omacrt_shell::crt::display::replay_stop();
    }
}

/// The game has ended: stop the ring and forget where it began.
pub fn end() {
    stop();
    if let Ok(mut g) = SINCE.lock() {
        *g = None;
    }
}

/// What the frame says about the game.
#[derive(Clone, Debug)]
pub struct Card {
    pub title: String,
    pub system: String,
}

/// Make a replay in the background. The answer is the file written, or what
/// stood in the way, in words for the pause menu.
pub fn save(card: Card, format: Format) -> Receiver<Result<PathBuf, String>> {
    let (tx, rx) = channel();
    let Some(from) = since() else {
        let _ = tx.send(Err("the replay ring is not running for this game".into()));
        return rx;
    };
    std::thread::spawn(move || {
        let _ = tx.send(compose(&card, format, Some(from)));
    });
    rx
}

/// The newest pieces of the ring, oldest first.
fn pieces(dir: &Path, since: Option<std::time::SystemTime>) -> Vec<PathBuf> {
    let mut found: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let name = p.file_name()?.to_str()?;
            if !(name.starts_with("seg") && name.ends_with(".ts")) {
                return None;
            }
            let meta = e.metadata().ok()?;
            // An empty file is a piece ffmpeg opened and never wrote, and a
            // piece older than this game's ring is another game's.
            let when = meta.modified().ok()?;
            (meta.len() > 0 && since.is_none_or(|s| when >= s)).then_some((when, p))
        })
        .collect();
    found.sort();
    let skip = found.len().saturating_sub(PIECES);
    found.into_iter().skip(skip).map(|(_, p)| p).collect()
}

fn compose(
    card: &Card,
    format: Format,
    since: Option<std::time::SystemTime>,
) -> Result<PathBuf, String> {
    let ring = ring_dir();
    let pieces = pieces(&ring, since);
    if pieces.is_empty() {
        return Err("nothing recorded for this game yet".into());
    }
    let work = ring.join(format!("make-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
    let result = compose_in(card, format, &pieces, &work);
    let _ = std::fs::remove_dir_all(&work);
    result
}

fn compose_in(
    card: &Card,
    format: Format,
    pieces: &[PathBuf],
    work: &Path,
) -> Result<PathBuf, String> {
    // The pieces joined without decoding: each starts on a key frame.
    let list = work.join("pieces.txt");
    let text: String = pieces
        .iter()
        .map(|p| format!("file '{}'\n", p.to_string_lossy().replace('\'', "'\\''")))
        .collect();
    std::fs::write(&list, text).map_err(|e| e.to_string())?;
    let joined = work.join("joined.ts");
    run(Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "concat", "-safe", "0", "-i"])
        .arg(&list)
        .args(["-c", "copy"])
        .arg(&joined))?;
    let total = probe_duration(&joined).ok_or("the recording could not be read")?;
    let len = total.min(CLIP_SECS);
    let from = (total - len).max(0.0);
    let has_sound = probe_has_audio(&joined);

    // The frame, the opening and the closing card, drawn here.
    let (w, h) = format.size();
    let layout = Layout::of(format);
    let logo = Logo::decode();
    let ty = Type::load().ok_or("the typefaces did not load")?;
    let mut fb = Framebuffer::new(w, h);
    draw_frame(&mut fb, &ty, format, &layout, card, logo.as_ref());
    let chrome = work.join("frame.png");
    write_png(&fb, &chrome)?;
    let intro = work.join("intro");
    let outro = work.join("outro");
    std::fs::create_dir_all(&intro).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&outro).map_err(|e| e.to_string())?;
    let intro_frames = (0.8 * FPS as f64) as u32;
    let outro_frames = 2 * FPS;
    for i in 0..intro_frames {
        draw_intro(&mut fb, logo.as_ref(), i as f32 / FPS as f32);
        write_png(&fb, &intro.join(format!("f{i:04}.png")))?;
    }
    for i in 0..outro_frames {
        draw_outro(&mut fb, &ty, logo.as_ref(), card, i as f32 / FPS as f32);
        write_png(&fb, &outro.join(format!("f{i:04}.png")))?;
    }

    let out_dir = out_dir();
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let stamp = chrono::Local::now().format("%Y-%m-%d %H.%M.%S");
    let out = out_dir.join(format!("{} {stamp}.mp4", file_safe(&card.title)));

    let [gx, gy, gw, gh] = layout.game;
    let [bx, by, bw, bh] = layout.bar;
    let clip_audio = if has_sound {
        format!(
            "[1:a]aresample=48000,aformat=channel_layouts=stereo,afade=t=in:d=0.3,afade=t=out:st={:.2}:d=0.4[ca]",
            (len - 0.4).max(0.0)
        )
    } else {
        "[6:a]anull[ca]".to_string()
    };
    let graph = [
        "[0:v]split=2[c1][c2]".to_string(),
        format!("[c2]crop={bw}:{bh}:{bx}:{by}[track]"),
        format!("color=c=0x{ACCENT:06x}:s={bw}x{bh}:r={FPS}[fill]"),
        format!("[track][fill]overlay=x='-{bw}+{bw}*t/{len:.3}':y=0:eval=frame[trk]"),
        format!("[1:v]fps={FPS},scale={gw}:{gh}:flags=neighbor,setsar=1[g]"),
        format!("[c1][g]overlay={gx}:{gy}:shortest=1[v1]"),
        format!("[v1][trk]overlay={bx}:{by}:shortest=1,format=yuv420p,setsar=1[cv]"),
        clip_audio,
        format!("[2:v]fps={FPS},format=yuv420p,setsar=1[iv]"),
        format!("[3:v]fps={FPS},format=yuv420p,setsar=1[ov]"),
        "[iv][4:a][cv][ca][ov][5:a]concat=n=3:v=1:a=1[v][a]".to_string(),
    ]
    .join(";");
    // At the lowest priority and on two threads: a replay is made in the
    // background, and the next game may already be running beside it.
    let mut cmd = Command::new("nice");
    cmd.args([
        "-n",
        "19",
        "ffmpeg",
        "-v",
        "error",
        "-y",
        "-filter_complex_threads",
        "2",
    ])
    .args(["-loop", "1", "-framerate", &FPS.to_string(), "-i"])
    .arg(&chrome)
    .args([
        "-ss",
        &format!("{from:.3}"),
        "-t",
        &format!("{len:.3}"),
        "-i",
    ])
    .arg(&joined)
    .args(["-framerate", &FPS.to_string(), "-i"])
    .arg(intro.join("f%04d.png"))
    .args(["-framerate", &FPS.to_string(), "-i"])
    .arg(outro.join("f%04d.png"))
    .args([
        "-f",
        "lavfi",
        "-t",
        "0.8",
        "-i",
        "anullsrc=r=48000:cl=stereo",
    ])
    .args(["-f", "lavfi", "-t", "2", "-i", "anullsrc=r=48000:cl=stereo"])
    .args([
        "-f",
        "lavfi",
        "-t",
        &format!("{len:.3}"),
        "-i",
        "anullsrc=r=48000:cl=stereo",
    ])
    .args(["-filter_complex", &graph, "-map", "[v]", "-map", "[a]"])
    .args([
        "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-pix_fmt", "yuv420p",
    ])
    .args(["-threads", "2"])
    .args([
        "-c:a",
        "aac",
        "-b:a",
        "192k",
        "-movflags",
        "+faststart",
        "--",
    ])
    .arg(&out);
    run(&mut cmd)?;
    eprintln!("replay: {} ({}, {len:.1} s)", out.display(), format.name());
    Ok(out)
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let out = cmd.output().map_err(|e| format!("ffmpeg: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(err.lines().last().unwrap_or("ffmpeg failed").to_string())
    }
}

fn probe_duration(path: &Path) -> Option<f64> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

fn probe_has_audio(path: &Path) -> bool {
    Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a",
            "-show_entries",
            "stream=index",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .map(|o| !o.stdout.trim_ascii().is_empty())
        .unwrap_or(false)
}

/// A title as a file name: what a file manager shows, without the characters
/// a path cannot hold.
fn file_safe(title: &str) -> String {
    let s: String = title
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '\0') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let s = s.trim();
    if s.is_empty() {
        "replay".into()
    } else {
        s.to_string()
    }
}

fn write_png(fb: &Framebuffer, path: &Path) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), fb.w as u32, fb.h as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Fast);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    let mut data = Vec::with_capacity(fb.w * fb.h * 3);
    for &c in &fb.px {
        data.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8]);
    }
    writer.write_image_data(&data).map_err(|e| e.to_string())
}

// ------------------------------------------------------------------ layout

/// Where the game and its progress bar sit in a format, in pixels.
struct Layout {
    game: [i32; 4],
    bar: [i32; 4],
}

impl Layout {
    fn of(format: Format) -> Layout {
        match format {
            // The game as large as each shape allows: the whole width of
            // the feed and the whole height of the wide frame.
            Format::Feed => Layout {
                game: [0, 0, 1080, 810],
                bar: [0, 810, 1080, 8],
            },
            Format::Vertical => Layout {
                game: [40, 340, 1000, 750],
                bar: [40, 1090, 1000, 8],
            },
            Format::Wide => Layout {
                game: [0, 0, 1440, 1080],
                bar: [0, 1072, 1440, 8],
            },
        }
    }
}

/// No gradient is smooth: the ground is two colours through the same ordered
/// dither the sky uses, in four pixel cells.
fn ground(fb: &mut Framebuffer) {
    for y in 0..fb.h {
        for x in 0..fb.w {
            let b = BAYER[(y / 4) & 3][(x / 4) & 3];
            fb.px[y * fb.w + x] = if b < 5 { GROUND_B } else { GROUND_A };
        }
    }
}

// ------------------------------------------------------------------ type

/// The site's typefaces, IBM Plex, for the frame: the launcher's own pixel
/// font reads as a screen of the launcher, and a replay is a picture of the
/// game with a caption. OFL licensed, carried in the binary.
const SANS_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf");
const SANS_REGULAR: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf");
const MONO_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/IBMPlexMono-SemiBold.ttf");

struct Type {
    title: fontdue::Font,
    body: fontdue::Font,
    mono: fontdue::Font,
}

/// One run of text: a face, a size in pixels and the extra space between
/// letters that the small capitals of the labels carry.
#[derive(Clone, Copy)]
struct Style<'a> {
    font: &'a fontdue::Font,
    px: f32,
    tracking: f32,
}

impl Type {
    fn load() -> Option<Type> {
        let face =
            |b: &'static [u8]| fontdue::Font::from_bytes(b, fontdue::FontSettings::default()).ok();
        Some(Type {
            title: face(SANS_SEMIBOLD)?,
            body: face(SANS_REGULAR)?,
            mono: face(MONO_SEMIBOLD)?,
        })
    }

    fn title(&self, px: f32) -> Style<'_> {
        Style {
            font: &self.title,
            px,
            tracking: -px * 0.01,
        }
    }

    fn body(&self, px: f32) -> Style<'_> {
        Style {
            font: &self.body,
            px,
            tracking: 0.0,
        }
    }

    /// The uppercase labels: monospaced, spaced out.
    fn label(&self, px: f32, tracking: f32) -> Style<'_> {
        Style {
            font: &self.mono,
            px,
            tracking,
        }
    }
}

impl Style<'_> {
    /// How wide a line is set in this style.
    fn width(&self, text: &str) -> i32 {
        let mut w = 0.0;
        for c in text.chars() {
            w += self.font.metrics(c, self.px).advance_width + self.tracking;
        }
        (w - self.tracking).max(0.0).round() as i32
    }

    /// Set a line with its top at `y`, blending each glyph's coverage over
    /// what is already drawn. Returns its width.
    fn draw(&self, fb: &mut Framebuffer, x: i32, y: i32, text: &str, colour: Color) -> i32 {
        let ascent = self
            .font
            .horizontal_line_metrics(self.px)
            .map(|m| m.ascent)
            .unwrap_or(self.px * 0.8);
        let baseline = y as f32 + ascent;
        let mut pen = x as f32;
        for c in text.chars() {
            let (m, coverage) = self.font.rasterize(c, self.px);
            let gx = pen.round() as i32 + m.xmin;
            let gy = (baseline - m.height as f32 - m.ymin as f32).round() as i32;
            for row in 0..m.height {
                for col in 0..m.width {
                    let a = coverage[row * m.width + col];
                    if a == 0 {
                        continue;
                    }
                    let (dx, dy) = (gx + col as i32, gy + row as i32);
                    let under = fb.at(dx, dy);
                    fb.put(dx, dy, lerp_color(under, colour, a as f32 / 255.0));
                }
            }
            pen += m.advance_width + self.tracking;
        }
        (pen - x as f32 - self.tracking).round() as i32
    }
}

/// Words broken onto lines no wider than `max` pixels, at most `lines` of
/// them; a title that needs more ends on an ellipsis.
fn wrap(style: &Style, text: &str, max: i32, lines: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let next = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if style.width(&next) <= max || cur.is_empty() {
            cur = next;
        } else {
            out.push(std::mem::take(&mut cur));
            cur = word.to_string();
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    if out.len() > lines {
        out.truncate(lines);
        if let Some(last) = out.last_mut() {
            while !last.is_empty() && style.width(&format!("{last}…")) > max {
                last.pop();
            }
            *last = format!("{}…", last.trim_end());
        }
    }
    out
}

/// The REPLAY tag: an outline in the accent, a play triangle and the word.
/// Returns its width.
fn tag(fb: &mut Framebuffer, t: &Type, x: i32, y: i32, px: f32, filled: bool) -> i32 {
    let label = t.label(px, px * 0.27);
    let pad_x = (px * 0.65) as i32;
    let pad_y = (px * 0.35) as i32;
    let tri = (px * 0.62) as i32;
    let gap = (px * 0.45) as i32;
    let text_w = label.width("REPLAY");
    let w = pad_x + tri + gap + text_w + pad_x;
    let h = (px * 1.25) as i32 + 2 * pad_y;
    if filled {
        fb.rect(x, y, w, h, GROUND_A);
    } else {
        let line = (px / 9.0).round().max(2.0) as i32;
        fb.rect(x, y, w, line, ACCENT);
        fb.rect(x, y + h - line, w, line, ACCENT);
        fb.rect(x, y, line, h, ACCENT);
        fb.rect(x + w - line, y, line, h, ACCENT);
    }
    let cy = y + h / 2;
    for i in 0..tri {
        let half = (tri - i) * tri / 2 / tri.max(1) + 1;
        fb.rect(x + pad_x + i, cy - half, 1, half * 2, ACCENT);
    }
    label.draw(fb, x + pad_x + tri + gap, y + pad_y, "REPLAY", ACCENT);
    w
}

fn draw_frame(
    fb: &mut Framebuffer,
    t: &Type,
    format: Format,
    l: &Layout,
    card: &Card,
    logo: Option<&Logo>,
) {
    ground(fb);
    let [gx, gy, gw, gh] = l.game;
    fb.rect(gx, gy, gw, gh, 0);
    let [bx, by, bw, bh] = l.bar;
    fb.rect(bx, by, bw, bh, LINE);
    let system = card.system.to_uppercase();
    let caption = "Played on a 15 kHz CRT television";
    match format {
        // The game the whole width of the frame, and the caption in the band
        // under it, where the tag no longer covers the game's own figures.
        Format::Feed => {
            let w = tag(fb, t, 56, 862, 28.0, false);
            t.label(28.0, 6.0)
                .draw(fb, 56 + w + 28, 874, &system, AMBER);
            let title = t.title(84.0);
            for (i, line) in wrap(&title, &card.title, 968, 2).iter().enumerate() {
                title.draw(fb, 56, 936 + i as i32 * 88, line, INK);
            }
            if let Some(logo) = logo {
                logo.draw(fb, 56, 1188, 60);
            }
            t.body(28.0).draw(fb, 56, 1262, caption, DIM);
            let url = t.label(34.0, 0.0);
            let uw = url.width("omacrt.com");
            url.draw(fb, 1024 - uw, 1252, "omacrt.com", LINK);
        }
        Format::Vertical => {
            tag(fb, t, 40, 96, 34.0, false);
            if let Some(logo) = logo {
                let lw = logo.width_at(72);
                logo.draw(fb, 1040 - lw, 98, 72);
            }
            t.label(32.0, 6.0).draw(fb, 40, 1150, &system, AMBER);
            let title = t.title(104.0);
            for (i, line) in wrap(&title, &card.title, 1000, 3).iter().enumerate() {
                title.draw(fb, 40, 1206 + i as i32 * 106, line, INK);
            }
            fb.rect(40, 1726, 1000, 2, LINE);
            let body = t.body(34.0);
            body.draw(fb, 40, 1766, "Played on a 15 kHz", DIM);
            body.draw(fb, 40, 1812, "CRT television", DIM);
            let url = t.label(40.0, 0.0);
            let uw = url.width("omacrt.com");
            url.draw(fb, 1040 - uw, 1800, "omacrt.com", LINK);
        }
        Format::Wide => {
            let x = 1484;
            fb.rect(1440, 0, 4, 1080, LINE);
            if let Some(logo) = logo {
                logo.draw(fb, x, 64, 380 * 448 / 1484);
            }
            tag(fb, t, x, 220, 26.0, false);
            t.label(24.0, 5.0).draw(fb, x, 600, &system, AMBER);
            let title = t.title(56.0);
            for (i, line) in wrap(&title, &card.title, 392, 4).iter().enumerate() {
                title.draw(fb, x, 642 + i as i32 * 60, line, INK);
            }
            let body = t.body(26.0);
            body.draw(fb, x, 930, "Played on a 15 kHz", DIM);
            body.draw(fb, x, 964, "CRT television", DIM);
            t.label(32.0, 0.0).draw(fb, x, 1010, "omacrt.com", LINK);
        }
    }
}

/// The opening: the mark drawn top to bottom behind a beam, the way the tube
/// draws it, over half a second, then held.
fn draw_intro(fb: &mut Framebuffer, logo: Option<&Logo>, t: f32) {
    ground(fb);
    let Some(logo) = logo else { return };
    let lw = (fb.w.min(fb.h) as f32 * 0.72) as i32;
    let lh = logo.height_at_width(lw);
    let x = (fb.w as i32 - lw) / 2;
    let y = (fb.h as i32 - lh) / 2;
    let p = (t / 0.5).clamp(0.0, 1.0);
    let rows = (lh as f32 * p) as i32;
    logo.draw_rows(fb, x, y, lh, rows);
    if p < 1.0 {
        fb.rect(x, y + rows, lw, 4, LINK);
    }
}

/// The closing card: the mark, the game and the address, each coming up in
/// turn and held long enough to read.
fn draw_outro(fb: &mut Framebuffer, ty: &Type, logo: Option<&Logo>, card: &Card, t: f32) {
    ground(fb);
    let w = fb.w as i32;
    let u = fb.w.min(fb.h) as f32;
    let lw = (u * 0.62) as i32;
    let lh = logo.map(|l| l.height_at_width(lw)).unwrap_or(0);
    let title = ty.title(u * 0.07);
    let lines = wrap(&title, &card.title, w * 86 / 100, 2);
    let line_h = (u * 0.078) as i32;
    let system = ty.label(u * 0.028, u * 0.006);
    let url = ty.label(u * 0.04, 0.0);
    let block =
        lh + 48 + lines.len() as i32 * line_h + 28 + (u * 0.03) as i32 + 44 + (u * 0.045) as i32;
    let mut y = (fb.h as i32 - block) / 2;
    if let Some(logo) = logo {
        logo.draw(fb, (w - lw) / 2, y, lh);
    }
    y += lh + 48;
    let fade = |at: f32| ((t - at) / 0.3).clamp(0.0, 1.0);
    let centre = |fb: &mut Framebuffer, s: &Style, y: i32, text: &str, c: Color| {
        let tw = s.width(text);
        s.draw(fb, (w - tw) / 2, y, text, c);
    };
    for line in &lines {
        centre(fb, &title, y, line, lerp_color(GROUND_A, INK, fade(0.3)));
        y += line_h;
    }
    y += 28;
    centre(
        fb,
        &system,
        y,
        &card.system.to_uppercase(),
        lerp_color(GROUND_A, AMBER, fade(0.15)),
    );
    y += (u * 0.03) as i32 + 44;
    centre(
        fb,
        &url,
        y,
        "omacrt.com",
        lerp_color(GROUND_A, LINK, fade(0.5)),
    );
}

// ------------------------------------------------------------------ the mark

/// The published logo, decoded once, with its dark ground taken out so it
/// sits on the dithered one.
struct Logo {
    w: usize,
    h: usize,
    /// Straight alpha, 0xAARRGGBB.
    px: Vec<u32>,
}

impl Logo {
    fn decode() -> Option<Logo> {
        let mut dec = png::Decoder::new(std::io::Cursor::new(LOGO_PNG));
        dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = dec.read_info().ok()?;
        let mut buf = vec![0; reader.output_buffer_size()?];
        let info = reader.next_frame(&mut buf).ok()?;
        let (w, h) = (info.width as usize, info.height as usize);
        let channels = info.color_type.samples();
        let ground = (buf[0], buf[1], buf[2]);
        let mut px = Vec::with_capacity(w * h);
        for i in 0..w * h {
            let p = &buf[i * channels..i * channels + channels];
            let (r, g, b) = (p[0], p[1], p[2]);
            let a = if channels == 4 { p[3] } else { 255 };
            // The file's own ground is one flat colour; it becomes clear.
            let a = if (r, g, b) == ground { 0 } else { a };
            px.push((a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32);
        }
        Some(Logo { w, h, px })
    }

    fn width_at(&self, height: i32) -> i32 {
        (self.w as i64 * height as i64 / self.h as i64) as i32
    }

    fn height_at_width(&self, width: i32) -> i32 {
        (self.h as i64 * width as i64 / self.w as i64) as i32
    }

    fn draw(&self, fb: &mut Framebuffer, x: i32, y: i32, height: i32) {
        self.draw_rows(fb, x, y, height, height);
    }

    /// The mark scaled to `height` by averaging the source pixels under each
    /// target pixel, which keeps its edges clean at a small size, and only
    /// the first `rows` rows of it.
    fn draw_rows(&self, fb: &mut Framebuffer, x: i32, y: i32, height: i32, rows: i32) {
        if height <= 0 {
            return;
        }
        let width = self.width_at(height).max(1);
        for ty in 0..rows.min(height) {
            let sy0 = ty as usize * self.h / height as usize;
            let sy1 = ((ty as usize + 1) * self.h / height as usize)
                .max(sy0 + 1)
                .min(self.h);
            for tx in 0..width {
                let sx0 = tx as usize * self.w / width as usize;
                let sx1 = ((tx as usize + 1) * self.w / width as usize)
                    .max(sx0 + 1)
                    .min(self.w);
                let (mut r, mut g, mut b, mut a, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
                for sy in sy0..sy1 {
                    for sx in sx0..sx1 {
                        let p = self.px[sy * self.w + sx];
                        let pa = p >> 24;
                        r += ((p >> 16) & 0xff) * pa;
                        g += ((p >> 8) & 0xff) * pa;
                        b += (p & 0xff) * pa;
                        a += pa;
                        n += 1;
                    }
                }
                if a == 0 {
                    continue;
                }
                let colour = ((r / a) << 16) | ((g / a) << 8) | (b / a);
                let alpha = (a / n) as f32 / 255.0;
                let (dx, dy) = (x + tx, y + ty);
                let under = fb.at(dx, dy);
                fb.put(dx, dy, lerp_color(under, colour, alpha));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_wraps_on_words_and_ends_on_an_ellipsis_when_too_long() {
        let t = Type::load().expect("the typefaces load");
        let title = t.title(84.0);
        assert_eq!(
            wrap(&title, "Sega Rally Championship", 968, 2),
            ["Sega Rally Championship"]
        );
        assert_eq!(
            wrap(&title, "Sega Rally Championship", 600, 2),
            ["Sega Rally", "Championship"]
        );
        let long = wrap(
            &title,
            "The Legend of Zelda A Link to the Past and Four Swords",
            968,
            2,
        );
        assert_eq!(long.len(), 2);
        assert!(long[1].ends_with('…'), "{long:?}");
        assert!(long.iter().all(|l| title.width(l) <= 968), "{long:?}");
    }

    #[test]
    fn a_piece_from_before_the_game_is_not_taken() {
        let dir = std::env::temp_dir().join(format!("omacrt-replay-since-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let at = |name: &str, secs: u64| {
            std::fs::write(dir.join(name), b"x").unwrap();
            let f = std::fs::File::options()
                .write(true)
                .open(dir.join(name))
                .unwrap();
            f.set_modified(
                std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs),
            )
            .unwrap();
        };
        at("seg00.ts", 1_000);
        at("seg01.ts", 2_000);
        let since = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_500);
        let got = pieces(&dir, Some(since));
        assert_eq!(got.len(), 1);
        assert!(got[0].ends_with("seg01.ts"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_formats_parse_by_name_and_by_shape() {
        assert_eq!(Format::parse(""), Some(Format::Feed));
        assert_eq!(Format::parse("9:16"), Some(Format::Vertical));
        assert_eq!(Format::parse("wide"), Some(Format::Wide));
        assert_eq!(Format::parse("square"), None);
    }

    #[test]
    fn the_game_fits_inside_each_format() {
        for f in [Format::Feed, Format::Vertical, Format::Wide] {
            let (w, h) = f.size();
            let l = Layout::of(f);
            let [x, y, gw, gh] = l.game;
            assert!(
                x >= 0 && y >= 0 && x + gw <= w as i32 && y + gh <= h as i32,
                "{f:?}"
            );
            let [bx, by, bw, bh] = l.bar;
            assert!(bx + bw <= w as i32 && by + bh <= h as i32, "{f:?}");
        }
    }

    #[test]
    fn the_mark_decodes_with_a_clear_ground() {
        let logo = Logo::decode().expect("the logo decodes");
        assert_eq!(logo.px[0] >> 24, 0, "the corner is ground");
        assert!(
            logo.px.iter().any(|p| p >> 24 == 255),
            "the mark itself is opaque"
        );
    }

    /// Makes the three replays from whatever ring `XDG_RUNTIME_DIR` points at,
    /// into `XDG_VIDEOS_DIR`. Run by hand, with both set:
    /// `cargo test --bin omacrt-shell replay::tests::compose_from_a_ring -- --ignored`
    #[test]
    #[ignore]
    fn compose_from_a_ring() {
        let card = Card {
            title: "Sega Rally Championship".into(),
            system: "Sega Model 2".into(),
        };
        for f in [Format::Feed, Format::Vertical, Format::Wide] {
            let out = compose(&card, f, None).expect("a replay");
            println!("{}", out.display());
        }
    }

    #[test]
    fn a_title_becomes_a_file_name() {
        assert_eq!(
            file_safe("Sonic/Knuckles: Hybrid"),
            "Sonic Knuckles  Hybrid"
        );
        assert_eq!(file_safe("  "), "replay");
    }

    #[test]
    fn only_the_newest_pieces_are_taken_oldest_first() {
        let dir = std::env::temp_dir().join(format!("omacrt-replay-pieces-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..20 {
            std::fs::write(dir.join(format!("seg{i:02}.ts")), b"x").unwrap();
            // mtime resolution: each file a little newer than the last.
            let t = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000 + i);
            let f = std::fs::File::options()
                .write(true)
                .open(dir.join(format!("seg{i:02}.ts")))
                .unwrap();
            f.set_modified(t).unwrap();
        }
        std::fs::write(dir.join("seg99.ts"), b"").unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        let got = pieces(&dir, None);
        assert_eq!(got.len(), PIECES);
        assert!(got[0].ends_with("seg03.ts"), "{got:?}");
        assert!(got.last().unwrap().ends_with("seg19.ts"), "{got:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
