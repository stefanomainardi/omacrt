//! How the command line looks.
//!
//! Two renderings of the same thing. In a terminal, `doctor` is the
//! launcher's own power on self test: the wordmark cut by a laser while the
//! machine is examined, the checks arriving in the vocabulary of a 1994 BIOS,
//! and the timings drawn rather than listed. Anywhere else, and that means a
//! pipe, a script, `NO_COLOR`, a terminal that says it is dumb, or `--plain`,
//! it is the same plain lines it has always printed, because a report that
//! changes shape when you redirect it is a report nobody can use twice.
//!
//! The rule the drawing follows: every drawn thing carries a fact. The etch
//! advances because a check finished, the gauge fills because work is being
//! done, the diagram is the modeline that is really configured. Nothing here
//! is animation for its own sake.

use crate::theme::Theme;
use std::io::IsTerminal;

pub mod etch;
pub mod mark;
pub mod rich;
pub mod sheet;

/// What a check found. `Warn` and `Fail` both count as a failure for the exit
/// code, exactly as before: the distinction is how it reads, not what it
/// means to a script.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

impl Level {
    pub fn ok(self) -> bool {
        self == Level::Ok
    }

    /// The four characters `doctor` has always started a line with.
    pub fn plain_tag(self) -> &'static str {
        match self {
            Level::Ok => "OK  ",
            _ => "FAIL",
        }
    }

    pub fn post_tag(self) -> &'static str {
        match self {
            Level::Ok => "OK",
            Level::Warn => "??",
            Level::Fail => "FAIL",
        }
    }
}

/// One question, and the answer once it has been asked.
pub struct Check {
    pub section: &'static str,
    pub label: String,
    pub level: Level,
    pub note: String,
}

/// One question, not yet asked. The closure runs on a worker thread so the
/// picture keeps moving while a subprocess takes its time.
pub struct Probe {
    pub section: &'static str,
    pub label: String,
    pub run: Box<dyn FnOnce() -> (Level, String) + Send>,
}

impl Probe {
    pub fn new(
        section: &'static str,
        label: impl Into<String>,
        run: impl FnOnce() -> (Level, String) + Send + 'static,
    ) -> Self {
        Self {
            section,
            label: label.into(),
            run: Box::new(run),
        }
    }

    /// The common case: a yes or no with a line of detail.
    pub fn yes_no(
        section: &'static str,
        label: impl Into<String>,
        run: impl FnOnce() -> (bool, String) + Send + 'static,
    ) -> Self {
        Self::new(section, label, move || {
            let (ok, note) = run();
            (if ok { Level::Ok } else { Level::Fail }, note)
        })
    }
}

/// What the POST calls itself.
pub const NAME: &str = "OMACRT";

pub const MACHINE: &str = "MACHINE";
pub const TELEVISION: &str = "TELEVISION";
pub const PROGRAMS: &str = "PROGRAMS";
pub const COLLECTION: &str = "COLLECTION";
pub const HOUSEKEEPING: &str = "HOUSEKEEPING";

/// Whether to draw or to print.
///
/// A terminal is not enough on its own: somebody who has asked for no colour,
/// or whose terminal cannot do it, or who piped us into `grep`, wants the
/// lines and not the picture.
pub fn interactive(plain_flag: bool) -> bool {
    if plain_flag || !std::io::stdout().is_terminal() {
        return false;
    }
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    match std::env::var("TERM") {
        Ok(t) => t != "dumb" && !t.is_empty(),
        Err(_) => false,
    }
}

/// True when the terminal will take 24 bit colour. Everything else is given
/// the sixteen it is sure to have.
pub fn truecolor() -> bool {
    matches!(
        std::env::var("COLORTERM").as_deref(),
        Ok("truecolor") | Ok("24bit")
    )
}

/// The colours of the television, for the terminal.
pub struct Palette {
    pub theme: Theme,
    pub truecolor: bool,
}

impl Palette {
    pub fn load() -> Self {
        let theme = Theme::default_path()
            .and_then(|p| Theme::load(&p))
            .unwrap_or_else(Theme::tokyo_night);
        Self {
            theme,
            truecolor: truecolor(),
        }
    }
}

/// The wordmark at half its size, drawn with quadrants.
///
/// The drawing in `assets/wordmark.txt` is sixty-eight columns of block
/// characters, and a block character is two pixels tall: twenty pixel rows by
/// sixty-eight, in square pixels. Half of that on the screen is thirty-four
/// columns by five rows, and there are two ways to get there.
///
/// Averaging every two by two square down to one pixel is the obvious one and
/// it ruins the word: the strokes are two pixels wide, so half of every
/// letter goes and what is left is mush. Instead the picture is halved
/// vertically only - a row survives if either of the two rows it stands for
/// had anything - and then drawn with the quadrant characters, which hold
/// four pixels in a cell. Every column of the original survives, which is
/// where the letterforms live, and the word is still readable at half size.
pub fn wordmark_half() -> Vec<String> {
    let art: Vec<Vec<char>> = crate::assets::WORDMARK_TXT
        .lines()
        .map(|l| l.chars().collect())
        .collect();
    let cols = art.iter().map(|l| l.len()).max().unwrap_or(0);
    // Two pixel rows per character row of the source.
    let px: Vec<Vec<bool>> = art
        .iter()
        .flat_map(|line| {
            let at = |c: usize| line.get(c).copied().unwrap_or(' ');
            let top: Vec<bool> = (0..cols).map(|c| matches!(at(c), '█' | '▀')).collect();
            let bottom: Vec<bool> = (0..cols).map(|c| matches!(at(c), '█' | '▄')).collect();
            [top, bottom]
        })
        .collect();
    // Half as tall, every column kept.
    let short: Vec<Vec<bool>> = (0..px.len() / 2)
        .map(|y| {
            (0..cols)
                .map(|x| px[y * 2][x] || px[y * 2 + 1][x])
                .collect()
        })
        .collect();
    let get = |y: usize, x: usize| {
        short
            .get(y)
            .and_then(|r| r.get(x))
            .copied()
            .unwrap_or(false)
    };
    (0..short.len().div_ceil(2))
        .map(|r| {
            (0..cols.div_ceil(2))
                .map(|c| {
                    quadrant(
                        get(r * 2, c * 2),
                        get(r * 2, c * 2 + 1),
                        get(r * 2 + 1, c * 2),
                        get(r * 2 + 1, c * 2 + 1),
                    )
                })
                .collect()
        })
        .collect()
}

/// The character that lights those four corners of a cell.
fn quadrant(tl: bool, tr: bool, bl: bool, br: bool) -> char {
    match (tl, tr, bl, br) {
        (false, false, false, false) => ' ',
        (true, false, false, false) => '▘',
        (false, true, false, false) => '▝',
        (true, true, false, false) => '▀',
        (false, false, true, false) => '▖',
        (true, false, true, false) => '▌',
        (false, true, true, false) => '▞',
        (true, true, true, false) => '▛',
        (false, false, false, true) => '▗',
        (true, false, false, true) => '▚',
        (false, true, false, true) => '▐',
        (true, true, false, true) => '▜',
        (false, false, true, true) => '▄',
        (true, false, true, true) => '▙',
        (false, true, true, true) => '▟',
        (true, true, true, true) => '█',
    }
}

/// How wide the terminal is, or eighty when there is nobody to ask.
pub fn width() -> usize {
    ratatui::crossterm::terminal::size()
        .ok()
        .map(|(cols, _)| cols as usize)
        .filter(|c| *c > 20)
        .unwrap_or(80)
}

/// A path cut to fit on one line, keeping the end.
///
/// A progress line is erased and rewritten in place, and `\x1b[2K` erases
/// one line: anything long enough to wrap leaves the rest of itself on the
/// screen for ever. Which is how a scan of a collection printed a thousand
/// folder names down the terminal. The end of a path is the part that says
/// where the scan has got to, so the front is what goes.
pub fn fit(text: &str, max: usize) -> String {
    let n = text.chars().count();
    if n <= max || max < 4 {
        return text.to_string();
    }
    let keep = max - 1;
    let tail: String = text.chars().skip(n - keep).collect();
    format!("\u{2026}{tail}")
}

/// A string with nothing in it that a terminal will act on.
///
/// Some of what a check reports comes from outside: the name a device wrote
/// into its own EDID, a line another program printed. An escape sequence in
/// there can repaint the line it is on, and a report that can be repainted
/// by the thing it is reporting on is worth nothing. The drawn version is
/// safe already, because a cell in a buffer holds a character and not a
/// command; this is the plain one.
pub fn printable(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// The lines `doctor` has always printed, unchanged.
///
/// Byte for byte what came before, because scripts read it, CI reads it, and
/// the floating terminal in the desktop menu reads it over somebody's
/// shoulder. The new sections exist only in the drawn version.
pub fn plain(probes: Vec<Probe>) -> Vec<Check> {
    let mut done: Vec<Check> = Vec::with_capacity(probes.len());
    let labels: Vec<usize> = probes.iter().map(|p| p.label.len()).collect();
    let width = labels.iter().copied().max().unwrap_or(10);
    for p in probes {
        let (level, note) = (p.run)();
        println!(
            "{} {:<width$}  {}",
            level.plain_tag(),
            p.label,
            printable(&note)
        );
        done.push(Check {
            section: p.section,
            label: p.label,
            level,
            note,
        });
    }
    done
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_long_line_is_cut_to_the_width_and_keeps_its_end() {
        let long = "/run/media/somebody/External HD/roms/arcade/Horizontal Games/00";
        let cut = super::fit(long, 30);
        assert_eq!(cut.chars().count(), 30);
        assert!(cut.starts_with('\u{2026}'));
        assert!(cut.ends_with("Games/00"));
        // Short enough to fit is left alone.
        assert_eq!(super::fit("nes", 30), "nes");
    }

    use super::*;

    /// What a check reports is not always ours: a device names itself in its
    /// own EDID, and another program's output is quoted verbatim. A report
    /// that the subject can repaint is not a report.
    #[test]
    fn nothing_a_check_reports_can_drive_the_terminal() {
        let hostile = "\u{1b}[2K\rOK   everything is fine\u{7}";
        let safe = printable(hostile);
        assert!(!safe.chars().any(|c| c.is_control()), "{safe:?}");
        assert!(safe.contains("everything is fine"), "the text is kept");
        assert_eq!(printable("MORTACA DEV00"), "MORTACA DEV00");
    }
}
