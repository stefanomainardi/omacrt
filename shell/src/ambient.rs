//! The two things an ambient screen needs besides the time: what it is doing
//! outside, and what is next in the calendar.
//!
//! Both are optional and both are cached, because a photo frame that stops to
//! wait on the network is not a photo frame. The weather comes from wttr.in,
//! which needs no key; with no place set it is asked about the city in the
//! machine's own timezone, which is a better guess than the one it would make
//! from the address. The calendar is any `.ics` a server will hand over, which is what
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
#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    pub kind: Kind,
    /// Where, shortened to something that fits: "Milano".
    pub place: String,
    /// Degrees celsius.
    pub temp: Option<f32>,
    /// The server's own words: "Patchy rain nearby".
    pub condition: String,
    pub wind_kmh: Option<f32>,
    /// How far through its month the moon is: 0 and 1 are new, 0.5 is full.
    /// The server sends the phase and the picture draws that one, so the moon
    /// on the television is the moon outside.
    pub moon: f32,
    /// Sunrise and sunset as minutes since midnight, for the sun's arc.
    pub sunrise: Option<u32>,
    pub sunset: Option<u32>,
    /// Is there a reading at all, or is this the empty default?
    pub known: bool,
}

impl Default for Reading {
    /// Nothing known yet. The moon is full rather than new, because a new
    /// moon is a moon you cannot see and the picture would look broken.
    fn default() -> Self {
        Self {
            kind: Kind::default(),
            place: String::new(),
            temp: None,
            condition: String::new(),
            wind_kmh: None,
            moon: 0.5,
            sunrise: None,
            sunset: None,
            known: false,
        }
    }
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
fn classify(condition: &str) -> Kind {
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
fn tidy_place(raw: &str) -> String {
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
fn parse_reading(line: &str) -> Reading {
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
        // Set by `read_reading` from the raw line, because the glyph does
        // not survive tidying.
        moon: 0.5,
        sunrise: f.get(6).and_then(|t| minutes_of(t)),
        sunset: f.get(7).and_then(|t| minutes_of(t)),
        known: true,
    }
}

/// The moon the server draws, as how far through its month it is: 0 and 1
/// are new, 0.5 is full.
///
/// wttr.in answers with one of the eight moon emoji somewhere in its line,
/// which is exactly the eight phases anybody names. Anything else, or
/// nothing, is read as full, because a picture of the sky needs *a* moon and
/// half of one is the least wrong guess.
fn moon_phase(line: &str) -> f32 {
    for (glyph, phase) in [
        ("\u{1f311}", 0.0),
        ("\u{1f312}", 0.125),
        ("\u{1f313}", 0.25),
        ("\u{1f314}", 0.375),
        ("\u{1f315}", 0.5),
        ("\u{1f316}", 0.625),
        ("\u{1f317}", 0.75),
        ("\u{1f318}", 0.875),
    ] {
        if line.contains(glyph) {
            return phase;
        }
    }
    0.5
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

    /// How dark the sky is: 1 in the night, 0 in daylight, and a ramp of
    /// forty minutes on either side of sunrise and sunset.
    ///
    /// `daylight` answers yes or no because the picture has to choose between
    /// a sun and a moon, but the stars do not vanish the instant the sun
    /// clears the horizon. Anything that fades needs this instead.
    pub fn darkness(&self, minutes: u32) -> f32 {
        const RAMP: f32 = 40.0;
        let (up, down) = (
            self.sunrise.unwrap_or(7 * 60) as f32,
            self.sunset.unwrap_or(19 * 60) as f32,
        );
        let now = minutes as f32;
        if now < up {
            // Before sunrise: full dark until forty minutes out.
            ((up - now) / RAMP).clamp(0.0, 1.0)
        } else if now > down {
            ((now - down) / RAMP).clamp(0.0, 1.0)
        } else {
            // Daylight, with the same ramp on the way in and out of it.
            let from_dawn = (now - up) / RAMP;
            let to_dusk = (down - now) / RAMP;
            (1.0 - from_dawn.min(to_dusk)).clamp(0.0, 1.0)
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
    PathBuf::from(home).join(".cache/omacrt")
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
    let out = crate::net::curl(20, 4_194_304).arg(url).output().ok()?;
    if !out.status.success() || out.stdout.is_empty() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let _ = std::fs::write(dest, &text);
    Some(text)
}

/// The city in the machine's own timezone: "Europe/Brussels" is Brussels.
///
/// This is the answer to an empty weather setting. Asking wttr.in with no
/// place at all makes it guess from the address, and an address is a country
/// away the moment there is a VPN in the way, while the timezone is where the
/// machine thinks it is. A zone with no region in it, `UTC`, is not a place
/// and gets no guess.
pub fn zone_place() -> String {
    if let Ok(tz) = std::env::var("TZ") {
        let named = place_from_zone(&tz);
        if !named.is_empty() {
            return named;
        }
    }
    match std::fs::read_link("/etc/localtime") {
        Ok(target) => place_from_zone(&target.to_string_lossy()),
        Err(_) => String::new(),
    }
}

/// "…/zoneinfo/America/New_York" as "New York".
fn place_from_zone(path: &str) -> String {
    let zone = match path.split_once("zoneinfo/") {
        Some((_, rest)) => rest,
        None => path,
    };
    let mut parts = zone.split('/');
    let region = parts.next().unwrap_or("");
    // The city is what follows the region, and there has to be a region:
    // `UTC` on its own is a rule about clocks, not a town.
    let city = match parts.next_back() {
        Some(city) if !region.is_empty() && !city.is_empty() => city,
        _ => return String::new(),
    };
    city.replace('_', " ")
}

/// One line of weather for `place`, or for the city in the machine's own
/// timezone when it is empty. Cached for half an hour.
pub fn weather(place: &str) -> Reading {
    let asked = place.trim().to_string();
    let place = if asked.is_empty() {
        zone_place()
    } else {
        asked
    };
    let place = place.as_str();
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
        return read_reading(&text);
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
    read_reading(&text)
}

/// One line from the server into a reading.
///
/// The moon is read from the raw line and everything else from the tidied
/// one, because tidying throws away anything the 8x8 font cannot draw and
/// the moon arrives as an emoji. It was worth one wasted afternoon to learn
/// that the phase was being filtered out before it was ever parsed.
fn read_reading(text: &str) -> Reading {
    let mut reading = parse_reading(&tidy_weather(text));
    reading.moon = moon_phase(text);
    reading
}

/// wttr.in's line, cleaned up for an 8x8 font: no degree sign, no plus in
/// front of a positive temperature, no double spaces.
fn tidy_weather(raw: &str) -> String {
    let line = raw.lines().next().unwrap_or("").trim();
    let line = line.replace(['°', '+'], "");
    let line: String = line
        .chars()
        .filter(|c| c.is_ascii() && !c.is_control())
        .collect();
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The next appointment from an `.ics` calendar. Cached for ten minutes.
fn next_event(url: &str) -> String {
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
fn next_from_ics(text: &str, now: &str) -> String {
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
        // `20260909T143000Z`: the four digits after the T are the time. The
        // value comes off a calendar somewhere on the network, so it is read
        // as bytes and checked for digits rather than sliced by byte offset -
        // one multibyte character in the field used to be a panic on the
        // thread that keeps the weather and the calendar up to date.
        Some((start, summary))
            if start.len() >= 13 && start.as_bytes()[9..13].iter().all(u8::is_ascii_digit) =>
        {
            let hhmm = &start.as_bytes()[9..13];
            let time = format!(
                "{}{}:{}{}",
                hhmm[0] as char, hhmm[1] as char, hhmm[2] as char, hhmm[3] as char
            );
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
    fn the_moon_survives_the_tidying() {
        // The phase arrives as an emoji and tidying throws away everything
        // the 8x8 font cannot draw, so it is read from the raw line. This is
        // the bug this test exists for.
        let raw = "Brussels|+12C|Clear|8km/h|0.0mm|\u{1f313}|07:06:54|20:14:11";
        let r = read_reading(raw);
        assert_eq!(r.moon, 0.25, "first quarter");
        assert_eq!(r.place, "Brussels");
        // Every glyph the server can send, and nothing else.
        assert_eq!(moon_phase("\u{1f311}"), 0.0);
        assert_eq!(moon_phase("\u{1f315}"), 0.5);
        assert_eq!(moon_phase("\u{1f318}"), 0.875);
        assert_eq!(
            moon_phase("no moon here"),
            0.5,
            "a guess, and a visible one"
        );
    }

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
    fn an_empty_setting_asks_about_the_city_in_the_timezone() {
        assert_eq!(
            place_from_zone("/usr/share/zoneinfo/Europe/Brussels"),
            "Brussels"
        );
        assert_eq!(place_from_zone("Europe/Rome"), "Rome");
        // An underscore stands in for the space in a name.
        assert_eq!(place_from_zone("America/New_York"), "New York");
        // Three parts: the city is still the last of them.
        assert_eq!(
            place_from_zone("America/Argentina/Buenos_Aires"),
            "Buenos Aires"
        );
        // A rule about clocks is not a town.
        assert_eq!(place_from_zone("/usr/share/zoneinfo/UTC"), "");
        assert_eq!(place_from_zone(""), "");
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
