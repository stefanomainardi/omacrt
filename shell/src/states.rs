//! Save states RetroArch wrote for a game. RetroArch keeps them under its
//! states directory in one folder per core (`Snes9x/Game.state`), and with
//! `savestate_auto_save` an `.state.auto` next to it that the next launch
//! loads on its own: that is "resume where you left". The launcher only reads
//! them to show what is there.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct StateInfo {
    pub path: PathBuf,
    pub when: SystemTime,
    /// The automatic one written on exit, as opposed to a manual slot.
    pub auto: bool,
    /// RetroArch's screenshot of the moment, when it saved one.
    pub thumb: Option<PathBuf>,
}

impl StateInfo {
    /// "12:03" today, "Sat 12:03" this week, "6 Sep" older.
    pub fn when_label(&self) -> String {
        let t: chrono::DateTime<chrono::Local> = self.when.into();
        let now = chrono::Local::now();
        let age = now.signed_duration_since(t);
        if age.num_hours() < 20 && t.date_naive() == now.date_naive() {
            t.format("%H:%M").to_string()
        } else if age.num_days() < 6 {
            t.format("%a %H:%M").to_string()
        } else {
            t.format("%-d %b").to_string()
        }
    }

    pub fn label(&self) -> String {
        format!(
            "{} {}",
            if self.auto { "left" } else { "saved" },
            self.when_label()
        )
    }
}

/// RetroArch's states directory as the launcher configures it.
pub fn dir() -> PathBuf {
    crate::library::expand("~/.config/retroarch/states")
}

/// Every state of a game, newest first: the manual slot and the automatic
/// one, in whichever core folder they sit.
pub fn find(rom: &Path) -> Vec<StateInfo> {
    let Some(stem) = rom.file_stem().and_then(|s| s.to_str()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let Ok(cores) = std::fs::read_dir(dir()) else {
        return out;
    };
    for core in cores.flatten() {
        let d = core.path();
        if !d.is_dir() {
            continue;
        }
        for (name, auto) in [(format!("{stem}.state"), false), (format!("{stem}.state.auto"), true)] {
            let p = d.join(&name);
            let Ok(meta) = std::fs::metadata(&p) else {
                continue;
            };
            let thumb = {
                let t = d.join(format!("{name}.png"));
                t.is_file().then_some(t)
            };
            out.push(StateInfo {
                path: p,
                when: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                auto,
                thumb,
            });
        }
    }
    out.sort_by(|a, b| b.when.cmp(&a.when));
    out
}

/// Per game lookups remembered for the session; a launch forgets the game's
/// entry so the next look sees the new files.
#[derive(Default)]
pub struct Cache {
    map: HashMap<PathBuf, Vec<StateInfo>>,
}

impl Cache {
    pub fn get(&mut self, rom: &Path) -> &[StateInfo] {
        if !self.map.contains_key(rom) {
            let found = find(rom);
            self.map.insert(rom.to_path_buf(), found);
        }
        self.map.get(rom).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn forget(&mut self, rom: &Path) {
        self.map.remove(rom);
    }

    /// The newest state of a game, if any.
    pub fn latest(&mut self, rom: &Path) -> Option<StateInfo> {
        self.get(rom).first().cloned()
    }
}
