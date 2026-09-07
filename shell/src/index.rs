//! The game index: a scan of whatever folders the user points at, however
//! they are organised, into games with a system, a title and tags.
//!
//! Detection works from the file inward: unambiguous extensions first (a
//! `.sfc` is a Super Nintendo game wherever it sits), then the words in the
//! folder names above the file (`psx`, `Sony - PlayStation`, `sega_dc`),
//! then the file itself: disc image signatures for `.cue`/`.bin`/`.iso`,
//! the names inside a `.zip`, cartridge headers. What still resists lands
//! in `unknown` for the user to assign once; the answer is remembered as a
//! folder hint.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// One playable thing found by the scan.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Item {
    pub path: PathBuf,
    pub system: String,
    pub title: String,
    /// Bracketed tags from the file name: `USA`, `Rev 1`, `Disc 2`.
    pub tags: Vec<String>,
    pub region: String,
    pub disc: Option<u8>,
    pub size: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Index {
    pub roots: Vec<PathBuf>,
    pub items: Vec<Item>,
    /// Files that look like games but whose system could not be told.
    pub unknown: Vec<PathBuf>,
    pub scanned_at: String,
}

/// User answers: a folder (or file) to a system name.
pub type Hints = BTreeMap<PathBuf, String>;

/// `~/.config/omarchy-crt/library.toml`, written by the tool: where to scan
/// and what the user told it about folders it could not read.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct LibraryConfig {
    pub roots: Vec<PathBuf>,
    pub hints: BTreeMap<String, String>,
}

impl LibraryConfig {
    pub fn path() -> PathBuf {
        crate::crt::config_dir().join("library.toml")
    }

    pub fn load() -> Self {
        crate::store::load_string(&Self::path())
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(crate::crt::config_dir())?;
        let body = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        crate::store::save(
            &Self::path(),
            format!(
                "# Written by omarchy-crt. Folders to scan for games and the systems\n# you assigned to folders the scan could not read.\n{body}"
            ),
        )
    }

    pub fn hints(&self) -> Hints {
        self.hints
            .iter()
            .map(|(k, v)| (PathBuf::from(k), v.clone()))
            .collect()
    }
}

pub fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::library::home().join(".local/share"))
        .join("omarchy-crt")
}

impl Index {
    pub fn path() -> PathBuf {
        data_dir().join("library.json")
    }

    pub fn load() -> Option<Self> {
        let text = crate::store::load_string(&Self::path())?;
        serde_json::from_str(&text).ok()
    }

    pub fn save(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(data_dir())?;
        let text = serde_json::to_string(self)?;
        crate::store::save(&Self::path(), text)
    }

    /// Systems present, with item counts, most games first.
    pub fn systems(&self) -> Vec<(String, usize)> {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for it in &self.items {
            *counts.entry(it.system.as_str()).or_default() += 1;
        }
        let mut v: Vec<(String, usize)> = counts
            .into_iter()
            .map(|(k, n)| (k.to_string(), n))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }

    pub fn items_of(&self, system: &str) -> Vec<&Item> {
        self.items.iter().filter(|i| i.system == system).collect()
    }
}

// ------------------------------------------------------------------ rules

/// Extensions that name a system on their own.
const BY_EXTENSION: &[(&str, &str)] = &[
    ("nes", "nes"),
    ("fds", "nes"),
    ("unf", "nes"),
    ("unif", "nes"),
    ("sfc", "snes"),
    ("smc", "snes"),
    ("swc", "snes"),
    ("fig", "snes"),
    ("bs", "snes"),
    ("md", "megadrive"),
    ("gen", "megadrive"),
    ("smd", "megadrive"),
    ("sms", "mastersystem"),
    ("gg", "gamegear"),
    ("sg", "sg1000"),
    ("32x", "sega32x"),
    ("pce", "pcengine"),
    ("sgx", "pcengine"),
    ("n64", "n64"),
    ("z64", "n64"),
    ("v64", "n64"),
    ("gb", "gb"),
    ("sgb", "gb"),
    ("gbc", "gbc"),
    ("gba", "gba"),
    ("nds", "nds"),
    ("ngp", "ngp"),
    ("ngc", "ngp"),
    ("ngpc", "ngp"),
    ("a26", "atari2600"),
    ("a52", "atari5200"),
    ("a78", "atari7800"),
    ("lnx", "lynx"),
    ("j64", "jaguar"),
    ("jag", "jaguar"),
    ("d64", "c64"),
    ("t64", "c64"),
    ("prg", "c64"),
    ("g64", "c64"),
    ("adf", "amiga"),
    ("adz", "amiga"),
    ("dms", "amiga"),
    ("ipf", "amiga"),
    ("hdf", "amiga"),
    ("lha", "amiga"),
    ("uae", "amiga"),
    ("tzx", "zxspectrum"),
    ("z80", "zxspectrum"),
    ("szx", "zxspectrum"),
    ("trd", "zxspectrum"),
    ("mx1", "msx"),
    ("mx2", "msx"),
    ("scummvm", "scummvm"),
    ("svm", "scummvm"),
    ("dim", "x68000"),
    ("xdf", "x68000"),
    ("hdm", "x68000"),
    ("pbp", "psx"),
    ("cso", "psp"),
    ("gdi", "dreamcast"),
    ("cdi", "dreamcast"),
    ("dosz", "dos"),
];

/// Extensions of things that are games but need more evidence.
const AMBIGUOUS: &[&str] = &[
    "zip", "7z", "bin", "cue", "chd", "iso", "img", "m3u", "rom", "dsk", "tap", "cdt", "sna",
    "ccd", "mds", "toc", "exe", "elf", "crt", "cpr",
];

/// Extensions that are never games, even in a game folder.
const NOISE: &[&str] = &[
    "txt", "pdf", "srm", "sav", "state", "png", "jpg", "jpeg", "gif", "xml", "dat", "nfo", "html",
    "htm", "md5", "sha1", "sfv", "log", "cfg", "ini", "json", "lr", "rec", "fav", "sh", "py", "db",
    "bak", "ips", "bps", "ups", "mp3", "ogg", "wav", "flac", "mp4", "mkv", "avi", "sub", "idx",
    "ecm", "nvram", "rtc",
];

/// Words in folder names, mapped to systems. Each alias is a sequence of
/// tokens that must appear in a row in the folder name (split on anything
/// that is not a letter or digit), so `dc` matches `sega_dc` and `DC games`
/// but not `dcp_backup`.
const FOLDER_WORDS: &[(&str, &str)] = &[
    ("nintendo nes", "nes"),
    ("nes", "nes"),
    ("famicom", "nes"),
    ("nintendo entertainment system", "nes"),
    ("fds", "nes"),
    ("nintendo snes", "snes"),
    ("snes", "snes"),
    ("super nintendo", "snes"),
    ("super famicom", "snes"),
    ("sfc", "snes"),
    ("sega smd", "megadrive"),
    ("sega md", "megadrive"),
    ("megadrive", "megadrive"),
    ("mega drive", "megadrive"),
    ("genesis", "megadrive"),
    ("md", "megadrive"),
    ("sega sms", "mastersystem"),
    ("mastersystem", "mastersystem"),
    ("master system", "mastersystem"),
    ("sms", "mastersystem"),
    ("sega gg", "gamegear"),
    ("gamegear", "gamegear"),
    ("game gear", "gamegear"),
    ("gg", "gamegear"),
    ("sega sg", "sg1000"),
    ("sg 1000", "sg1000"),
    ("sg1000", "sg1000"),
    ("sega cd", "segacd"),
    ("segacd", "segacd"),
    ("mega cd", "segacd"),
    ("megacd", "segacd"),
    ("sega 32x", "sega32x"),
    ("32x", "sega32x"),
    ("sega st", "saturn"),
    ("saturn", "saturn"),
    ("sega dc", "dreamcast"),
    ("dreamcast", "dreamcast"),
    ("dc", "dreamcast"),
    ("arcade dc", "naomi"),
    ("naomi", "naomi"),
    ("atomiswave", "naomi"),
    ("arcade stv", "stv"),
    ("stv", "stv"),
    ("nec pcecd", "pcenginecd"),
    ("pcecd", "pcenginecd"),
    ("pce cd", "pcenginecd"),
    ("turbografx cd", "pcenginecd"),
    ("tgcd", "pcenginecd"),
    ("nec pce", "pcengine"),
    ("pcengine", "pcengine"),
    ("pc engine", "pcengine"),
    ("turbografx", "pcengine"),
    ("pce", "pcengine"),
    ("tg16", "pcengine"),
    ("nintendo n64", "n64"),
    ("n64", "n64"),
    ("nintendo 64", "n64"),
    ("nintendo gbc", "gbc"),
    ("gbc", "gbc"),
    ("game boy color", "gbc"),
    ("gameboy color", "gbc"),
    ("nintendo gba", "gba"),
    ("gba", "gba"),
    ("game boy advance", "gba"),
    ("gameboy advance", "gba"),
    ("nintendo gb", "gb"),
    ("gameboy", "gb"),
    ("game boy", "gb"),
    ("gb", "gb"),
    ("nintendo ds", "nds"),
    ("nds", "nds"),
    ("sony psx", "psx"),
    ("psx", "psx"),
    ("playstation", "psx"),
    ("ps1", "psx"),
    ("psone", "psx"),
    ("sony psp", "psp"),
    ("psp", "psp"),
    ("snk ng", "neogeo"),
    ("neogeo", "neogeo"),
    ("neo geo", "neogeo"),
    ("snk ngcd", "neogeocd"),
    ("neocd", "neogeocd"),
    ("neogeocd", "neogeocd"),
    ("neo geo cd", "neogeocd"),
    ("snk ngp", "ngp"),
    ("ngp", "ngp"),
    ("neo geo pocket", "ngp"),
    ("arcade fbneo", "arcade"),
    ("fbneo", "arcade"),
    ("finalburn", "arcade"),
    ("fba", "arcade"),
    ("arcade mame 2k3p", "mame2003"),
    ("mame 2003", "mame2003"),
    ("mame2003", "mame2003"),
    ("arcade mame", "mame"),
    ("mame", "mame"),
    ("arcade", "mame"),
    ("commodore c64", "c64"),
    ("c64", "c64"),
    ("commodore 64", "c64"),
    ("commodore ami", "amiga"),
    ("amiga", "amiga"),
    ("commodore amicd", "amigacd32"),
    ("cd32", "amigacd32"),
    ("amstrad cpc", "amstradcpc"),
    ("amstrad", "amstradcpc"),
    ("cpc", "amstradcpc"),
    ("atari 2600", "atari2600"),
    ("atari2600", "atari2600"),
    ("2600", "atari2600"),
    ("atari 5200", "atari5200"),
    ("atari5200", "atari5200"),
    ("atari 7800", "atari7800"),
    ("atari7800", "atari7800"),
    ("atari lynx", "lynx"),
    ("lynx", "lynx"),
    ("atari jaguar", "jaguar"),
    ("jaguar", "jaguar"),
    ("panasonic 3do", "3do"),
    ("3do", "3do"),
    ("philips cdi", "cdi"),
    ("cdi", "cdi"),
    ("microsoft msx", "msx"),
    ("msx", "msx"),
    ("sinclair zx", "zxspectrum"),
    ("zx spectrum", "zxspectrum"),
    ("zxspectrum", "zxspectrum"),
    ("spectrum", "zxspectrum"),
    ("ibm pc", "dos"),
    ("dos", "dos"),
    ("msdos", "dos"),
    ("dosbox", "dos"),
    ("scummvm", "scummvm"),
    ("sharp x68k", "x68000"),
    ("x68000", "x68000"),
    ("x68k", "x68000"),
];

/// Signatures inside disc images and cartridge dumps.
const DISC_SIGNATURES: &[(&[u8], &str)] = &[
    (b"PLAYSTATION", "psx"),
    (b"Sony Computer Entertainment", "psx"),
    (b"SEGA SEGASATURN", "saturn"),
    (b"SEGADISCSYSTEM", "segacd"),
    (b"SEGABOOTDISC", "segacd"),
    (b"SEGA SEGAKATANA", "dreamcast"),
    (b"PC Engine CD-ROM SYSTEM", "pcenginecd"),
    (b"NEO-GEO", "neogeocd"),
    (b"CD-I", "cdi"),
    (b"iamaduckiamaduck", "3do"),
    (b"PSP GAME", "psp"),
];

fn tokens(name: &str) -> Vec<String> {
    name.to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// System named by the words of one folder, longest alias first.
pub fn system_from_folder_name(name: &str) -> Option<&'static str> {
    let toks = tokens(name);
    if toks.is_empty() {
        return None;
    }
    let mut best: Option<(usize, &'static str)> = None;
    for (alias, system) in FOLDER_WORDS {
        let a: Vec<&str> = alias.split(' ').collect();
        if a.len() > toks.len() {
            continue;
        }
        let hit = toks
            .windows(a.len())
            .any(|w| w.iter().zip(a.iter()).all(|(x, y)| x == y));
        if hit && best.map(|(n, _)| a.len() > n).unwrap_or(true) {
            best = Some((a.len(), system));
        }
    }
    best.map(|(_, s)| s)
}

/// Walk the folders between the root and the file, nearest first.
fn system_from_parents(path: &Path, root: &Path) -> Option<&'static str> {
    let mut p = path.parent();
    while let Some(dir) = p {
        if dir == root || !dir.starts_with(root) {
            // The root itself may be named after a system too (`~/roms/snes`).
            return dir
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(system_from_folder_name);
        }
        if let Some(s) = dir
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(system_from_folder_name)
        {
            return Some(s);
        }
        p = dir.parent();
    }
    None
}

fn ext_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

fn read_head(path: &Path, len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    match File::open(path).and_then(|mut f| {
        let mut got = 0;
        while got < len {
            let n = f.read(&mut buf[got..])?;
            if n == 0 {
                break;
            }
            got += n;
        }
        Ok(got)
    }) {
        Ok(n) => {
            buf.truncate(n);
            buf
        }
        Err(_) => Vec::new(),
    }
}

fn find(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

/// System from the bytes of a disc image or cartridge dump.
fn system_from_content(path: &Path) -> Option<&'static str> {
    let head = read_head(path, 96 * 1024);
    if head.is_empty() {
        return None;
    }
    if head.len() > 0x110 {
        let at = &head[0x100..0x110];
        if at.starts_with(b"SEGA MEGA DRIVE") || at.starts_with(b"SEGA GENESIS") {
            return Some("megadrive");
        }
        if at.starts_with(b"SEGA 32X") {
            return Some("sega32x");
        }
    }
    if head.starts_with(b"NES\x1a") {
        return Some("nes");
    }
    for (sig, system) in DISC_SIGNATURES {
        if find(&head, sig) {
            return Some(system);
        }
    }
    None
}

/// First data file a `.cue` refers to, next to it.
fn cue_first_file(cue: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(cue).ok()?;
    for line in text.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("FILE ") {
            let name = rest.trim().trim_start_matches('"');
            let name = name.split('"').next().unwrap_or("").trim();
            if !name.is_empty() {
                return Some(cue.parent().unwrap_or(Path::new(".")).join(name));
            }
        }
    }
    None
}

/// Files a `.m3u` playlist refers to.
pub fn m3u_files(m3u: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(m3u) else {
        return Vec::new();
    };
    let dir = m3u.parent().unwrap_or(Path::new("."));
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| dir.join(l))
        .collect()
}

/// Names inside a zip, from its central directory. Std only: the end of
/// central directory record sits in the last 64 KB.
pub fn zip_names(path: &Path) -> Vec<String> {
    let Ok(mut f) = File::open(path) else {
        return Vec::new();
    };
    let Ok(len) = f.metadata().map(|m| m.len()) else {
        return Vec::new();
    };
    let tail_len = len.min(66_000) as usize;
    if tail_len < 22 {
        return Vec::new();
    }
    if f.seek(SeekFrom::Start(len - tail_len as u64)).is_err() {
        return Vec::new();
    }
    let mut tail = vec![0u8; tail_len];
    if f.read_exact(&mut tail).is_err() {
        return Vec::new();
    }
    let Some(eocd) = tail.windows(4).rposition(|w| w == [0x50, 0x4b, 0x05, 0x06]) else {
        return Vec::new();
    };
    let r = &tail[eocd..];
    if r.len() < 22 {
        return Vec::new();
    }
    let cd_size = u32::from_le_bytes([r[12], r[13], r[14], r[15]]) as u64;
    let cd_off = u32::from_le_bytes([r[16], r[17], r[18], r[19]]) as u64;
    if cd_size == 0 || cd_size > 8_000_000 || cd_off + cd_size > len {
        return Vec::new();
    }
    if f.seek(SeekFrom::Start(cd_off)).is_err() {
        return Vec::new();
    }
    let mut cd = vec![0u8; cd_size as usize];
    if f.read_exact(&mut cd).is_err() {
        return Vec::new();
    }
    let mut names = Vec::new();
    let mut i = 0usize;
    while i + 46 <= cd.len() && cd[i..i + 4] == [0x50, 0x4b, 0x01, 0x02] {
        let name_len = u16::from_le_bytes([cd[i + 28], cd[i + 29]]) as usize;
        let extra_len = u16::from_le_bytes([cd[i + 30], cd[i + 31]]) as usize;
        let comment_len = u16::from_le_bytes([cd[i + 32], cd[i + 33]]) as usize;
        let start = i + 46;
        let end = start + name_len;
        if end > cd.len() {
            break;
        }
        names.push(String::from_utf8_lossy(&cd[start..end]).into_owned());
        i = end + extra_len + comment_len;
    }
    names
}

fn ext_system(ext: &str) -> Option<&'static str> {
    BY_EXTENSION
        .iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, s)| *s)
}

/// Arcade set names are short, lowercase, letters and digits: `sf2ce`.
fn looks_like_arcade_set(stem: &str) -> bool {
    !stem.is_empty()
        && stem.len() <= 12
        && stem
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Decide the system of one file, or None when nothing tells.
pub fn detect(path: &Path, root: &Path, hints: &Hints) -> Option<String> {
    // The user's word first: the closest hinted ancestor wins.
    let mut p = Some(path);
    while let Some(q) = p {
        if let Some(s) = hints.get(q) {
            return Some(s.clone());
        }
        if q == root {
            break;
        }
        p = q.parent();
    }
    let ext = ext_of(path);
    if let Some(s) = ext_system(&ext) {
        return Some(s.into());
    }
    let hint = system_from_parents(path, root);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    match ext.as_str() {
        "zip" | "7z" => {
            if let Some(h) = hint {
                return Some(h.into());
            }
            if ext == "zip" {
                let names = zip_names(path);
                for n in &names {
                    let inner = ext_of(Path::new(n));
                    if let Some(s) = ext_system(&inner) {
                        return Some(s.into());
                    }
                }
                if !names.is_empty() && looks_like_arcade_set(stem) {
                    return Some("mame".into());
                }
            } else if looks_like_arcade_set(stem) {
                return Some("mame".into());
            }
            None
        }
        "cue" | "ccd" | "toc" | "mds" => {
            let data = if ext == "cue" {
                cue_first_file(path)
            } else {
                None
            };
            if let Some(d) = data
                && let Some(s) = system_from_content(&d)
            {
                return Some(s.into());
            }
            hint.map(str::to_string)
        }
        "iso" | "img" | "bin" | "rom" => {
            if let Some(s) = system_from_content(path) {
                return Some(s.into());
            }
            // A `.bin` next to a `.cue` is a track, not a game.
            if ext == "bin" && path.with_extension("cue").exists() {
                return None;
            }
            hint.map(str::to_string)
        }
        "m3u" => {
            for f in m3u_files(path) {
                if let Some(s) = detect(&f, root, hints) {
                    return Some(s);
                }
            }
            hint.map(str::to_string)
        }
        _ => hint.map(str::to_string),
    }
}

// ------------------------------------------------------------------ names

const REGIONS: &[&str] = &[
    "World",
    "USA",
    "Europe",
    "Japan",
    "Brazil",
    "Korea",
    "Australia",
    "Germany",
    "France",
    "Italy",
    "Spain",
    "Netherlands",
    "Sweden",
    "China",
    "Taiwan",
    "Asia",
    "Canada",
    "UK",
    "Unknown",
];

/// Title, tags, region and disc number from a No-Intro or Redump style name.
pub fn parse_name(stem: &str) -> (String, Vec<String>, String, Option<u8>) {
    let title = crate::library::clean_title(Path::new(stem));
    let mut tags = Vec::new();
    let mut rest = stem;
    while let Some(start) = rest.find(['(', '[']) {
        let close = if rest.as_bytes()[start] == b'(' {
            ')'
        } else {
            ']'
        };
        let Some(end) = rest[start + 1..].find(close) else {
            break;
        };
        let inner = rest[start + 1..start + 1 + end].trim();
        for part in inner.split(',') {
            let t = part.trim();
            if !t.is_empty() {
                tags.push(t.to_string());
            }
        }
        rest = &rest[start + 1 + end + 1..];
    }
    let region = tags
        .iter()
        .find(|t| REGIONS.iter().any(|r| r.eq_ignore_ascii_case(t)))
        .cloned()
        .unwrap_or_default();
    let disc = tags.iter().find_map(|t| {
        let l = t.to_lowercase();
        let n = l
            .strip_prefix("disc ")
            .or_else(|| l.strip_prefix("disk "))
            .or_else(|| l.strip_prefix("cd "))?;
        n.split_whitespace().next()?.parse().ok()
    });
    (title, tags, region, disc)
}

// ------------------------------------------------------------------ scan

/// Folders never worth entering.
fn skip_dir(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name.to_lowercase().as_str(),
            "saves"
                | "states"
                | "captures"
                | "screenshots"
                | "bios"
                | "system"
                | "config"
                | "media"
                | "images"
                | "manuals"
                | "cheats"
                | "downloaded_images"
                | "downloaded_videos"
                | "lost+found"
                | "$recycle.bin"
                | "system volume information"
                | "_extra"
                | "_recent"
                | "_favorites"
                | "_autostart"
                | "node_modules"
        )
}

/// Scan `roots` and build the index. `progress` gets a line per folder.
pub fn scan(roots: &[PathBuf], hints: &Hints, mut progress: impl FnMut(&str)) -> Index {
    let mut items = Vec::new();
    let mut unknown = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for root in roots {
        let root = match std::fs::canonicalize(root) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let mut stack = vec![(root.clone(), 0u32)];
        while let Some((dir, depth)) = stack.pop() {
            progress(&dir.display().to_string());
            let Ok(rd) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut files: Vec<PathBuf> = Vec::new();
            for e in rd.flatten() {
                let Ok(ft) = e.file_type() else { continue };
                let name = e.file_name().to_string_lossy().into_owned();
                if ft.is_dir() {
                    if depth < 6 && !skip_dir(&name) {
                        stack.push((e.path(), depth + 1));
                    }
                } else if ft.is_file() {
                    files.push(e.path());
                }
            }
            // Files a playlist owns are not games on their own.
            let mut owned: HashSet<PathBuf> = HashSet::new();
            for f in files.iter().filter(|f| ext_of(f) == "m3u") {
                for o in m3u_files(f) {
                    owned.insert(o);
                }
            }
            for f in files {
                let ext = ext_of(&f);
                if ext.is_empty() || NOISE.contains(&ext.as_str()) || owned.contains(&f) {
                    continue;
                }
                if !seen.insert(f.clone()) {
                    continue;
                }
                let known = ext_system(&ext).is_some() || AMBIGUOUS.contains(&ext.as_str());
                if !known {
                    continue;
                }
                match detect(&f, &root, hints) {
                    Some(system) => {
                        let stem = f.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                        let (title, tags, region, disc) = parse_name(stem);
                        let size = f.metadata().map(|m| m.len()).unwrap_or(0);
                        items.push(Item {
                            path: f,
                            system,
                            title,
                            tags,
                            region,
                            disc,
                            size,
                        });
                    }
                    None => {
                        // Track files of a cue sheet are silently part of it.
                        if !(ext == "bin" && f.with_extension("cue").exists()) {
                            unknown.push(f);
                        }
                    }
                }
            }
        }
    }
    items.sort_by(|a, b| {
        a.system
            .cmp(&b.system)
            .then(a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
    unknown.sort();
    Index {
        roots: roots.to_vec(),
        items,
        unknown,
        scanned_at: chrono::Local::now().to_rfc3339(),
    }
}

/// Mounted places that may hold a collection: removable disks under
/// `/run/media/<user>`, `/media`, `/mnt`, plus `~/Games` and `~/ROMs`.
pub fn discover() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let home = crate::library::home();
    let user = std::env::var("USER").unwrap_or_default();
    let mut bases = vec![
        PathBuf::from("/run/media").join(&user),
        PathBuf::from("/media").join(&user),
        PathBuf::from("/media"),
        PathBuf::from("/mnt"),
    ];
    for name in ["Games", "games", "ROMs", "roms", "Roms", "Emulation"] {
        bases.push(home.join(name));
    }
    for base in bases {
        let Ok(rd) = std::fs::read_dir(&base) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            // A mount point or folder is a candidate when it, or a `roms`
            // folder inside it, contains folders named after systems.
            for cand in [p.clone(), p.join("roms"), p.join("ROMs"), p.join("Roms")] {
                if cand.is_dir() && has_system_folders(&cand) && !out.contains(&cand) {
                    out.push(cand);
                    break;
                }
            }
        }
    }
    out
}

fn has_system_folders(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    let mut hits = 0;
    for e in rd.flatten().take(400) {
        if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            let name = e.file_name().to_string_lossy().into_owned();
            if system_from_folder_name(&name).is_some() {
                hits += 1;
                if hits >= 2 {
                    return true;
                }
            }
        }
    }
    false
}

/// Systems catalogue: our system names with the core Arch ships and the
/// extensions worth listing. Anything the scan finds but this table lacks
/// still appears, without a core, so the user sees it and can choose one.
pub const CATALOG: &[(&str, &str, &str, &[&str])] = &[
    // (system, label, core, extensions)
    (
        "nes",
        "Nintendo Entertainment System",
        "mesen",
        &["nes", "fds", "unf", "unif", "zip"],
    ),
    (
        "snes",
        "Super Nintendo",
        "snes9x",
        &["sfc", "smc", "swc", "fig", "bs", "zip"],
    ),
    (
        "megadrive",
        "Sega Mega Drive",
        "genesis_plus_gx",
        &["md", "smd", "gen", "bin", "zip"],
    ),
    (
        "mastersystem",
        "Sega Master System",
        "genesis_plus_gx",
        &["sms", "zip"],
    ),
    (
        "gamegear",
        "Sega Game Gear",
        "genesis_plus_gx",
        &["gg", "zip"],
    ),
    ("sg1000", "Sega SG-1000", "genesis_plus_gx", &["sg", "zip"]),
    (
        "segacd",
        "Sega Mega-CD",
        "genesis_plus_gx",
        &["cue", "chd", "iso", "m3u"],
    ),
    (
        "sega32x",
        "Sega 32X",
        "picodrive",
        &["32x", "chd", "m3u", "zip"],
    ),
    (
        "saturn",
        "Sega Saturn",
        "kronos",
        &["cue", "ccd", "chd", "toc", "m3u"],
    ),
    (
        "dreamcast",
        "Sega Dreamcast",
        "flycast",
        &["chd", "cdi", "gdi", "cue", "m3u"],
    ),
    (
        "naomi",
        "Sega Naomi",
        "flycast",
        &["zip", "chd", "dat", "lst"],
    ),
    ("stv", "Sega ST-V", "kronos", &["zip"]),
    (
        "pcengine",
        "PC Engine",
        "mednafen_pce_fast",
        &["pce", "sgx", "zip"],
    ),
    (
        "pcenginecd",
        "PC Engine CD",
        "mednafen_pce",
        &["cue", "ccd", "chd", "m3u"],
    ),
    (
        "n64",
        "Nintendo 64",
        "mupen64plus_next",
        &["n64", "v64", "z64", "zip"],
    ),
    ("gb", "Game Boy", "mgba", &["gb", "sgb", "zip"]),
    ("gbc", "Game Boy Color", "mgba", &["gbc", "zip"]),
    ("gba", "Game Boy Advance", "mgba", &["gba", "zip"]),
    ("nds", "Nintendo DS", "melonds", &["nds", "zip"]),
    (
        "psx",
        "PlayStation",
        "mednafen_psx_hw",
        &["cue", "chd", "pbp", "m3u", "img", "iso"],
    ),
    (
        "psp",
        "PlayStation Portable",
        "ppsspp",
        &["iso", "cso", "pbp", "chd"],
    ),
    ("neogeo", "Neo Geo", "fbneo", &["zip"]),
    ("neogeocd", "Neo Geo CD", "neocd", &["cue", "chd"]),
    (
        "ngp",
        "Neo Geo Pocket",
        "mednafen_ngp",
        &["ngp", "ngc", "ngpc", "zip"],
    ),
    ("arcade", "Arcade (FBNeo)", "fbneo", &["zip"]),
    ("mame", "Arcade (MAME)", "mame", &["zip", "chd"]),
    (
        "mame2003",
        "Arcade (MAME 2003 Plus)",
        "mame2003_plus",
        &["zip"],
    ),
    (
        "c64",
        "Commodore 64",
        "vice_x64sc",
        &["d64", "t64", "tap", "prg", "crt", "g64", "m3u", "zip"],
    ),
    (
        "amiga",
        "Commodore Amiga",
        "puae",
        &[
            "adf", "adz", "dms", "ipf", "hdf", "lha", "uae", "m3u", "zip",
        ],
    ),
    (
        "amigacd32",
        "Amiga CD32",
        "puae",
        &["cue", "chd", "iso", "m3u"],
    ),
    (
        "amstradcpc",
        "Amstrad CPC",
        "cap32",
        &["dsk", "sna", "tap", "cdt", "cpr", "m3u", "zip"],
    ),
    ("atari2600", "Atari 2600", "stella", &["a26", "bin", "zip"]),
    (
        "atari5200",
        "Atari 5200",
        "atari800",
        &["a52", "bin", "zip"],
    ),
    (
        "atari7800",
        "Atari 7800",
        "prosystem",
        &["a78", "bin", "zip"],
    ),
    ("lynx", "Atari Lynx", "handy", &["lnx", "zip"]),
    (
        "jaguar",
        "Atari Jaguar",
        "virtualjaguar",
        &["j64", "jag", "zip"],
    ),
    ("3do", "Panasonic 3DO", "opera", &["iso", "chd", "cue"]),
    ("cdi", "Philips CD-i", "same_cdi", &["iso", "chd", "cue"]),
    (
        "msx",
        "MSX",
        "bluemsx",
        &["rom", "mx1", "mx2", "dsk", "cas", "m3u", "zip"],
    ),
    (
        "zxspectrum",
        "ZX Spectrum",
        "fuse",
        &["tzx", "tap", "z80", "szx", "trd", "dsk", "zip"],
    ),
    (
        "dos",
        "MS-DOS",
        "dosbox_pure",
        &["zip", "dosz", "exe", "com", "bat", "iso", "cue", "m3u"],
    ),
    ("scummvm", "ScummVM", "scummvm", &["scummvm", "svm"]),
    (
        "x68000",
        "Sharp X68000",
        "px68k",
        &["dim", "xdf", "hdm", "d88", "m3u", "zip"],
    ),
];

pub fn catalog(system: &str) -> Option<(&'static str, &'static str, &'static [&'static str])> {
    CATALOG
        .iter()
        .find(|(s, _, _, _)| *s == system)
        .map(|(_, label, core, exts)| (*label, *core, *exts))
}

/// Package that ships a libretro core: Arch repository names first, the AUR
/// `-git` builds for the rest. `(package, aur)`.
pub fn core_package(core: &str) -> (String, bool) {
    const REPO: &[(&str, &str)] = &[
        ("mesen", "libretro-mesen"),
        ("mesen-s", "libretro-mesen-s"),
        ("nestopia", "libretro-nestopia"),
        ("snes9x", "libretro-snes9x"),
        ("bsnes", "libretro-bsnes"),
        ("bsnes_hd_beta", "libretro-bsnes-hd"),
        ("genesis_plus_gx", "libretro-genesis-plus-gx"),
        ("picodrive", "libretro-picodrive"),
        ("blastem", "libretro-blastem"),
        ("kronos", "libretro-kronos"),
        ("yabause", "libretro-yabause"),
        ("flycast", "libretro-flycast"),
        ("mednafen_pce_fast", "libretro-beetle-pce-fast"),
        ("mednafen_pce", "libretro-beetle-pce"),
        ("mednafen_supergrafx", "libretro-beetle-supergrafx"),
        ("mednafen_psx_hw", "libretro-beetle-psx-hw"),
        ("mednafen_psx", "libretro-beetle-psx"),
        ("mupen64plus_next", "libretro-mupen64plus-next"),
        ("parallel_n64", "libretro-parallel-n64"),
        ("mgba", "libretro-mgba"),
        ("gambatte", "libretro-gambatte"),
        ("sameboy", "libretro-sameboy"),
        ("melonds", "libretro-melonds"),
        ("desmume", "libretro-desmume"),
        ("ppsspp", "libretro-ppsspp"),
        ("play", "libretro-play"),
        ("mame", "libretro-mame"),
        ("scummvm", "libretro-scummvm"),
        ("dolphin", "libretro-dolphin"),
    ];
    const AUR: &[(&str, &str)] = &[
        ("mame2003_plus", "libretro-mame2003-plus-git"),
        ("mame2003", "libretro-mame2003-git"),
        ("fbneo", "libretro-fbneo-git"),
        ("neocd", "libretro-neocd-git"),
        ("mednafen_ngp", "libretro-beetle-ngp-git"),
        ("puae", "libretro-puae-git"),
        ("cap32", "libretro-cap32-git"),
    ];
    if let Some((_, p)) = REPO.iter().find(|(c, _)| *c == core) {
        return (p.to_string(), false);
    }
    if let Some((_, p)) = AUR.iter().find(|(c, _)| *c == core) {
        return (p.to_string(), true);
    }
    // VICE ships every machine in one package.
    if core.starts_with("vice_") {
        return ("libretro-vice-git".into(), true);
    }
    (format!("libretro-{}-git", core.replace('_', "-")), true)
}
