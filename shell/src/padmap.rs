//! Mapping wizard for pads SDL does not know. SDL's game controller layer
//! only sees a pad whose GUID has a mapping; anything else is a bare
//! joystick with numbered buttons, axes and hats. The wizard asks for one
//! RetroPad control at a time, records the raw input that answers, and
//! writes the mapping line SDL wants (`SDL_GameControllerDB` format) into
//! `~/.config/omacrt/gamecontrollerdb.txt`, which the launcher loads at
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

fn db_path() -> PathBuf {
    crate::crt::config_dir().join("gamecontrollerdb.txt")
}

/// Append the mapping to the launcher's database, replacing an older line
/// for the same GUID.
pub fn save(mapping: &str) -> std::io::Result<()> {
    let path = db_path();
    let guid = mapping.split(',').next().unwrap_or("");
    let mut lines: Vec<String> = crate::store::load_string(&path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.starts_with(guid) || guid.is_empty())
        .map(|l| l.to_string())
        .collect();
    if lines.is_empty() {
        lines.push("# Pads mapped with the omacrt launcher (SDL_GameControllerDB format).".into());
    }
    lines.push(mapping.to_string());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::store::save(&path, lines.join("\n") + "\n")
}

// ------------------------------------------------- RetroArch autoconfig

/// Where RetroArch reads pad profiles from, ours to write into.
pub fn autoconfig_dir() -> PathBuf {
    crate::crt::home().join(".config/retroarch/autoconfig/udev")
}

/// Where the distribution keeps the profiles RetroArch ships with.
pub const SYSTEM_AUTOCONFIG: &str = "/usr/share/libretro/autoconfig/udev";

/// A RetroArch udev profile written from an SDL mapping.
///
/// SDL's game controller database is the best kept list of pad layouts there
/// is, and RetroArch keeps its own, in its own format, matched by USB vendor
/// and product. A pad in one and not the other works in the launcher and not
/// in the games, or the other way round, which is how a controller ends up
/// with no Start button on a Dreamcast title screen.
///
/// So the launcher translates. The face buttons cross over: RetroPad B is
/// the bottom button, which SDL calls A, and RetroPad Y is the left one,
/// which SDL calls X. Everything else is a name change.
fn retroarch_profile(name: &str, vendor: u16, product: u16, sdl_mapping: &str) -> String {
    // field -> value, from `a:b0,b:b1,...`
    let mut f = std::collections::BTreeMap::new();
    for part in sdl_mapping.split(',').skip(2) {
        if let Some((k, v)) = part.split_once(':') {
            f.insert(k.trim(), v.trim());
        }
    }
    let mut out = String::new();
    let mut put = |k: &str, v: &str| out.push_str(&format!("{k} = \"{v}\"\n"));
    put("input_driver", "udev");
    put("input_device", name);
    put("input_device_display_name", name);
    put("input_vendor_id", &vendor.to_string());
    put("input_product_id", &product.to_string());

    // The face buttons, crossed over as RetroPad names them.
    let buttons: &[(&str, &str)] = &[
        ("a", "input_b_btn"),
        ("b", "input_a_btn"),
        ("x", "input_y_btn"),
        ("y", "input_x_btn"),
        ("back", "input_select_btn"),
        ("start", "input_start_btn"),
        ("guide", "input_menu_toggle_btn"),
        ("leftshoulder", "input_l_btn"),
        ("rightshoulder", "input_r_btn"),
        ("leftstick", "input_l3_btn"),
        ("rightstick", "input_r3_btn"),
        ("dpup", "input_up_btn"),
        ("dpdown", "input_down_btn"),
        ("dpleft", "input_left_btn"),
        ("dpright", "input_right_btn"),
        ("lefttrigger", "input_l2_btn"),
        ("righttrigger", "input_r2_btn"),
    ];
    for (sdl, ra) in buttons {
        let Some(v) = f.get(sdl) else { continue };
        match binding(v) {
            Some(Bound::Button(n)) => put(ra, &n.to_string()),
            Some(Bound::Hat(h, dir)) => put(ra, &format!("h{h}{dir}")),
            // A trigger on an axis is an axis to RetroArch as well, and a
            // d-pad on an axis is the same story.
            Some(Bound::Axis(n, sign)) => {
                let key = ra.trim_end_matches("_btn").to_string() + "_axis";
                put(&key, &format!("{sign}{n}"));
            }
            None => {}
        }
    }

    // The sticks, both directions of both axes.
    for (sdl, ra) in [
        ("leftx", "input_l_x"),
        ("lefty", "input_l_y"),
        ("rightx", "input_r_x"),
        ("righty", "input_r_y"),
    ] {
        if let Some(Bound::Axis(n, _)) = f.get(sdl).and_then(|v| binding(v)) {
            put(&format!("{ra}_plus_axis"), &format!("+{n}"));
            put(&format!("{ra}_minus_axis"), &format!("-{n}"));
        }
    }
    out
}

enum Bound {
    Button(u32),
    Axis(u32, char),
    Hat(u32, &'static str),
}

/// One SDL binding: `b3`, `a2`, `+a2`, `-a2`, `a2~`, `h0.1`.
fn binding(v: &str) -> Option<Bound> {
    let v = v.trim().trim_end_matches('~');
    let (sign, rest) = match v.strip_prefix('+') {
        Some(r) => ('+', r),
        None => match v.strip_prefix('-') {
            Some(r) => ('-', r),
            None => ('+', v),
        },
    };
    if let Some(n) = rest.strip_prefix('b') {
        return n.parse().ok().map(Bound::Button);
    }
    if let Some(n) = rest.strip_prefix('a') {
        return n.parse().ok().map(|n| Bound::Axis(n, sign));
    }
    if let Some(rest) = rest.strip_prefix('h') {
        let (h, mask) = rest.split_once('.')?;
        let dir = match mask.parse::<u32>().ok()? {
            1 => "up",
            2 => "right",
            4 => "down",
            8 => "left",
            _ => return None,
        };
        return h.parse().ok().map(|h| Bound::Hat(h, dir));
    }
    None
}

/// True when RetroArch already ships a profile for this pad, by the ids it
/// matches on. Nothing is written over a profile somebody tuned by hand.
fn has_profile(dir: &std::path::Path, vendor: u16, product: u16) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    let want_v = format!("input_vendor_id = \"{vendor}\"");
    let want_p = format!("input_product_id = \"{product}\"");
    rd.filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "cfg"))
        .any(|e| {
            std::fs::read_to_string(e.path())
                .map(|t| t.contains(&want_v) && t.contains(&want_p))
                .unwrap_or(false)
        })
}

/// The USB vendor and product a pad's SDL GUID carries.
///
/// SDL packs them little endian at fixed places in the GUID, which is the
/// only place the Rust binding exposes them: bytes 4 to 6 are the vendor and
/// 8 to 10 the product, each written back to front.
///
/// `0300604e c82d0000 0a310000 14010000` is vendor 0x2dc8, product 0x310a,
/// which is how RetroArch knows the same pad as 11720/12554.
pub fn ids_from_guid(guid: &str) -> Option<(u16, u16)> {
    if guid.len() < 24 {
        return None;
    }
    let le = |at: usize| -> Option<u16> {
        let lo = u16::from_str_radix(guid.get(at..at + 2)?, 16).ok()?;
        let hi = u16::from_str_radix(guid.get(at + 2..at + 4)?, 16).ok()?;
        Some((hi << 8) | lo)
    };
    let (v, p) = (le(8)?, le(16)?);
    (v != 0 && p != 0).then_some((v, p))
}

/// The profile in `dir` matching these ids, if there is one.
fn profile_in(dir: &std::path::Path, vendor: u16, product: u16) -> Option<PathBuf> {
    let want_v = format!("input_vendor_id = \"{vendor}\"");
    let want_p = format!("input_product_id = \"{product}\"");
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "cfg"))
        .find(|p| {
            std::fs::read_to_string(p)
                .map(|t| t.contains(&want_v) && t.contains(&want_p))
                .unwrap_or(false)
        })
}

/// Make sure the pad profile directory RetroArch is pointed at holds one for
/// this pad, and say what happened.
///
/// The directory is ours, under the user's own configuration, because
/// RetroArch reads exactly one and a profile written into `/usr/share` would
/// not survive an update. So a profile the distribution ships is copied in,
/// and a pad it has never heard of gets one written from the SDL mapping.
/// Neither ever replaces a file already there: a profile somebody tuned by
/// hand stays as it is.
pub fn ensure_retroarch_profile(
    name: &str,
    vendor: u16,
    product: u16,
    sdl_mapping: &str,
) -> Option<String> {
    if vendor == 0 || product == 0 {
        return None;
    }
    let dir = autoconfig_dir();
    if has_profile(&dir, vendor, product) {
        return None;
    }
    std::fs::create_dir_all(&dir).ok()?;
    if let Some(shipped) = profile_in(std::path::Path::new(SYSTEM_AUTOCONFIG), vendor, product) {
        let to = dir.join(shipped.file_name()?);
        std::fs::copy(&shipped, &to).ok()?;
        return Some(format!("{name}: profile from {}", shipped.display()));
    }
    if sdl_mapping.is_empty() {
        return None;
    }
    let file = dir.join(format!(
        "{}.cfg",
        name.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>()
    ));
    let body = format!(
        "# Written by omacrt from the SDL mapping of this pad.\n{}",
        retroarch_profile(name, vendor, product, sdl_mapping)
    );
    crate::store::save(&file, body).ok()?;
    Some(format!("{name}: profile written from its SDL mapping"))
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
        assert!(
            m.starts_with("03000000aaaa,Some  Pad,a:b0,b:b1,y:b3,dpup:h0.1,dpdown:+a1,dpleft:-a1,")
        );
        assert!(m.ends_with("platform:Linux,"));
        assert!(w.usable());
    }

    #[test]
    fn an_sdl_mapping_becomes_a_retroarch_profile() {
        let sdl = "0300604ec82d00000a31000014010000,8BitDo Ultimate 2C Wireless Controller,\
a:b0,b:b1,x:b2,y:b3,back:b6,guide:b8,start:b7,leftstick:b9,rightstick:b10,\
leftshoulder:b4,rightshoulder:b5,dpup:h0.1,dpdown:h0.4,dpleft:h0.8,dpright:h0.2,\
leftx:a0,lefty:a1,rightx:a3,righty:a4,lefttrigger:a2,righttrigger:a5,platform:Linux,";
        let out = retroarch_profile("8BitDo Ultimate 2C Wireless Controller", 11720, 12554, sdl);
        // The face buttons cross over: SDL A is the bottom button, RetroPad
        // calls that B.
        assert!(out.contains("input_b_btn = \"0\""), "{out}");
        assert!(out.contains("input_a_btn = \"1\""), "{out}");
        assert!(out.contains("input_y_btn = \"2\""), "{out}");
        assert!(out.contains("input_x_btn = \"3\""), "{out}");
        // The one that was missing on the tube.
        assert!(out.contains("input_start_btn = \"7\""), "{out}");
        assert!(out.contains("input_select_btn = \"6\""), "{out}");
        assert!(out.contains("input_up_btn = \"h0up\""), "{out}");
        assert!(out.contains("input_left_btn = \"h0left\""), "{out}");
        assert!(out.contains("input_l2_axis = \"+2\""), "{out}");
        assert!(out.contains("input_l_x_plus_axis = \"+0\""), "{out}");
        assert!(out.contains("input_l_y_minus_axis = \"-1\""), "{out}");
        assert!(out.contains("input_vendor_id = \"11720\""), "{out}");
    }

    #[test]
    fn every_shape_of_sdl_binding_is_understood() {
        assert!(matches!(binding("b3"), Some(Bound::Button(3))));
        assert!(matches!(binding("a2"), Some(Bound::Axis(2, '+'))));
        assert!(matches!(binding("-a2"), Some(Bound::Axis(2, '-'))));
        assert!(matches!(binding("a2~"), Some(Bound::Axis(2, '+'))));
        assert!(matches!(binding("h0.4"), Some(Bound::Hat(0, "down"))));
        assert!(binding("nonsense").is_none());
    }

    #[test]
    fn the_usb_ids_come_out_of_the_sdl_guid() {
        assert_eq!(
            ids_from_guid("0300604ec82d00000a31000014010000"),
            Some((0x2dc8, 0x310a)),
            "8BitDo Ultimate 2C: 11720/12554 in RetroArch's decimal"
        );
        assert_eq!(ids_from_guid("03000000000000000000000000000000"), None);
        assert_eq!(ids_from_guid("short"), None);
    }
}

// ----------------------------------------------------- what is printed on it

/// Which of the launcher's own buttons sit where the pad's letters say.
///
/// SDL names a face button by where it is, not by what is printed on it: `a`
/// is the southern one whatever the letter beside it. On most pads the
/// southern button is the one marked A and nothing has to be done. On a pad
/// built like a Mega Drive or a Saturn controller it is not: the 8BitDo M30
/// has A B C along the bottom, SDL calls the middle one `a`, and the launcher
/// confirmed on the button marked B while the one marked A went back. The
/// letters on the plastic are what a person reads, and RetroArch obeys them
/// inside the game, so the launcher obeys them too.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Faces {
    /// The southern and eastern buttons carry each other's meaning.
    pub swap_ab: bool,
    /// The western and northern ones do.
    pub swap_xy: bool,
}

impl Faces {
    /// The SDL button this one means on this pad.
    pub fn of(&self, b: sdl2::controller::Button) -> sdl2::controller::Button {
        use sdl2::controller::Button::*;
        match b {
            A if self.swap_ab => B,
            B if self.swap_ab => A,
            X if self.swap_xy => Y,
            Y if self.swap_xy => X,
            other => other,
        }
    }
}

/// The raw button a mapping puts an SDL name on: `a:b1` is 1.
fn raw_of(mapping: &str, sdl: &str) -> Option<u32> {
    mapping.split(',').find_map(|part| {
        let (name, bind) = part.split_once(':')?;
        (name.trim() == sdl)
            .then(|| bind.trim().strip_prefix('b')?.parse().ok())
            .flatten()
    })
}

/// Raw button to the letter printed beside it, from a RetroArch profile.
///
/// The profile carries both: `input_b_btn = "0"` says which raw button
/// RetroPad B is, and `input_b_btn_label = "A"` says what that button is
/// called on the plastic.
fn labels_of(profile: &str) -> Vec<(u32, String)> {
    let value = |line: &str| -> Option<(String, String)> {
        let (k, v) = line.split_once('=')?;
        Some((
            k.trim().to_string(),
            v.trim().trim_matches('"').trim().to_string(),
        ))
    };
    let pairs: Vec<(String, String)> = profile.lines().filter_map(value).collect();
    let mut out = Vec::new();
    for (k, v) in &pairs {
        let Some(stem) = k.strip_suffix("_btn") else {
            continue;
        };
        let Ok(raw) = v.parse::<u32>() else { continue };
        let want = format!("{stem}_btn_label");
        if let Some((_, label)) = pairs.iter().find(|(k, _)| *k == want)
            && !label.is_empty()
        {
            out.push((raw, label.clone()));
        }
    }
    out
}

/// Whether this pad's letters disagree with where SDL puts them.
///
/// Only when it can be shown: the letter has to be somewhere, and somewhere
/// else than SDL put it. A pad whose profile carries no labels, or whose
/// labels agree, is left exactly as SDL has it.
pub fn faces_from(mapping: &str, profile: &str) -> Faces {
    let labels = labels_of(profile);
    let letter = |sdl: &str| -> Option<&str> {
        let raw = raw_of(mapping, sdl)?;
        labels
            .iter()
            .find(|(r, _)| *r == raw)
            .map(|(_, l)| l.as_str())
    };
    let crossed = |low: &str, high: &str, want: &str| {
        letter(low).is_some_and(|l| !l.eq_ignore_ascii_case(want))
            && letter(high).is_some_and(|l| l.eq_ignore_ascii_case(want))
    };
    Faces {
        swap_ab: crossed("a", "b", "A"),
        swap_xy: crossed("x", "y", "X"),
    }
}

/// The RetroArch profile for a pad, from the directory the launcher keeps
/// filled and then from the one the distribution ships.
pub fn profile_for(vendor: u16, product: u16) -> Option<String> {
    if vendor == 0 && product == 0 {
        return None;
    }
    let wanted = (
        format!("input_vendor_id = \"{}\"", vendor as u32),
        format!("input_product_id = \"{}\"", product as u32),
    );
    for dir in [autoconfig_dir(), PathBuf::from(SYSTEM_AUTOCONFIG)] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(text) = std::fs::read_to_string(e.path()) else {
                continue;
            };
            if text.contains(&wanted.0) && text.contains(&wanted.1) {
                return Some(text);
            }
        }
    }
    None
}

#[cfg(test)]
mod faces_tests {
    use super::*;

    /// The mapping and the profile of a real pad, as SDL and RetroArch have
    /// them. A B C along the bottom, X Y Z above: SDL's southern button is
    /// the one marked B.
    const M30_MAPPING: &str = "05009b42c82d00005106000000010000,8BitDo M30 Gamepad,\
        a:b1,b:b0,back:b10,guide:b2,leftshoulder:b6,lefttrigger:a5,leftx:a0,lefty:a1,\
        rightshoulder:b7,righttrigger:a4,start:b11,x:b4,y:b3,platform:Linux,";
    const M30_PROFILE: &str = r#"
input_b_btn = "0"
input_y_btn = "3"
input_a_btn = "1"
input_x_btn = "4"
input_start_btn = "11"
input_b_btn_label = "A"
input_y_btn_label = "X"
input_a_btn_label = "B"
input_x_btn_label = "Y"
input_start_btn_label = "Start"
"#;

    /// And one built the usual way, where SDL and the plastic agree.
    const PAD_MAPPING: &str = "030000005e0400008e02000010010000,Xbox 360,\
        a:b0,b:b1,x:b2,y:b3,start:b7,platform:Linux,";
    const PAD_PROFILE: &str = r#"
input_b_btn = "0"
input_a_btn = "1"
input_y_btn = "2"
input_x_btn = "3"
input_b_btn_label = "A"
input_a_btn_label = "B"
input_y_btn_label = "X"
input_x_btn_label = "Y"
"#;

    #[test]
    fn a_pad_with_its_letters_where_sdl_expects_them_is_left_alone() {
        assert_eq!(faces_from(PAD_MAPPING, PAD_PROFILE), Faces::default());
    }

    #[test]
    fn a_six_button_pad_has_its_letters_the_other_way_round() {
        let f = faces_from(M30_MAPPING, M30_PROFILE);
        assert!(f.swap_ab, "the button marked A is SDL's eastern one");
        assert!(f.swap_xy, "and the one marked X is SDL's northern one");
        use sdl2::controller::Button::*;
        assert_eq!(f.of(A), B, "pressing the letter A confirms");
        assert_eq!(f.of(B), A);
        assert_eq!(f.of(X), Y);
        assert_eq!(f.of(Y), X);
        // Everything else is where it was.
        assert_eq!(f.of(Start), Start);
        assert_eq!(f.of(DPadUp), DPadUp);
    }

    #[test]
    fn nothing_is_swapped_on_a_guess() {
        // No profile at all, a profile with no labels, and a mapping with no
        // face buttons: three ways of not knowing, and none of them moves a
        // button.
        assert_eq!(faces_from(M30_MAPPING, ""), Faces::default());
        assert_eq!(
            faces_from(M30_MAPPING, "input_b_btn = \"0\"\ninput_a_btn = \"1\"\n"),
            Faces::default()
        );
        assert_eq!(
            faces_from("guid,name,start:b11,", M30_PROFILE),
            Faces::default()
        );
    }

    #[test]
    fn a_letter_is_read_off_the_button_it_belongs_to() {
        assert_eq!(raw_of(M30_MAPPING, "a"), Some(1));
        assert_eq!(raw_of(M30_MAPPING, "b"), Some(0));
        assert_eq!(raw_of(M30_MAPPING, "lefttrigger"), None, "that is an axis");
        assert_eq!(raw_of(M30_MAPPING, "nonesuch"), None);
        let labels = labels_of(M30_PROFILE);
        assert!(labels.contains(&(0, "A".to_string())));
        assert!(labels.contains(&(1, "B".to_string())));
    }
}
