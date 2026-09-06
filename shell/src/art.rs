//! Pictures for the launcher: box art from the libretro thumbnail repository
//! and console illustrations from the RetroArch "systematic" asset set.
//!
//! Everything slow happens on a worker thread: downloading (through `curl`,
//! cached under `~/.cache/omarchy-crt/art`), PNG decoding and downscaling to
//! the few dozen pixels a 320x240 screen can show. The scene asks for an
//! image every frame and gets `None` until it is ready; a miss is remembered
//! so the network is asked once per title.

use crate::fb::Color;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

/// Straight alpha RGBA pixels, `0xAARRGGBB`, already scaled to fit.
#[derive(Clone, Debug)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u32>,
}

impl Image {
    /// Colour of a pixel blended over `bg`.
    pub fn over(&self, x: usize, y: usize, bg: Color) -> Color {
        let p = self.px[y * self.w + x];
        let a = (p >> 24) as u32;
        if a == 255 {
            return p & 0x00ff_ffff;
        }
        if a == 0 {
            return bg;
        }
        let mix = |shift: u32| -> u32 {
            let s = (p >> shift) & 0xff;
            let d = (bg >> shift) & 0xff;
            (s * a + d * (255 - a)) / 255
        };
        (mix(16) << 16) | (mix(8) << 8) | mix(0)
    }
}

/// libretro thumbnail folder (and systematic asset) per system name.
pub fn system_label(system: &str) -> Option<&'static str> {
    Some(match system {
        "nes" => "Nintendo - Nintendo Entertainment System",
        "snes" => "Nintendo - Super Nintendo Entertainment System",
        "megadrive" => "Sega - Mega Drive - Genesis",
        "mastersystem" => "Sega - Master System - Mark III",
        "gamegear" => "Sega - Game Gear",
        "sg1000" => "Sega - SG-1000",
        "segacd" => "Sega - Mega-CD - Sega CD",
        "sega32x" => "Sega - 32X",
        "saturn" => "Sega - Saturn",
        "dreamcast" => "Sega - Dreamcast",
        "naomi" => "Sega - Naomi",
        "stv" => "Sega - ST-V",
        "pcengine" => "NEC - PC Engine - TurboGrafx 16",
        "pcenginecd" => "NEC - PC Engine CD - TurboGrafx-CD",
        "n64" => "Nintendo - Nintendo 64",
        "gb" => "Nintendo - Game Boy",
        "gbc" => "Nintendo - Game Boy Color",
        "gba" => "Nintendo - Game Boy Advance",
        "nds" => "Nintendo - Nintendo DS",
        "psx" => "Sony - PlayStation",
        "psp" => "Sony - PlayStation Portable",
        "neogeo" => "SNK - Neo Geo",
        "neogeocd" => "SNK - Neo Geo CD",
        "ngp" => "SNK - Neo Geo Pocket Color",
        "arcade" => "FBNeo - Arcade Games",
        "mame" => "MAME",
        "mame2003" => "MAME 2003-Plus",
        "c64" => "Commodore - 64",
        "amiga" => "Commodore - Amiga",
        "amigacd32" => "Commodore - CD32",
        "amstradcpc" => "Amstrad - CPC",
        "atari2600" => "Atari - 2600",
        "atari5200" => "Atari - 5200",
        "atari7800" => "Atari - 7800",
        "lynx" => "Atari - Lynx",
        "jaguar" => "Atari - Jaguar",
        "3do" => "The 3DO Company - 3DO",
        "cdi" => "Philips - CD-i",
        "msx" => "Microsoft - MSX",
        "zxspectrum" => "Sinclair - ZX Spectrum",
        "dos" => "DOS",
        "scummvm" => "ScummVM",
        "x68000" => "Sharp - X68000",
        _ => return None,
    })
}

const SYSTEMATIC: &str = "/usr/share/retroarch/assets/xmb/systematic/png";
const THUMBS: &str = "https://thumbnails.libretro.com";

/// libretro replaces characters that cannot be file names with `_`.
fn thumb_name(stem: &str) -> String {
    stem.chars()
        .map(|c| if "&*/:`<>?\\|".contains(c) { '_' } else { c })
        .collect()
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

enum Source {
    /// Download to the cache path when missing, then decode.
    Remote { url: String, cache: PathBuf },
    /// Decode a file that is already on disk.
    Local(PathBuf),
}

struct Request {
    key: String,
    source: Source,
    max_w: usize,
    max_h: usize,
}

struct Done {
    key: String,
    image: Option<Image>,
}

pub struct Art {
    cache_dir: PathBuf,
    tx: Sender<Request>,
    rx: Receiver<Done>,
    ready: HashMap<String, Option<Image>>,
    pending: HashSet<String>,
}

impl Art {
    pub fn new() -> Self {
        let cache_dir = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| crate::library::home().join(".cache"))
            .join("omarchy-crt/art");
        let (tx, worker_rx) = channel::<Request>();
        let (done_tx, rx) = channel::<Done>();
        std::thread::Builder::new()
            .name("art".into())
            .spawn(move || {
                for req in worker_rx {
                    let image = fetch(&req);
                    if done_tx
                        .send(Done {
                            key: req.key,
                            image,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .ok();
        Self {
            cache_dir,
            tx,
            rx,
            ready: HashMap::new(),
            pending: HashSet::new(),
        }
    }

    /// Drain finished work. Call once per frame.
    pub fn poll(&mut self) {
        while let Ok(done) = self.rx.try_recv() {
            self.pending.remove(&done.key);
            self.ready.insert(done.key, done.image);
        }
    }

    fn request(&mut self, key: &str, source: Source, max_w: usize, max_h: usize) {
        if self.ready.contains_key(key) || self.pending.contains(key) {
            return;
        }
        self.pending.insert(key.to_string());
        let _ = self.tx.send(Request {
            key: key.to_string(),
            source,
            max_w,
            max_h,
        });
    }

    /// True while a request for `key` is in flight.
    pub fn loading(&self, key: &str) -> bool {
        self.pending.contains(key)
    }

    pub fn cover_key(system: &str, path: &Path) -> String {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        format!("cover:{system}:{stem}")
    }

    /// Box art of a game, fitted into `max_w` x `max_h`. `None` while it
    /// loads or when the repository has none.
    pub fn cover(
        &mut self,
        system: &str,
        path: &Path,
        max_w: usize,
        max_h: usize,
    ) -> Option<&Image> {
        let key = Self::cover_key(system, path);
        if !self.ready.contains_key(&key) {
            let Some(label) = system_label(system) else {
                self.ready.insert(key.clone(), None);
                return None;
            };
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let name = thumb_name(stem);
            let url = format!(
                "{THUMBS}/{}/Named_Boxarts/{}.png",
                percent_encode(label),
                percent_encode(&name)
            );
            let cache = self.cache_dir.join(system).join(format!("{name}.png"));
            self.request(&key, Source::Remote { url, cache }, max_w, max_h);
            return None;
        }
        self.ready.get(&key).and_then(|i| i.as_ref())
    }

    /// The console picture of a system, fitted into a `size` square.
    pub fn system_image(&mut self, system: &str, size: usize) -> Option<&Image> {
        let key = format!("system:{system}:{size}");
        if !self.ready.contains_key(&key) {
            let Some(label) = system_label(system) else {
                self.ready.insert(key.clone(), None);
                return None;
            };
            let path = PathBuf::from(SYSTEMATIC).join(format!("{label}.png"));
            if !path.exists() {
                self.ready.insert(key.clone(), None);
                return None;
            }
            self.request(&key, Source::Local(path), size, size);
            return None;
        }
        self.ready.get(&key).and_then(|i| i.as_ref())
    }
}

fn fetch(req: &Request) -> Option<Image> {
    let path = match &req.source {
        Source::Local(p) => p.clone(),
        Source::Remote { url, cache } => {
            let missing = cache.with_extension("missing");
            if missing.exists() {
                return None;
            }
            if !cache.exists() {
                if let Some(dir) = cache.parent() {
                    std::fs::create_dir_all(dir).ok()?;
                }
                let tmp = cache.with_extension("part");
                let status = std::process::Command::new("curl")
                    .args(["-fsSL", "--max-time", "20", "-o"])
                    .arg(&tmp)
                    .arg(url)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                match status {
                    Ok(s) if s.success() && std::fs::rename(&tmp, cache).is_ok() => {}
                    _ => {
                        let _ = std::fs::remove_file(&tmp);
                        let _ = std::fs::write(&missing, b"");
                        return None;
                    }
                }
            }
            cache.clone()
        }
    };
    decode(&path).map(|img| fit(&img, req.max_w, req.max_h))
}

/// Decode a PNG into straight alpha RGBA.
fn decode(path: &Path) -> Option<Image> {
    let file = std::fs::File::open(path).ok()?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let data = &buf[..info.buffer_size()];
    let px: Vec<u32> = match info.color_type {
        png::ColorType::Rgba => data
            .chunks_exact(4)
            .map(|c| {
                ((c[3] as u32) << 24) | ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32
            })
            .collect(),
        png::ColorType::Rgb => data
            .chunks_exact(3)
            .map(|c| 0xff00_0000 | ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32)
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .chunks_exact(2)
            .map(|c| {
                ((c[1] as u32) << 24) | ((c[0] as u32) << 16) | ((c[0] as u32) << 8) | c[0] as u32
            })
            .collect(),
        png::ColorType::Grayscale => data
            .iter()
            .map(|&g| 0xff00_0000 | ((g as u32) << 16) | ((g as u32) << 8) | g as u32)
            .collect(),
        _ => return None,
    };
    if px.len() != w * h {
        return None;
    }
    Some(Image { w, h, px })
}

/// Shrink to fit inside `max_w` x `max_h`, averaging source pixels (alpha
/// weighted) so box art keeps its colours at 90 pixels wide.
fn fit(img: &Image, max_w: usize, max_h: usize) -> Image {
    if img.w == 0 || img.h == 0 {
        return img.clone();
    }
    let f = (max_w as f64 / img.w as f64)
        .min(max_h as f64 / img.h as f64)
        .min(1.0);
    let ow = ((img.w as f64 * f).round() as usize).max(1);
    let oh = ((img.h as f64 * f).round() as usize).max(1);
    if ow == img.w && oh == img.h {
        return img.clone();
    }
    let mut out = Vec::with_capacity(ow * oh);
    for oy in 0..oh {
        let y0 = oy * img.h / oh;
        let y1 = ((oy + 1) * img.h / oh).max(y0 + 1);
        for ox in 0..ow {
            let x0 = ox * img.w / ow;
            let x1 = ((ox + 1) * img.w / ow).max(x0 + 1);
            let (mut r, mut g, mut b, mut a, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = img.px[y * img.w + x];
                    let pa = (p >> 24) as u64;
                    r += ((p >> 16) & 0xff) as u64 * pa;
                    g += ((p >> 8) & 0xff) as u64 * pa;
                    b += (p & 0xff) as u64 * pa;
                    a += pa;
                    n += 1;
                }
            }
            let px = if a == 0 {
                0
            } else {
                let aa = (a / n) as u32;
                (aa << 24) | (((r / a) as u32) << 16) | (((g / a) as u32) << 8) | (b / a) as u32
            };
            out.push(px);
        }
    }
    Image {
        w: ow,
        h: oh,
        px: out,
    }
}
