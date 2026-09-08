//! The two things an ambient screen needs besides the time: what it is doing
//! outside, and what is next in the calendar.
//!
//! Both are optional and both are cached, because a photo frame that stops to
//! wait on the network is not a photo frame. The weather comes from wttr.in,
//! which needs no key and guesses the place from the address when none is
//! given. The calendar is any `.ics` a server will hand over, which is what
//! Nextcloud, Google and Fastmail all offer for a single calendar.

use std::path::{Path, PathBuf};

/// What to show along the bottom of an ambient screen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Info {
    /// "Milano: 22C Patchy rain nearby", already put together.
    pub weather: String,
    /// The next appointment, as "19:30 dinner", or empty.
    pub next: String,
    /// The same reading in parts, for the page that draws it.
    pub sky: Reading,
}

/// What the sky is doing, in the parts a drawing needs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reading {
    pub kind: Kind,
    /// Where, shortened to something that fits: "Milano".
    pub place: String,
    /// Degrees celsius.
    pub temp: Option<f32>,
    /// The server's own words: "Patchy rain nearby".
    pub condition: String,
    pub wind_kmh: Option<f32>,
    /// Sunrise and sunset as minutes since midnight, for the sun's arc.
    pub sunrise: Option<u32>,
    pub sunset: Option<u32>,
    /// Is there a reading at all, or is this the empty default?
    pub known: bool,
}

/// The weather, in the few kinds a 240 line picture can tell apart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Clear,
    Partly,
    Cloudy,
    Overcast,
    Fog,
    Rain,
    Heavy,
    Snow,
    Thunder,
}

/// The server's words for the sky, sorted into the kinds a picture can draw.
///
/// wttr.in speaks World Weather Online's vocabulary, which is a long list of
/// phrases built out of a few words. Reading the words rather than matching
/// the phrases is what keeps this from being a table of two hundred lines,
/// and the order matters: "thundery outbreaks in nearby" is thunder before it
/// is rain, and "heavy snow" is snow before it is heavy.
pub fn classify(condition: &str) -> Kind {
    let c = condition.to_ascii_lowercase();
    let has = |w: &str| c.contains(w);
    if has("thunder") || has("thundery") {
        return Kind::Thunder;
    }
    if has("snow") || has("sleet") || has("blizzard") || has("ice pellets") {
        return Kind::Snow;
    }
    if has("torrential") || has("heavy rain") || has("heavy freezing") {
        return Kind::Heavy;
    }
    if has("rain") || has("drizzle") || has("shower") {
        return Kind::Rain;
    }
    if has("fog") || has("mist") || has("freezing fog") {
        return Kind::Fog;
    }
    if has("overcast") {
        return Kind::Overcast;
    }
    if has("cloudy") && has("partly") {
        return Kind::Partly;
    }
    if has("cloudy") || has("cloud") {
        return Kind::Cloudy;
    }
    Kind::Clear
}

/// "Milano" out of "Milano", and "Brussels" out of
/// ", Brussels Capital, BE": the server puts a town, a region and a country
/// in one field and sometimes leaves the town out.
pub fn tidy_place(raw: &str) -> String {
    let parts: Vec<&str> = raw.split(',').map(|p| p.trim()).collect();
    let first = parts.iter().find(|p| !p.is_empty()).copied().unwrap_or("");
    // A country code on its own says nothing; a region does.
    first.chars().take(20).collect()
}

/// "06:52:59" as minutes since midnight.
fn minutes_of(time: &str) -> Option<u32> {
    let mut parts = time.trim().split(':');
    let h: u32 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    if h > 23 || m > 59 {
        return None;
    }
    Some(h * 60 + m)
}

/// A number out of a field that carries decoration: "+22°C", "↘4km/h".
fn number_in(field: &str) -> Option<f32> {
    let mut seen = String::new();
    for ch in field.chars() {
        if ch.is_ascii_digit() || ch == '.' || (ch == '-' && seen.is_empty()) {
            seen.push(ch);
        } else if !seen.is_empty() {
            break;
        }
    }
    seen.parse().ok()
}

/// The reading out of the line wttr.in was asked for:
/// `place|temp|condition|wind|precipitation|moon|sunrise|sunset`.
pub fn parse_reading(line: &str) -> Reading {
    let f: Vec<&str> = line.trim().split('|').collect();
    if f.len() < 3 {
        return Reading::default();
    }
    let condition = f[2].trim().to_string();
    Reading {
        kind: classify(&condition),
        place: tidy_place(f[0]),
        temp: number_in(f[1]),
        condition,
        wind_kmh: f.get(3).and_then(|w| number_in(w)),
        sunrise: f.get(6).and_then(|t| minutes_of(t)),
        sunset: f.get(7).and_then(|t| minutes_of(t)),
        known: true,
    }
}

impl Reading {
    /// The one line version, for a caption or the frame's ambient panel.
    pub fn line(&self) -> String {
        if !self.known {
            return String::new();
        }
        let mut out = String::new();
        if !self.place.is_empty() {
            out.push_str(&self.place);
            out.push_str(": ");
        }
        if let Some(t) = self.temp {
            out.push_str(&format!("{t:.0}C "));
        }
        out.push_str(&self.condition);
        out.trim().to_string()
    }

    /// Is the sun up, at this many minutes past midnight? With no times from
    /// the server, the answer is the usual daylight of a temperate place.
    pub fn daylight(&self, minutes: u32) -> bool {
        match (self.sunrise, self.sunset) {
            (Some(up), Some(down)) => minutes >= up && minutes < down,
            _ => (7 * 60..19 * 60).contains(&minutes),
        }
    }

    /// How far through the day it is, 0 at sunrise and 1 at sunset, for the
    /// sun's place in its arc. Outside daylight this is the same fraction of
    /// the night, for the moon.
    pub fn arc(&self, minutes: u32) -> f32 {
        let (up, down) = (
            self.sunrise.unwrap_or(7 * 60) as f32,
            self.sunset.unwrap_or(19 * 60) as f32,
        );
        let now = minutes as f32;
        if self.daylight(minutes) {
            ((now - up) / (down - up).max(1.0)).clamp(0.0, 1.0)
        } else {
            // The night wraps midnight, so it is measured from sunset.
            let night = (24.0 * 60.0 - down) + up;
            let since = if now >= down {
                now - down
            } else {
                now + (24.0 * 60.0 - down)
            };
            (since / night.max(1.0)).clamp(0.0, 1.0)
        }
    }
}

fn cache() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".cache/omarchy-crt")
}

/// Is a cached file young enough to use?
fn fresh(path: &Path, max_age_secs: u64) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| {
            t.elapsed()
                .map(|e| e.as_secs() < max_age_secs)
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

fn fetch(url: &str, dest: &Path) -> Option<String> {
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).ok()?;
    }
    let out = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "20",
            "-A",
            "omarchy-crt",
            "--proto",
            "=http,https",
            "--proto-redir",
            "=http,https",
            "--max-filesize",
            "4194304",
            url,
        ])
        .output()
        .ok()?;
    if !out.status.success() || out.stdout.is_empty() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let _ = std::fs::write(dest, &text);
    Some(text)
}

/// One line of weather for `place`, or for wherever the address says when it
/// is empty. Cached for half an hour.
pub fn weather(place: &str) -> Reading {
    // The place is part of the cache's name: asking for Milan must not be
    // answered with the line that was fetched for wherever the address said.
    let slug: String = place
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let file = cache().join(format!(
        "weather{}.txt",
        if slug.is_empty() {
            String::new()
        } else {
            format!("-{slug}")
        }
    ));
    if fresh(&file, 1800)
        && let Ok(text) = std::fs::read_to_string(&file)
    {
        return parse_reading(&tidy_weather(&text));
    }
    // One line with the parts in it, rather than the whole forecast: the
    // place, the temperature, the condition, the wind, the rain, the moon and
    // the two times the sun crosses the horizon, in about sixty bytes.
    let query = place.trim().replace(' ', "+");
    let url = format!("https://wttr.in/{query}?format=%l|%t|%C|%w|%p|%m|%S|%s&m");
    let text = match fetch(&url, &file) {
        Some(text) => text,
        None => std::fs::read_to_string(&file).unwrap_or_default(),
    };
    parse_reading(&tidy_weather(&text))
}

/// wttr.in's line, cleaned up for an 8x8 font: no degree sign, no plus in
/// front of a positive temperature, no double spaces.
pub fn tidy_weather(raw: &str) -> String {
    let line = raw.lines().next().unwrap_or("").trim();
    let line = line.replace(['°', '+'], "");
    let line: String = line
        .chars()
        .filter(|c| c.is_ascii() && !c.is_control())
        .collect();
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The next appointment from an `.ics` calendar. Cached for ten minutes.
pub fn next_event(url: &str) -> String {
    if url.trim().is_empty() {
        return String::new();
    }
    let file = cache().join("calendar.ics");
    let text = if fresh(&file, 600) {
        std::fs::read_to_string(&file).unwrap_or_default()
    } else {
        fetch(url, &file).unwrap_or_else(|| std::fs::read_to_string(&file).unwrap_or_default())
    };
    let now = chrono::Local::now();
    next_from_ics(&text, &now.format("%Y%m%dT%H%M%S").to_string())
}

/// The first event that starts after `now`, as "HH:MM what".
///
/// This reads the shape of an `.ics` file and nothing more: folded lines are
/// joined, `DTSTART` is taken with or without a timezone, and repeating
/// events are left alone, because an ambient screen that is wrong about a
/// weekly meeting is worse than one that says nothing.
pub fn next_from_ics(text: &str, now: &str) -> String {
    let mut best: Option<(String, String)> = None;
    let mut start = String::new();
    let mut summary = String::new();
    let mut inside = false;
    for line in unfold(text) {
        if line == "BEGIN:VEVENT" {
            inside = true;
            start.clear();
            summary.clear();
            continue;
        }
        if !inside {
            continue;
        }
        if line == "END:VEVENT" {
            inside = false;
            if !start.is_empty() && start.as_str() > now {
                let better = best.as_ref().map(|(s, _)| start < *s).unwrap_or(true);
                if better {
                    best = Some((start.clone(), summary.clone()));
                }
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("DTSTART") {
            let value = rest.split_once(':').map(|(_, v)| v).unwrap_or("");
            // A date with no time is a whole day: treat it as starting at
            // midnight, which is what a calendar shows.
            start = if value.len() == 8 {
                format!("{value}T000000")
            } else {
                value.trim_end_matches('Z').to_string()
            };
        } else if let Some(rest) = line.strip_prefix("SUMMARY") {
            summary = rest
                .split_once(':')
                .map(|(_, v)| v)
                .unwrap_or("")
                .to_string();
        }
    }
    match best {
        Some((start, summary)) if start.len() >= 13 => {
            let time = format!("{}:{}", &start[9..11], &start[11..13]);
            let what = summary.replace("\\,", ",").replace("\\n", " ");
            if what.is_empty() {
                time
            } else {
                format!("{time} {what}")
            }
        }
        _ => String::new(),
    }
}

/// An `.ics` file wraps long lines and continues them with a space or a tab.
fn unfold(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        if (line.starts_with(' ') || line.starts_with('\t'))
            && let Some(last) = out.last_mut()
        {
            last.push_str(&line[1..]);
            continue;
        }
        out.push(line.to_string());
    }
    out
}

/// Both, ready for the screen.
pub fn info(place: &str, calendar: &str) -> Info {
    let sky = weather(place);
    Info {
        weather: sky.line(),
        next: next_event(calendar),
        sky,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_weather_line_loses_what_the_font_cannot_draw() {
        assert_eq!(
            tidy_weather("Milan: 🌦 +14°C Light rain\n"),
            "Milan: 14C Light rain"
        );
        assert_eq!(tidy_weather("Rome:  -2°C  Snow"), "Rome: -2C Snow");
        assert_eq!(tidy_weather(""), "");
    }

    #[test]
    fn a_reading_comes_out_of_the_one_line_the_server_sends() {
        let raw = tidy_weather("Milano|+22°C|Patchy rain nearby|↘4km/h|0.1mm|🌘|06:52:59|19:49:24");
        let r = parse_reading(&raw);
        assert!(r.known);
        assert_eq!(r.place, "Milano");
        assert_eq!(r.temp, Some(22.0));
        assert_eq!(r.condition, "Patchy rain nearby");
        assert_eq!(r.kind, Kind::Rain);
        assert_eq!(r.wind_kmh, Some(4.0));
        assert_eq!(r.sunrise, Some(6 * 60 + 52));
        assert_eq!(r.sunset, Some(19 * 60 + 49));
        assert_eq!(r.line(), "Milano: 22C Patchy rain nearby");
    }

    #[test]
    fn a_reading_below_zero_keeps_its_sign() {
        let r = parse_reading(&tidy_weather(
            "Oslo|-8°C|Light snow|↗9km/h|0.0mm|🌘|07:10:00|17:02:00",
        ));
        assert_eq!(r.temp, Some(-8.0));
        assert_eq!(r.kind, Kind::Snow);
    }

    #[test]
    fn nothing_at_all_is_not_a_reading() {
        let r = parse_reading("");
        assert!(!r.known);
        assert_eq!(r.line(), "");
        assert_eq!(r.kind, Kind::Clear);
    }

    #[test]
    fn the_words_for_the_sky_sort_into_pictures() {
        use Kind::*;
        for (words, want) in [
            ("Sunny", Clear),
            ("Clear", Clear),
            ("Partly cloudy", Partly),
            ("Cloudy", Cloudy),
            ("Overcast", Overcast),
            ("Mist", Fog),
            ("Freezing fog", Fog),
            ("Patchy rain nearby", Rain),
            ("Light drizzle", Rain),
            ("Moderate rain shower", Rain),
            ("Heavy rain at times", Heavy),
            ("Torrential rain shower", Heavy),
            ("Light snow", Snow),
            ("Heavy snow", Snow),
            ("Light sleet showers", Snow),
            ("Blizzard", Snow),
            ("Thundery outbreaks in nearby", Thunder),
            ("Moderate or heavy rain with thunder", Thunder),
        ] {
            assert_eq!(classify(words), want, "{words}");
        }
    }

    #[test]
    fn a_place_the_server_left_half_empty_still_has_a_name() {
        assert_eq!(tidy_place("Milano"), "Milano");
        assert_eq!(tidy_place(", Brussels Capital, BE"), "Brussels Capital");
        assert_eq!(
            tidy_place("Watermael-Boitsfort, Brussels, BE"),
            "Watermael-Boitsfort"
        );
        assert_eq!(tidy_place(""), "");
    }

    #[test]
    fn the_sun_is_up_between_the_two_times_the_server_gives() {
        let r = parse_reading("Milano|+22°C|Clear|4km/h|0mm|m|06:00:00|20:00:00");
        assert!(!r.daylight(5 * 60));
        assert!(r.daylight(13 * 60));
        assert!(!r.daylight(21 * 60));
        // Halfway between sunrise and sunset is the top of the arc.
        assert!((r.arc(13 * 60) - 0.5).abs() < 0.01);
        assert_eq!(r.arc(6 * 60), 0.0);
        // The night is measured from sunset, wrapping midnight: four hours
        // after sunset out of ten hours of darkness.
        assert!((r.arc(24 * 60) - 0.4).abs() < 0.01);
    }

    const ICS: &str = "BEGIN:VCALENDAR\r
BEGIN:VEVENT\r
DTSTART:20260908T090000Z\r
SUMMARY:standup\r
END:VEVENT\r
BEGIN:VEVENT\r
DTSTART;TZID=Europe/Rome:20260908T193000\r
SUMMARY:dinner with a very long name that the file\r
  wraps onto a second line\r
END:VEVENT\r
BEGIN:VEVENT\r
DTSTART;VALUE=DATE:20260101\r
SUMMARY:last year\r
END:VEVENT\r
END:VCALENDAR\r
";

    #[test]
    fn the_next_event_is_the_first_one_still_to_come() {
        // Before both of today's events: the earlier one wins.
        assert_eq!(next_from_ics(ICS, "20260908T070000"), "09:00 standup");
        // After the standup: the dinner, with its folded line joined.
        assert_eq!(
            next_from_ics(ICS, "20260908T100000"),
            "19:30 dinner with a very long name that the file wraps onto a second line"
        );
        // After everything: nothing to say.
        assert_eq!(next_from_ics(ICS, "20270101T000000"), "");
        // A calendar with nothing in it.
        assert_eq!(
            next_from_ics("BEGIN:VCALENDAR\nEND:VCALENDAR\n", "20260908T070000"),
            ""
        );
    }

    #[test]
    fn a_whole_day_event_starts_at_midnight() {
        let ics = "BEGIN:VEVENT\nDTSTART;VALUE=DATE:20260909\nSUMMARY:holiday\nEND:VEVENT\n";
        assert_eq!(next_from_ics(ics, "20260908T230000"), "00:00 holiday");
    }
}
