//! Bluetooth pad pairing through `bluetoothctl`.
//!
//! Every step is a child process started and then polled, never waited on:
//! this runs on the thread that draws, and a `wait()` there is a television
//! that stops moving. `pair` on a pad that is not in pairing mode takes the
//! best part of a minute to give up, and the old code waited for it.
//!
//! Every step has a deadline and every step's output is read. What the old
//! code did instead was
//!
//! ```text
//! let _ = bluetoothctl(&["pair", &mac], false).map(|mut c| c.wait());
//! let _ = bluetoothctl(&["trust", &mac], false).map(|mut c| c.wait());
//! ```
//!
//! which threw away both results and then reported the third command's, so a
//! pad that never paired was announced as connected. The screen now says
//! which step failed and quotes what bluetoothctl said about it.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// What a run is doing, which is also what the screen draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    /// Turning the adapter on.
    Power,
    /// Looking for devices.
    Scan,
    /// Collecting what the scan found.
    List,
    Pair,
    Trust,
    Connect,
}

impl Phase {
    /// How long this step is given before it is taken to have failed.
    ///
    /// A pad that is not in pairing mode makes `pair` sit there; the numbers
    /// are what a pad that is listening actually takes, with room over.
    fn deadline(self) -> Duration {
        Duration::from_secs(match self {
            Phase::Idle => 0,
            Phase::Power => 5,
            Phase::Scan => 12,
            Phase::List => 5,
            Phase::Pair => 25,
            Phase::Trust => 10,
            Phase::Connect => 20,
        })
    }

    /// The word for the screen.
    pub fn label(self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Power => "waking the adapter",
            Phase::Scan => "looking",
            Phase::List => "reading the list",
            Phase::Pair => "pairing",
            Phase::Trust => "trusting",
            Phase::Connect => "connecting",
        }
    }

    /// Which of the three pairing steps this is, for "step 2 of 3".
    pub fn step(self) -> Option<usize> {
        match self {
            Phase::Pair => Some(1),
            Phase::Trust => Some(2),
            Phase::Connect => Some(3),
            _ => None,
        }
    }
}

struct Run {
    child: Child,
    started: Instant,
    phase: Phase,
    /// The device being paired, carried through the three steps.
    target: Option<(String, String)>,
}

pub struct Bluetooth {
    run: Option<Run>,
    /// (address, name) of every device bluetoothctl knows.
    pub devices: Vec<(String, String)>,
    /// One line for the screen.
    pub status: String,
    /// What bluetoothctl said when a step failed, quoted as it said it.
    pub detail: String,
    /// Whether the last thing that finished failed.
    pub failed: bool,
}

/// One `bluetoothctl` run, arguments passed as arguments. Nothing here goes
/// through a shell: the device list is parsed from a program's output, and a
/// name or an address from a stranger's device has no business being read as
/// shell syntax.
fn bluetoothctl(args: &[&str]) -> std::io::Result<Child> {
    Command::new("bluetoothctl")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
}

/// A Bluetooth address as bluetoothctl prints it, six hex pairs. Anything else
/// did not come from the device list and is not passed on.
fn is_address(s: &str) -> bool {
    let parts: Vec<&str> = s.split(':').collect();
    parts.len() == 6
        && parts
            .iter()
            .all(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The line worth showing out of everything a step printed.
///
/// bluetoothctl prints its banner and its prompt on the way through, and the
/// one line that says what happened is the one with the reason in it.
pub fn reason(output: &str) -> String {
    let interesting = |l: &str| {
        let l = l.trim();
        !l.is_empty()
            && (l.starts_with("Failed")
                || l.contains("org.bluez.Error")
                || l.contains("not available")
                || l.contains("Host is down")
                || l.contains("Device not ready")
                || l.starts_with("Pairing successful")
                || l.starts_with("Connection successful"))
    };
    output
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| interesting(l))
        .map(|l| {
            // `Failed to pair: org.bluez.Error.AuthenticationCanceled` is the
            // shape; the part after the last dot is the part a person reads.
            l.rsplit_once("org.bluez.Error.")
                .map(|(_, e)| e.to_string())
                .unwrap_or_else(|| l.to_string())
        })
        .unwrap_or_default()
}

/// Everything a finished child printed, both streams.
fn output_of(c: &mut Child) -> String {
    let mut text = String::new();
    if let Some(mut out) = c.stdout.take() {
        let _ = out.read_to_string(&mut text);
    }
    if let Some(mut err) = c.stderr.take() {
        let _ = err.read_to_string(&mut text);
    }
    text
}

/// The devices in `bluetoothctl devices` output, one a line.
pub fn parse_devices(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = text
        .lines()
        .filter_map(|l| {
            let mut parts = l.trim().splitn(3, ' ');
            let tag = parts.next()?;
            let mac = parts.next()?;
            let name = parts.next().unwrap_or("").trim();
            // Without the address check a line with a short token in the
            // address position reaches the list, and the screen that draws it
            // slices the address by byte.
            (tag == "Device" && !name.is_empty() && is_address(mac))
                .then(|| (mac.to_string(), name.to_string()))
        })
        .collect();
    out.sort_by_key(|a| a.1.to_lowercase());
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

impl Default for Bluetooth {
    fn default() -> Self {
        Self::new()
    }
}

impl Bluetooth {
    pub fn new() -> Self {
        Self {
            run: None,
            devices: Vec::new(),
            status: "press A to look for a pad".into(),
            detail: String::new(),
            failed: false,
        }
    }

    /// Whether `bluetoothctl` is on this machine at all.
    ///
    /// Read once and kept: this used to run a program every time the screen
    /// asked, on the thread that draws.
    pub fn available() -> bool {
        use std::sync::OnceLock;
        static FOUND: OnceLock<bool> = OnceLock::new();
        *FOUND.get_or_init(|| {
            std::env::var_os("PATH")
                .map(|p| std::env::split_paths(&p).any(|d| d.join("bluetoothctl").is_file()))
                .unwrap_or(false)
        })
    }

    pub fn busy(&self) -> bool {
        self.run.is_some()
    }

    pub fn phase(&self) -> Phase {
        self.run.as_ref().map(|r| r.phase).unwrap_or(Phase::Idle)
    }

    /// How far into the current step, 0 to 1, for the screen's animation.
    pub fn progress(&self) -> f32 {
        let Some(r) = self.run.as_ref() else {
            return 0.0;
        };
        let d = r.phase.deadline().as_secs_f32();
        if d <= 0.0 {
            return 0.0;
        }
        (r.started.elapsed().as_secs_f32() / d).clamp(0.0, 1.0)
    }

    /// Start a step, keeping whatever device the run is about.
    fn begin(&mut self, phase: Phase, args: &[&str], target: Option<(String, String)>) {
        match bluetoothctl(args) {
            Ok(child) => {
                self.run = Some(Run {
                    child,
                    started: Instant::now(),
                    phase,
                    target,
                });
                self.failed = false;
            }
            Err(e) => {
                self.run = None;
                self.failed = true;
                self.status = "bluetoothctl would not start".into();
                self.detail = e.to_string();
            }
        }
    }

    /// Look for pads. Allowed at any time, including with a list already on
    /// screen: a pad put into pairing mode after the last look is only found
    /// by looking again.
    pub fn start_scan(&mut self) {
        if self.busy() {
            return;
        }
        self.detail.clear();
        self.begin(Phase::Power, &["power", "on"], None);
        self.status = "waking the adapter".into();
    }

    pub fn pair(&mut self, index: usize) {
        if self.busy() {
            return;
        }
        let Some((mac, name)) = self.devices.get(index).cloned() else {
            return;
        };
        if !is_address(&mac) {
            self.failed = true;
            self.status = "that device has no usable address".into();
            return;
        }
        self.detail.clear();
        self.status = format!("pairing {name}");
        let target = Some((mac.clone(), name));
        self.begin(Phase::Pair, &["pair", &mac], target);
    }

    /// Stop whatever is running and say so. B on the screen.
    pub fn cancel(&mut self) {
        if let Some(mut r) = self.run.take() {
            let _ = r.child.kill();
            // Reaped here rather than dropped: a child that is never waited
            // for stays in the process table as a zombie for the life of the
            // launcher.
            let _ = r.child.wait();
            self.status = "stopped".into();
            self.detail.clear();
            self.failed = false;
        }
    }

    /// Advance the run; returns true when something on the screen changed.
    pub fn poll(&mut self) -> bool {
        let Some(mut r) = self.run.take() else {
            return false;
        };
        match r.child.try_wait() {
            Ok(None) => {
                if r.started.elapsed() > r.phase.deadline() {
                    let _ = r.child.kill();
                    let _ = r.child.wait();
                    self.failed = true;
                    self.status = format!("{} timed out", r.phase.label());
                    self.detail = match r.phase {
                        Phase::Pair | Phase::Connect => {
                            "the pad did not answer: hold its pairing button and try again".into()
                        }
                        _ => String::new(),
                    };
                    return true;
                }
                self.run = Some(r);
                false
            }
            Ok(Some(status)) => {
                let text = output_of(&mut r.child);
                self.advance(r.phase, r.target, status.success(), &text);
                true
            }
            Err(e) => {
                // Reap it: the old code dropped the Child here and left a
                // zombie behind.
                let _ = r.child.kill();
                let _ = r.child.wait();
                self.failed = true;
                self.status = format!("{} failed", r.phase.label());
                self.detail = e.to_string();
                true
            }
        }
    }

    /// One step has finished: either the next one starts or the run ends.
    fn advance(&mut self, phase: Phase, target: Option<(String, String)>, ok: bool, text: &str) {
        let said = reason(text);
        let name = target.as_ref().map(|(_, n)| n.clone()).unwrap_or_default();
        let mac = target.as_ref().map(|(m, _)| m.clone()).unwrap_or_default();
        match phase {
            // Powering on is allowed to fail: an adapter that is already on
            // answers with an error on some versions, and the scan will say
            // for real whether there is an adapter.
            Phase::Power => {
                self.begin(Phase::Scan, &["--timeout", "8", "scan", "on"], None);
                self.status = "looking, put the pad in pairing mode".into();
            }
            Phase::Scan => {
                if !ok && !said.is_empty() {
                    self.fail("the search failed", &said);
                    return;
                }
                self.begin(Phase::List, &["devices"], None);
                self.status = "reading the list".into();
            }
            Phase::List => {
                self.devices = parse_devices(text);
                self.status = if self.devices.is_empty() {
                    "nothing found, A to look again".into()
                } else {
                    format!("{} found, A to pair", self.devices.len())
                };
                self.failed = self.devices.is_empty();
                self.detail.clear();
            }
            // The two steps the old code threw away.
            Phase::Pair => {
                if !ok {
                    self.fail(&format!("{name}: pairing failed"), &said);
                    return;
                }
                self.status = format!("{name}: trusting");
                self.begin(Phase::Trust, &["trust", &mac], target);
            }
            Phase::Trust => {
                if !ok {
                    self.fail(&format!("{name}: could not be trusted"), &said);
                    return;
                }
                self.status = format!("{name}: connecting");
                self.begin(Phase::Connect, &["connect", &mac], target);
            }
            Phase::Connect => {
                if !ok {
                    self.fail(&format!("{name}: would not connect"), &said);
                    return;
                }
                self.status = format!("{name} connected");
                self.detail.clear();
                self.failed = false;
            }
            Phase::Idle => {}
        }
    }

    fn fail(&mut self, what: &str, said: &str) {
        self.run = None;
        self.failed = true;
        self.status = what.to_string();
        self.detail = said.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_six_hex_pairs_are_an_address() {
        assert!(is_address("AA:BB:CC:DD:EE:FF"));
        assert!(!is_address("short"));
        assert!(!is_address("AA:BB:CC"));
    }

    #[test]
    fn only_real_addresses_are_passed_to_bluetoothctl() {
        assert!(is_address("A4:C1:38:9F:2B:07"));
        assert!(!is_address("A4:C1:38:9F:2B"));
        assert!(!is_address("A4:C1:38:9F:2B:0Z"));
        assert!(!is_address("; rm -rf ~"));
        assert!(!is_address(""));
    }

    #[test]
    fn the_reason_is_the_line_that_says_why() {
        assert_eq!(
            reason("Attempting to pair\nFailed to pair: org.bluez.Error.AuthenticationCanceled\n"),
            "AuthenticationCanceled"
        );
        assert_eq!(
            reason("Agent registered\nFailed to connect: br-connection-page-timeout\n"),
            "Failed to connect: br-connection-page-timeout"
        );
        assert_eq!(reason("Pairing successful"), "Pairing successful");
        // A banner and a prompt say nothing, and nothing is what is shown.
        assert_eq!(reason("Agent registered\n[bluetooth]# \n"), "");
        assert_eq!(reason(""), "");
    }

    #[test]
    fn the_device_list_takes_only_lines_that_are_devices() {
        let text = "\
Device E4:17:D8:02:9A:71 8BitDo M30 gamepad
Device DC:2C:26:11:04:8B 8BitDo Ultimate 2C
Device NOTANADDR Something
Device AA:BB:CC:DD:EE:FF
[bluetooth]# quit
";
        let got = parse_devices(text);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].1, "8BitDo M30 gamepad", "sorted by name");
        assert!(got.iter().all(|(m, _)| is_address(m)));
    }

    #[test]
    fn a_step_is_given_a_deadline_and_pairing_the_longest_one() {
        assert!(Phase::Pair.deadline() > Phase::List.deadline());
        assert!(
            Phase::Scan.deadline() >= Duration::from_secs(8),
            "the scan itself is 8 s"
        );
        assert_eq!(Phase::Pair.step(), Some(1));
        assert_eq!(Phase::Connect.step(), Some(3));
        assert_eq!(Phase::Scan.step(), None);
    }

    #[test]
    fn nothing_is_running_to_start_with_and_cancelling_is_harmless() {
        let mut bt = Bluetooth::new();
        assert!(!bt.busy());
        assert_eq!(bt.phase(), Phase::Idle);
        assert_eq!(bt.progress(), 0.0);
        bt.cancel();
        assert!(!bt.busy());
        assert!(!bt.poll(), "nothing to advance");
    }
}
