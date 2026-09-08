//! Photographs from an Immich server, ready for a 240 line television.
//!
//! The server is asked for a list of interesting pictures, and each one is
//! fetched at preview size and reduced to the exact shape of the tube by
//! ffmpeg, which is already needed for video. The result is a PNG the
//! launcher can decode with the decoder it already has, cached under
//! `~/.cache/omarchy-crt/frame`, so the frame keeps working with the server
//! switched off and never waits on the network in the middle of a fade.
//!
//! Nothing leaves the house: the address is the one in
//! `~/.config/omarchy-crt/immich.toml`, and the key is handed to curl on its
//! standard input rather than on a command line, where every other process
//! on the machine could read it.

use serde::Deserialize;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Where the server is and how to prove we may ask it.
#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    /// Base address, without a trailing slash.
    pub url: String,
    /// An Immich API key with read access to assets, albums and memories.
    pub key: String,
}

impl Config {
    /// Read `immich.toml` from the configuration directory. Absent means the
    /// photo frame is simply not set up, which is not an error.
    pub fn load(config_dir: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(config_dir.join("immich.toml")).ok()?;
        let mut cfg: Config = toml::from_str(&text).ok()?;
        cfg.url = cfg.url.trim_end_matches('/').to_string();
        if cfg.url.is_empty() || cfg.key.is_empty() {
            return None;
        }
        Some(cfg)
    }
}

/// Where the pictures come from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// What the server calls memories: this day, in the years before.
    Memories,
    /// One album, by name.
    Album,
    /// Everything marked a favourite.
    Favorites,
    /// Anything at all.
    All,
}

impl Source {
    /// The name as it is written in `settings.toml`. Not `FromStr`: an
    /// unknown name is not a failure, it is the default.
    pub fn named(s: &str) -> Self {
        match s {
            "album" => Self::Album,
            "favorites" => Self::Favorites,
            "all" => Self::All,
            _ => Self::Memories,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Memories => "memories",
            Self::Album => "album",
            Self::Favorites => "favorites",
            Self::All => "all",
        }
    }
}

/// One picture, as the server describes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Shot {
    pub id: String,
    /// Original pixel size, for deciding how it should be fitted.
    pub w: u32,
    pub h: u32,
    /// When it was taken, as the server wrote it.
    pub taken: String,
    /// The years ago, when the picture came from a memory.
    pub years_ago: Option<i32>,
}

// The parts of Immich's answers this needs, and nothing else.

#[derive(Deserialize)]
struct RawAsset {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default, rename = "localDateTime")]
    local_date: Option<String>,
    #[serde(default, rename = "fileCreatedAt")]
    created: Option<String>,
    #[serde(default, rename = "isArchived")]
    archived: bool,
    #[serde(default, rename = "isTrashed")]
    trashed: bool,
}

#[derive(Deserialize)]
struct RawMemory {
    #[serde(default)]
    data: MemoryData,
    #[serde(default)]
    assets: Vec<RawAsset>,
}

#[derive(Deserialize, Default)]
struct MemoryData {
    #[serde(default)]
    year: Option<i32>,
}

#[derive(Deserialize)]
struct RawAlbum {
    id: String,
    #[serde(rename = "albumName")]
    name: String,
    #[serde(default)]
    assets: Vec<RawAsset>,
}

#[derive(Deserialize)]
struct SearchAnswer {
    assets: SearchAssets,
}

#[derive(Deserialize)]
struct SearchAssets {
    items: Vec<RawAsset>,
}

/// Details worth printing under a photograph.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Details {
    pub place: String,
    pub taken: String,
    pub people: Vec<String>,
}

#[derive(Deserialize)]
struct RawDetails {
    #[serde(default, rename = "exifInfo")]
    exif: Option<Exif>,
    #[serde(default)]
    people: Vec<Person>,
}

#[derive(Deserialize)]
struct Exif {
    #[serde(default)]
    city: Option<String>,
    #[serde(default)]
    country: Option<String>,
    #[serde(default, rename = "dateTimeOriginal")]
    taken: Option<String>,
}

#[derive(Deserialize)]
struct Person {
    #[serde(default)]
    name: String,
}

/// Is this an identifier, and only an identifier?
///
/// The id becomes part of a file name in the cache, so a server that sent
/// `../../.ssh/authorized_keys` would otherwise decide where a picture is
/// written. Immich's own ids are UUIDs; anything that is not letters, digits
/// and dashes is not one, and a picture carrying it is skipped.
fn is_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// One line of a note, with anything that would break the file's own shape
/// taken out: it is four lines, and a name with a newline in it would move
/// every field after it.
fn one_line(s: &str) -> String {
    s.chars()
        .filter(|c| *c != '\n' && *c != '\r')
        .take(120)
        .collect()
}

impl RawAsset {
    fn shot(self, years_ago: Option<i32>) -> Option<Shot> {
        if self.kind != "IMAGE" || self.archived || self.trashed {
            return None;
        }
        if !is_id(&self.id) {
            return None;
        }
        Some(Shot {
            id: self.id,
            w: self.width.unwrap_or(0),
            h: self.height.unwrap_or(0),
            taken: self.local_date.or(self.created).unwrap_or_default(),
            years_ago,
        })
    }
}

/// Ask the server for something, with the key on curl's standard input.
///
/// A key on a command line is readable by every process on the machine, and
/// this one opens somebody's whole photograph collection.
fn ask(cfg: &Config, path: &str, body: Option<&str>, out: Option<&Path>) -> Option<Vec<u8>> {
    let mut cmd = crate::net::curl(40, 33_554_432);
    // curl reads the key from its own standard input, not from a flag.
    cmd.args(["-K", "-"]);
    if let Some(json) = body {
        cmd.args(["-H", "Content-Type: application/json", "-X", "POST", "-d"])
            .arg(json);
    }
    if let Some(dest) = out {
        cmd.arg("-o").arg(dest);
    }
    cmd.arg(format!("{}{path}", cfg.url));
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::null());
    let mut child = cmd.spawn().ok()?;
    {
        let stdin = child.stdin.as_mut()?;
        // curl reads its own options here, so the key never appears in the
        // process list.
        writeln!(stdin, "header = \"x-api-key: {}\"", cfg.key).ok()?;
    }
    let done = child.wait_with_output().ok()?;
    if !done.status.success() {
        return None;
    }
    Some(done.stdout)
}

fn ask_json<T: for<'a> Deserialize<'a>>(cfg: &Config, path: &str, body: Option<&str>) -> Option<T> {
    let bytes = ask(cfg, path, body, None)?;
    serde_json::from_slice(&bytes).ok()
}

/// Is the server there and does it accept the key?
pub fn check(cfg: &Config) -> Result<String, String> {
    #[derive(Deserialize)]
    struct About {
        version: String,
    }
    match ask_json::<About>(cfg, "/api/server/about", None) {
        Some(a) => Ok(a.version),
        None => Err(format!("{} did not answer, or refused the key", cfg.url)),
    }
}

/// The pictures to show, newest memory first, in the order they should go up.
pub fn list(cfg: &Config, source: Source, album: &str, want: usize) -> Vec<Shot> {
    let mut out = match source {
        Source::Memories => {
            let memories: Vec<RawMemory> = ask_json(cfg, "/api/memories", None).unwrap_or_default();
            let this_year = chrono::Local::now().format("%Y").to_string();
            let this_year: i32 = this_year.parse().unwrap_or(0);
            memories
                .into_iter()
                .flat_map(|m| {
                    let ago = m.data.year.map(|y| this_year - y).filter(|a| *a > 0);
                    m.assets
                        .into_iter()
                        .filter_map(move |a| a.shot(ago))
                        .collect::<Vec<_>>()
                })
                .collect()
        }
        Source::Album => {
            let albums: Vec<RawAlbum> = ask_json(cfg, "/api/albums", None).unwrap_or_default();
            let wanted = album.trim().to_lowercase();
            let found = albums
                .into_iter()
                .find(|a| a.name.to_lowercase() == wanted)
                .map(|a| a.id);
            match found {
                Some(id) => {
                    let full: Option<RawAlbum> = ask_json(cfg, &format!("/api/albums/{id}"), None);
                    full.map(|a| a.assets)
                        .unwrap_or_default()
                        .into_iter()
                        .filter_map(|a| a.shot(None))
                        .collect()
                }
                None => Vec::new(),
            }
        }
        Source::Favorites => {
            let body = format!(
                r#"{{"isFavorite":true,"type":"IMAGE","size":{},"withPeople":false}}"#,
                want.clamp(1, 250)
            );
            let answer: Option<SearchAnswer> = ask_json(cfg, "/api/search/metadata", Some(&body));
            answer
                .map(|a| a.assets.items)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|a| a.shot(None))
                .collect()
        }
        Source::All => {
            let body = format!(r#"{{"size":{},"type":"IMAGE"}}"#, want.clamp(1, 250));
            let assets: Vec<RawAsset> =
                ask_json(cfg, "/api/search/random", Some(&body)).unwrap_or_default();
            assets.into_iter().filter_map(|a| a.shot(None)).collect()
        }
    };
    out.truncate(want.max(1));
    out
}

/// Where the place and the faces come from, asked one picture at a time
/// because the list endpoints do not carry them.
pub fn details(cfg: &Config, id: &str) -> Details {
    let raw: Option<RawDetails> = ask_json(cfg, &format!("/api/assets/{id}"), None);
    let Some(raw) = raw else {
        return Details::default();
    };
    let exif = raw.exif.unwrap_or(Exif {
        city: None,
        country: None,
        taken: None,
    });
    let place = match (exif.city, exif.country) {
        (Some(c), Some(k)) if !c.is_empty() && !k.is_empty() => format!("{c}, {k}"),
        (Some(c), _) if !c.is_empty() => c,
        (_, Some(k)) if !k.is_empty() => k,
        _ => String::new(),
    };
    Details {
        place,
        taken: exif.taken.unwrap_or_default(),
        people: raw
            .people
            .into_iter()
            .map(|p| p.name)
            .filter(|n| !n.is_empty())
            .take(3)
            .collect(),
    }
}

pub fn cache_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".cache/omarchy-crt/frame")
}

/// How a picture should meet a 4:3 tube.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fit {
    /// Close enough to the shape of the screen to fill it, with a margin
    /// left over on purpose so the picture can drift while it is up.
    Pan,
    /// The wrong shape for the screen: fitted whole, with the gap filled by
    /// a blurred, darkened copy of itself rather than by black bars.
    Blurred,
}

/// Which way a picture of this shape should be fitted.
fn fit_for(w: u32, h: u32, screen_w: u32, screen_h: u32) -> Fit {
    if w == 0 || h == 0 || screen_h == 0 {
        return Fit::Blurred;
    }
    let want = screen_w as f32 / screen_h as f32;
    let have = w as f32 / h as f32;
    // A quarter either way still crops into something worth looking at.
    if (have / want).clamp(0.75, 1.34) == have / want {
        Fit::Pan
    } else {
        Fit::Blurred
    }
}

/// Room left around the picture when it can drift, as a fraction.
pub const PAN_MARGIN: f32 = 0.18;

/// The size a picture of this shape is prepared at for this screen.
fn prepared_size(shot: &Shot, screen_w: u32, screen_h: u32) -> (Fit, u32, u32) {
    let fit = fit_for(shot.w, shot.h, screen_w, screen_h);
    match fit {
        Fit::Pan => (
            fit,
            (screen_w as f32 * (1.0 + PAN_MARGIN)).round() as u32,
            (screen_h as f32 * (1.0 + PAN_MARGIN)).round() as u32,
        ),
        Fit::Blurred => (fit, screen_w, screen_h),
    }
}

/// Where this picture would be in the cache, whether or not it is there.
pub fn prepared_path(shot: &Shot, screen_w: u32, screen_h: u32) -> Option<PathBuf> {
    let (_, w, h) = prepared_size(shot, screen_w, screen_h);
    Some(cache_dir().join(format!("{}-{w}x{h}.png", shot.id)))
}

/// Fetch one picture and leave it in the cache at the size of the tube.
/// Returns the file, which is wider and taller than the screen when the
/// picture is allowed to drift.
pub fn prepare(cfg: &Config, shot: &Shot, screen_w: u32, screen_h: u32) -> Option<PathBuf> {
    let dir = cache_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let (fit, out_w, out_h) = prepared_size(shot, screen_w, screen_h);
    let dest = dir.join(format!("{}-{out_w}x{out_h}.png", shot.id));
    if dest.exists() {
        return Some(dest);
    }
    let tmp = dir.join(format!("{}.part", shot.id));
    ask(
        cfg,
        &format!("/api/assets/{}/thumbnail?size=preview", shot.id),
        None,
        Some(&tmp),
    )?;
    if std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0) < 1024 {
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    // Lanczos, because a photograph reduced to a quarter of a megapixel by a
    // cheaper filter turns to mush.
    let filter = match fit {
        Fit::Pan => format!(
            "scale={out_w}:{out_h}:force_original_aspect_ratio=increase:flags=lanczos,\
             crop={out_w}:{out_h}"
        ),
        Fit::Blurred => format!(
            "[0:v]scale={out_w}:{out_h}:force_original_aspect_ratio=increase:flags=bilinear,\
             crop={out_w}:{out_h},boxblur=14:2,eq=brightness=-0.16:saturation=0.65[bg];\
             [0:v]scale={out_w}:{out_h}:force_original_aspect_ratio=decrease:flags=lanczos[fg];\
             [bg][fg]overlay=(W-w)/2:(H-h)/2"
        ),
    };
    let flag = if fit == Fit::Pan {
        "-vf"
    } else {
        "-filter_complex"
    };
    let ok = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-i"])
        .arg(&tmp)
        .args([flag, &filter])
        .args(["-frames:v", "1"])
        .arg(&dest)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let _ = std::fs::remove_file(&tmp);
    if !ok {
        let _ = std::fs::remove_file(&dest);
        return None;
    }
    Some(dest)
}

/// What is written beside a prepared picture: where, when, how long ago and
/// who, so the frame can put a caption under a photograph it has already got
/// without asking the server a thing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Note {
    pub place: String,
    pub when: String,
    pub ago: String,
    pub people: Vec<String>,
}

impl Note {
    /// The note for a picture: what the server says about it, and how long
    /// ago it was. Derived here because both the launcher's own thread and
    /// `omarchy-crt frame fill` need exactly the same answer.
    pub fn of(shot: &Shot, details: &Details) -> Self {
        let when = if details.taken.is_empty() {
            spoken_date(&shot.taken)
        } else {
            spoken_date(&details.taken)
        };
        let ago = match shot.years_ago {
            Some(1) => "a year ago today".to_string(),
            Some(n) if n > 1 => format!("{n} years ago today"),
            _ => String::new(),
        };
        Self {
            place: details.place.clone(),
            when,
            ago,
            people: details.people.clone(),
        }
    }

    pub fn path(picture: &Path) -> PathBuf {
        picture.with_extension("txt")
    }

    pub fn write(&self, picture: &Path) {
        let text = format!(
            "{}\n{}\n{}\n{}\n",
            one_line(&self.place),
            one_line(&self.when),
            one_line(&self.ago),
            one_line(&self.people.join(", "))
        );
        let _ = std::fs::write(Self::path(picture), text);
    }

    pub fn read(picture: &Path) -> Self {
        let text = std::fs::read_to_string(Self::path(picture)).unwrap_or_default();
        let mut lines = text.lines();
        let place = lines.next().unwrap_or("").to_string();
        let when = lines.next().unwrap_or("").to_string();
        let ago = lines.next().unwrap_or("").to_string();
        let people = lines
            .next()
            .unwrap_or("")
            .split(',')
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .collect();
        Self {
            place,
            when,
            ago,
            people,
        }
    }
}

/// Pictures already prepared for a screen this size, in no order.
///
/// The cache is the frame's own collection: with the server off, or the
/// house's network down, this is what the television shows, and it is also
/// what fills the screen in the first second rather than a blank wait.
pub fn cached(screen_w: u32, screen_h: u32) -> Vec<PathBuf> {
    let dir = cache_dir();
    let exact = format!("-{screen_w}x{screen_h}.png");
    let panned = format!(
        "-{}x{}.png",
        (screen_w as f32 * (1.0 + PAN_MARGIN)).round() as u32,
        (screen_h as f32 * (1.0 + PAN_MARGIN)).round() as u32
    );
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if name.ends_with(&exact) || name.ends_with(&panned) {
            out.push(path);
        }
    }
    out
}

/// Throw away everything cached, so the next frame fetches again.
pub fn clear_cache() -> std::io::Result<()> {
    let dir = cache_dir();
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(&dir)?.flatten() {
        let _ = std::fs::remove_file(entry.path());
    }
    Ok(())
}

/// A date the server wrote, as a person would say it: "10 September 2025".
pub fn spoken_date(raw: &str) -> String {
    let Some(day) = raw.get(0..10) else {
        return String::new();
    };
    let mut parts = day.split('-');
    let year = parts.next().unwrap_or("");
    let month: usize = parts.next().unwrap_or("").parse().unwrap_or(0);
    let d: u32 = parts.next().unwrap_or("").parse().unwrap_or(0);
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    if month == 0 || month > 12 || d == 0 {
        return String::new();
    }
    format!("{d} {} {year}", MONTHS[month - 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_cannot_choose_where_a_picture_is_written() {
        // The id becomes part of a file name, so only an identifier will do.
        assert!(is_id("c159da59-3751-4d1e-b044-6e79277d104f"));
        assert!(!is_id("../../.ssh/authorized_keys"));
        assert!(!is_id("a/b"));
        assert!(!is_id("a b"));
        assert!(!is_id(""));
        assert!(!is_id(&"a".repeat(65)));
        // A picture carrying one is skipped rather than fetched.
        let raw: Vec<RawAsset> = serde_json::from_str(
            r#"[{"id":"../../escape","type":"IMAGE"},{"id":"ok-1","type":"IMAGE"}]"#,
        )
        .unwrap();
        let ids: Vec<String> = raw
            .into_iter()
            .filter_map(|a| a.shot(None))
            .map(|s| s.id)
            .collect();
        assert_eq!(ids, vec!["ok-1"]);
    }

    #[test]
    fn a_note_stays_four_lines_whatever_the_names_hold() {
        let note = Note {
            place: "Somewhere\nelse".into(),
            when: "today".into(),
            ago: String::new(),
            people: vec!["a\r\nb".into()],
        };
        let dir = std::env::temp_dir().join(format!("omarchy-crt-note-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let picture = dir.join("x.png");
        note.write(&picture);
        let text = std::fs::read_to_string(Note::path(&picture)).unwrap();
        assert_eq!(text.lines().count(), 4);
        let back = Note::read(&picture);
        assert_eq!(back.place, "Somewhereelse");
        let _ = std::fs::remove_file(Note::path(&picture));
    }

    #[test]
    fn a_video_is_not_a_photograph() {
        let raw: Vec<RawAsset> = serde_json::from_str(
            r#"[{"id":"a","type":"IMAGE","width":4,"height":3,"localDateTime":"2025-09-10T16:00:00Z"},
                {"id":"b","type":"VIDEO","width":4,"height":3},
                {"id":"c","type":"IMAGE","isArchived":true},
                {"id":"d","type":"IMAGE","isTrashed":true}]"#,
        )
        .unwrap();
        let shots: Vec<Shot> = raw.into_iter().filter_map(|a| a.shot(Some(1))).collect();
        assert_eq!(shots.len(), 1);
        assert_eq!(shots[0].id, "a");
        assert_eq!(shots[0].years_ago, Some(1));
        assert_eq!(shots[0].taken, "2025-09-10T16:00:00Z");
    }

    #[test]
    fn the_shape_decides_how_a_picture_is_fitted() {
        // A photograph from a phone, held upright: nothing to crop to 4:3.
        assert_eq!(fit_for(1440, 1920, 320, 240), Fit::Blurred);
        // A landscape photograph from the same phone: fills the screen.
        assert_eq!(fit_for(4032, 3024, 320, 240), Fit::Pan);
        // Sixteen by nine is close enough to crop.
        assert_eq!(fit_for(1920, 1080, 320, 240), Fit::Pan);
        // A panorama is not.
        assert_eq!(fit_for(6000, 1200, 320, 240), Fit::Blurred);
        // Nothing known about it: leave the whole picture in.
        assert_eq!(fit_for(0, 0, 320, 240), Fit::Blurred);
    }

    #[test]
    fn dates_read_as_a_person_would_say_them() {
        assert_eq!(
            spoken_date("2025-09-10T16:00:29.034+00:00"),
            "10 September 2025"
        );
        assert_eq!(spoken_date("2019-01-01"), "1 January 2019");
        assert_eq!(spoken_date(""), "");
        assert_eq!(spoken_date("not a date"), "");
    }

    #[test]
    fn a_memory_carries_how_long_ago_it_was() {
        let m: RawMemory =
            serde_json::from_str(r#"{"data":{"year":2019},"assets":[{"id":"x","type":"IMAGE"}]}"#)
                .unwrap();
        assert_eq!(m.data.year, Some(2019));
        assert_eq!(m.assets.len(), 1);
    }
}
