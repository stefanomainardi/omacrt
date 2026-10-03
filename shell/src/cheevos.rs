//! RetroAchievements: RetroArch earns them, the launcher shows how far each
//! game has got.
//!
//! RetroArch does all the earning. It needs the account to log in with, and
//! the launcher hands it over in the per launch configuration, which is
//! written readable by its owner alone and tells RetroArch not to save its
//! configuration at exit, so the password never lands in `retroarch.cfg`.
//! Hardcore mode is off unless asked for: it turns off save states and
//! rewind, which the pause menu is built on.
//!
//! What the launcher shows comes from the site's web API, which takes a
//! separate key (the "web API key" on the account's settings page). The key
//! is handed to curl on its standard input, never on a command line, and the
//! answer is kept for an hour. A game is matched by its console and its title
//! with the punctuation, articles and bracketed tags taken out, because the
//! site names a game once and a library names it with its region and
//! revision.
//!
//! Everything is read from `~/.config/omacrt/retroachievements.toml`, written
//! by hand:
//!
//! ```toml
//! username = "you"
//! password = "for RetroArch to log in with"
//! api_key = "the web API key, for the launcher to read progress"
//! hardcore = false
//! ```

use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub hardcore: bool,
}

impl Config {
    pub fn path(config_dir: &Path) -> PathBuf {
        config_dir.join("retroachievements.toml")
    }

    /// Read the account, if one is set up. Absent is not an error.
    pub fn load(config_dir: &Path) -> Option<Self> {
        let path = Self::path(config_dir);
        let text = std::fs::read_to_string(&path).ok()?;
        warn_if_readable(&path);
        let cfg: Config = toml::from_str(&text).ok()?;
        (!cfg.username.trim().is_empty()).then_some(cfg)
    }

    /// The keys for RetroArch's per launch configuration.
    pub fn retroarch_keys(&self) -> String {
        let quote = |v: &str| v.replace(['"', '\n', '\r'], "");
        let mut out = String::new();
        out.push_str("cheevos_enable = \"true\"\n");
        out.push_str(&format!(
            "cheevos_username = \"{}\"\n",
            quote(&self.username)
        ));
        if !self.password.is_empty() {
            out.push_str(&format!(
                "cheevos_password = \"{}\"\n",
                quote(&self.password)
            ));
        }
        out.push_str(&format!(
            "cheevos_hardcore_mode_enable = \"{}\"\n",
            self.hardcore
        ));
        // RetroArch writes everything it holds into its main configuration
        // when it exits, the appended keys too: the password would end up in
        // a file any user can read.
        out.push_str("config_save_on_exit = \"false\"\n");
        out
    }
}

/// Say so when the file holding the account can be read by somebody else.
fn warn_if_readable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path)
        && meta.permissions().mode() & 0o077 != 0
    {
        eprintln!(
            "{}: mode {:o} lets other users read your RetroAchievements password; chmod 600 it",
            path.display(),
            meta.permissions().mode() & 0o777
        );
    }
}

// ------------------------------------------------------------- progress

/// How far one game has got, as the site counts it.
#[derive(Clone, Debug, Deserialize, serde::Serialize, PartialEq)]
pub struct Progress {
    #[serde(rename = "Title")]
    pub title: String,
    #[serde(rename = "ConsoleID")]
    pub console: u32,
    #[serde(rename = "NumAwarded", default)]
    pub awarded: u32,
    #[serde(rename = "MaxPossible", default)]
    pub possible: u32,
    /// "beaten-softcore", "beaten-hardcore", "completed", "mastered", or
    /// nothing.
    #[serde(rename = "HighestAwardKind", default)]
    pub award: Option<String>,
}

impl Progress {
    /// What the game list shows: the count, and a letter for an award.
    pub fn label(&self) -> String {
        let mark = match self.award.as_deref() {
            Some(a) if a.starts_with("mastered") || a.starts_with("completed") => " M",
            Some(a) if a.starts_with("beaten") => " B",
            _ => "",
        };
        format!("RA {}/{}{mark}", self.awarded, self.possible)
    }
}

#[derive(Deserialize)]
struct Page {
    #[serde(rename = "Total", default)]
    total: u32,
    #[serde(rename = "Results", default)]
    results: Vec<Progress>,
}

const API: &str = "https://retroachievements.org/API/API_GetUserCompletionProgress.php";
const PAGE: u32 = 500;
const FRESH: Duration = Duration::from_secs(3600);

fn cache_path() -> PathBuf {
    crate::crt::home().join(".cache/omacrt/cheevos.json")
}

/// One page of the account's progress, with the key on curl's standard
/// input: the URL it is part of is read there too, so neither shows in the
/// process list.
fn page(cfg: &Config, offset: u32) -> Option<Page> {
    use std::io::Write;
    let mut cmd = crate::net::curl_no_redirect(30, 16_777_216);
    cmd.args(["-K", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut child = cmd.spawn().ok()?;
    {
        let stdin = child.stdin.as_mut()?;
        let enc = |s: &str| -> String {
            s.bytes()
                .map(|b| match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        (b as char).to_string()
                    }
                    _ => format!("%{b:02X}"),
                })
                .collect()
        };
        writeln!(
            stdin,
            "url = \"{API}?y={}&u={}&c={PAGE}&o={offset}\"",
            enc(cfg.api_key.trim()),
            enc(cfg.username.trim())
        )
        .ok()?;
    }
    let out = child.wait_with_output().ok()?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

/// Every game the account has played, fetched afresh.
pub fn fetch(cfg: &Config) -> Option<Vec<Progress>> {
    if cfg.api_key.trim().is_empty() {
        return None;
    }
    let mut all = Vec::new();
    let mut offset = 0;
    loop {
        let p = page(cfg, offset)?;
        let got = p.results.len() as u32;
        all.extend(p.results);
        offset += got;
        // Ten pages is five thousand games; past that is a loop, not a list.
        if got < PAGE || offset >= p.total || offset >= 10 * PAGE {
            break;
        }
    }
    let _ = std::fs::create_dir_all(cache_path().parent()?);
    let _ = crate::store::save(&cache_path(), serde_json::to_string(&all).ok()?);
    Some(all)
}

/// The account's progress from the cache, when it is younger than an hour.
pub fn cached() -> Option<Vec<Progress>> {
    let path = cache_path();
    let age = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())?;
    if age > FRESH {
        return None;
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

// ------------------------------------------------------------- matching

/// The launcher's systems a RetroAchievements console covers.
pub fn systems_for(console: u32) -> &'static [&'static str] {
    match console {
        1 => &["megadrive"],
        2 => &["n64"],
        3 => &["snes"],
        4 => &["gb"],
        5 => &["gba"],
        6 => &["gbc"],
        7 => &["nes"],
        8 => &["pcengine"],
        9 => &["segacd", "megacd"],
        10 => &["32x"],
        11 => &["mastersystem"],
        12 => &["psx"],
        13 => &["lynx"],
        14 => &["ngp"],
        15 => &["gamegear"],
        16 => &["gamecube"],
        17 => &["jaguar"],
        18 => &["nds"],
        21 => &["ps2"],
        25 => &["atari2600"],
        26 => &["dos"],
        27 => &["arcade", "mame", "fbneo", "neogeo", "mame2003"],
        29 => &["msx"],
        30 => &["c64"],
        33 => &["sg1000"],
        35 => &["amiga"],
        37 => &["amstradcpc"],
        39 => &["saturn"],
        40 => &["dreamcast"],
        41 => &["psp"],
        42 => &["cdi"],
        43 => &["3do"],
        50 => &["atari5200"],
        51 => &["atari7800"],
        52 => &["x68000"],
        56 => &["neogeocd"],
        59 => &["zxspectrum"],
        76 => &["pcenginecd"],
        _ => &[],
    }
}

/// A title reduced to what two spellings of it share: lower case words,
/// no bracketed tags, no punctuation, no articles, and none of the site's
/// `~Hack~` style prefixes.
pub fn normalise(title: &str) -> String {
    let mut s = String::new();
    let mut depth = 0i32;
    let mut tilde = false;
    for c in title.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            '~' => tilde = !tilde,
            _ if depth > 0 || tilde => {}
            c if c.is_alphanumeric() => s.extend(c.to_lowercase()),
            _ => s.push(' '),
        }
    }
    s.split_whitespace()
        .filter(|w| !matches!(*w, "the" | "a" | "an"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Progress by system and normalised title.
#[derive(Default)]
pub struct Book {
    by: HashMap<(String, String), Progress>,
}

impl Book {
    pub fn new(list: Vec<Progress>) -> Self {
        let mut by = HashMap::new();
        for p in list {
            // A game the site has no achievements for yet says nothing.
            if p.possible == 0 {
                continue;
            }
            let key = normalise(&p.title);
            for sys in systems_for(p.console) {
                by.insert((sys.to_string(), key.clone()), p.clone());
            }
        }
        Self { by }
    }

    pub fn get(&self, system: &str, title: &str) -> Option<&Progress> {
        self.by.get(&(system.to_string(), normalise(title)))
    }

    pub fn len(&self) -> usize {
        self.by.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of the site's answer, as its documentation gives it.
    const ANSWER: &str = r#"{"Count":3,"Total":3,"Results":[
        {"GameID":228,"Title":"Super Mario World","ImageIcon":"/Images/066593.png",
         "ConsoleID":3,"ConsoleName":"SNES/Super Famicom","MaxPossible":96,
         "NumAwarded":12,"NumAwardedHardcore":0,
         "MostRecentAwardedDate":"2026-09-30T20:11:02+00:00",
         "HighestAwardKind":null,"HighestAwardDate":null},
        {"GameID":1,"Title":"Legend of Zelda, The: A Link to the Past","ImageIcon":"/x.png",
         "ConsoleID":3,"ConsoleName":"SNES/Super Famicom","MaxPossible":40,
         "NumAwarded":40,"NumAwardedHardcore":0,"MostRecentAwardedDate":"2026-09-01T00:00:00+00:00",
         "HighestAwardKind":"mastered","HighestAwardDate":"2026-09-01T00:00:00+00:00"},
        {"GameID":20246,"Title":"~Hack~ Knuckles the Echidna in Sonic the Hedgehog",
         "ImageIcon":"/x.png","ConsoleID":1,"ConsoleName":"Mega Drive / Genesis",
         "MaxPossible":0,"NumAwarded":0,"NumAwardedHardcore":0,
         "MostRecentAwardedDate":"2023-10-27T02:52:34+00:00",
         "HighestAwardKind":"beaten-hardcore","HighestAwardDate":"2023-10-27T02:52:34+00:00"}]}"#;

    #[test]
    fn the_answer_is_read_and_matched_to_library_names() {
        let page: Page = serde_json::from_str(ANSWER).unwrap();
        assert_eq!(page.total, 3);
        let book = Book::new(page.results);
        assert_eq!(book.len(), 2, "a game with no achievements yet is left out");
        let smw = book.get("snes", "Super Mario World (USA)").unwrap();
        assert_eq!(smw.label(), "RA 12/96");
        let zelda = book
            .get("snes", "Legend of Zelda, The - A Link to the Past (USA)")
            .unwrap();
        assert_eq!(zelda.label(), "RA 40/40 M");
        assert!(book.get("megadrive", "Super Mario World").is_none());
    }

    #[test]
    fn titles_are_reduced_to_what_two_spellings_share() {
        assert_eq!(
            normalise("Street Fighter II' Turbo (USA) [!]"),
            "street fighter ii turbo"
        );
        assert_eq!(normalise("~Hack~ Sonic 1 SMS"), "sonic 1 sms");
        assert_eq!(normalise("The Lost Vikings"), "lost vikings");
    }

    #[test]
    fn the_retroarch_keys_keep_the_password_out_of_its_saved_config() {
        let cfg = Config {
            username: "me".into(),
            password: "pa\"ss".into(),
            api_key: String::new(),
            hardcore: false,
        };
        let keys = cfg.retroarch_keys();
        assert!(keys.contains("cheevos_enable = \"true\""));
        assert!(
            keys.contains("cheevos_password = \"pass\""),
            "a quote cannot end the value"
        );
        assert!(keys.contains("cheevos_hardcore_mode_enable = \"false\""));
        assert!(keys.contains("config_save_on_exit = \"false\""));
    }
}
