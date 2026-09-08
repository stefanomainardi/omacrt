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
    /// "Milan 14C partly cloudy", already put together.
    pub weather: String,
    /// The next appointment, as "19:30 dinner", or empty.
    pub next: String,
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
pub fn weather(place: &str) -> String {
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
        return tidy_weather(&text);
    }
    // wttr.in's own one line format: "Milan: 🌦 +14°C". Asking for the
    // format rather than the whole forecast keeps this to a few bytes, and
    // there is nothing here that wants an emoji at eight pixels.
    let query = place.trim().replace(' ', "+");
    let url = format!("https://wttr.in/{query}?format=%l:+%t+%C&m");
    match fetch(&url, &file) {
        Some(text) => tidy_weather(&text),
        None => std::fs::read_to_string(&file)
            .map(|t| tidy_weather(&t))
            .unwrap_or_default(),
    }
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
    Info {
        weather: weather(place),
        next: next_event(calendar),
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
