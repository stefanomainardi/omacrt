//! The weather, heard rather than seen.
//!
//! The ambient page draws rain, wind and a sun; this is what those sound
//! like on a machine that had four channels and no room for a recording.
//! Every voice here is synthesized from noise and square waves and then put
//! through the same two indignities a sample suffered in 1990: held at 8 kHz
//! and quantised to five bits. That is the whole trick. Rain recorded on a
//! phone sounds like rain; rain crushed like this sounds like rain in a
//! game, which is what the rest of this television sounds like.
//!
//! A loop is four seconds long, seamless, and rendered once when it is first
//! asked for, because nobody needs eight of them in memory to hear one.
//!
//! Everything is quiet on purpose, and measured rather than guessed. This
//! comes on by itself when a television is left alone, so it has to be the
//! sort of sound somebody can read a book next to: the loudest of them, a
//! thunderstorm, measures 0.012 against the boot chime's 0.030, and the
//! quietest, snow, is 0.004.

use crate::audio::{Lcg, RATE, lowpass, seconds};
use omacrt_shell::ambient::Kind;

/// The weather as the ear needs it, which is fewer cases than the eye does:
/// nine kinds of sky make eight sounds, and a clear night is not a clear
/// day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ambience {
    /// A clear day: almost nothing, and a bird every so often.
    Calm,
    /// A clear night: crickets, and the hum of a warm evening.
    Night,
    /// Cloud and wind, gusting.
    Breeze,
    /// Rain on a roof.
    Rain,
    /// Rain that means it.
    Downpour,
    /// A downpour with thunder walking about behind it.
    Storm,
    /// Snow, which is the sound of everything else stopping.
    Snow,
    /// Fog: a low room tone and a horn a long way off.
    Fog,
}

impl Ambience {
    /// The sound for a sky. `day` is false after sunset, which changes only
    /// the clear one: birds by day, crickets by night.
    pub fn of(kind: Kind, day: bool) -> Option<Self> {
        Some(match kind {
            Kind::Clear | Kind::Partly if !day => Ambience::Night,
            Kind::Clear => Ambience::Calm,
            Kind::Partly | Kind::Cloudy => Ambience::Breeze,
            Kind::Overcast => Ambience::Breeze,
            Kind::Fog => Ambience::Fog,
            Kind::Rain => Ambience::Rain,
            Kind::Heavy => Ambience::Downpour,
            Kind::Thunder => Ambience::Storm,
            Kind::Snow => Ambience::Snow,
        })
    }

    /// The loop: the voice, then the crush, then scaled to the level this
    /// one is allowed and joined to itself.
    pub fn render(self) -> Vec<f32> {
        let mut out = match self {
            Ambience::Calm => calm(),
            Ambience::Night => night(),
            Ambience::Breeze => breeze(),
            Ambience::Rain => rain(0.55, 2600.0, 0.115),
            Ambience::Downpour => rain(1.0, 4200.0, 0.055),
            Ambience::Storm => storm(),
            Ambience::Snow => snow(),
            Ambience::Fog => fog(),
        };
        crush(&mut out, self.hold(), 32.0);
        level(&mut out, self.rms());
        seamless(&mut out);
        out
    }

    /// How many samples each one is held for, which is the sample rate the
    /// voice pretends to have. Six of them is 8 kHz, and that is the sound
    /// of the era. The birds and the crickets are held for four, 12 kHz,
    /// because a chirp of two kilohertz folds over at eight and comes back
    /// as a buzz.
    fn hold(self) -> usize {
        match self {
            Ambience::Calm | Ambience::Night => 4,
            Ambience::Breeze | Ambience::Fog => 8,
            Ambience::Snow => 10,
            _ => 6,
        }
    }

    /// How loud, as root mean square, measured rather than guessed. The
    /// launcher's own chime is 0.030 and moving the cursor is 0.011, and
    /// this plays for as long as the television is left alone, so all of
    /// them sit under both.
    fn rms(self) -> f32 {
        match self {
            Ambience::Snow => 0.004,
            Ambience::Calm => 0.005,
            Ambience::Night => 0.006,
            Ambience::Fog => 0.008,
            Ambience::Rain => 0.008,
            Ambience::Breeze => 0.009,
            Ambience::Downpour => 0.012,
            Ambience::Storm => 0.012,
        }
    }

    /// For a file name, a test and `--dump-audio`.
    pub fn name(self) -> &'static str {
        match self {
            Ambience::Calm => "calm",
            Ambience::Night => "night",
            Ambience::Breeze => "breeze",
            Ambience::Rain => "rain",
            Ambience::Downpour => "downpour",
            Ambience::Storm => "storm",
            Ambience::Snow => "snow",
            Ambience::Fog => "fog",
        }
    }

    /// Every one of them, for `--dump-audio` and for the tests.
    pub const ALL: [Ambience; 8] = [
        Ambience::Calm,
        Ambience::Night,
        Ambience::Breeze,
        Ambience::Rain,
        Ambience::Downpour,
        Ambience::Storm,
        Ambience::Snow,
        Ambience::Fog,
    ];
}

/// How long a loop is. Four seconds is long enough that the ear does not
/// hear where it joins and short enough to render while a page slides in.
const SECS: f32 = 4.0;
/// The join: the last quarter second is folded over the first.
const SEAM: f32 = 0.25;

/// A buffer with room for the loop and the tail that gets folded into its
/// beginning. Everything below writes at its own natural scale, because the
/// balance inside a loop is the synth's business and its volume is not.
fn canvas() -> Vec<f32> {
    vec![0.0; seconds(SECS + SEAM)]
}

/// Scale a buffer to a root mean square, and hold it back if the loudest
/// moment would still be harsh.
fn level(buf: &mut [f32], want: f32) {
    let n = seconds(SECS).min(buf.len());
    let rms = (buf[..n].iter().map(|s| s * s).sum::<f32>() / n as f32).sqrt();
    if rms <= f32::EPSILON {
        return;
    }
    let mut gain = want / rms;
    let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    // A quarter of full scale: a rumble or a drop can stand out without
    // making somebody reach for the volume.
    if peak * gain > 0.25 {
        gain = 0.25 / peak;
    }
    for s in buf.iter_mut() {
        *s *= gain;
    }
}

/// Fold the tail over the beginning and cut the buffer to the loop's length,
/// so playing it end to end has no click and no gap.
fn seamless(buf: &mut Vec<f32>) {
    let n = seconds(SECS);
    let f = seconds(SEAM);
    for i in 0..f {
        let k = i as f32 / f as f32;
        buf[i] = buf[i] * k + buf[n + i] * (1.0 - k);
    }
    buf.truncate(n);
}

/// The 1990 voice: hold each sample for `hold` of them, then round to
/// `steps` levels. Everything in this module goes through it.
fn crush(buf: &mut [f32], hold: usize, steps: f32) {
    let mut held = 0.0;
    for (i, s) in buf.iter_mut().enumerate() {
        if i % hold == 0 {
            held = (*s * steps).round() / steps;
        }
        *s = held;
    }
}

/// A square wave: the chirps, the crickets and the horn's edge.
fn square(t: f32, hz: f32) -> f32 {
    if (t * hz).fract() < 0.5 { 1.0 } else { -1.0 }
}

/// Fill a buffer with noise and filter it: the bed under every one of these.
fn bed(cutoff: f32, seed: u32) -> Vec<f32> {
    let mut out = canvas();
    let mut rng = Lcg(seed);
    for s in out.iter_mut() {
        *s = rng.next();
    }
    lowpass(&mut out, cutoff);
    out
}

/// Add a note to a buffer at `at` seconds, `hz`, `len` long, shaped by
/// `env` over its own 0 to 1 and at `gain`. The little events, the drops,
/// the chirps and the horn, are all this.
fn note(out: &mut [f32], at: f32, len: f32, gain: f32, mut voice: impl FnMut(f32, f32) -> f32) {
    let start = seconds(at);
    for i in 0..seconds(len) {
        let t = i as f32 / RATE as f32;
        if let Some(s) = out.get_mut(start + i) {
            *s += voice(t, t / len) * gain;
        }
    }
}

const TAU: f32 = std::f32::consts::TAU;

/// Rain: a filtered hiss with drops on it. `cutoff` decides whether it is a
/// shower or a downpour, `every` how often a drop lands, and the drops are
/// what the ear actually hears as rain: hiss alone is a broken television.
fn rain(strength: f32, cutoff: f32, every: f32) -> Vec<f32> {
    let mut out = bed(cutoff, 7);
    for s in out.iter_mut() {
        *s *= 0.55 * strength;
    }
    let mut at = 0.0f32;
    let mut rng = Lcg(11);
    while at < SECS + SEAM {
        let pitch = 900.0 + rng.next().abs() * 1400.0;
        let len = 0.02 + rng.next().abs() * 0.025;
        let gain = (0.8 + rng.next().abs() * 0.9) * strength;
        note(&mut out, at, len, gain, move |t, k| {
            (t * pitch * TAU).sin() * (1.0 - k).powi(3)
        });
        // An interval that wanders: rain that ticks evenly is a metronome.
        at += every * (0.4 + rng.next().abs() * 1.2);
    }
    out
}

/// Wind: the same noise filtered much lower, gusting. Two swells of
/// different periods, and the quietest moment is not silence, because a
/// hole in a wind loop is heard as a fault.
fn breeze() -> Vec<f32> {
    let mut out = bed(520.0, 23);
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        let gust = 0.55 + 0.3 * (t * TAU * 0.25).sin() + 0.15 * (t * TAU * 0.75).sin();
        *s *= gust;
    }
    out
}

/// A downpour with thunder in it: two rolls, one close and one a long way
/// off, each with the shape every thunderclap has. Up fast, down slowly.
fn storm() -> Vec<f32> {
    let mut out = rain(1.0, 4200.0, 0.05);
    for (at, distance, seed) in [(0.6f32, 0.2f32, 97u32), (2.35, 1.0, 131)] {
        let mut roll = vec![0.0f32; seconds(2.2)];
        let mut rng = Lcg(seed);
        for s in roll.iter_mut() {
            *s = rng.next();
        }
        // A near strike keeps its crack; a far one has lost everything above
        // a hum by the time it arrives.
        lowpass(&mut roll, 80.0 + 300.0 * (1.0 - distance));
        for (i, s) in roll.iter_mut().enumerate() {
            let t = i as f32 / RATE as f32;
            let env = (1.0 - (-t * 16.0).exp()) * (-t * 1.7).exp();
            *s *= env * (3.4 - 2.0 * distance);
        }
        let start = seconds(at);
        for (i, r) in roll.iter().enumerate() {
            if let Some(s) = out.get_mut(start + i) {
                *s += r;
            }
        }
    }
    out
}

/// Snow: the sound of everything else stopping. Noise so low and so slow
/// that it is a room tone.
fn snow() -> Vec<f32> {
    let mut out = bed(640.0, 41);
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        *s *= 0.55 + 0.4 * (t * TAU * 0.18).sin();
    }
    out
}

/// Fog: a low room tone, two tones a fifth apart under it, and a horn a
/// long way off once in the loop.
fn fog() -> Vec<f32> {
    let mut out = bed(220.0, 59);
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        let hum = (t * TAU * 55.0).sin() * 0.5 + (t * TAU * 82.5).sin() * 0.25;
        *s = *s * 0.6 + hum;
    }
    // Two notes, the second lower, the way a real one answers itself.
    for (at, hz) in [(1.1f32, 165.0f32), (1.8, 110.0)] {
        note(&mut out, at, 0.55, 1.6, move |t, k| {
            let env = (1.0 - (-t * 9.0).exp()) * (1.0 - k).max(0.0).powf(1.4);
            ((t * TAU * hz).sin() * 0.75 + square(t, hz * 2.0) * 0.15) * env
        });
    }
    out
}

/// A clear day: a breath of air, and a bird. The chirp is a square wave
/// sliding up over fifty milliseconds, two or three notes to a call, which
/// is how every bird in a 1990 platform game was made.
fn calm() -> Vec<f32> {
    let mut out = bed(380.0, 13);
    // A breath of air under the birds, not a wind: loud enough that the
    // gaps between calls are a garden rather than a broken speaker.
    for s in out.iter_mut() {
        *s *= 0.6;
    }
    let mut rng = Lcg(67);
    let mut at = 0.35f32;
    while at < SECS + SEAM {
        let notes = 2 + (rng.next().abs() * 2.0) as usize;
        let base = 1250.0 + rng.next().abs() * 700.0;
        for n in 0..notes {
            let len = 0.05 + rng.next().abs() * 0.03;
            let hz = base * (1.0 + 0.1 * n as f32);
            note(&mut out, at, len, 0.5, move |t, k| {
                // The slide up is what makes it a bird and not a beep.
                let env = (t / 0.008).min(1.0) * (1.0 - k).max(0.0).powf(0.8);
                square(t, hz * (1.0 + 0.3 * k)) * env
            });
            at += len + 0.035;
        }
        at += 0.7 + rng.next().abs() * 0.9;
    }
    out
}

/// A clear night: crickets, three chirps to a call, and the hum of a warm
/// evening under them.
fn night() -> Vec<f32> {
    let mut out = canvas();
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        *s = (t * TAU * 70.0).sin() * 0.25;
    }
    let mut rng = Lcg(83);
    let mut at = 0.2f32;
    while at < SECS + SEAM {
        let hz = 2400.0 + rng.next().abs() * 400.0;
        let gain = 0.9 + rng.next().abs() * 0.5;
        for k in 0..3 {
            note(&mut out, at + k as f32 * 0.038, 0.016, gain, move |t, u| {
                square(t, hz) * (1.0 - u)
            });
        }
        at += 0.42 + rng.next().abs() * 0.5;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn energy(data: &[f32]) -> f32 {
        (data.iter().map(|s| s * s).sum::<f32>() / data.len() as f32).sqrt()
    }

    #[test]
    fn every_loop_is_four_seconds_and_quiet() {
        for a in Ambience::ALL {
            let data = a.render();
            assert_eq!(data.len(), seconds(SECS), "{}", a.name());
            let rms = energy(&data);
            // Quiet enough to read next to, loud enough to hear at all. The
            // launcher's own chime is around 0.08.
            assert!(rms > 0.0004, "{} is silent: {rms}", a.name());
            assert!(rms < 0.035, "{} is too loud: {rms}", a.name());
            assert!(data.iter().all(|s| s.abs() <= 1.0), "{} clips", a.name());
        }
    }

    #[test]
    fn the_loop_has_no_seam() {
        // The join is a crossfade, so the last sample and the first have to
        // be within a hair of each other or a loop clicks every four
        // seconds.
        for a in Ambience::ALL {
            let data = a.render();
            let first = data[0];
            let last = data[data.len() - 1];
            assert!(
                (first - last).abs() < 0.12,
                "{}: {first} then {last}",
                a.name()
            );
        }
    }

    #[test]
    fn a_clear_sky_sounds_different_by_night() {
        assert_eq!(Ambience::of(Kind::Clear, true), Some(Ambience::Calm));
        assert_eq!(Ambience::of(Kind::Clear, false), Some(Ambience::Night));
        assert_eq!(Ambience::of(Kind::Thunder, true), Some(Ambience::Storm));
        assert_eq!(Ambience::of(Kind::Snow, false), Some(Ambience::Snow));
    }
}
