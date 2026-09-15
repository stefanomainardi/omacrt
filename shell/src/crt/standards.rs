//! What a fifteen kilohertz line is supposed to look like.
//!
//! Everything else in this crate counts pixels. A television counts
//! microseconds: its horizontal deflection sweeps for a fixed time, starts
//! from the sync pulse and not from the data, and puts the picture wherever
//! the timing tells it to. So a modeline can have exactly the right line rate,
//! exactly the right field rate, pass every check this project had, and still
//! run off the edge of the screen.
//!
//! That is not a hypothetical. The PAL line this project shipped for months
//! carried 53.3 microseconds of picture where the standard says 52.0, with
//! 4.4 of back porch where it says 5.8. It summed to 15.625 kHz and 50.08 Hz,
//! so nothing noticed, and the picture ran off the left of the screen until
//! somebody looked at the tube. The NTSC line put the picture 1.2 microseconds
//! right of centre, which is what the picture shift was being spent on.
//!
//! This module is the missing knowledge: the shape of a line, per standard, in
//! the unit the set is built in.
//!
//! **It refuses nothing.** [`super::output::Modeline::fault`] exists to keep a
//! timing that could damage the deflection circuit away from the kernel, and
//! it stays exactly as it is. A line of the wrong shape makes a bad picture,
//! not a dead television, and somebody driving an arcade chassis wants the
//! values under [`Standard::Arcade15`] which are "wrong" for a television on
//! purpose. So this measures and reports, and the caller decides.
//!
//! The figures are the analogue broadcast standards, cross-checked against the
//! monitor presets in Switchres (`monitor.cpp`), which is the modeline engine
//! GroovyMAME uses and the reference this community works from. The three
//! preset names below are the same ones `profile.toml` already offers.

use super::output::Modeline;

/// The shape of one line, in microseconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub front_porch_us: f64,
    pub sync_us: f64,
    pub back_porch_us: f64,
    pub active_us: f64,
    pub line_us: f64,
}

impl Shape {
    /// Where the middle of the picture falls, counted from the end of sync.
    pub fn centre_us(&self) -> f64 {
        self.back_porch_us + self.active_us / 2.0
    }
}

/// The line shapes this project can be asked to produce.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standard {
    Ntsc,
    Pal,
    /// An arcade chassis, which wants a narrower picture inside a longer
    /// blanking than a television does. Here because `profile.toml` offers it
    /// and because it is the shape this project's NTSC line used to have, so
    /// naming it keeps that an intention rather than an accident.
    Arcade15,
}

impl Standard {
    /// The standard a modeline is trying to be, from its line rate. Nothing
    /// else in a modeline says which one it is.
    pub fn of(ml: &Modeline) -> Option<Self> {
        match ml.hfreq_khz() {
            hz if (15.55..=15.70).contains(&hz) => Some(Self::Pal),
            hz if (15.70..=15.85).contains(&hz) => Some(Self::Ntsc),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Ntsc => "NTSC",
            Self::Pal => "PAL",
            Self::Arcade15 => "arcade 15 kHz",
        }
    }

    pub fn shape(self) -> Shape {
        match self {
            // 63.556 us a line, 4.7 of sync, 4.7 of back porch.
            Self::Ntsc => Shape {
                front_porch_us: 1.50,
                sync_us: 4.70,
                back_porch_us: 4.70,
                active_us: 52.66,
                line_us: 63.556,
            },
            // 64.0 us a line, and the longer back porch is why a PAL picture
            // starts later than an NTSC one.
            Self::Pal => Shape {
                front_porch_us: 1.50,
                sync_us: 4.70,
                back_porch_us: 5.80,
                active_us: 52.00,
                line_us: 64.0,
            },
            Self::Arcade15 => Shape {
                front_porch_us: 2.00,
                sync_us: 4.70,
                back_porch_us: 8.00,
                active_us: 48.86,
                line_us: 63.556,
            },
        }
    }
}

/// One thing about a line that is not what the standard says.
#[derive(Clone, Debug, PartialEq)]
pub struct Deviation {
    /// `back porch`, `picture`, and so on: the words a person would use.
    pub what: &'static str,
    pub is_us: f64,
    pub wants_us: f64,
}

impl Deviation {
    pub fn by_us(&self) -> f64 {
        self.is_us - self.wants_us
    }
}

impl std::fmt::Display for Deviation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {:.2} us where the standard says {:.2}, {:+.2} out",
            self.what,
            self.is_us,
            self.wants_us,
            self.by_us()
        )
    }
}

/// How far a timing's four durations are from the standard it serves, and how
/// far the picture sits from where a set puts it.
///
/// `tolerance_us` is what counts as the same: 0.5 is the number this project
/// uses, because the two defects it exists to catch were 1.3 and 1.4
/// microseconds out and nothing legitimate is that far.
pub fn deviation(ml: &Modeline, std: Standard, tolerance_us: f64) -> Vec<Deviation> {
    let want = std.shape();
    let mut out = Vec::new();
    let mut check = |what: &'static str, is: f64, wants: f64| {
        if (is - wants).abs() > tolerance_us {
            out.push(Deviation {
                what,
                is_us: is,
                wants_us: wants,
            });
        }
    };
    check("the picture", ml.active_us(), want.active_us);
    check("the front porch", ml.front_porch_us(), want.front_porch_us);
    check("the sync pulse", ml.sync_us(), want.sync_us);
    check("the back porch", ml.back_porch_us(), want.back_porch_us);
    // Last, because it is the one a person sees: the other four can each be
    // inside tolerance while their sum puts the picture off centre.
    check("the picture centre", ml.centre_us(), want.centre_us());
    out
}

/// The tolerance the shipped timings are held to. Half a microsecond,
/// because the two defects this exists to catch were 1.3 and 1.6 out and
/// nothing legitimate is that far.
pub const TOLERANCE_US: f64 = 0.5;

/// The tolerance on where the picture sits, which is tighter because it is
/// the half an eye notices: 0.3 microseconds is about two of the launcher's
/// own pixels on a 3520 sample line.
pub const CENTRE_TOLERANCE_US: f64 = 0.3;

/// The three timings that share the NTSC line's shape. They are one
/// decision, so they carry one allowance between them, and another test
/// holds them to the same porches.
const NTSC_SHAPED: &[&str] = &["ntsc", "film", "ntsc_i"];

/// And the two that share the PAL line's, for the same reason.
const PAL_SHAPED: &[&str] = &["pal", "pal_i"];

/// Where a shipped timing is knowingly not the shape of its standard, and
/// why. An entry here is a decision somebody took, with the reason written
/// down; anything not listed is a defect and fails the build.
///
/// The pattern is the one `scripts/audit.py` already uses for the two
/// security advisories this project accepts: an allowance is a line of code
/// with a sentence beside it, not a silence.
pub const ALLOWED: &[(&[&str], &str, &str)] = &[
    (
        PAL_SHAPED,
        "the picture",
        "48.89 us against the standard's 52.00, the same choice as the NTSC \
         line below and for the same reasons: it is the shape of Switchres's \
         `generic_15`, and holding both standards to one shape means a game \
         keeps its size across a change of region. \
         \
         The clock is 72 MHz and not the 74 this project shipped for months. \
         Measured on 2026-09-15 with the converter's lock register polled a \
         thousand times a second, a still menu on a 74 MHz PAL line lost lock \
         eighty times in sixty seconds and on a 72 MHz one not once. Each \
         loss is 276 ms with no picture. `docs/rgb-pi-2.md` had a 72 MHz PAL \
         line listed as validated on this hardware the whole time.",
    ),
    (
        PAL_SHAPED,
        "the front porch",
        "3.06 us against 1.50, the other side of the same choice: the picture \
         is narrower than the standard's, so the blanking it does not use has \
         to go somewhere. It is split to put the centre of the picture where \
         a European set puts it, 31.8 us after the end of sync, which is what \
         a person notices.",
    ),
    (
        PAL_SHAPED,
        "the back porch",
        "7.36 us against 5.80, the same. Both porches move together and the \
         centre lands where the standard wants it.",
    ),
    (
        NTSC_SHAPED,
        "the picture",
        "48.89 us against the standard's 52.66, and this one is not a \
         compromise but the practice of everybody who drives this hardware. \
         Switchres's `generic_15` preset - 2.000, 4.700, 8.000 microseconds \
         of porch, sync and back porch, so 48.86 of picture on a 63.556 line \
         - is what both reference systems for this DAC select: RGB-Pi OS V4 \
         writes 320 of 417 samples active (76.7%) in its own timing table, \
         and ReplayOS carries libswitchres and asks it for `generic_15`. Ours \
         is 3520 of 4577, 76.9%. The standard's 52.66 is what a broadcaster \
         sends, of which a set shows about ninety percent; the 15 kHz world \
         shrinks it by seven percent up front so an emulator's whole frame \
         lands inside the glass. Read on a tube on 2026-09-14: at the \
         standard's width the launcher's own writing ran off the sides. \
         Widening it to 52.5 needs a 67 MHz clock, which the CH7101 does \
         lock to, so the option is there for a monitor that shows the lot.",
    ),
    (
        NTSC_SHAPED,
        "the front porch",
        "3.63 us against 1.50, the other side of the same choice: the picture \
         is narrower than the standard's, so the blanking it does not use has \
         to go somewhere. It is split to put the centre where a television \
         puts it, which is not how `generic_15` splits it - that preset puts \
         2.00 in front and 8.00 behind, and its picture therefore sits 1.6 \
         microseconds further right than ours.",
    ),
    (
        NTSC_SHAPED,
        "the back porch",
        "6.36 us against 4.70, the same. Both porches move together and the \
         centre lands at 30.8 us, which is the number that decides where the \
         picture sits.",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crt::SHIPPED;

    fn ml(text: &str) -> Modeline {
        Modeline::parse(text).expect("a modeline")
    }

    /// Every timing this project ships is the shape of the standard it
    /// claims, or is listed in [`ALLOWED`] with the reason.
    ///
    /// This is the test that was missing. It fails on both of the defects
    /// that reached the television: the PAL line at 72 MHz, and the NTSC line
    /// with the picture 1.2 microseconds right of centre.
    #[test]
    fn every_shipped_timing_is_the_shape_of_its_standard() {
        for (name, text) in SHIPPED {
            let m = ml(text);
            let std = Standard::of(&m)
                .unwrap_or_else(|| panic!("{name}: {:.3} kHz is neither standard", m.hfreq_khz()));
            for d in deviation(&m, std, TOLERANCE_US) {
                let allowed = ALLOWED
                    .iter()
                    .any(|(names, what, _)| names.contains(&name) && *what == d.what);
                assert!(
                    allowed,
                    "{name} is not a {} line: {d}. If this is deliberate it \
                     belongs in ALLOWED with the reason.",
                    std.name()
                );
            }
        }
    }

    /// Where the picture sits is the half that an eye notices, and no shipped
    /// timing is allowed to be wrong about it.
    ///
    /// Kept apart from the durations because the two fail differently: a
    /// picture can be the wrong width on purpose, as the NTSC line is, and
    /// still land where a television expects it. Off centre is never on
    /// purpose. There is no entry in `ALLOWED` for a centre and there should
    /// never be one.
    #[test]
    fn no_shipped_timing_puts_the_picture_off_centre() {
        for (name, text) in SHIPPED {
            let m = ml(text);
            let std = Standard::of(&m).expect("a standard");
            let out = m.centre_us() - std.shape().centre_us();
            assert!(
                out.abs() <= CENTRE_TOLERANCE_US,
                "{name} puts the picture {out:+.2} us from where a {} set \
                 puts it, at {:.2} against {:.2}",
                std.name(),
                m.centre_us(),
                std.shape().centre_us()
            );
        }
    }

    /// Every entry in [`ALLOWED`] is still needed. An allowance that has been
    /// fixed and left behind is how a list like this stops meaning anything.
    #[test]
    fn nothing_is_allowed_that_no_longer_deviates() {
        for (names, what, _) in ALLOWED {
            for name in *names {
                let (_, text) = SHIPPED
                    .iter()
                    .find(|(n, _)| n == name)
                    .unwrap_or_else(|| panic!("ALLOWED names {name}, which is not shipped"));
                let m = ml(text);
                let std = Standard::of(&m).expect("a standard");
                assert!(
                    deviation(&m, std, TOLERANCE_US)
                        .iter()
                        .any(|d| d.what == *what),
                    "{name}/{what} is allowed and no longer deviates: take it out"
                );
            }
        }
    }

    /// The three NTSC shaped lines carry the same porches. They differ in
    /// what they are for - one is exactly 60.00 Hz for filming, one is
    /// interlaced - and if their porches drifted apart the picture would jump
    /// sideways when the tube changed between them.
    #[test]
    fn the_lines_of_one_standard_carry_the_same_porches() {
        let of = |want: &str| {
            let (_, text) = SHIPPED
                .iter()
                .find(|(n, _)| *n == want)
                .unwrap_or_else(|| panic!("{want} is shipped"));
            ml(text)
        };
        // A tenth of a microsecond: `film` is three samples longer than
        // `ntsc` so that its field rate is exactly 60.00 for a camera, and
        // those three samples land in the back porch. That is 0.04 us and
        // nothing can see it; a real drift is measured in whole ones.
        for group in [["ntsc", "film", "ntsc_i"], ["pal", "pal_i", "pal"]] {
            let first = of(group[0]);
            for name in group {
                let m = of(name);
                for (what, a, b) in [
                    ("front porch", m.front_porch_us(), first.front_porch_us()),
                    ("sync", m.sync_us(), first.sync_us()),
                    ("back porch", m.back_porch_us(), first.back_porch_us()),
                ] {
                    assert!(
                        (a - b).abs() < 0.1,
                        "{name} has a different {what} from {}: {a:.2} against {b:.2}",
                        group[0]
                    );
                }
            }
        }
    }

    /// The two defects, written down as the tests that would have caught
    /// them. If either shape ever comes back, these say what is wrong in the
    /// words somebody would use looking at the screen.
    #[test]
    fn the_two_timings_that_reached_the_television_are_caught() {
        let pal_before = ml("72 3840 3948 4290 4608 288 291 294 312 -hsync -vsync");
        let out = deviation(&pal_before, Standard::Pal, TOLERANCE_US);
        assert!(
            out.iter().any(|d| d.what == "the back porch"),
            "the back porch was 4.4 us against 5.8: {out:?}"
        );
        assert!(
            out.iter().any(|d| d.what == "the picture"),
            "the picture was 53.3 us against 52.0: {out:?}"
        );

        // The NTSC one is subtler: its four durations were arcade shaped, so
        // against a television it is the centre that gives it away.
        let ntsc_before = ml("72 3520 3695 4033 4577 240 242 245 262 -hsync -vsync");
        let out = deviation(&ntsc_before, Standard::Ntsc, TOLERANCE_US);
        assert!(
            out.iter().any(|d| d.what == "the picture centre"),
            "the picture sat 1.2 us right of centre: {out:?}"
        );
    }

    /// A line rate that is neither standard has no shape to be held to, which
    /// is the honest answer for a multisync monitor or a widened band.
    #[test]
    fn a_timing_that_is_neither_standard_says_so() {
        let vga = ml("25 640 656 752 800 480 490 492 525 -hsync -vsync");
        assert_eq!(Standard::of(&vga), None);
    }

    /// The arcade shape is a real answer, not a wrong one: the same NTSC line
    /// this project used to ship is a good arcade line, and saying so keeps
    /// the difference an intention.
    #[test]
    fn the_arcade_shape_is_a_shape_and_not_a_mistake() {
        let arcade = ml("72 3520 3695 4033 4577 240 242 245 262 -hsync -vsync");
        let out = deviation(&arcade, Standard::Arcade15, TOLERANCE_US);
        assert!(out.is_empty(), "it is an arcade line: {out:?}");
    }
}
