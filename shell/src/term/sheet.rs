//! The shape every other command takes in a terminal.
//!
//! The self test opens with the wordmark because it is the one command that
//! opens something. Everywhere else the mark alone is the signature: six rows
//! of it, the command and what it did beside it, and then the fields in the
//! same vocabulary the report uses - the label in the accent colour, the
//! value in the paper one, the aside dim.
//!
//! A sheet is only built when there is somebody to look at it. Piped,
//! redirected, under `NO_COLOR` or `TERM=dumb`, every command prints the
//! lines it always printed, because that is what scripts read.

use super::{Palette, mark};
use ratatui::style::{Color as TColor, Modifier, Style};
use ratatui::text::{Line, Span};

/// A page of fields under the mark.
pub struct Sheet {
    pal: Palette,
    lines: Vec<Line<'static>>,
    /// The width of the label column, so the values line up.
    label_width: usize,
}

impl Sheet {
    /// Start a sheet, or nothing at all when the output is not a terminal.
    ///
    /// `title` is the command as it was typed, `subtitle` one line of what
    /// the answer amounts to.
    pub fn open(plain: bool, title: &str, subtitle: &str) -> Option<Self> {
        if !super::interactive(plain) {
            return None;
        }
        let pal = Palette::load();
        let mut s = Sheet {
            pal,
            lines: Vec::new(),
            label_width: 10,
        };
        s.plaque(title, subtitle);
        Some(s)
    }

    /// The mark, with the command and its one line beside it.
    fn plaque(&mut self, title: &str, subtitle: &str) {
        let beside: Vec<Vec<Span<'static>>> = vec![
            vec![],
            vec![
                Span::styled("omacrt ", self.style(self.pal.theme.dim)),
                Span::styled(title.to_string(), self.bold(self.pal.theme.paper)),
            ],
            vec![Span::styled(
                subtitle.to_string(),
                self.style(self.pal.theme.cyan),
            )],
        ];
        self.lines.push(Line::raw(""));
        for (i, row) in mark::rows(mark::SIZE, crate::assets::RETRACE.rest)
            .iter()
            .enumerate()
        {
            let mut spans = vec![Span::raw("  ")];
            spans.extend(super::rich::mark_spans_with(
                self.pal.theme.green,
                self.pal.theme.paper,
                self.pal.truecolor,
                row,
            ));
            spans.push(Span::raw("   "));
            if let Some(rest) = beside.get(i) {
                spans.extend(rest.clone());
            }
            self.lines.push(Line::from(spans));
        }
        self.lines.push(Line::raw(""));
    }

    /// A field: the name of a thing and what it is.
    pub fn field(&mut self, label: &str, value: impl Into<String>) {
        let w = self.label_width;
        self.lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{:<w$}", label.to_uppercase()),
                self.bold(self.pal.theme.accent),
            ),
            Span::styled(value.into(), self.style(self.pal.theme.paper)),
        ]));
    }

    /// A field whose value carries an aside: the aside is quieter.
    pub fn field_note(&mut self, label: &str, value: impl Into<String>, note: impl Into<String>) {
        let w = self.label_width;
        self.lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{:<w$}", label.to_uppercase()),
                self.bold(self.pal.theme.accent),
            ),
            Span::styled(value.into(), self.style(self.pal.theme.paper)),
            Span::raw("  "),
            Span::styled(note.into(), self.style(self.pal.theme.dim)),
        ]));
    }

    /// A field with a lamp: green when the thing is up, red when it is not.
    pub fn lamp(&mut self, label: &str, on: bool, value: impl Into<String>) {
        let w = self.label_width;
        let colour = if on {
            self.pal.theme.bright_green
        } else {
            self.pal.theme.red
        };
        self.lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{:<w$}", label.to_uppercase()),
                self.bold(self.pal.theme.accent),
            ),
            Span::styled("███", self.style(colour)),
            Span::raw("  "),
            Span::styled(value.into(), self.style(self.pal.theme.paper)),
        ]));
    }

    /// A row of a table under a field, indented past the label column.
    pub fn row(&mut self, left: &str, middle: &str, right: &str) {
        let w = self.label_width;
        self.lines.push(Line::from(vec![
            Span::raw("  "),
            Span::raw(" ".repeat(w)),
            Span::styled(left.to_string(), self.style(self.pal.theme.paper)),
            Span::styled(middle.to_string(), self.style(self.pal.theme.cyan)),
            Span::styled(right.to_string(), self.style(self.pal.theme.dim)),
        ]));
    }

    /// A row that answers yes or no, in the self test's vocabulary.
    pub fn check(&mut self, level: super::Level, label: &str, note: impl Into<String>) {
        let (tag, colour) = match level {
            super::Level::Ok => ("OK  ", self.pal.theme.green),
            super::Level::Warn => ("WARN", self.pal.theme.yellow),
            super::Level::Fail => ("FAIL", self.pal.theme.red),
        };
        self.lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(tag.to_string(), self.bold(colour)),
            Span::raw("  "),
            Span::styled(label.to_string(), self.style(self.pal.theme.paper)),
            Span::raw("  "),
            Span::styled(note.into(), self.style(self.pal.theme.dim)),
        ]));
    }

    /// A verb and what it does, for the help.
    ///
    /// The command is the part somebody types the same way every time; what
    /// follows it is theirs to fill in. They are coloured differently for
    /// that reason and no other.
    pub fn verb(&mut self, invocation: &str, what: &str) {
        let col = 34usize;
        // The leading words that are the command: plain lowercase letters,
        // up to the first thing with a bracket or a choice in it.
        let mut command = String::new();
        for word in invocation.split(' ') {
            if word.is_empty() || !word.chars().all(|c| c.is_ascii_lowercase()) {
                break;
            }
            if !command.is_empty() {
                command.push(' ');
            }
            command.push_str(word);
        }
        let rest = invocation[command.len()..].trim_start().to_string();
        let mut spans = vec![
            Span::raw("  "),
            Span::styled(command, self.bold(self.pal.theme.paper)),
        ];
        if !rest.is_empty() {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(rest, self.style(self.pal.theme.cyan)));
        }
        if what.is_empty() {
            self.lines.push(Line::from(spans));
            return;
        }
        // A long invocation takes the line to itself and the description goes
        // under it, at the column the others use.
        if invocation.chars().count() > col - 2 {
            self.lines.push(Line::from(spans));
            self.lines.push(Line::from(vec![
                Span::raw(" ".repeat(col + 2)),
                Span::styled(what.to_string(), self.style(self.pal.theme.dim)),
            ]));
            return;
        }
        spans.push(Span::raw(" ".repeat(col - invocation.chars().count())));
        spans.push(Span::styled(
            what.to_string(),
            self.style(self.pal.theme.dim),
        ));
        self.lines.push(Line::from(spans));
    }

    /// The name of a group of rows.
    pub fn section(&mut self, name: &str) {
        self.lines.push(Line::raw(""));
        self.lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(name.to_uppercase(), self.bold(self.pal.theme.accent)),
        ]));
    }

    /// A line of a file, shown as it is: a comment quiet, a key set apart
    /// from its value, a section head in the accent colour.
    pub fn raw(&mut self, line: &str) {
        let t = line.trim_start();
        let spans = if t.starts_with('#') {
            vec![
                Span::raw("  "),
                Span::styled(line.to_string(), self.style(self.pal.theme.dim)),
            ]
        } else if t.starts_with('[') {
            vec![
                Span::raw("  "),
                Span::styled(line.to_string(), self.bold(self.pal.theme.accent)),
            ]
        } else if let Some((key, value)) = line.split_once('=') {
            vec![
                Span::raw("  "),
                Span::styled(key.to_string(), self.style(self.pal.theme.paper)),
                Span::raw("="),
                Span::styled(value.to_string(), self.style(self.pal.theme.cyan)),
            ]
        } else {
            vec![
                Span::raw("  "),
                Span::styled(line.to_string(), self.style(self.pal.theme.paper)),
            ]
        };
        self.lines.push(Line::from(spans));
    }

    /// A line of nothing, to separate one group of fields from the next.
    pub fn blank(&mut self) {
        self.lines.push(Line::raw(""));
    }

    /// A quiet line: what to do next, or what was not there to report.
    pub fn note(&mut self, text: impl Into<String>) {
        self.lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(text.into(), self.style(self.pal.theme.dim)),
        ]));
    }

    /// Write it out, where it stays.
    pub fn print(mut self) {
        self.lines.push(Line::raw(""));
        super::rich::print_lines(&self.lines);
    }

    fn style(&self, c: crate::colour::Color) -> Style {
        Style::default().fg(super::rich::conv(c, self.pal.truecolor))
    }

    fn bold(&self, c: crate::colour::Color) -> Style {
        self.style(c).add_modifier(Modifier::BOLD)
    }
}

/// The colour a terminal is given for a value, for callers that build their
/// own spans.
pub fn colour(c: crate::colour::Color, truecolor: bool) -> TColor {
    super::rich::conv(c, truecolor)
}
