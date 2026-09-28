//! Pictures for the launcher: box art from the libretro thumbnail repository
//! and console illustrations from the RetroArch "systematic" asset set.
//!
//! Everything slow happens on a worker thread: downloading (through `curl`,
//! cached under `~/.cache/omacrt/art`), PNG decoding and downscaling to
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
        let a = p >> 24;
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
    crate::covers::label(system)
}

const SYSTEMATIC: &str = "/usr/share/retroarch/assets/xmb/systematic/png";

enum Source {
    /// A game's box art: exact name first, the fuzzy match second.
    Cover {
        label: String,
        stem: String,
        cache: PathBuf,
    },
    /// Decode a file that is already on disk.
    Local(PathBuf),
}

struct Request {
    key: String,
    source: Source,
    max_w: usize,
    max_h: usize,
    regions: Vec<&'static str>,
}

struct Done {
    key: String,
    image: Option<Image>,
}

pub struct Art {
    cache_dir: PathBuf,
    /// Region order for covers that exist in several editions.
    regions: Vec<&'static str>,
    tx: Sender<Request>,
    rx: Receiver<Done>,
    ready: HashMap<String, Option<Image>>,
    pending: HashSet<String>,
    /// Set when the worker thread has gone. A picture that will never arrive
    /// has to stop looking like one that is on its way, or the loading mark
    /// stays on the screen for the rest of the session.
    stopped: bool,
}

impl Art {
    pub fn new(regions: Vec<&'static str>) -> Self {
        let cache_dir = crate::covers::cache_dir();
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
            regions,
            tx,
            rx,
            ready: HashMap::new(),
            pending: HashSet::new(),
            stopped: false,
        }
    }

    /// Drain finished work. Call once per frame.
    pub fn poll(&mut self) {
        loop {
            match self.rx.try_recv() {
                Ok(done) => {
                    self.pending.remove(&done.key);
                    self.ready.insert(done.key, done.image);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Nothing is coming. Everything outstanding is answered
                    // with no picture, which the screens already draw for a
                    // cover that does not exist.
                    for key in self.pending.drain() {
                        self.ready.insert(key, None);
                    }
                    self.stopped = true;
                    break;
                }
            }
        }
    }

    fn request(&mut self, key: &str, source: Source, max_w: usize, max_h: usize) {
        if self.stopped || self.ready.contains_key(key) || self.pending.contains(key) {
            return;
        }
        self.pending.insert(key.to_string());
        let _ = self.tx.send(Request {
            key: key.to_string(),
            source,
            max_w,
            max_h,
            regions: self.regions.clone(),
        });
    }

    /// True while a request for `key` is in flight.
    pub fn loading(&self, key: &str) -> bool {
        self.pending.contains(key)
    }

    /// One entry per game and size: the list wants a small box, the cover
    /// flow a large one, both from the same file in the cache.
    pub fn cover_key(system: &str, path: &Path, max_w: usize, max_h: usize) -> String {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        format!("cover:{system}:{stem}:{max_w}x{max_h}")
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
        let key = Self::cover_key(system, path, max_w, max_h);
        if !self.ready.contains_key(&key) {
            let Some(label) = system_label(system) else {
                self.ready.insert(key.clone(), None);
                return None;
            };
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let cache = crate::covers::cache_path(system, stem);
            // Arcade files are named after the set: the cache keeps the file
            // name, the repository is asked for the title.
            let stem = crate::covers::title_for(system, stem);
            let _ = &self.cache_dir;
            self.request(
                &key,
                Source::Cover {
                    label: label.to_string(),
                    stem: stem.to_string(),
                    cache,
                },
                max_w,
                max_h,
            );
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
        Source::Cover { label, stem, cache } => {
            if !cache.exists() {
                if cache.with_extension("missing").exists() {
                    return None;
                }
                if !crate::covers::fetch_cover(label, stem, cache, &req.regions) {
                    let _ = std::fs::write(cache.with_extension("missing"), b"");
                    return None;
                }
            }
            cache.clone()
        }
    };
    decode(&path).map(|img| fit(&img, req.max_w, req.max_h))
}

/// Decode a PNG into straight alpha RGBA.
pub fn decode(path: &Path) -> Option<Image> {
    let file = std::fs::File::open(path).ok()?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    // A cover is a few hundred pixels on a side. Without a limit the decoder
    // believes whatever the file's header claims and allocates it, so one
    // downloaded picture declaring enormous dimensions is the launcher gone.
    decoder.set_limits(png::Limits { bytes: 64 << 20 });
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    // png 0.18 returns None when the buffer the header asks for would not fit
    // in a usize, which is the same hostile file the limit above is for: a
    // picture that cannot be decoded is simply not a cover.
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let data = &buf[..info.buffer_size()];
    let px: Vec<u32> = match info.color_type {
        png::ColorType::Rgba => data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| {
                ((c[3] as u32) << 24) | ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32
            })
            .collect(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| 0xff00_0000 | ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32)
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
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
pub fn fit(img: &Image, max_w: usize, max_h: usize) -> Image {
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
