//! What MAME knows about an arcade set, for the filters over an arcade list.
//!
//! A list of three thousand arcade games is mostly games nobody wants on this
//! television tonight: vertical ones that come out as a strip in the middle of
//! a horizontal tube, ones that need a trackball or a gun, ones MAME cannot
//! run yet. Which is which is written in MAME's own list of machines, the XML
//! its `-listxml` prints, and MAME publishes that list with every release. The
//! libretro core cannot be asked for it, so it is fetched once for the version
//! the core is, reduced to one line per machine, and kept.
//!
//! The same names serve FBNeo and MAME 2003: a set is named after the board
//! and the game, and the three keep to MAME's names for nearly everything. A
//! set the list does not know passes a filter only when that filter asks for
//! nothing in particular.

use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::settings::ArcadeFilter;

/// What one machine needs and does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Machine {
    /// The screen is turned a quarter: a vertical game.
    pub vertical: bool,
    pub players: u8,
    pub screens: u8,
    /// A set of the `CTL_` bits.
    pub controls: u8,
    pub status: Status,
    pub clone: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Status {
    Good,
    Imperfect,
    /// MAME's word for a machine that does not run yet.
    Preliminary,
}

pub const CTL_JOYSTICK: u8 = 1;
pub const CTL_TWIN: u8 = 2;
pub const CTL_WHEEL: u8 = 4;
pub const CTL_DIAL: u8 = 8;
pub const CTL_TRACKBALL: u8 = 16;
pub const CTL_GUN: u8 = 32;
pub const CTL_OTHER: u8 = 64;

/// MAME's control types as the few kinds a person holds. A racing game says
/// `paddle` for its wheel and `pedal` for its pedals; a paddle on its own is
/// a knob, Arkanoid's, and goes with the dials. An analogue `stick` is a
/// flight yoke or a big joystick, which a pad's stick stands in for.
fn kind(types: &[String]) -> u8 {
    let pedal = types.iter().any(|t| t == "pedal");
    let mut bits = 0;
    for t in types {
        bits |= match t.as_str() {
            "joy" | "stick" => CTL_JOYSTICK,
            "doublejoy" | "triplejoy" => CTL_TWIN,
            "pedal" => CTL_WHEEL,
            "paddle" if pedal => CTL_WHEEL,
            "paddle" | "dial" | "positional" => CTL_DIAL,
            "trackball" | "mouse" => CTL_TRACKBALL,
            "lightgun" => CTL_GUN,
            "only_buttons" => 0,
            _ => CTL_OTHER,
        };
    }
    bits
}

/// The value of `name="..."` in one line of the list.
fn attr<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let at = line.find(&key)? + key.len();
    let end = line[at..].find('"')?;
    Some(&line[at..at + end])
}

/// MAME's machine list, read a line at a time. It is a third of a gigabyte
/// and written one element per line, so it is scanned rather than parsed: no
/// XML library, and nothing held but the machine being read. BIOSes, devices,
/// mechanical machines and anything MAME marks as not runnable are left out,
/// because none of them is a game somebody picks.
pub fn parse(reader: impl BufRead) -> Vec<(String, Machine, String)> {
    let mut out = Vec::new();
    let mut name: Option<String> = None;
    let mut keep = false;
    let mut title = String::new();
    let mut m = blank();
    let mut types: Vec<String> = Vec::new();
    for line in reader.lines().map_while(Result::ok) {
        let s = line.trim_start();
        if s.starts_with("<machine ") {
            name = attr(s, "name").map(str::to_string);
            keep = attr(s, "isbios") != Some("yes")
                && attr(s, "isdevice") != Some("yes")
                && attr(s, "ismechanical") != Some("yes")
                && attr(s, "runnable") != Some("no");
            m = blank();
            m.clone = attr(s, "cloneof").is_some();
            types.clear();
            title.clear();
        } else if name.is_none() {
            continue;
        } else if let Some(t) = s
            .strip_prefix("<description>")
            .and_then(|t| t.strip_suffix("</description>"))
        {
            title = unescape(t);
        } else if s.starts_with("<display ") {
            if m.screens == 0 {
                m.vertical = matches!(attr(s, "rotate"), Some("90" | "270"));
            }
            m.screens = m.screens.saturating_add(1);
        } else if s.starts_with("<input ") {
            m.players = attr(s, "players").and_then(|p| p.parse().ok()).unwrap_or(0);
        } else if s.starts_with("<control ") {
            if let Some(t) = attr(s, "type") {
                types.push(t.to_string());
            }
        } else if s.starts_with("<driver ") {
            m.status = match attr(s, "status") {
                Some("good") => Status::Good,
                Some("preliminary") => Status::Preliminary,
                _ => Status::Imperfect,
            };
        } else if s.starts_with("</machine>")
            && let Some(n) = name.take()
            && keep
        {
            m.controls = kind(&types);
            out.push((n, m, std::mem::take(&mut title)));
        }
    }
    out
}

/// The five entities XML has; MAME's titles use the ampersand and the
/// apostrophe more than anything, `Ghosts'n Goblins`, `Dungeons & Dragons`.
fn unescape(t: &str) -> String {
    t.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn blank() -> Machine {
    Machine {
        vertical: false,
        players: 0,
        screens: 0,
        controls: 0,
        status: Status::Imperfect,
        clone: false,
    }
}

fn path() -> PathBuf {
    crate::crt::cache_dir().join("arcade").join("machines.tsv")
}

/// One machine per line: name, vertical, players, screens, controls, status,
/// clone, title. Small enough to read whole at startup and grep by hand.
fn write_table(machines: &[(String, Machine, String)], version: &str) -> String {
    let mut out = format!("# MAME {version} machine list, reduced by omacrt\n");
    for (n, m, title) in machines {
        let status = match m.status {
            Status::Good => 'g',
            Status::Imperfect => 'i',
            Status::Preliminary => 'p',
        };
        let title = title.replace(['\t', '\n'], " ");
        out.push_str(&format!(
            "{n}\t{}\t{}\t{}\t{}\t{status}\t{}\t{title}\n",
            m.vertical as u8, m.players, m.screens, m.controls, m.clone as u8
        ));
    }
    out
}

/// The table back as machines and titles. A file written before titles were
/// kept has seven columns and gives no titles; `omacrt library arcade`
/// writes it again with them.
fn read_table(text: &str) -> (HashMap<String, Machine>, HashMap<String, String>) {
    let mut machines = HashMap::new();
    let mut titles = HashMap::new();
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 7 {
            continue;
        }
        let num = |s: &str| s.parse::<u8>().unwrap_or(0);
        machines.insert(
            f[0].to_string(),
            Machine {
                vertical: f[1] == "1",
                players: num(f[2]),
                screens: num(f[3]),
                controls: num(f[4]),
                status: match f[5] {
                    "g" => Status::Good,
                    "p" => Status::Preliminary,
                    _ => Status::Imperfect,
                },
                clone: f[6] == "1",
            },
        );
        if let Some(t) = f.get(7).filter(|t| !t.is_empty()) {
            titles.insert(f[0].to_string(), t.to_string());
        }
    }
    (machines, titles)
}

type Machines = Arc<HashMap<String, Machine>>;
type Titles = Arc<HashMap<String, String>>;

static TABLE: Mutex<Option<(Machines, Titles)>> = Mutex::new(None);

fn loaded() -> Option<(Machines, Titles)> {
    let mut held = TABLE.lock().ok()?;
    if held.is_none() {
        let text = std::fs::read_to_string(path()).ok()?;
        let (machines, titles) = read_table(&text);
        if machines.is_empty() {
            return None;
        }
        *held = Some((Arc::new(machines), Arc::new(titles)));
    }
    held.clone()
}

/// The machine list, read from the cache the first time it is asked for.
/// Nothing when it has not been fetched yet.
pub fn table() -> Option<Machines> {
    loaded().map(|(m, _)| m)
}

/// MAME's title for a set, `Ace Driver: Racing Evolution (World, AD2 Ver.B)`
/// for `acedrive`, when the list has been fetched.
pub fn title(set: &str) -> Option<String> {
    loaded().and_then(|(_, t)| t.get(set).cloned())
}

/// Read the cache again on the next `table`, after a fetch wrote it.
pub fn forget() {
    if let Ok(mut held) = TABLE.lock() {
        *held = None;
    }
}

/// The MAME version a libretro core was built from, `0289` for 0.289, read
/// out of the core itself: it carries the string `0.289 (4fc9a93...)`.
pub fn core_version(core: &Path) -> Option<String> {
    let bytes = std::fs::read(core).ok()?;
    version_in(&bytes)
}

fn version_in(bytes: &[u8]) -> Option<String> {
    bytes.windows(7).find_map(|w| {
        let ok = w[0] == b'0'
            && w[1] == b'.'
            && w[2..5].iter().all(u8::is_ascii_digit)
            && w[5] == b' '
            && w[6] == b'(';
        ok.then(|| format!("0{}", String::from_utf8_lossy(&w[2..5])))
    })
}

/// The version to fetch when the core cannot say: the one the MAME core on
/// this machine was at when this was written.
const FALLBACK: &str = "0289";

/// Fetch MAME's machine list for the core's version and keep it reduced.
/// Twenty megabytes compressed, so this is for a thread or the CLI, never the
/// frame loop. Returns how many machines were kept.
pub fn fetch(core: Option<&Path>) -> Result<usize, String> {
    let version = core
        .and_then(core_version)
        .unwrap_or_else(|| FALLBACK.to_string());
    let dir = path().parent().map(Path::to_path_buf).unwrap_or_default();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let zip = dir.join(format!("mame{version}lx.zip"));
    let url = format!(
        "https://github.com/mamedev/mame/releases/download/mame{version}/mame{version}lx.zip"
    );
    let got = crate::net::curl(900, 100 << 20)
        .arg("-o")
        .arg(&zip)
        .arg(&url)
        .status()
        .map_err(|e| format!("curl: {e}"))?;
    if !got.success() {
        let _ = std::fs::remove_file(&zip);
        return Err(format!("could not fetch {url}"));
    }
    let mut unzip = std::process::Command::new("unzip")
        .arg("-p")
        .arg(&zip)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("unzip: {e}"))?;
    let stdout = unzip.stdout.take().ok_or("unzip gave no output")?;
    let machines = parse(std::io::BufReader::new(stdout));
    let _ = unzip.wait();
    let _ = std::fs::remove_file(&zip);
    if machines.is_empty() {
        return Err("the machine list came back empty".into());
    }
    crate::store::save(&path(), write_table(&machines, &version))
        .map_err(|e| format!("{}: {e}", path().display()))?;
    forget();
    Ok(machines.len())
}

/// The set a file is: `mslug.zip` is `mslug`.
pub fn set_of(path: &Path) -> Option<String> {
    path.file_stem().map(|s| s.to_string_lossy().to_lowercase())
}

/// Whether a set passes the filter. A set the list does not know passes only
/// a filter that asks for nothing in particular.
pub fn passes(f: &ArcadeFilter, m: Option<&Machine>) -> bool {
    let Some(m) = m else {
        return f.screen.is_empty()
            && f.players == 0
            && f.controls.is_empty()
            && f.screens.is_empty();
    };
    let screen = match f.screen.as_str() {
        "horizontal" => !m.vertical,
        "vertical" => m.vertical,
        _ => true,
    };
    let controls = match f.controls.as_str() {
        "joystick" => m.controls & CTL_JOYSTICK != 0,
        "twin" => m.controls & CTL_TWIN != 0,
        "wheel" => m.controls & CTL_WHEEL != 0,
        "dial" => m.controls & CTL_DIAL != 0,
        "trackball" => m.controls & CTL_TRACKBALL != 0,
        "gun" => m.controls & CTL_GUN != 0,
        _ => true,
    };
    let screens = match f.screens.as_str() {
        "one" => m.screens <= 1,
        "several" => m.screens > 1,
        _ => true,
    };
    screen
        && controls
        && screens
        && m.players >= f.players
        && !(f.working && m.status == Status::Preliminary)
        && !(f.hide_clones && m.clone)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = r#"<?xml version="1.0"?>
<mame build="0.289">
	<machine name="neogeo" sourcefile="neogeo.cpp" isbios="yes">
		<display tag="screen" type="raster" rotate="0"/>
	</machine>
	<machine name="outrun" sourcefile="sega/segaorun.cpp">
		<description>Out Run</description>
		<display tag="screen" type="raster" rotate="0" width="320" height="224"/>
		<input players="1" coins="2">
			<control type="paddle" minimum="0" maximum="255"/>
			<control type="pedal" player="1"/>
		</input>
		<driver status="good" emulation="good"/>
	</machine>
	<machine name="galaga" sourcefile="namco/galaga.cpp">
		<description>Galaga &amp; Friends&apos; Run</description>
		<display tag="screen" type="raster" rotate="90"/>
		<input players="2" coins="2">
			<control type="joy" player="1" buttons="1" ways="2"/>
		</input>
		<driver status="good"/>
	</machine>
	<machine name="arkanoidj" sourcefile="taito/arkanoid.cpp" cloneof="arkanoid" romof="arkanoid">
		<display tag="screen" type="raster" rotate="270"/>
		<input players="2" coins="2">
			<control type="paddle" player="1"/>
		</input>
		<driver status="imperfect"/>
	</machine>
	<machine name="ninjaw" sourcefile="taito/ninjaw.cpp">
		<display tag="lscreen" type="raster" rotate="0"/>
		<display tag="mscreen" type="raster" rotate="0"/>
		<display tag="rscreen" type="raster" rotate="0"/>
		<input players="2" coins="2"><control type="joy" ways="8"/></input>
		<driver status="preliminary"/>
	</machine>
</mame>
"#;

    fn machines() -> HashMap<String, Machine> {
        parse(LIST.as_bytes())
            .into_iter()
            .map(|(n, m, _)| (n, m))
            .collect()
    }

    #[test]
    fn the_list_reads_into_machines_and_leaves_out_the_bios() {
        let m = machines();
        assert_eq!(m.len(), 4);
        assert!(!m.contains_key("neogeo"));
        let outrun = m["outrun"];
        assert!(!outrun.vertical);
        assert_eq!(outrun.players, 1);
        assert_eq!(
            outrun.controls, CTL_WHEEL,
            "a paddle beside a pedal is a wheel"
        );
        assert_eq!(outrun.status, Status::Good);
        assert!(m["galaga"].vertical);
        assert_eq!(
            m["arkanoidj"].controls, CTL_DIAL,
            "a paddle on its own is a knob"
        );
        assert!(m["arkanoidj"].clone);
        assert_eq!(m["ninjaw"].screens, 3);
        assert_eq!(m["ninjaw"].status, Status::Preliminary);
    }

    #[test]
    fn the_table_comes_back_as_it_went() {
        let list = parse(LIST.as_bytes());
        let (back, titles) = read_table(&write_table(&list, "0289"));
        assert_eq!(back.len(), list.len());
        for (n, m, _) in &list {
            assert_eq!(back[n], *m, "{n}");
        }
        assert_eq!(titles["outrun"], "Out Run");
        assert_eq!(titles["galaga"], "Galaga & Friends' Run");
    }

    #[test]
    fn each_filter_keeps_what_it_says() {
        let m = machines();
        let keep = |f: &ArcadeFilter| {
            let mut v: Vec<&str> = m
                .iter()
                .filter(|(_, mm)| passes(f, Some(mm)))
                .map(|(n, _)| n.as_str())
                .collect();
            v.sort();
            v
        };
        let mut f = ArcadeFilter::default();
        assert_eq!(keep(&f).len(), 4, "no filter keeps everything");
        f.screen = "vertical".into();
        assert_eq!(keep(&f), ["arkanoidj", "galaga"]);
        f = ArcadeFilter {
            controls: "wheel".into(),
            ..Default::default()
        };
        assert_eq!(keep(&f), ["outrun"]);
        f = ArcadeFilter {
            players: 2,
            working: true,
            ..Default::default()
        };
        assert_eq!(keep(&f), ["arkanoidj", "galaga"]);
        f = ArcadeFilter {
            hide_clones: true,
            screens: "one".into(),
            ..Default::default()
        };
        assert_eq!(keep(&f), ["galaga", "outrun"]);
    }

    #[test]
    fn a_set_the_list_does_not_know_only_passes_an_empty_filter() {
        let mut f = ArcadeFilter {
            working: true,
            hide_clones: true,
            ..Default::default()
        };
        assert!(
            passes(&f, None),
            "the two switches cannot judge it either way"
        );
        f.screen = "horizontal".into();
        assert!(!passes(&f, None));
    }

    #[test]
    fn the_version_is_read_out_of_the_core() {
        let core = b"\x00\x01garbage 0.2 nothing0.289 (4fc9a9312ba)\x00more";
        assert_eq!(version_in(core).as_deref(), Some("0289"));
        assert_eq!(version_in(b"no version here"), None);
    }
}
