//! Game controller helpers: which family a pad belongs to (for on-screen
//! button labels) and analog stick to d-pad conversion with key repeat.

use crate::scene::Nav;
use sdl2::controller::Axis;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadKind {
    Xbox,
    PlayStation,
    Nintendo,
    Generic,
    /// No pad connected: hints name the keys instead.
    Keyboard,
}

/// Labels of the four actions the shell uses, in the pad's own vocabulary.
pub struct Labels {
    pub accept: &'static str,
    pub back: &'static str,
    pub fav: &'static str,
    pub alt: &'static str,
}

impl PadKind {
    /// SDL does not expose the controller type through the Rust binding we
    /// use, so guess from the name SDL reports.
    pub fn from_name(name: &str) -> Self {
        let n = name.to_lowercase();
        if [
            "playstation",
            "dualshock",
            "dualsense",
            "ps3",
            "ps4",
            "ps5",
            "sony",
        ]
        .iter()
        .any(|k| n.contains(k))
        {
            PadKind::PlayStation
        } else if [
            "nintendo", "switch", "joy-con", "wii", "8bitdo", "n30", "sn30", "sf30",
        ]
        .iter()
        .any(|k| n.contains(k))
        {
            PadKind::Nintendo
        } else if ["xbox", "x-box", "xinput", "microsoft"]
            .iter()
            .any(|k| n.contains(k))
        {
            PadKind::Xbox
        } else {
            PadKind::Generic
        }
    }

    /// SDL reports buttons by position (A = south). PlayStation pads show
    /// symbols there; Nintendo pads carry the same letters SDL uses.
    pub fn labels(&self) -> Labels {
        match self {
            PadKind::PlayStation => Labels {
                accept: "X",
                back: "O",
                fav: "^",
                alt: "[]",
            },
            PadKind::Keyboard => Labels {
                accept: "Enter",
                back: "Esc",
                fav: "F",
                alt: "X",
            },
            _ => Labels {
                accept: "A",
                back: "B",
                fav: "Y",
                alt: "X",
            },
        }
    }
}

/// Left stick as a d-pad: a direction fires once when the stick leaves the
/// dead zone, then repeats while held.
pub struct Stick {
    x: i16,
    y: i16,
    held: Option<Nav>,
    next_repeat: f64,
}

const DEAD_ZONE: i16 = 16_000;
const FIRST_REPEAT: f64 = 0.35;
const REPEAT: f64 = 0.12;

impl Stick {
    pub fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            held: None,
            next_repeat: 0.0,
        }
    }

    pub fn set(&mut self, axis: Axis, value: i16) {
        match axis {
            Axis::LeftX => self.x = value,
            Axis::LeftY => self.y = value,
            _ => {}
        }
    }

    fn direction(&self) -> Option<Nav> {
        if self.y.abs() >= self.x.abs() {
            if self.y <= -DEAD_ZONE {
                Some(Nav::Up)
            } else if self.y >= DEAD_ZONE {
                Some(Nav::Down)
            } else {
                None
            }
        } else if self.x <= -DEAD_ZONE {
            Some(Nav::Left)
        } else if self.x >= DEAD_ZONE {
            Some(Nav::Right)
        } else {
            None
        }
    }

    /// Call once per frame; returns a navigation step when one is due.
    pub fn poll(&mut self, now: f64) -> Option<Nav> {
        let dir = self.direction();
        match (dir, self.held) {
            (None, _) => {
                self.held = None;
                None
            }
            (Some(d), Some(h)) if d == h => {
                if now >= self.next_repeat {
                    self.next_repeat = now + REPEAT;
                    Some(d)
                } else {
                    None
                }
            }
            (Some(d), _) => {
                self.held = Some(d);
                self.next_repeat = now + FIRST_REPEAT;
                Some(d)
            }
        }
    }
}
