//! Synthesized sounds, mixed in the SDL audio callback.
//! Every sound is rendered once at startup into a sample buffer.

use crate::weather_sound::Ambience;
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};
use std::sync::{Arc, Mutex};

pub const RATE: u32 = 48_000;

/// A sound effect is at the gain it asked for within about fifty
/// milliseconds; the weather takes a second and a half to arrive and the
/// same to leave, because a loop that starts at full volume is a jump scare.
const EFFECT_RAMP: f32 = 0.0004;
const AMBIENCE_RAMP: f32 = 0.000015;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sound {
    PowerOn,
    Crunch,
    Chime,
    Move,
    Select,
    TagReveal,
    Lock,
    Whoosh,
    Insert,
    Click,
    /// Radio static between two stations.
    Static,
    /// A needle set down on a record: a tick and a breath of surface noise.
    /// What a change of track sounds like, if it sounds like anything.
    Needle,
}

struct Voice {
    data: Arc<Vec<f32>>,
    pos: usize,
    looping: bool,
    gain: f32,
    /// Target gain; the mixer ramps toward it (fade in and out).
    target: f32,
    /// How fast it ramps, per sample. A sound effect arrives at once; the
    /// weather takes a couple of seconds to come and to go.
    ramp: f32,
}

pub struct Mixer {
    voices: Arc<Mutex<Vec<Voice>>>,
}

impl AudioCallback for Mixer {
    type Channel = f32;
    fn callback(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        // A poisoned lock means the audio thread panicked; there is no
        // sound to salvage after that, and unwinding here is the honest end.
        let mut voices = self.voices.lock().unwrap();
        for v in voices.iter_mut() {
            for sample in out.iter_mut() {
                if v.pos >= v.data.len() {
                    if v.looping && !v.data.is_empty() {
                        v.pos = 0;
                    } else {
                        break;
                    }
                }
                v.gain += (v.target - v.gain) * v.ramp;
                *sample += v.data[v.pos] * v.gain;
                v.pos += 1;
            }
        }
        voices.retain(|v| {
            (v.pos < v.data.len() || v.looping) && !(v.target == 0.0 && v.gain < 0.002)
        });
        for s in out.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
    }
}

pub struct Audio {
    _device: Option<AudioDevice<Mixer>>,
    voices: Arc<Mutex<Vec<Voice>>>,
    bank: Vec<(Sound, Arc<Vec<f32>>)>,
    /// The weather playing now, and the loops rendered so far. A loop is
    /// four seconds of samples, so they are kept once asked for rather than
    /// all rendered at startup.
    ambience: Option<Ambience>,
    loops: Vec<(Ambience, Arc<Vec<f32>>)>,
    /// A loop being rendered on a thread, and which one it is. Four seconds
    /// of samples take long enough that rendering one where the picture is
    /// drawn costs a frame, and rendering it with the voice lock held costs
    /// an audible gap as well.
    rendering: Option<(Ambience, std::sync::mpsc::Receiver<Vec<f32>>)>,
}

impl Audio {
    pub fn silent() -> Self {
        Self {
            _device: None,
            voices: Arc::new(Mutex::new(Vec::new())),
            bank: Vec::new(),
            ambience: None,
            loops: Vec::new(),
            rendering: None,
        }
    }

    pub fn open(subsystem: &sdl2::AudioSubsystem) -> Result<Self, String> {
        let voices = Arc::new(Mutex::new(Vec::new()));
        let spec = AudioSpecDesired {
            freq: Some(RATE as i32),
            channels: Some(1),
            samples: Some(512),
        };
        let cb_voices = voices.clone();
        let device = subsystem.open_playback(None, &spec, move |_| Mixer { voices: cb_voices })?;
        device.resume();
        let bank = render_bank()
            .into_iter()
            .map(|(k, v)| (k, Arc::new(v)))
            .collect();
        Ok(Self {
            _device: Some(device),
            voices,
            bank,
            ambience: None,
            loops: Vec::new(),
            rendering: None,
        })
    }

    /// What the weather sounds like, or nothing. Called every frame with
    /// what the scene wants: the same answer twice changes nothing, a
    /// different one fades the old loop out and the new one in, and `None`
    /// leaves silence behind.
    pub fn set_ambience(&mut self, want: Option<Ambience>) {
        self.take_rendered();
        if self._device.is_none() || want == self.ambience {
            return;
        }
        self.ambience = want;
        self.fade_loops_out();
        let Some(a) = want else {
            return;
        };
        match self.loops.iter().find(|(k, _)| *k == a) {
            Some((_, data)) => {
                let data = data.clone();
                self.start_loop(data);
            }
            None => self.render_in_background(a),
        }
    }

    /// Fade whatever weather is playing towards silence.
    fn fade_loops_out(&self) {
        // Not `unwrap`: this runs on the thread that draws. A panic in the
        // audio callback poisons the lock, and taking it down with `unwrap`
        // here turned a lost sound into a black television on the next frame.
        let Ok(mut voices) = self.voices.lock() else {
            return;
        };
        for v in voices.iter_mut().filter(|v| v.looping) {
            v.target = 0.0;
            v.ramp = AMBIENCE_RAMP;
        }
    }

    /// Start a rendered loop, fading in.
    fn start_loop(&self, data: Arc<Vec<f32>>) {
        let Ok(mut voices) = self.voices.lock() else {
            return;
        };
        voices.push(Voice {
            data,
            pos: 0,
            looping: true,
            gain: 0.0,
            target: 1.0,
            ramp: AMBIENCE_RAMP,
        });
    }

    /// Render a loop on a thread. Only one is ever in flight: the weather
    /// changes slowly, and a second request replaces the first.
    fn render_in_background(&mut self, a: Ambience) {
        let (tx, rx) = std::sync::mpsc::channel();
        if std::thread::Builder::new()
            .name("ambience".into())
            .spawn(move || {
                let _ = tx.send(a.render());
            })
            .is_ok()
        {
            self.rendering = Some((a, rx));
        }
    }

    /// Take up a loop a thread has finished, keeping it for next time and
    /// starting it when it is still the weather that is wanted.
    fn take_rendered(&mut self) {
        let Some((a, rx)) = self.rendering.as_ref() else {
            return;
        };
        let (a, data) = match rx.try_recv() {
            Ok(data) => (*a, data),
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.rendering = None;
                return;
            }
        };
        self.rendering = None;
        let data = Arc::new(data);
        self.loops.push((a, data.clone()));
        if self.ambience == Some(a) {
            self.start_loop(data);
        }
    }

    /// Play a buffer generated at runtime (the laser etch follows a random walk).
    pub fn play_samples(&self, data: Vec<f32>) {
        if self._device.is_none() {
            return;
        }
        // Poisoned only if the audio callback panicked; the picture carries on
        // without the sound rather than going with it.
        let Ok(mut voices) = self.voices.lock() else {
            return;
        };
        voices.push(Voice {
            data: Arc::new(data),
            pos: 0,
            looping: false,
            gain: 1.0,
            target: 1.0,
            ramp: EFFECT_RAMP,
        });
    }

    pub fn play(&self, s: Sound) {
        if let Some((_, data)) = self.bank.iter().find(|(k, _)| *k == s) {
            // Poisoned only if the audio callback panicked; see above.
            let Ok(mut voices) = self.voices.lock() else {
                return;
            };
            voices.push(Voice {
                data: data.clone(),
                pos: 0,
                looping: false,
                gain: 1.0,
                target: 1.0,
                ramp: EFFECT_RAMP,
            });
        }
    }
}

/// Every sound the shell uses, rendered to samples.
pub fn render_bank() -> Vec<(Sound, Vec<f32>)> {
    vec![
        (Sound::PowerOn, synth_power_on()),
        (Sound::Crunch, synth_crunch()),
        (Sound::Chime, synth_chime()),
        (Sound::Move, synth_beep(880.0, 0.025, 0.035)),
        (Sound::Select, synth_beep(1320.0, 0.06, 0.04)),
        (Sound::TagReveal, crate::crt_tag::synth(RATE)),
        (Sound::Lock, synth_beep(2200.0, 0.02, 0.05)),
        (Sound::Whoosh, synth_whoosh()),
        (Sound::Insert, synth_insert()),
        (Sound::Click, synth_click()),
        (Sound::Static, synth_static()),
        (Sound::Needle, synth_needle()),
    ]
}

/// Write one sound as a 16 bit mono WAV, for listening outside the shell.
pub fn write_wav(path: &std::path::Path, data: &[f32]) -> std::io::Result<()> {
    let mut out = Vec::with_capacity(44 + data.len() * 2);
    let bytes = (data.len() * 2) as u32;
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + bytes).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&bytes.to_le_bytes());
    for s in data {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, out)
}

pub(crate) fn seconds(n: f32) -> usize {
    (n * RATE as f32) as usize
}

/// Deterministic noise, good enough for a click.
pub(crate) struct Lcg(pub u32);
impl Lcg {
    pub(crate) fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    }
}

/// One-pole lowpass over a buffer.
pub(crate) fn lowpass(buf: &mut [f32], cutoff_hz: f32) {
    let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff_hz);
    let dt = 1.0 / RATE as f32;
    let a = dt / (rc + dt);
    let mut y = 0.0;
    for s in buf.iter_mut() {
        y += a * (*s - y);
        *s = y;
    }
}

/// Toggle switch clunk plus the degauss thump of a TV coming to life.
fn synth_power_on() -> Vec<f32> {
    let n = seconds(0.6);
    let mut out = vec![0.0; n];
    let mut rng = Lcg(7);
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        let click = if t < 0.012 {
            rng.next() * (1.0 - t / 0.012) * 0.9
        } else {
            0.0
        };
        let thump = (2.0 * std::f32::consts::PI * 52.0 * t).sin() * (-t * 9.0).exp() * 0.7;
        let hum = (2.0 * std::f32::consts::PI * 15_625.0 * t).sin()
            * (-(t - 0.05).max(0.0) * 6.0).exp()
            * 0.02;
        let hiss = rng.next() * (-t * 12.0).exp() * 0.08;
        *s = click + thump + hum + hiss;
    }
    lowpass(&mut out[..], 6000.0);
    for s in out.iter_mut() {
        *s *= 0.35;
    }
    out
}

/// Frozen HDD seek: voice-coil click, arm rings around 1.3 and 2.1 kHz, carriage clunk.
fn synth_crunch() -> Vec<f32> {
    let n = seconds(0.07);
    let mut out = vec![0.0; n];
    let mut rng = Lcg(3);
    let tau = 2.0 * std::f32::consts::PI;
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        let env = (-t * 58.0).exp();
        let click = if i == 0 {
            0.85
        } else if i < 6 {
            0.12
        } else {
            0.0
        };
        let clunk = (tau * 270.0 * t).sin() * (-t * 78.0).exp() * 0.4;
        let ring13 = (tau * 1300.0 * t).sin() * (-t * 68.0).exp() * 0.32;
        let ring21 = (tau * 2080.0 * t).sin() * (-t * 88.0).exp() * 0.26;
        let grit = rng.next() * (-t * 110.0).exp() * 0.16;
        *s = (click + clunk + ring13 + ring21 + grit) * env;
    }
    lowpass(&mut out[..], 4200.0);
    for s in out.iter_mut() {
        *s *= 0.3;
    }
    out
}

/// Systems-online chord: soft partials, slow tape echo, long tail.
fn synth_chime() -> Vec<f32> {
    let tail = 5.4;
    let n = seconds(tail);
    let mut dry = vec![0.0; n];
    let tau = 2.0 * std::f32::consts::PI;
    // D major with an added ninth, spread over two octaves.
    let notes = [146.83, 220.0, 293.66, 369.99, 440.0, 587.33, 659.25];
    let gains = [0.9, 0.7, 0.8, 0.6, 0.55, 0.45, 0.25];
    for (i, s) in dry.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        let attack = (t / 0.03).min(1.0);
        let hold = if t < 1.4 {
            1.0
        } else {
            (-(t - 1.4) * 1.6).exp()
        };
        let mut v = 0.0;
        for (k, f) in notes.iter().enumerate() {
            // Triangle-ish: sine plus a quiet third harmonic.
            let ph = tau * f * t;
            v += gains[k]
                * (ph.sin() + 0.18 * (3.0 * ph).sin())
                * (1.0 + 0.01 * (t * 0.7 + k as f32).sin());
        }
        *s = v * attack * hold * 0.045;
    }
    // Lowpass sweep approximation: brighter for the first 0.3 s, then mellow.
    lowpass(&mut dry[..], 1400.0);
    // Two tape echoes with feedback and damping.
    let mut out = dry.clone();
    let d1 = seconds(0.36);
    let d2 = seconds(0.54);
    let mut fb = vec![0.0; n];
    for i in 0..n {
        let e1 = if i >= d1 { fb[i - d1] } else { 0.0 };
        fb[i] = dry[i] + e1 * 0.42;
        let e2 = if i >= d2 { fb[i - d2] } else { 0.0 };
        out[i] += (e1 + e2) * 0.46;
    }
    lowpass(&mut out[..], 2400.0);
    // Master envelope: full for 1.4 s, then to silence at the tail.
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        let env = if t < 1.4 {
            1.0
        } else {
            (-(t - 1.4) * 1.15).exp()
        };
        *s *= env;
    }
    out
}

fn synth_beep(freq: f32, dur: f32, gain: f32) -> Vec<f32> {
    let n = seconds(dur);
    let mut out = vec![0.0; n];
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        let square = if (t * freq).fract() < 0.5 { 1.0 } else { -1.0 };
        let env = (-(t / dur) * 5.0).exp();
        *s = square * env * gain;
    }
    out
}

/// Short filtered noise sweep for a submenu opening.
fn synth_whoosh() -> Vec<f32> {
    let n = seconds(0.14);
    let mut out = vec![0.0; n];
    let mut rng = Lcg(21);
    let mut lp = 0.0f32;
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        let p = t / 0.14;
        let cut = 600.0 + 3400.0 * (1.0 - p);
        let a = 1.0 / (1.0 + RATE as f32 / (2.0 * std::f32::consts::PI * cut));
        lp += a * (rng.next() - lp);
        let env = (p * std::f32::consts::PI).sin();
        *s = lp * env * 0.12;
    }
    out
}

/// A cartridge sliding home: plastic scrape, then a firm click.
fn synth_insert() -> Vec<f32> {
    let n = seconds(0.55);
    let mut out = vec![0.0; n];
    let mut rng = Lcg(33);
    let mut lp = 0.0f32;
    let tau = 2.0 * std::f32::consts::PI;
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        // Scrape while sliding (0 to 0.4 s), band-limited noise with a slow swell.
        let a = 1.0 / (1.0 + RATE as f32 / (tau * 1400.0));
        lp += a * (rng.next() - lp);
        let slide = if t < 0.4 {
            (t / 0.4 * std::f32::consts::PI).sin() * 0.05
        } else {
            0.0
        };
        let mut v = lp * slide;
        // Click at 0.42 s: two short resonances.
        let d = t - 0.42;
        if d >= 0.0 {
            v += (tau * 900.0 * d).sin() * (-d * 90.0).exp() * 0.35;
            v += (tau * 2400.0 * d).sin() * (-d * 140.0).exp() * 0.2;
            v += rng.next() * (-d * 300.0).exp() * 0.2;
        }
        *s = v;
    }
    out
}

/// Between two stations: hiss that swells and cuts as the tuner locks.
fn synth_static() -> Vec<f32> {
    let n = seconds(0.7);
    let mut out = vec![0.0; n];
    let mut rng = Lcg(77);
    let mut lp = 0.0f32;
    let tau = 2.0 * std::f32::consts::PI;
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        let p = t / 0.7;
        let cut = 900.0 + 2600.0 * (0.5 + 0.5 * (p * 9.0).sin());
        let a = 1.0 / (1.0 + RATE as f32 / (tau * cut));
        lp += a * (rng.next() - lp);
        let env = if p < 0.85 {
            (p * std::f32::consts::PI / 0.85).sin().powf(0.6)
        } else {
            0.0
        };
        // Broadband noise is heard as far louder than a beep of the same
        // level, and this one lasts twenty times longer than the beeps do.
        *s = lp * env * 0.075;
    }
    out
}

/// A needle set down: one soft tick, then a little surface noise that dies
/// away. A fifth of a second, and quiet: a track change should be noticed
/// rather than announced.
fn synth_needle() -> Vec<f32> {
    let n = seconds(0.2);
    let mut out = vec![0.0; n];
    let mut rng = Lcg(31);
    let mut lp = 0.0f32;
    let tau = 2.0 * std::f32::consts::PI;
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        // The tick: a very short thump, gone in twelve milliseconds.
        let tick = if t < 0.012 {
            let env = (1.0 - t / 0.012).powi(2);
            (t * tau * 190.0).sin() * env * 0.05
        } else {
            0.0
        };
        // The groove: filtered noise fading out under it.
        let a = 1.0 / (1.0 + RATE as f32 / (tau * 1800.0));
        lp += a * (rng.next() - lp);
        let hiss = lp * (1.0 - t / 0.2).max(0.0).powf(1.5) * 0.022;
        *s = tick + hiss;
    }
    out
}

/// Tiny mechanical click for typewriter text.
fn synth_click() -> Vec<f32> {
    let n = seconds(0.008);
    let mut out = vec![0.0; n];
    let mut rng = Lcg(5);
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        *s = rng.next() * (-t * 900.0).exp() * 0.12;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A weather loop is four seconds of samples and takes long enough to
    /// render that doing it where the picture is drawn costs a frame. It goes
    /// to a thread, and the voice starts on a later frame: this walks that
    /// path with no sound card in the machine.
    #[test]
    fn ambience_renders_on_a_thread_and_then_plays() {
        let mut a = Audio::silent();
        a.ambience = Some(Ambience::Rain);
        a.render_in_background(Ambience::Rain);
        assert!(a.rendering.is_some(), "no thread took the work");
        assert!(a.loops.is_empty(), "the render happened on this thread");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while a.loops.is_empty() && std::time::Instant::now() < deadline {
            a.take_rendered();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(a.loops.len(), 1, "the rendered loop was never taken up");
        assert!(a.rendering.is_none(), "the thread was not let go");
        let voices = a.voices.lock().expect("voices");
        assert_eq!(voices.len(), 1, "the loop was not started");
        assert!(voices[0].looping);
    }

    /// A loop already rendered is started without a thread at all.
    #[test]
    fn a_cached_loop_starts_at_once() {
        let mut a = Audio::silent();
        a.loops.push((Ambience::Calm, Arc::new(vec![0.0; 16])));
        a.ambience = Some(Ambience::Calm);
        let data = a.loops[0].1.clone();
        a.start_loop(data);
        assert!(a.rendering.is_none());
        assert_eq!(a.voices.lock().expect("voices").len(), 1);
    }
}
