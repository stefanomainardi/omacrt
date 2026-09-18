//! Which pad is which, and which port it plays in.
//!
//! Two questions the launcher could not answer before. "The pad" was
//! `controllers.last()`, the last one plugged in, and the ports were whatever
//! order RetroArch's own enumeration happened to produce, so P1 was luck.
//!
//! A pad's identity is its SDL GUID, which names the model, plus a unit id
//! read from sysfs, which names the individual: the Bluetooth address for a
//! pad that came over the air, the USB serial for one on a cable. Two of the
//! same model are the same GUID and different unit ids, which is the only way
//! to tell them apart. A model that reports neither has an empty unit id and
//! cannot be told from its twin; the screen says so rather than guessing.
//!
//! The order is a list the player arranges, saved in `pads.toml`, and it is
//! the only thing that decides who is P1. Ports are handed out at the moment
//! a game starts, to the pads on that list that are connected then, skipping
//! the ones that are not. Nothing moves while a game is running.

use std::path::{Path, PathBuf};

/// One pad the launcher has met, as it is remembered between sessions.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Pad {
    /// SDL's GUID: the model, shared by every unit of it.
    pub guid: String,
    /// What to call it on screen.
    pub name: String,
    /// The individual: a Bluetooth address, a USB serial, or empty when the
    /// device reports neither.
    #[serde(default)]
    pub unit: String,
}

impl Pad {
    pub fn new(guid: &str, name: &str, unit: &str) -> Self {
        Self {
            guid: guid.trim().to_string(),
            name: name.trim().to_string(),
            unit: unit.trim().to_string(),
        }
    }

    /// Whether these two are the same physical pad.
    ///
    /// Same model and same unit is certain. Same model and no unit on either
    /// side is a guess, and it is the right guess for the common case of one
    /// pad of that model; [`Pads::ambiguous`] is what warns about the other.
    pub fn is(&self, other: &Pad) -> bool {
        self.guid == other.guid && self.unit == other.unit
    }

    /// A line for the screen when two of the same model are about.
    pub fn distinct(&self) -> bool {
        !self.unit.is_empty()
    }
}

/// A pad's name cut down to something that fits a socket.
///
/// SDL reports what the device calls itself, and a device calls itself
/// "8BitDo Ultimate 2C Wireless Controller". Thirty eight characters into
/// twelve is "8BitDo Ultim", which names nothing. The words that say only
/// that it is a pad go first, then the maker, and the model is what is left.
pub fn short_name(name: &str, cols: usize) -> String {
    const NOISE: [&str; 7] = [
        "controller",
        "gamepad",
        "joystick",
        "wireless",
        "bluetooth",
        "usb",
        "game pad",
    ];
    let mut words: Vec<String> = name.split_whitespace().map(str::to_string).collect();
    // "8BitDo 8BitDo Ultimate" is what the kernel calls one of these.
    words.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    words.retain(|w| !NOISE.iter().any(|n| w.eq_ignore_ascii_case(n)));
    // Still too long: the maker matters less than the model.
    while words.len() > 1 && words.join(" ").chars().count() > cols {
        words.remove(0);
    }
    let out = words.join(" ");
    if out.chars().count() > cols {
        out.chars().take(cols).collect()
    } else {
        out
    }
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct File {
    #[serde(default)]
    pad: Vec<Pad>,
}

/// The ordered list, P1 first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pads {
    pub order: Vec<Pad>,
}

pub fn path() -> PathBuf {
    crate::crt::config_dir().join("pads.toml")
}

impl Pads {
    pub fn load() -> Self {
        Self::load_from(&path())
    }

    /// Read the list, falling back to the backup and setting a broken file
    /// aside rather than starting empty: an order thrown away is an evening
    /// of rearranging pads.
    pub fn load_from(p: &Path) -> Self {
        let order =
            crate::store::load_parsed(p, |text| toml::from_str::<File>(text).ok().map(|f| f.pad))
                .unwrap_or_default();
        Self { order }
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&path())
    }

    pub fn save_to(&self, p: &Path) -> std::io::Result<()> {
        let text = toml::to_string_pretty(&File {
            pad: self.order.clone(),
        })
        .map_err(std::io::Error::other)?;
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        crate::store::save(p, text)
    }

    pub fn position(&self, pad: &Pad) -> Option<usize> {
        self.order.iter().position(|p| p.is(pad))
    }

    /// A pad has turned up. One already on the list keeps its place and its
    /// name is refreshed; a new one goes on the end, so plugging a pad in
    /// never rearranges the ports of the pads already there.
    ///
    /// Returns true when the list changed and is worth saving.
    pub fn seen(&mut self, pad: &Pad) -> bool {
        match self.position(pad) {
            Some(i) => {
                if self.order[i].name != pad.name && !pad.name.is_empty() {
                    self.order[i].name = pad.name.clone();
                    return true;
                }
                false
            }
            None => {
                self.order.push(pad.clone());
                true
            }
        }
    }

    pub fn forget(&mut self, i: usize) -> bool {
        if i < self.order.len() {
            self.order.remove(i);
            return true;
        }
        false
    }

    /// Move the pad at `i` one place towards P1 (`dir` negative) or away from
    /// it. Returns the place it ended up in.
    pub fn shift(&mut self, i: usize, dir: i32) -> usize {
        if self.order.is_empty() {
            return 0;
        }
        let last = self.order.len() - 1;
        let to = (i as i32 + dir.signum()).clamp(0, last as i32) as usize;
        if to != i {
            self.order.swap(i, to);
        }
        to
    }

    /// Two pads on the list that nothing can tell apart: same model, and
    /// neither reports a unit id. The screen warns instead of pretending the
    /// order means something.
    pub fn ambiguous(&self) -> bool {
        self.order.iter().enumerate().any(|(i, a)| {
            !a.distinct()
                && self.order[..i]
                    .iter()
                    .any(|b| b.guid == a.guid && !b.distinct())
        })
    }

    /// Ports 1..N for a game starting now: the pads on the list that are
    /// connected, in list order, skipping the ones that are not.
    ///
    /// A connected pad that is not on the list yet goes after them, so
    /// somebody who has never opened the screen still gets every pad.
    pub fn ports<'a>(&self, connected: &'a [Pad]) -> Vec<&'a Pad> {
        let mut out: Vec<&Pad> = Vec::new();
        for want in &self.order {
            if let Some(p) = connected.iter().find(|c| c.is(want)) {
                out.push(p);
            }
        }
        for c in connected {
            if !out.iter().any(|p| p.is(c)) {
                out.push(c);
            }
        }
        out
    }
}

// ------------------------------------------------------------- the devices

/// One pad as the kernel has it: the event node RetroArch will see, the name
/// it reports, and the unit id read from sysfs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub event: PathBuf,
    pub name: String,
    pub unit: String,
    /// USB or Bluetooth ids, as sysfs prints them. SDL reports the same two
    /// for a controller, and they are what ties one to the other.
    pub vendor: u16,
    pub product: u16,
}

/// Where the input devices live. An argument so the parsing can be tested
/// against a tree written for the purpose.
fn input_class(root: &Path) -> PathBuf {
    root.join("sys/class/input")
}

/// The individual behind an input device.
///
/// A Bluetooth pad carries its address in the input device's own `uniq`. A
/// USB one carries a serial on the USB device a few levels up, so the walk
/// goes up until it finds one. The USB controller at the top of that walk has
/// a `serial` too, and it is the PCI address of the controller rather than
/// anything about the pad, so anything shaped like one is refused.
pub fn unit_of_device(dev: &Path) -> String {
    if let Ok(u) = std::fs::read_to_string(dev.join("uniq")) {
        let u = u.trim();
        if !u.is_empty() {
            return u.to_string();
        }
    }
    let mut at = std::fs::canonicalize(dev).unwrap_or_else(|_| dev.to_path_buf());
    for _ in 0..8 {
        let Some(parent) = at.parent() else { break };
        at = parent.to_path_buf();
        if let Ok(s) = std::fs::read_to_string(at.join("serial")) {
            let s = s.trim();
            if is_unit_id(s) {
                return s.to_string();
            }
        }
    }
    String::new()
}

/// Whether a serial names a device rather than the bus it hangs off.
///
/// A PCI address (`0000:0b:00.0`) is what the root hub reports, and reading it
/// as a pad's identity makes every pad on that controller the same pad.
fn is_unit_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars().all(|c| c.is_ascii_graphic())
        && !(s.contains(':') && s.contains('.'))
}

/// Every event device that is a pad, in the order the numbers run, which is
/// the order RetroArch's udev driver builds its own list in.
///
/// The order is a guess until a game has run: `ports_in_log` reads back what
/// RetroArch actually did, and the launcher says so when the two disagree
/// rather than reporting the request as the result.
pub fn devices() -> Vec<Device> {
    devices_under(Path::new("/"))
}

pub fn devices_under(root: &Path) -> Vec<Device> {
    let Ok(entries) = std::fs::read_dir(input_class(root)) else {
        return Vec::new();
    };
    let mut found: Vec<(u32, Device)> = Vec::new();
    for e in entries.flatten() {
        let file = e.file_name();
        let Some(name) = file.to_str() else { continue };
        let Some(n) = name
            .strip_prefix("event")
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        let dev = e.path().join("device");
        if !is_pad(&dev) {
            continue;
        }
        let label = std::fs::read_to_string(dev.join("name"))
            .unwrap_or_default()
            .trim()
            .to_string();
        let id = |what: &str| {
            std::fs::read_to_string(dev.join("id").join(what))
                .ok()
                .and_then(|v| u16::from_str_radix(v.trim(), 16).ok())
                .unwrap_or(0)
        };
        found.push((
            n,
            Device {
                event: root.join("dev/input").join(name),
                name: label,
                unit: unit_of_device(&dev),
                vendor: id("vendor"),
                product: id("product"),
            },
        ));
    }
    found.sort_by_key(|(n, _)| *n);
    found.into_iter().map(|(_, d)| d).collect()
}

/// Whether an input device is a pad rather than a keyboard or a mouse.
///
/// The kernel publishes the buttons a device carries as a hex bitmap in
/// `capabilities/key`. A pad has one of the joystick or gamepad buttons,
/// 0x120..0x13f, and nothing else this project cares about does.
fn is_pad(dev: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(dev.join("capabilities/key")) else {
        return false;
    };
    has_gamepad_button(&text)
}

fn has_gamepad_button(bitmap: &str) -> bool {
    // The bitmap is printed as groups of 64 bit words, highest first.
    let words: Vec<u64> = bitmap
        .split_whitespace()
        .rev()
        .filter_map(|w| u64::from_str_radix(w, 16).ok())
        .collect();
    (0x120..=0x13f).any(|bit: usize| {
        words
            .get(bit / 64)
            .is_some_and(|w| w >> (bit % 64) & 1 == 1)
    })
}

/// The unit id of the `nth` connected pad of this model.
///
/// SDL reports a controller's vendor and product but not which event node it
/// came from, and the Rust binding has no serial. Two pads of one model are
/// matched in the order the kernel numbers them, which is the order SDL opens
/// them in, so the first of a pair is the first of the pair on both sides.
/// One pad of a model needs no tie breaking at all, which is every case but
/// the twins.
pub fn unit_for(devices: &[Device], vendor: u16, product: u16, nth: usize) -> String {
    devices
        .iter()
        .filter(|d| d.vendor == vendor && d.product == product)
        .nth(nth)
        .map(|d| d.unit.clone())
        .unwrap_or_default()
}

/// How much charge a pad has left, in quarters, when the kernel says.
///
/// bluez publishes a pad's battery as `hid-<address>-battery` under
/// `/sys/class/power_supply`, so the unit id is what finds it. Most pads
/// report nothing at all, and nothing is what the screen then draws: a
/// battery gauge invented out of no reading is worse than no gauge.
pub fn battery_of(unit: &str) -> Option<u8> {
    battery_under(Path::new("/"), unit)
}

fn battery_under(root: &Path, unit: &str) -> Option<u8> {
    if unit.is_empty() {
        return None;
    }
    let want = unit.to_ascii_lowercase();
    let dir = root.join("sys/class/power_supply");
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let name = e.file_name().to_string_lossy().to_ascii_lowercase();
        if !name.contains(&want) {
            continue;
        }
        let percent = std::fs::read_to_string(e.path().join("capacity"))
            .ok()?
            .trim()
            .parse::<u32>()
            .ok()?;
        return Some(quarters(percent));
    }
    None
}

/// A percentage as bars on a four bar gauge. Anything above nothing lights one
/// bar, so a pad about to die is not drawn as a pad with no battery fitted.
fn quarters(percent: u32) -> u8 {
    match percent.min(100) {
        0 => 0,
        1..=25 => 1,
        26..=50 => 2,
        51..=75 => 3,
        _ => 4,
    }
}

/// What RetroArch says it did, read back from its log.
///
/// `[INFO] [udev] Pad #0 (/dev/input/event27) supports force feedback.` and
/// the other udev lines name the pad index and the node it opened, which is
/// the only honest answer to which pad ended up in which port.
pub fn ports_in_log(log: &str) -> Vec<(usize, PathBuf)> {
    let mut out: Vec<(usize, PathBuf)> = Vec::new();
    for line in log.lines() {
        let Some(rest) = line.split("[udev] Pad #").nth(1) else {
            continue;
        };
        let Some((num, rest)) = rest.split_once(' ') else {
            continue;
        };
        let Ok(idx) = num.trim().parse::<usize>() else {
            continue;
        };
        let Some(path) = rest
            .trim()
            .strip_prefix('(')
            .and_then(|r| r.split(')').next())
        else {
            continue;
        };
        if !path.starts_with("/dev/input/") {
            continue;
        }
        let path = PathBuf::from(path);
        if !out.iter().any(|(i, p)| *i == idx && *p == path) {
            out.push((idx, path));
        }
    }
    out.sort_by_key(|(i, _)| *i);
    out
}

/// The pads that will hold ports 1..N when a game starts now, in port order,
/// as the kernel devices RetroArch will see them on.
///
/// A pad the list does not know still plays: it goes after the listed ones,
/// named from the kernel, so somebody who has never opened the pads screen
/// gets every pad they plugged in.
pub fn plan() -> Vec<Device> {
    plan_from(&devices(), &Pads::load())
}

pub fn plan_from(devices: &[Device], list: &Pads) -> Vec<Device> {
    let connected: Vec<Pad> = devices
        .iter()
        .filter(|d| !d.unit.is_empty())
        .map(|d| {
            list.order
                .iter()
                .find(|p| p.unit == d.unit)
                .cloned()
                .unwrap_or_else(|| Pad::new("", &d.name, &d.unit))
        })
        .collect();
    list.ports(&connected)
        .into_iter()
        .filter_map(|p| devices.iter().find(|d| d.unit == p.unit).cloned())
        .take(4)
        .collect()
}

/// The RetroArch keys that put that plan into effect.
///
/// `input_playerN_joypad_index` takes RetroArch's own index, which is the
/// device's place in the enumeration above. That is an assertion about what
/// RetroArch will do, not a reading of what it did: [`disagreement`] reads the
/// log afterwards and says so when the two differ.
pub fn launch_keys(plan: &[Device], devices: &[Device]) -> String {
    let mut out = String::new();
    for (port, want) in plan.iter().enumerate().take(4) {
        let Some(i) = devices.iter().position(|d| d.event == want.event) else {
            continue;
        };
        out.push_str(&format!(
            "input_player{}_joypad_index = \"{i}\"\n",
            port + 1
        ));
    }
    out
}

/// The keys for a game starting now, for the launch config.
pub fn port_keys() -> String {
    let devices = devices();
    launch_keys(&plan_from(&devices, &Pads::load()), &devices)
}

/// What RetroArch actually did against what was asked for, in one line, or
/// nothing when they agree.
///
/// The ports are asserted before the emulator opens and read back from its
/// log afterwards. A launcher that reports the request as the result is how
/// a pad ends up in the wrong port and nobody is told.
pub fn disagreement(plan: &[Device], log: &str) -> Option<String> {
    let got = ports_in_log(log);
    if got.is_empty() {
        return None;
    }
    for (port, want) in plan.iter().enumerate() {
        match got.iter().find(|(i, _)| *i == port) {
            None => {
                return Some(format!(
                    "port {} was meant for {} and RetroArch opened no pad there",
                    port + 1,
                    want.name
                ));
            }
            Some((_, path)) if *path != want.event => {
                return Some(format!(
                    "port {} was meant for {} on {} and RetroArch put {} there",
                    port + 1,
                    want.name,
                    want.event.display(),
                    path.display()
                ));
            }
            Some(_) => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(guid: &str, name: &str, unit: &str) -> Pad {
        Pad::new(guid, name, unit)
    }

    #[test]
    fn the_order_decides_who_is_p1_and_absent_pads_are_skipped() {
        let ultimate = pad("03000000", "Ultimate 2C", "E6AAE5604B");
        let m30 = pad("05000000", "M30", "E4:17:D8:02:9A:71");
        let pads = Pads {
            order: vec![ultimate.clone(), m30.clone()],
        };
        // Both on: the list decides.
        let both = [m30.clone(), ultimate.clone()];
        assert_eq!(pads.ports(&both)[0].name, "Ultimate 2C");
        // The first one off: the second moves up, and nothing was touched.
        let one = [m30.clone()];
        assert_eq!(pads.ports(&one).len(), 1);
        assert_eq!(pads.ports(&one)[0].name, "M30");
    }

    #[test]
    fn a_pad_nobody_has_listed_still_plays() {
        let known = pad("03000000", "Ultimate 2C", "AAA");
        let stranger = pad("09000000", "Some Pad", "BBB");
        let pads = Pads {
            order: vec![known.clone()],
        };
        let on = [stranger.clone(), known.clone()];
        let ports = pads.ports(&on);
        assert_eq!(ports[0].name, "Ultimate 2C", "the listed one is still P1");
        assert_eq!(ports[1].name, "Some Pad", "and the stranger is P2");
    }

    #[test]
    fn a_pad_that_turns_up_again_keeps_its_place() {
        let a = pad("03000000", "A", "1");
        let b = pad("03000000", "B", "2");
        let mut pads = Pads::default();
        assert!(pads.seen(&a));
        assert!(pads.seen(&b));
        assert!(!pads.seen(&a), "already there, nothing to save");
        assert_eq!(pads.position(&a), Some(0));
        // Two units of one model are two pads, not one.
        assert_eq!(pads.order.len(), 2);
    }

    #[test]
    fn shifting_moves_one_place_and_stops_at_the_ends() {
        let mut pads = Pads {
            order: vec![pad("g", "A", "1"), pad("g", "B", "2"), pad("g", "C", "3")],
        };
        assert_eq!(pads.shift(2, -1), 1);
        assert_eq!(pads.order[1].name, "C");
        assert_eq!(pads.shift(0, -1), 0, "P1 has nowhere further to go");
        assert_eq!(pads.shift(2, 1), 2, "and neither has the last");
    }

    #[test]
    fn two_of_one_model_with_no_serial_are_owned_up_to() {
        let mut pads = Pads {
            order: vec![pad("g", "M30", ""), pad("h", "Other", "")],
        };
        assert!(!pads.ambiguous(), "different models, nothing to confuse");
        pads.order.push(pad("g", "M30", ""));
        assert!(pads.ambiguous());
        pads.order[2].unit = "E4:17:D8:02:9A:71".into();
        assert!(!pads.ambiguous(), "a serial tells them apart");
    }

    #[test]
    fn the_log_says_which_pad_retroarch_put_where() {
        let log = "\
[INFO] [udev] Pad #0 (/dev/input/event27) supports force feedback.
[INFO] [udev] Pad #0 (/dev/input/event27) supports 16 force feedback effects.
[INFO] [udev] Pad #1 (/dev/input/event12) connected.
[INFO] [Input] Found joypad driver: \"udev\".
";
        assert_eq!(
            ports_in_log(log),
            vec![
                (0, PathBuf::from("/dev/input/event27")),
                (1, PathBuf::from("/dev/input/event12")),
            ]
        );
        assert!(ports_in_log("nothing here").is_empty());
    }

    #[test]
    fn the_launch_keys_name_retroarchs_own_index() {
        let devices = vec![
            Device {
                event: "/dev/input/event5".into(),
                name: "M30".into(),
                unit: "MAC".into(),
                vendor: 0x2dc8,
                product: 0x5006,
            },
            Device {
                event: "/dev/input/event27".into(),
                name: "Ultimate".into(),
                unit: "SER".into(),
                vendor: 0x2dc8,
                product: 0x310a,
            },
        ];
        let list = Pads {
            order: vec![pad("g", "Ultimate", "SER"), pad("h", "M30", "MAC")],
        };
        let plan = plan_from(&devices, &list);
        assert_eq!(plan[0].name, "Ultimate", "the list decides, not the kernel");
        // Ultimate is P1 although it is second in the enumeration.
        assert_eq!(
            launch_keys(&plan, &devices),
            "input_player1_joypad_index = \"1\"\ninput_player2_joypad_index = \"0\"\n"
        );
    }

    #[test]
    fn the_first_pad_off_hands_its_port_to_the_next() {
        let only = vec![Device {
            event: "/dev/input/event5".into(),
            name: "M30".into(),
            unit: "MAC".into(),
            vendor: 0x2dc8,
            product: 0x5006,
        }];
        let list = Pads {
            order: vec![pad("g", "Ultimate", "SER"), pad("h", "M30", "MAC")],
        };
        let plan = plan_from(&only, &list);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].name, "M30", "the second moves up on its own");
        assert_eq!(
            launch_keys(&plan, &only),
            "input_player1_joypad_index = \"0\"\n"
        );
    }

    #[test]
    fn the_log_is_read_back_rather_than_the_request_reported() {
        let plan = vec![
            Device {
                event: "/dev/input/event27".into(),
                name: "Ultimate".into(),
                unit: "SER".into(),
                vendor: 0x2dc8,
                product: 0x310a,
            },
            Device {
                event: "/dev/input/event5".into(),
                name: "M30".into(),
                unit: "MAC".into(),
                vendor: 0x2dc8,
                product: 0x5006,
            },
        ];
        let agreed = "[udev] Pad #0 (/dev/input/event27) connected.\n\
                      [udev] Pad #1 (/dev/input/event5) connected.\n";
        assert_eq!(disagreement(&plan, agreed), None);
        let swapped = "[udev] Pad #0 (/dev/input/event5) connected.\n\
                       [udev] Pad #1 (/dev/input/event27) connected.\n";
        assert!(
            disagreement(&plan, swapped)
                .is_some_and(|m| m.contains("port 1") && m.contains("Ultimate")),
            "a swap has to be said out loud"
        );
        // A log with nothing in it yet is not a disagreement.
        assert_eq!(disagreement(&plan, ""), None);
    }

    #[test]
    fn a_pad_the_kernel_cannot_name_is_left_out_rather_than_guessed_at() {
        // No serial means nothing to match on, and a wrong guess puts
        // somebody else's pad in port one.
        let devices = vec![Device {
            event: "/dev/input/event5".into(),
            name: "M30".into(),
            unit: String::new(),
            vendor: 0x2dc8,
            product: 0x5006,
        }];
        let plan = plan_from(&devices, &Pads::default());
        assert!(plan.is_empty());
        assert_eq!(launch_keys(&plan, &devices), "");
    }

    #[test]
    fn two_of_one_model_are_matched_in_the_order_the_kernel_numbers_them() {
        let devices = vec![
            Device {
                event: "/dev/input/event5".into(),
                name: "M30".into(),
                unit: "FIRST".into(),
                vendor: 0x2dc8,
                product: 0x5006,
            },
            Device {
                event: "/dev/input/event9".into(),
                name: "M30".into(),
                unit: "SECOND".into(),
                vendor: 0x2dc8,
                product: 0x5006,
            },
        ];
        assert_eq!(unit_for(&devices, 0x2dc8, 0x5006, 0), "FIRST");
        assert_eq!(unit_for(&devices, 0x2dc8, 0x5006, 1), "SECOND");
        // A model that is not there answers with nothing rather than with
        // somebody else's pad.
        assert_eq!(unit_for(&devices, 0x2dc8, 0x5006, 2), "");
        assert_eq!(unit_for(&devices, 0x054c, 0x09cc, 0), "");
    }

    #[test]
    fn a_charge_becomes_bars_and_a_nearly_flat_pad_still_shows_one() {
        assert_eq!(quarters(100), 4);
        assert_eq!(quarters(76), 4);
        assert_eq!(quarters(75), 3);
        assert_eq!(quarters(26), 2);
        assert_eq!(quarters(1), 1, "nearly flat is not the same as no battery");
        assert_eq!(quarters(0), 0);
        assert_eq!(quarters(400), 4, "a reading that cannot be is clamped");
    }

    #[test]
    fn a_pad_with_no_battery_to_read_reports_none() {
        let dir = std::env::temp_dir().join(format!("omacrt-batt-{}", std::process::id()));
        let supply = dir.join("sys/class/power_supply/hid-e4:17:d8:02:9a:71-battery");
        std::fs::create_dir_all(&supply).unwrap();
        std::fs::write(supply.join("capacity"), "63\n").unwrap();
        assert_eq!(battery_under(&dir, "E4:17:D8:02:9A:71"), Some(3));
        assert_eq!(battery_under(&dir, "SOMETHINGELSE"), None);
        assert_eq!(battery_under(&dir, ""), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_controllers_serial_is_not_a_pads_serial() {
        assert!(is_unit_id("E6AAE5604B"));
        assert!(is_unit_id("E4:17:D8:02:9A:71"));
        assert!(!is_unit_id("0000:0b:00.0"), "that is a PCI address");
        assert!(!is_unit_id(""));
    }

    #[test]
    fn a_gamepad_button_is_what_makes_a_device_a_pad() {
        // Both bitmaps are what the kernel prints for a real device. BTN_SOUTH
        // is 0x130, bit 48 of the fifth word counting from the low end, and
        // the kernel prints the words highest first.
        let pad_map = "7cdb000000000000 0 0 0 0";
        assert!(has_gamepad_button(pad_map));
        // The keyboard the same dongle also presents: plenty of keys, none of
        // them in 0x120..0x13f.
        let keyboard = "3f00733fff 0 0 483ffff17aff32d bfd4444600000000 1 \
                        130ff38b17d007 ffff7bfad9415fff ffbeffdfffefffff \
                        fffffffffffffffe";
        assert!(!has_gamepad_button(keyboard));
        assert!(!has_gamepad_button(""));
    }

    #[test]
    fn a_broken_file_does_not_take_the_order_with_it() {
        let dir = std::env::temp_dir().join(format!("omacrt-pads-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("pads.toml");
        let good = Pads {
            order: vec![pad("g", "A", "1"), pad("h", "B", "2")],
        };
        good.save_to(&p).unwrap();
        // A second save promotes the first to the backup.
        good.save_to(&p).unwrap();
        std::fs::write(&p, "this is not toml [[[").unwrap();
        assert_eq!(Pads::load_from(&p), good, "the backup answered");
        assert!(p.with_extension("toml.bad").exists() || !p.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pads_name_is_cut_down_to_its_model() {
        // The words that say only that it is a pad go first.
        assert_eq!(
            short_name("8BitDo Ultimate 2C Wireless Controller", 20),
            "8BitDo Ultimate 2C"
        );
        // Then the maker, when the model still does not fit.
        assert_eq!(
            short_name("8BitDo Ultimate 2C Wireless Controller", 12),
            "Ultimate 2C"
        );
        // The kernel says some of these twice.
        assert_eq!(short_name("8BitDo 8BitDo M30 gamepad", 20), "8BitDo M30");
        // One word and no room left: cut, because something is better than
        // an empty socket that has a pad in it.
        assert_eq!(short_name("Supercalifragilistic", 6), "Superc");
        assert_eq!(short_name("", 12), "");
        // A name that already fits is left alone.
        assert_eq!(short_name("DualShock 4", 15), "DualShock 4");
    }
}
