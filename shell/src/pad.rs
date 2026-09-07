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
    /// A direction held on the d-pad or the keyboard, which beats the stick.
    pressed: Option<Nav>,
    held: Option<Nav>,
    since: f64,
    next_repeat: f64,
}

const DEAD_ZONE: i16 = 16_000;
/// How long a direction has to be held before it starts repeating at all.
const FIRST_REPEAT: f64 = 0.34;
/// The repeat gets faster the longer the direction is held: one step every
/// `SLOW` seconds at first, one every `FAST` after `RAMP` seconds of holding.
/// A list of thirty thousand games is unusable at a fixed rate, and a list of
/// eight is unusable at a fast one.
const SLOW: f64 = 0.13;
const FAST: f64 = 0.028;
const RAMP: f64 = 1.4;

/// The gap before the next step, for a direction held this long.
fn interval(held_for: f64) -> f64 {
    let t = ((held_for - FIRST_REPEAT) / RAMP).clamp(0.0, 1.0);
    SLOW + (FAST - SLOW) * t * t
}

impl Stick {
    pub fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            pressed: None,
            held: None,
            since: 0.0,
            next_repeat: 0.0,
        }
    }

    /// A direction pressed or released on the d-pad or the keyboard.
    ///
    /// The caller has already acted on the press, so the repeat starts from
    /// the delay rather than from another step: holding a direction must not
    /// count twice at the moment it goes down.
    pub fn set_pressed(&mut self, nav: Nav, down: bool, now: f64) {
        if down {
            self.pressed = Some(nav);
            self.held = Some(nav);
            self.since = now;
            self.next_repeat = now + FIRST_REPEAT;
        } else if self.pressed == Some(nav) {
            self.pressed = None;
            self.held = None;
        }
    }

    /// Nothing is held any more: called when the launcher loses the input, so
    /// a direction held into a game does not carry on when it comes back.
    pub fn release(&mut self) {
        self.pressed = None;
        self.held = None;
    }

    pub fn set(&mut self, axis: Axis, value: i16) {
        match axis {
            Axis::LeftX => self.x = value,
            Axis::LeftY => self.y = value,
            _ => {}
        }
    }

    fn direction(&self) -> Option<Nav> {
        if let Some(nav) = self.pressed {
            return Some(nav);
        }
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
                    self.next_repeat = now + interval(now - self.since);
                    Some(d)
                } else {
                    None
                }
            }
            (Some(d), _) => {
                self.held = Some(d);
                self.since = now;
                self.next_repeat = now + FIRST_REPEAT;
                Some(d)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Steps a direction produces when it is held for `secs`, polled at 60 Hz.
    fn steps_while_held(secs: f64, prime: bool) -> usize {
        let mut stick = Stick::new();
        if prime {
            stick.set_pressed(Nav::Down, true, 0.0);
        } else {
            stick.set(Axis::LeftY, 30_000);
        }
        let mut count = 0;
        let mut t = 0.0;
        while t < secs {
            if stick.poll(t).is_some() {
                count += 1;
            }
            t += 1.0 / 60.0;
        }
        count
    }

    #[test]
    fn a_held_direction_starts_slow_and_speeds_up() {
        // The first half second is the delay and one or two steps after it.
        let early = steps_while_held(0.5, true);
        assert!((1..=3).contains(&early), "{early} steps in half a second");
        // Holding for three seconds moves a long way, but not so far that a
        // list of thirty thousand games goes past in one press.
        let long = steps_while_held(3.0, true);
        assert!((40..=90).contains(&long), "{long} steps in three seconds");
        // The second half of the hold is faster than the first.
        let first_half = steps_while_held(1.5, true);
        assert!(
            long - first_half > first_half,
            "{first_half} then {} more",
            long - first_half
        );
    }

    #[test]
    fn a_press_does_not_count_twice() {
        let mut stick = Stick::new();
        // The caller acted on the press itself, so nothing is due yet.
        stick.set_pressed(Nav::Up, true, 0.0);
        assert_eq!(stick.poll(0.0), None);
        assert_eq!(stick.poll(0.2), None, "still inside the delay");
        assert_eq!(stick.poll(0.4), Some(Nav::Up), "then it repeats");
        stick.set_pressed(Nav::Up, false, 0.5);
        assert_eq!(stick.poll(1.0), None, "released, nothing repeats");
    }

    #[test]
    fn the_stick_answers_on_its_own_and_the_pad_wins_over_it() {
        let mut stick = Stick::new();
        stick.set(Axis::LeftY, -30_000);
        assert_eq!(stick.poll(0.0), Some(Nav::Up), "the stick steps at once");
        stick.set_pressed(Nav::Down, true, 0.1);
        assert_eq!(stick.poll(0.6), Some(Nav::Down), "the d-pad decides");
    }
}
