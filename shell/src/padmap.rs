//! Mapping wizard for pads SDL does not know. SDL's game controller layer
//! only sees a pad whose GUID has a mapping; anything else is a bare
//! joystick with numbered buttons, axes and hats. The wizard asks for one
//! RetroPad control at a time, records the raw input that answers, and
//! writes the mapping line SDL wants (`SDL_GameControllerDB` format) into
//! `~/.config/omarchy-crt/gamecontrollerdb.txt`, which the launcher loads at
//! start. RetroArch keeps its own autoconfig; this is for the launcher.

use std::path::PathBuf;

/// The controls asked, in order: SDL field name and what to tell the player.
pub const STEPS: &[(&str, &str)] = &[
    ("a", "A, the bottom face button"),
    ("b", "B, the right face button"),
    ("x", "X, the left face button"),
    ("y", "Y, the top face button"),
    ("start", "Start"),
    ("back", "Select or Back"),
    ("dpup", "D-pad up"),
    ("dpdown", "D-pad down"),
    ("dpleft", "D-pad left"),
    ("dpright", "D-pad right"),
    ("leftshoulder", "left shoulder (L1, LB)"),
    ("rightshoulder", "right shoulder (R1, RB)"),
    ("lefttrigger", "left trigger (L2, LT)"),
    ("righttrigger", "right trigger (R2, RT)"),
    ("leftx", "left stick, push right"),
    ("lefty", "left stick, push down"),
    ("guide", "home or guide button"),
];

/// One raw joystick input as SDL reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Raw {
    Button(u8),
    /// Axis index and the sign it moved to.
    Axis(u8, bool),
    /// Hat index and direction mask.
    Hat(u8, u8),
}

impl Raw {
    /// The binding SDL reads for this input on the given control.
    fn bind(&self, field: &str) -> String {
        match self {
            Raw::Button(b) => format!("b{b}"),
            Raw::Hat(h, m) => format!("h{h}.{m}"),
            Raw::Axis(a, positive) => {
                // Sticks and triggers take the whole axis; a d-pad on an
                // axis takes one half of it.
                if field.starts_with("left") || field.starts_with("right") {
                    format!("a{a}")
                } else if *positive {
                    format!("+a{a}")
                } else {
                    format!("-a{a}")
                }
            }
        }
    }
}

pub struct Wizard {
    pub name: String,
    pub guid: String,
    /// SDL joystick instance id the raw events carry.
    pub which: u32,
    pub step: usize,
    pub binds: Vec<(&'static str, String)>,
    /// Raw inputs already taken, so one button cannot answer twice.
    taken: Vec<Raw>,
    /// Time of the last answer; axes are ignored for a moment after it so a
    /// stick swinging back does not answer the next question.
    last: f64,
    pub finished: bool,
}

impl Wizard {
    pub fn new(name: &str, guid: &str, which: u32) -> Self {
        Self {
            name: name.replace(',', " "),
            guid: guid.to_string(),
            which,
            step: 0,
            binds: Vec::new(),
            taken: Vec::new(),
            last: 0.0,
            finished: false,
        }
    }

    pub fn current(&self) -> Option<(&'static str, &'static str)> {
        STEPS.get(self.step).copied()
    }

    /// A raw input arrived. Returns true when it answered the question. An
    /// input already assigned skips the question instead (so the first
    /// button, A, doubles as "this pad has no such control").
    pub fn feed(&mut self, raw: Raw, now: f64) -> bool {
        if self.finished {
            return false;
        }
        if matches!(raw, Raw::Axis(..)) && now - self.last < 0.6 {
            return false;
        }
        let Some((field, _)) = self.current() else {
            return false;
        };
        if self.taken.contains(&raw) {
            self.skip();
            self.last = now;
            return true;
        }
        self.binds.push((field, raw.bind(field)));
        self.taken.push(raw);
        self.last = now;
        self.advance();
        true
    }

    pub fn skip(&mut self) {
        self.advance();
    }

    fn advance(&mut self) {
        self.step += 1;
        if self.step >= STEPS.len() {
            self.finished = true;
        }
    }

    /// The mapping line for SDL, complete with the platform tag.
    pub fn mapping(&self) -> String {
        let mut s = format!("{},{},", self.guid, self.name);
        for (field, bind) in &self.binds {
            s.push_str(field);
            s.push(':');
            s.push_str(bind);
            s.push(',');
        }
        s.push_str("platform:Linux,");
        s
    }

    /// Enough answered to be a usable pad: the face buttons and a way to move.
    pub fn usable(&self) -> bool {
        let has = |f: &str| self.binds.iter().any(|(k, _)| *k == f);
        has("a") && has("b") && (has("dpup") || has("lefty"))
    }
}

pub fn db_path() -> PathBuf {
    crate::crt::config_dir().join("gamecontrollerdb.txt")
}

/// Append the mapping to the launcher's database, replacing an older line
/// for the same GUID.
pub fn save(mapping: &str) -> std::io::Result<()> {
    let path = db_path();
    let guid = mapping.split(',').next().unwrap_or("");
    let mut lines: Vec<String> = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.starts_with(guid) || guid.is_empty())
        .map(|l| l.to_string())
        .collect();
    if lines.is_empty() {
        lines.push("# Pads mapped with the omarchy-crt launcher (SDL_GameControllerDB format).".into());
    }
    lines.push(mapping.to_string());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, lines.join("\n") + "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_line() {
        let mut w = Wizard::new("Some, Pad", "03000000aaaa", 7);
        assert!(w.feed(Raw::Button(0), 1.0)); // a
        assert!(w.feed(Raw::Button(1), 2.0)); // b
        assert!(w.feed(Raw::Button(0), 3.0)); // x: A again skips
        assert_eq!(w.step, 3);
        assert!(w.feed(Raw::Button(3), 4.0)); // y
        w.skip(); // start
        w.skip(); // back
        assert!(w.feed(Raw::Hat(0, 1), 5.0)); // dpup
        assert!(w.feed(Raw::Axis(1, true), 6.0)); // dpdown on an axis
        assert!(!w.feed(Raw::Axis(1, false), 6.2)); // too soon after an answer
        assert!(w.feed(Raw::Axis(1, false), 7.0)); // dpleft
        let m = w.mapping();
        assert!(m.starts_with("03000000aaaa,Some  Pad,a:b0,b:b1,y:b3,dpup:h0.1,dpdown:+a1,dpleft:-a1,"));
        assert!(m.ends_with("platform:Linux,"));
        assert!(w.usable());
    }
}
