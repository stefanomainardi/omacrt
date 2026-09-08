//! The clock the launcher draws, and the one seam that moves it.
//!
//! Every screen that shows the time, and the ambient page's whole idea of
//! where the sun is, reads the time from here rather than from `chrono`
//! directly. Normally this is the wall clock and nothing else happens. With
//! `--clock` it starts at an hour that is not now, and with `--clock-speed`
//! it runs faster than it should, which is the only way to render an hour of
//! sky into eight seconds of pictures. Nothing else in the project uses it:
//! a log line, a scan stamp and a recently played time are real times and
//! stay real.

use chrono::{DateTime, Local, NaiveTime, TimeZone, Timelike};
use std::sync::OnceLock;

/// What was asked for on the command line, if anything.
struct Wound {
    /// The time of day the picture starts at.
    start: NaiveTime,
    /// How many seconds of clock pass per second of picture.
    speed: f32,
}

static WOUND: OnceLock<Wound> = OnceLock::new();

/// Set the clock for this run. `HH:MM` or `HH:MM:SS`, and a speed of 1.0 is
/// a clock that keeps time. Called once, from the argument parsing.
pub fn wind(start: &str, speed: f32) -> Result<(), String> {
    let time = NaiveTime::parse_from_str(start, "%H:%M:%S")
        .or_else(|_| NaiveTime::parse_from_str(start, "%H:%M"))
        .map_err(|_| format!("{start}: not a time of day, want HH:MM"))?;
    let _ = WOUND.set(Wound {
        start: time,
        speed: speed.max(0.0),
    });
    Ok(())
}

/// The time to draw, `secs` being how long the launcher has been running.
///
/// Without `--clock` the argument is ignored and this is the wall clock, so
/// a screen that asks every frame gets what a clock on the wall would say.
pub fn now(secs: f64) -> DateTime<Local> {
    let Some(w) = WOUND.get() else {
        return Local::now();
    };
    let today = Local::now().date_naive();
    let from = today.and_time(w.start);
    let moved = from + chrono::Duration::milliseconds((secs * w.speed as f64 * 1000.0) as i64);
    Local
        .from_local_datetime(&moved)
        .single()
        .unwrap_or_else(Local::now)
}

/// Minutes since midnight, which is what the sky needs to place the sun.
pub fn minutes(secs: f64) -> u32 {
    let t = now(secs);
    t.hour() * 60 + t.minute()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wound_clock_runs_at_the_speed_it_was_given() {
        // The static can only be set once per process, so this test owns it.
        wind("05:30", 600.0).expect("a time of day");
        assert_eq!(minutes(0.0), 5 * 60 + 30);
        // Ten seconds of pictures at six hundred times is an hour and forty
        // minutes of sky.
        assert_eq!(minutes(10.0), 5 * 60 + 30 + 100);
        // And a speed of nothing holds the clock still.
        assert_eq!(minutes(0.0), minutes(0.0));
    }

    #[test]
    fn a_time_that_is_not_one_is_refused() {
        assert!(NaiveTime::parse_from_str("25:00", "%H:%M").is_err());
        assert!(NaiveTime::parse_from_str("half five", "%H:%M").is_err());
    }
}
