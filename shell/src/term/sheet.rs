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
