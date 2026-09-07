//! Box art that does not match by file name. The libretro thumbnail
//! repositories name every cover the No-Intro or Redump way ("007 Racing
//! (USA).png"); many collections do not (a RePlayOS PlayStation set is
//! "007 Racing.chd"). An exact lookup then finds nothing for a whole system.
//!
//! This module fetches the list of names a repository holds, once per system
//! and cached for two weeks, and matches a ROM's title to it the way a person
//! would: same words after the tags, punctuation and case are dropped, the
//! preferred region when several editions exist, a close enough set of words
//! otherwise. `omarchy-crt library covers` runs it over the whole collection;
//! the launcher uses the same match when a cover is missing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const THUMBS: &str = "https://thumbnails.libretro.com";
const INDEX_MAX_AGE: Duration = Duration::from_secs(14 * 24 * 3600);

/// libretro thumbnail folder (and systematic asset) per system name.
pub fn label(system: &str) -> Option<&'static str> {
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
        "gamecube" => "Nintendo - GameCube",
        "wii" => "Nintendo - Wii",
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

/// libretro replaces characters that cannot be file names with `_`.
pub fn thumb_name(stem: &str) -> String {
    stem.chars()
        .map(|c| if "&*/:`<>?\\|".contains(c) { '_' } else { c })
        .collect()
}

pub fn percent_encode(s: &str) -> String {
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

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

pub fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::library::home().join(".cache"))
        .join("omarchy-crt/art")
}

/// Where a game's cover lives in the cache, whatever name it was found under.
pub fn cache_path(system: &str, stem: &str) -> PathBuf {
    cache_dir()
        .join(system)
        .join(format!("{}.png", thumb_name(stem)))
}

/// The names a repository folder holds, with their normalized forms.
pub struct NameIndex {
    pub names: Vec<String>,
    keys: Vec<String>,
    by_key: HashMap<String, Vec<usize>>,
}

impl NameIndex {
    /// The index of one system's `Named_Boxarts`, from the cache or the
    /// directory listing of the thumbnail server (once a fortnight).
    pub fn load(label: &str) -> Option<Self> {
        let dir = cache_dir().join("_index");
        let file = dir.join(format!("{}.txt", thumb_name(label)));
        let fresh = std::fs::metadata(&file)
            .and_then(|m| m.modified())
            .map(|m| SystemTime::now().duration_since(m).unwrap_or_default() < INDEX_MAX_AGE)
            .unwrap_or(false);
        let text = if fresh {
            std::fs::read_to_string(&file).ok()?
        } else {
            let url = format!("{THUMBS}/{}/Named_Boxarts/", percent_encode(label));
            let out = std::process::Command::new("curl")
                .args([
                    "-fsSL",
                    "--max-time",
                    "60",
                    "-A",
                    "omarchy-crt",
                    "--proto",
                    "=http,https",
                    "--proto-redir",
                    "=http,https",
                    "--max-filesize",
                    "26214400",
                    "--retry",
                    "1",
                ])
                .arg(&url)
                .output()
                .ok()?;
            if !out.status.success() {
                return std::fs::read_to_string(&file)
                    .ok()
                    .map(|t| Self::from_text(&t));
            }
            let html = String::from_utf8_lossy(&out.stdout);
            let mut names = Vec::new();
            for part in html.split("href=\"").skip(1) {
                let Some(end) = part.find('"') else { continue };
                let href = &part[..end];
                if let Some(stem) = href.strip_suffix(".png")
                    && !stem.contains('/')
                {
                    names.push(percent_decode(stem));
                }
            }
            if names.is_empty() {
                return None;
            }
            let text = names.join("\n");
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(&file, &text);
            text
        };
        Some(Self::from_text(&text))
    }

    fn from_text(text: &str) -> Self {
        let names: Vec<String> = text
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect();
        let keys: Vec<String> = names.iter().map(|n| normalize(n)).collect();
        let mut by_key: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, k) in keys.iter().enumerate() {
            by_key.entry(k.clone()).or_default().push(i);
        }
        Self {
            names,
            keys,
            by_key,
        }
    }

    pub fn contains(&self, name: &str) -> bool {
        self.names.iter().any(|n| n == name)
    }

    /// The best name for a ROM stem: the exact file name when the repository
    /// has it, else the same title in the preferred region, else the closest
    /// title by words. None when nothing is close enough.
    pub fn best(&self, stem: &str, regions: &[&str]) -> Option<&str> {
        let wanted = thumb_name(stem);
        if self.contains(&wanted) {
            return self
                .names
                .iter()
                .find(|n| **n == wanted)
                .map(|s| s.as_str());
        }
        let key = normalize(stem);
        if key.is_empty() {
            return None;
        }
        if let Some(hits) = self.by_key.get(&key) {
            return Some(self.pick_region(hits, stem, regions));
        }
        // Words: every word of the title in the candidate, most words shared.
        let words: Vec<&str> = key.split(' ').collect();
        let mut best: Option<(f32, usize)> = None;
        for (i, k) in self.keys.iter().enumerate() {
            let cw: Vec<&str> = k.split(' ').collect();
            let shared = words.iter().filter(|w| cw.contains(w)).count();
            if shared == 0 {
                continue;
            }
            let score = shared as f32 * 2.0 / (words.len() + cw.len()) as f32;
            let prefix_bonus = if k.starts_with(&key) || key.starts_with(k) {
                0.15
            } else {
                0.0
            };
            let score = score + prefix_bonus;
            if score >= 0.72 && best.map(|(s, _)| score > s).unwrap_or(true) {
                best = Some((score, i));
            }
        }
        let (_, i) = best?;
        // Every candidate with the same normalized title as the winner: the
        // preferred region among them.
        let k = &self.keys[i];
        let hits = self.by_key.get(k)?;
        Some(self.pick_region(hits, stem, regions))
    }

    fn pick_region<'a>(&'a self, hits: &[usize], stem: &str, regions: &[&str]) -> &'a str {
        // A region tag in the ROM name itself wins.
        let stem_tags = tags(stem);
        for i in hits {
            let t = tags(&self.names[*i]);
            if !stem_tags.is_empty() && stem_tags.iter().any(|s| t.contains(s)) {
                return &self.names[*i];
            }
        }
        for r in regions {
            for i in hits {
                if tags(&self.names[*i])
                    .iter()
                    .any(|t| t.eq_ignore_ascii_case(r))
                {
                    return &self.names[*i];
                }
            }
        }
        // Prefer a plain edition over demos, betas and prototypes.
        hits.iter()
            .map(|i| &self.names[*i])
            .min_by_key(|n| {
                let l = n.to_lowercase();
                (l.contains("(demo")
                    || l.contains("(beta")
                    || l.contains("(proto")
                    || l.contains("(sample")) as u8
            })
            .map(|s| s.as_str())
            .unwrap_or(&self.names[hits[0]])
    }
}

/// Region preference for a country code: home region first.
pub fn regions_for(country: &str) -> Vec<&'static str> {
    let europe = ["Europe", "World", "USA", "Japan"];
    let usa = ["USA", "World", "Europe", "Japan"];
    let japan = ["Japan", "World", "USA", "Europe"];
    match country.to_ascii_uppercase().as_str() {
        "US" | "CA" | "MX" | "BR" | "AR" => usa.to_vec(),
        "JP" | "KR" | "CN" | "TW" => japan.to_vec(),
        _ => europe.to_vec(),
    }
}

/// Tags in parentheses or brackets, as they appear.
fn tags(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = name;
    while let Some(start) = rest.find(['(', '[']) {
        let close = if rest.as_bytes()[start] == b'(' {
            ')'
        } else {
            ']'
        };
        let Some(end) = rest[start + 1..].find(close) else {
            break;
        };
        for t in rest[start + 1..start + 1 + end].split(',') {
            out.push(t.trim().to_string());
        }
        rest = &rest[start + 1 + end + 1..];
    }
    out
}

/// Title without tags, lower case, letters and digits only, one space between
/// words, "the" and "&" folded so "The Legend of Zelda" meets "Legend of Zelda".
pub fn normalize(name: &str) -> String {
    let mut base = String::new();
    let mut depth = 0i32;
    for c in name.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            _ if depth <= 0 => base.push(c),
            _ => {}
        }
    }
    let base = base.to_lowercase().replace('&', " and ");
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    for c in base.chars() {
        if c.is_alphanumeric() {
            cur.push(c);
        } else if !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words.retain(|w| w != "the");
    words.join(" ")
}

/// A game's cover into the cache: the exact name first, then the closest
/// name in the repository's index. True when a file is now in place.
pub fn fetch_cover(label: &str, stem: &str, dest: &Path, regions: &[&str]) -> bool {
    if download(label, &thumb_name(stem), dest) {
        return true;
    }
    let Some(index) = NameIndex::load(label) else {
        return false;
    };
    match index.best(stem, regions) {
        Some(name) if name != thumb_name(stem) => download(label, name, dest),
        _ => false,
    }
}

/// Fetch a cover file from the repository into `dest`, shrunk for a 240 line
/// screen when ffmpeg is around (full size thumbnails are half a megabyte).
pub fn download(label: &str, name: &str, dest: &Path) -> bool {
    if let Some(dir) = dest.parent()
        && std::fs::create_dir_all(dir).is_err()
    {
        return false;
    }
    let url = format!(
        "{THUMBS}/{}/Named_Boxarts/{}.png",
        percent_encode(label),
        percent_encode(name)
    );
    let tmp = dest.with_extension("part");
    let ok = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "30",
            "-A",
            "omarchy-crt",
            "--proto",
            "=http,https",
            "--proto-redir",
            "=http,https",
            "--max-filesize",
            "26214400",
            "--retry",
            "1",
            "-o",
        ])
        .arg(&tmp)
        .arg(&url)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    // A empty or truncated answer is no picture: the server sometimes
    // returns a zero length body with a 200.
    let size = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
    if !ok || size < 512 {
        let _ = std::fs::remove_file(&tmp);
        return false;
    }
    let shrunk = dest.with_extension("small");
    let small = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-i"])
        .arg(&tmp)
        .args([
            "-vf",
            "scale='min(320,iw)':-1",
            "-frames:v",
            "1",
            "-f",
            "image2",
            "-c:v",
            "png",
        ])
        .arg(&shrunk)
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let done = if small && std::fs::rename(&shrunk, dest).is_ok() {
        let _ = std::fs::remove_file(&tmp);
        true
    } else {
        let _ = std::fs::remove_file(&shrunk);
        std::fs::rename(&tmp, dest).is_ok()
    };
    if done {
        let _ = std::fs::remove_file(dest.with_extension("missing"));
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization() {
        assert_eq!(normalize("007 Racing (USA)"), "007 racing");
        assert_eq!(
            normalize("The Legend of Zelda - A Link to the Past (USA)"),
            "legend of zelda a link to past"
        );
        assert_eq!(normalize("Sonic & Knuckles"), "sonic and knuckles");
    }

    #[test]
    fn matching() {
        let ix = NameIndex::from_text(
            "007 Racing (USA)\n007 Racing (Europe)\n007 - The World Is Not Enough (USA)\nTekken 3 (USA)\nTekken 3 (Japan) (Demo)\nCrash Bandicoot (USA)",
        );
        assert_eq!(
            ix.best("007 Racing", &["Europe", "USA"]),
            Some("007 Racing (Europe)")
        );
        assert_eq!(
            ix.best("007 Racing (USA)", &["Europe"]),
            Some("007 Racing (USA)")
        );
        assert_eq!(
            ix.best("Tekken 3", &["Europe", "USA"]),
            Some("Tekken 3 (USA)")
        );
        assert_eq!(
            ix.best("The World Is Not Enough", &["USA"]),
            Some("007 - The World Is Not Enough (USA)")
        );
        assert_eq!(ix.best("Gran Turismo", &["USA"]), None);
    }
}

// --------------------------------------------------------- arcade set names

/// Arcade collections name their files after the emulated set, not the game:
/// `mslug.zip`, not `Metal Slug`. The thumbnail repository is keyed by title
/// for every system, so a set name finds nothing. RetroArch ships the
/// databases that pair the two, and this reads them: `rom_name` is the file,
/// `name` the title.
///
/// The databases are MessagePack records; rather than parse the format, the
/// two keys are found by their own headers and the string that follows each
/// is read, which is all this needs.
fn rdb_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        out.push(PathBuf::from(&home).join(".config/retroarch/database/rdb"));
    }
    out.push(PathBuf::from("/usr/share/libretro/database/rdb"));
    out
}

/// The databases to read for a system, in order: its own first, then the
/// arcade sets that also carry its games.
fn rdb_names(system: &str) -> &'static [&'static str] {
    match system {
        "arcade" => &["FBNeo - Arcade Games", "MAME"],
        "mame" => &["MAME", "FBNeo - Arcade Games"],
        "mame2003" => &["MAME 2003-Plus", "MAME"],
        "neogeo" => &["FBNeo - Arcade Games", "MAME"],
        "naomi" => &["MAME", "FBNeo - Arcade Games"],
        "stv" => &["MAME", "FBNeo - Arcade Games"],
        _ => &[],
    }
}

/// One MessagePack string starting at `i`, and where it ends.
fn mp_str(b: &[u8], i: usize) -> Option<(String, usize)> {
    let head = *b.get(i)?;
    let (len, start) = match head {
        0xa0..=0xbf => ((head & 0x1f) as usize, i + 1),
        0xd9 => (*b.get(i + 1)? as usize, i + 2),
        0xda => (
            u16::from_be_bytes([*b.get(i + 1)?, *b.get(i + 2)?]) as usize,
            i + 3,
        ),
        _ => return None,
    };
    let end = start + len;
    let s = std::str::from_utf8(b.get(start..end)?).ok()?;
    Some((s.to_string(), end))
}

/// Set name (without extension, lowercased) to title, from one database.
fn read_rdb(path: &Path) -> Option<HashMap<String, String>> {
    let b = std::fs::read(path).ok()?;
    let mut out: HashMap<String, String> = HashMap::new();
    let mut title: Option<String> = None;
    let mut i = 0usize;
    // The records put `name` before `rom_name`, so the last title seen when a
    // file name turns up is the title of that file.
    while i + 9 < b.len() {
        if b[i] == 0xa4
            && &b[i + 1..i + 5] == b"name"
            && let Some((s, end)) = mp_str(&b, i + 5)
        {
            title = Some(s);
            i = end;
            continue;
        }
        if b[i] == 0xa8
            && &b[i + 1..i + 9] == b"rom_name"
            && let Some((file, end)) = mp_str(&b, i + 9)
        {
            if let Some(t) = &title {
                let stem = file.rsplit_once('.').map(|(s, _)| s).unwrap_or(&file);
                out.entry(stem.to_ascii_lowercase())
                    .or_insert_with(|| t.clone());
            }
            i = end;
            continue;
        }
        i += 1;
    }
    if out.is_empty() { None } else { Some(out) }
}

type SetNames = std::sync::Arc<HashMap<String, String>>;

fn set_names(system: &str) -> Option<SetNames> {
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<String, Option<SetNames>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(hit) = cache.lock().ok()?.get(system) {
        return hit.clone();
    }
    let mut merged: HashMap<String, String> = HashMap::new();
    for name in rdb_names(system) {
        for dir in rdb_dirs() {
            let path = dir.join(format!("{name}.rdb"));
            if !path.is_file() {
                continue;
            }
            if let Some(map) = read_rdb(&path) {
                for (k, v) in map {
                    merged.entry(k).or_insert(v);
                }
            }
            break;
        }
    }
    let value = if merged.is_empty() {
        None
    } else {
        Some(std::sync::Arc::new(merged))
    };
    if let Ok(mut c) = cache.lock() {
        c.insert(system.to_string(), value.clone());
    }
    value
}

/// The title of an arcade set, when the file is named after the set and a
/// database knows it. Anything else is returned unchanged.
pub fn title_for(system: &str, stem: &str) -> String {
    // A real title has spaces or capitals; a set name is a short lowercase
    // word, so nothing else is looked up.
    if stem.contains(' ') || stem.chars().any(|c| c.is_ascii_uppercase()) || stem.len() > 16 {
        return stem.to_string();
    }
    match set_names(system).and_then(|m| m.get(&stem.to_ascii_lowercase()).cloned()) {
        Some(t) => t,
        None => stem.to_string(),
    }
}

#[cfg(test)]
mod set_name_tests {
    /// The databases RetroArch ships are not part of this repository, so the
    /// test only asks for a well known set when they are installed.
    #[test]
    fn a_neo_geo_set_finds_its_title() {
        if super::set_names("neogeo").is_none() {
            return;
        }
        assert_eq!(
            super::title_for("neogeo", "mslug"),
            "Metal Slug - Super Vehicle-001"
        );
        assert_eq!(
            super::title_for("neogeo", "Metal Slug (Europe)"),
            "Metal Slug (Europe)"
        );
    }
}
