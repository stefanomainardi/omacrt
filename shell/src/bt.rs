//! Bluetooth pad pairing through `bluetoothctl`, without blocking the frame
//! loop: every step is a child process polled once per frame.

use std::io::Read;
use std::process::{Child, Command, Stdio};

enum State {
    Idle,
    /// `bluetoothctl scan on` running for a few seconds.
    Scanning(Child),
    /// `bluetoothctl devices` collecting the list.
    Listing(Child),
    /// pair, trust and connect chained in one shell.
    Pairing(Child, String),
}

pub struct Bluetooth {
    state: State,
    /// (address, name) of every device bluetoothctl knows.
    pub devices: Vec<(String, String)>,
    pub status: String,
    pub busy: bool,
}

/// One `bluetoothctl` run, arguments passed as arguments. Nothing here goes
/// through a shell: the device list is parsed from a program's output, and a
/// name or an address from a stranger's device has no business being read as
/// shell syntax.
fn bluetoothctl(args: &[&str], capture: bool) -> std::io::Result<Child> {
    Command::new("bluetoothctl")
        .args(args)
        .stdin(Stdio::null())
        .stdout(if capture {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stderr(Stdio::null())
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

impl Bluetooth {
    pub fn new() -> Self {
        Self {
            state: State::Idle,
            devices: Vec::new(),
            status: "press A to scan".into(),
            busy: false,
        }
    }

    pub fn available() -> bool {
        Command::new("bluetoothctl")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    pub fn start_scan(&mut self) {
        if self.busy {
            return;
        }
        let _ = bluetoothctl(&["power", "on"], false).map(|mut c| c.wait());
        match bluetoothctl(&["--timeout", "8", "scan", "on"], false) {
            Ok(c) => {
                self.state = State::Scanning(c);
                self.busy = true;
                self.status = "scanning, put the pad in pairing mode".into();
            }
            Err(e) => self.status = format!("bluetoothctl: {e}"),
        }
    }

    pub fn pair(&mut self, index: usize) {
        if self.busy {
            return;
        }
        let Some((mac, name)) = self.devices.get(index).cloned() else {
            return;
        };
        if !is_address(&mac) {
            self.status = "that device has no usable address".into();
            return;
        }
        // Pair, then trust, then connect: each one its own run, so a failure
        // stops the sequence the way the shell's && used to.
        let _ = bluetoothctl(&["pair", &mac], false).map(|mut c| c.wait());
        let _ = bluetoothctl(&["trust", &mac], false).map(|mut c| c.wait());
        match bluetoothctl(&["connect", &mac], false) {
            Ok(c) => {
                self.state = State::Pairing(c, name.clone());
                self.busy = true;
                self.status = format!("pairing {name}");
            }
            Err(e) => self.status = format!("bluetoothctl: {e}"),
        }
    }

    /// Advance the state machine; returns true when something changed.
    pub fn poll(&mut self) -> bool {
        let state = std::mem::replace(&mut self.state, State::Idle);
        match state {
            State::Idle => false,
            State::Scanning(mut c) => match c.try_wait() {
                Ok(Some(_)) => {
                    match bluetoothctl(&["devices"], true) {
                        Ok(l) => self.state = State::Listing(l),
                        Err(e) => {
                            self.status = format!("bluetoothctl: {e}");
                            self.busy = false;
                        }
                    }
                    true
                }
                Ok(None) => {
                    self.state = State::Scanning(c);
                    false
                }
                Err(_) => {
                    self.status = "scan failed".into();
                    self.busy = false;
                    true
                }
            },
            State::Listing(mut c) => match c.try_wait() {
                Ok(Some(_)) => {
                    let mut text = String::new();
                    if let Some(mut out) = c.stdout.take() {
                        let _ = out.read_to_string(&mut text);
                    }
                    self.devices = text
                        .lines()
                        .filter_map(|l| {
                            let mut parts = l.splitn(3, ' ');
                            let tag = parts.next()?;
                            let mac = parts.next()?;
                            let name = parts.next().unwrap_or("").trim();
                            (tag == "Device" && !name.is_empty())
                                .then(|| (mac.to_string(), name.to_string()))
                        })
                        .collect();
                    self.devices
                        .sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
                    self.status = if self.devices.is_empty() {
                        "no devices found, A to scan again".into()
                    } else {
                        format!("{} devices, A to pair", self.devices.len())
                    };
                    self.busy = false;
                    true
                }
                Ok(None) => {
                    self.state = State::Listing(c);
                    false
                }
                Err(_) => {
                    self.status = "listing failed".into();
                    self.busy = false;
                    true
                }
            },
            State::Pairing(mut c, name) => match c.try_wait() {
                Ok(Some(st)) => {
                    self.status = if st.success() {
                        format!("{name} connected")
                    } else {
                        format!("{name}: pairing failed, retry in pairing mode")
                    };
                    self.busy = false;
                    true
                }
                Ok(None) => {
                    self.state = State::Pairing(c, name);
                    false
                }
                Err(_) => {
                    self.status = "pairing failed".into();
                    self.busy = false;
                    true
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_real_addresses_are_passed_to_bluetoothctl() {
        assert!(super::is_address("A4:C1:38:9F:2B:07"));
        assert!(!super::is_address("A4:C1:38:9F:2B"));
        assert!(!super::is_address("A4:C1:38:9F:2B:0Z"));
        assert!(!super::is_address("; rm -rf ~"));
        assert!(!super::is_address(""));
    }
}
