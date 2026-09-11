//! `doctor` as the launcher's power on self test.
//!
//! Two phases, and the split is deliberate. While the machine is being asked
//! questions, an inline viewport at the bottom of the terminal cuts the
//! wordmark out of the dark with the laser, and the laser advances because a
//! check finished. When the questions run out the viewport is cleared and the
//! report is written into the scrollback above it, in final colours, where it
//! can be scrolled back to, read twice and pasted into an issue.
//!
//! The animation is never the artefact. What survives the command is text.

use super::etch::Etch;
use super::mark;
use super::{Check, Level, Palette, Probe};
use crate::colour::{Color, lerp_color};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::layout::Rect;
use ratatui::style::{Color as TColor, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use std::io::{IsTerminal, stdout};
use std::sync::mpsc;
use std::time::Duration;

/// The timing of the television, drawn rather than listed.
pub struct Timing {
    pub name: String,
    pub clock_mhz: f64,
    /// active, sync start, sync end, total
    pub h: [u32; 4],
    pub v: [u32; 4],
    pub khz: f64,
    pub hz: f64,
}

/// What an output is doing, for the map.
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Role {
    Desktop,
    Tube,
    Free,
    Disconnected,
}

pub struct Output {
    pub name: String,
    pub edid: String,
    pub role: Role,
}

/// The pieces of the report that are pictures rather than rows.
#[derive(Default)]
pub struct Extras {
    pub timing: Option<Timing>,
    pub outputs: Vec<Output>,
    /// Whether the DAC has a lock, and what its sync mode is.
    pub lock: Option<(bool, String)>,
}

/// What the reader may press when the report is on the screen.
pub struct Action {
    pub key: char,
    pub what: &'static str,
}

/// Raw mode, given back whatever happens.
///
/// In raw mode a terminal does not echo, does not translate newlines and
/// does not turn Ctrl+C into a signal. A panic between turning it on and
/// turning it off would leave somebody with a shell they cannot type into,
/// so it is turned off by a value going out of scope rather than by a line
/// of code at the end that a panic can skip.
struct RawMode(bool);

impl RawMode {
    fn on() -> Self {
        Self(enable_raw_mode().is_ok())
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        if self.0 {
            let _ = disable_raw_mode();
        }
    }
}

/// Below this many rows the terminal has no room for the picture.
const MIN_ROWS: u16 = 24;

fn conv(c: Color, truecolor: bool) -> TColor {
    let (r, g, b) = ((c >> 16) as u8, (c >> 8) as u8, c as u8);
    if truecolor {
        return TColor::Rgb(r, g, b);
    }
    let (rf, gf, bf) = (r as i32, g as i32, b as i32);
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    if max < 60 {
        return TColor::Black;
    }
    if max - min < 40 {
        return if max > 170 {
            TColor::White
        } else {
            TColor::Gray
        };
    }
    let bright = max > 170;
    if rf == max && gf > bf + 40 {
        return if bright {
            TColor::LightYellow
        } else {
            TColor::Yellow
        };
    }
    if rf == max && bf > gf + 40 {
        return if bright {
            TColor::LightMagenta
        } else {
            TColor::Magenta
        };
    }
    if rf == max {
        return if bright {
            TColor::LightRed
        } else {
            TColor::Red
        };
    }
    if gf == max && bf > rf + 40 {
        return if bright {
            TColor::LightCyan
        } else {
            TColor::Cyan
        };
    }
    if gf == max {
        return if bright {
            TColor::LightGreen
        } else {
            TColor::Green
        };
    }
    if bright {
        TColor::LightBlue
    } else {
        TColor::Blue
    }
}

struct Paint {
    pal: Palette,
}

impl Paint {
    fn style(&self, c: Color) -> Style {
        Style::default().fg(conv(c, self.pal.truecolor))
    }

    fn bold(&self, c: Color) -> Style {
        self.style(c).add_modifier(Modifier::BOLD)
    }

    fn tag(&self, level: Level) -> Span<'static> {
        let (text, colour) = match level {
            Level::Ok => ("  OK", self.pal.theme.green),
            Level::Warn => ("  ??", self.pal.theme.yellow),
            Level::Fail => ("FAIL", self.pal.theme.red),
        };
        Span::styled(text, self.bold(colour))
    }

    /// The ten rows of the wordmark, as the laser has left them.
    fn wordmark(&self, etch: &Etch) -> Vec<Line<'static>> {
        let beam = etch.beam();
        (0..etch.rows)
            .map(|r| {
                let mut spans = vec![Span::raw("  ")];
                for c in 0..etch.cols {
                    if let Some((ch, colour)) = etch.cell(r, c) {
                        spans.push(Span::styled(ch.to_string(), self.style(colour)));
                    } else if let Some((_, _, bright)) =
                        beam.iter().find(|(br, bc, _)| *br == r && *bc == c)
                    {
                        let glyph = if *bright > 0.6 { "╱" } else { "·" };
                        spans.push(Span::styled(glyph, self.style(self.pal.theme.cyan)));
                    } else if etch
                        .sparks
                        .iter()
                        .any(|s| s.row.round() as i32 == r && s.col.round() as i32 == c)
                    {
                        spans.push(Span::styled("·", self.style(self.pal.theme.orange)));
                    } else {
                        spans.push(Span::raw(" "));
                    }
                }
                Line::from(spans)
            })
            .collect()
    }
}

/// Run the checks with the picture, then leave the report behind.
///
/// Three movements, and every one of them is paced by the machine rather
/// than by a clock.
///
/// The wordmark is cut while the first section is being asked, which is the
/// part that takes the longest: the compositor, the driver, the lease. When
/// it is cut it stays where it is, at the top of the report.
///
/// Then the answers land, one line at a time as they arrive, under a live
/// block at the bottom of the terminal: the mark, and the counter. The beam
/// runs back across the mark every time a check answers, so the one thing
/// moving is the one thing that means something.
///
/// Last, the block is replaced by what it was standing in for: the timings,
/// the outputs and the DAC's lock.
pub fn run(probes: Vec<Probe>, extras: Extras, actions: &[Action]) -> (Vec<Check>, Option<char>) {
    let pal = Palette::load();
    let stops = [pal.theme.magenta, pal.theme.cyan, pal.theme.paper];
    let paint = Paint { pal };
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(1)
        | 1;
    let mut etch = Etch::new(seed, stops);

    let total = probes.len();
    let (tx, rx) = mpsc::channel::<(usize, Check)>();
    let labels: Vec<String> = probes.iter().map(|p| p.label.clone()).collect();
    // The width of the label column, from the labels rather than from the
    // answers: a report that streams cannot measure what has not arrived.
    let width = labels.iter().map(|l| l.len()).max().unwrap_or(10);
    // How many checks the wordmark is cut by: the first section, which is
    // the machine itself and the slowest thing here to ask.
    let first_section = probes.first().map(|p| p.section).unwrap_or("");
    let etch_of = probes
        .iter()
        .filter(|p| p.section == first_section)
        .count()
        .max(1);
    std::thread::spawn(move || {
        for (i, p) in probes.into_iter().enumerate() {
            let (level, note) = (p.run)();
            let check = Check {
                section: p.section,
                label: p.label,
                level,
                note,
            };
            if tx.send((i, check)).is_err() {
                return;
            }
        }
    });

    let mut done: Vec<Option<Check>> = (0..total).map(|_| None).collect();
    let mut finished = 0usize;

    // The picture needs room, and it needs a keyboard: an inline viewport is
    // created by asking the terminal where its cursor is, which a terminal
    // with nothing on its input never answers.
    let short = ratatui::crossterm::terminal::size()
        .map(|(_, rows)| rows < MIN_ROWS)
        .unwrap_or(true)
        || !std::io::stdin().is_terminal();

    // Whatever happens to the picture, the answers are collected and the
    // report is written. A viewport that will not start is a reason to draw
    // less, never a reason to report nothing.
    let drain = |done: &mut Vec<Option<Check>>, finished: &mut usize, etch: &mut Etch| {
        while *finished < total {
            match rx.recv() {
                Ok((i, c)) => {
                    if let Some(slot) = done.get_mut(i) {
                        *slot = Some(c);
                        *finished += 1;
                    }
                }
                Err(_) => break,
            }
        }
        etch.tick(1.0, usize::MAX);
    };

    if short {
        drain(&mut done, &mut finished, &mut etch);
        let checks: Vec<Check> = done.into_iter().flatten().collect();
        report(&paint, &etch, &checks, &extras, actions);
        return (checks, None);
    }

    // The area at the bottom of the terminal is reserved by printing it once
    // and then moving back up over it each frame. ratatui's own inline
    // viewport asks the terminal where its cursor is and waits for the
    // answer, which a terminal being recorded, or piped through anything,
    // may never give; the report then came out empty. Moving a known number
    // of lines needs nobody's permission.
    let raw = RawMode::on();
    let frame = Duration::from_millis(33);
    // Set once the input has gone away: after that the frames are timed
    // rather than polled, so a recording still gets its animation.
    let mut quiet = false;
    // Set when the questions stop coming, which is a probe having panicked.
    // What was answered is still reported.
    let mut gone = false;
    // Ctrl+C: the report is skipped and the terminal handed back.
    let mut interrupted = false;
    // Somebody pressed a key: the pictures stop and the answers are taken as
    // fast as they arrive.
    let mut hurried = false;

    let mut live = 0usize; // rows of the block at the bottom, as last drawn
    let mut shown = 0usize; // answers already written into the scrollback
    let mut section = "";
    let mut retrace: Option<std::time::Instant> = None;
    let mut cutting = true; // the wordmark is still being cut

    loop {
        let mut answered = false;
        loop {
            match rx.try_recv() {
                Ok((i, c)) => {
                    if let Some(slot) = done.get_mut(i) {
                        *slot = Some(c);
                        finished += 1;
                        answered = true;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                // The thread that asks the questions has gone, which means a
                // probe panicked. Without this the loop waits at thirty
                // frames a second for answers that are not coming.
                Err(mpsc::TryRecvError::Disconnected) => {
                    gone = true;
                    break;
                }
            }
        }
        // The beam runs back when a check answers, and only then.
        if answered {
            retrace = Some(std::time::Instant::now());
        }

        let mut perm: Vec<Line<'static>> = Vec::new();
        if cutting {
            let target = if gone {
                1.0
            } else {
                (finished as f32 / etch_of as f32).min(1.0)
            };
            etch.tick(target, 6);
            // Cut and cooled: the head is printed once and never redrawn, so
            // it has to be printed in the colours the word keeps.
            if etch.cold() {
                cutting = false;
                perm.extend(head_block(&paint, &etch, crate::assets::RETRACE.rest, None));
            }
        }
        if !cutting {
            // One answer a frame: they arrive faster than an eye reads them,
            // and a report that appears all at once says nothing about the
            // machine having done any work.
            if shown < finished
                && let Some(c) = done.get(shown).and_then(|c| c.as_ref())
            {
                if c.section != section {
                    section = c.section;
                    perm.push(Line::raw(""));
                    perm.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(section.to_string(), paint.bold(paint.pal.theme.accent)),
                    ]));
                }
                perm.push(check_line(&paint, c, width));
                shown += 1;
            }
        }

        let block = if cutting {
            head_block(&paint, &etch, mark_base(retrace), Some((finished, etch_of)))
        } else {
            footer_block(
                &paint,
                shown,
                total,
                if shown < total {
                    labels.get(shown).cloned()
                } else {
                    Some(verdict(&done, total))
                },
            )
        };
        live = draw_block(&perm, &block, live);

        if !cutting && shown >= total {
            break;
        }
        if gone && shown >= finished && !cutting {
            break;
        }
        if hurried || quiet {
            std::thread::sleep(if hurried {
                Duration::from_millis(6)
            } else {
                frame
            });
        } else if event::poll(frame).unwrap_or(false) {
            match event::read() {
                // Ctrl+C in raw mode is a keypress, not a signal: it has to
                // be answered here or it does nothing.
                Ok(event::Event::Key(k))
                    if k.kind == KeyEventKind::Press
                        && k.code == KeyCode::Char('c')
                        && k.modifiers.contains(event::KeyModifiers::CONTROL) =>
                {
                    interrupted = true;
                    break;
                }
                Ok(event::Event::Key(k)) if k.kind == KeyEventKind::Press => {
                    // Somebody pressed a key: stop posing. The answers still
                    // arrive in order, six milliseconds a line rather than
                    // thirty-three.
                    hurried = true;
                    if cutting {
                        etch.tick(1.0, usize::MAX);
                    }
                }
                Ok(_) => {}
                Err(_) => quiet = true,
            }
        }
    }

    // Take the block away: what it was standing in for is printed under it.
    clear_block(live);
    drop(raw);
    let checks: Vec<Check> = done.into_iter().flatten().collect();
    if interrupted {
        println!();
        return (checks, None);
    }
    print_lines(&tail_lines(&paint, &extras, actions));
    let pressed = if actions.is_empty() {
        None
    } else {
        wait_for_key(actions)
    };
    (checks, pressed)
}

/// Where the cut is: travelling if a check has just answered, at rest if not.
fn mark_base(since: Option<std::time::Instant>) -> i32 {
    match since {
        Some(t) => mark::base_at(t.elapsed().as_secs_f32() / mark::LASTS),
        None => crate::assets::RETRACE.rest,
    }
}

/// What the answers came to, in one line.
fn verdict(done: &[Option<Check>], total: usize) -> String {
    let bad = done.iter().flatten().filter(|c| !c.level.ok()).count();
    if bad == 0 {
        format!("{total} checks, all clear")
    } else {
        format!("{total} checks, {bad} to look at")
    }
}

/// Print any new permanent lines, then redraw the block at the bottom.
///
/// The permanent lines go where the block's first row was, which pushes the
/// block down the terminal exactly as if they had been printed on their own.
/// Returns the number of rows the block now occupies.
fn draw_block(perm: &[Line<'static>], block: &[Line<'static>], live: usize) -> usize {
    use std::io::Write;
    let mut out = String::new();
    if live > 0 {
        out.push_str(&format!("\x1b[{live}A"));
    }
    for l in perm {
        out.push_str("\r\x1b[2K");
        out.push_str(&line_text(l));
        out.push_str("\r\n");
    }
    for l in block {
        out.push_str("\r\x1b[2K");
        out.push_str(&line_text(l));
        out.push_str("\r\n");
    }
    // The block shrank: wipe what it used to cover and step back over it.
    let extra = live.saturating_sub(block.len());
    for _ in 0..extra {
        out.push_str("\r\x1b[2K\r\n");
    }
    if extra > 0 {
        out.push_str(&format!("\x1b[{extra}A"));
    }
    let mut stdout = stdout().lock();
    let _ = stdout.write_all(out.as_bytes());
    let _ = stdout.flush();
    block.len()
}

/// Wipe the block at the bottom and leave the cursor where it started.
fn clear_block(live: usize) {
    use std::io::Write;
    if live == 0 {
        return;
    }
    let mut out = format!("\x1b[{live}A");
    for _ in 0..live {
        out.push_str("\r\x1b[2K\r\n");
    }
    out.push_str(&format!("\x1b[{live}A"));
    let mut stdout = stdout().lock();
    let _ = stdout.write_all(out.as_bytes());
    let _ = stdout.flush();
}

/// The head: the wordmark, and under it the mark beside what this is.
///
/// While the machine is being asked this is the live block, redrawn in
/// place: the laser cuts the word, the beam runs back across the mark every
/// time an answer lands, and the line beside it counts them. When the word
/// is cut and cold the same block is printed once, with the mark at rest and
/// nothing counting, and the answers start under it.
fn head_block(
    paint: &Paint,
    etch: &Etch,
    base: i32,
    asking: Option<(usize, usize)>,
) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = vec![Line::raw("")];
    out.extend(paint.wordmark(etch));
    out.push(Line::raw(""));
    let mut beside: Vec<Vec<Span<'static>>> = vec![
        vec![],
        vec![Span::styled(
            format!("{} BIOS {} / 15kHz", super::NAME, version()),
            paint.bold(paint.pal.theme.green),
        )],
        vec![Span::styled(
            "(C) 2026 OmaCRT, self test",
            paint.style(paint.pal.theme.dim),
        )],
        vec![],
        match asking {
            Some((n, of)) => vec![Span::styled(
                format!("asking the machine  {}/{of}", n.min(of)),
                paint.style(paint.pal.theme.cyan),
            )],
            None => vec![],
        },
    ];
    for (i, row) in mark::rows(mark::SIZE, base).iter().enumerate() {
        let mut spans = vec![Span::raw("  ")];
        spans.extend(mark_spans(paint, row));
        spans.push(Span::raw("   "));
        if let Some(rest) = beside.get_mut(i) {
            spans.append(rest);
        }
        out.push(Line::from(spans));
    }
    out
}

/// The block at the bottom while the answers land: how many have, and the
/// name of the one being asked.
fn footer_block(
    paint: &Paint,
    shown: usize,
    total: usize,
    current: Option<String>,
) -> Vec<Line<'static>> {
    let bar = 16usize;
    let filled = (shown * bar).checked_div(total).unwrap_or(bar);
    vec![
        Line::raw(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "█".repeat(filled),
                paint.style(paint.pal.theme.bright_green),
            ),
            Span::styled(
                "░".repeat(bar.saturating_sub(filled)),
                paint.style(paint.pal.theme.dim),
            ),
            Span::styled(
                format!("  {shown:>2}/{total}  "),
                paint.style(paint.pal.theme.dim),
            ),
            Span::styled(
                current.unwrap_or_default(),
                paint.style(paint.pal.theme.cyan),
            ),
        ]),
    ]
}

/// The mark's cells, in the colours the launcher gives them: the bars in
/// green, and the edge the cut has just left a shade warmer.
fn mark_spans(paint: &Paint, row: &[mark::Cell]) -> Vec<Span<'static>> {
    let warm = lerp_color(paint.pal.theme.green, paint.pal.theme.paper, 0.45);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut run = String::new();
    let mut hot = false;
    for c in row {
        if c.hot != hot && !run.is_empty() {
            let colour = if hot { warm } else { paint.pal.theme.green };
            spans.push(Span::styled(std::mem::take(&mut run), paint.style(colour)));
        }
        hot = c.hot;
        run.push(c.ch);
    }
    if !run.is_empty() {
        let colour = if hot { warm } else { paint.pal.theme.green };
        spans.push(Span::styled(run, paint.style(colour)));
    }
    spans
}

/// One answer.
fn check_line(paint: &Paint, c: &Check, width: usize) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        paint.tag(c.level),
        Span::raw("  "),
        Span::styled(
            format!("{:<width$}", c.label),
            paint.style(paint.pal.theme.paper),
        ),
        Span::raw("  "),
        Span::styled(
            c.note.clone(),
            paint.style(if c.level.ok() {
                paint.pal.theme.dim
            } else {
                paint.pal.theme.fg
            }),
        ),
    ])
}

/// Everything under the answers: the timings drawn, the outputs mapped, the
/// DAC's lock, and whatever there is to press.
fn tail_lines(paint: &Paint, extras: &Extras, actions: &[Action]) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    if let Some(t) = &extras.timing {
        out.push(Line::raw(""));
        out.extend(timing_lines(paint, t));
    }
    if !extras.outputs.is_empty() {
        out.push(Line::raw(""));
        out.extend(output_lines(paint, &extras.outputs));
    }
    if let Some((locked, sync)) = &extras.lock {
        out.push(Line::raw(""));
        out.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("DAC     ", paint.bold(paint.pal.theme.accent)),
            Span::styled(
                "███",
                paint.style(if *locked {
                    paint.pal.theme.bright_green
                } else {
                    paint.pal.theme.red
                }),
            ),
            Span::styled(
                format!(
                    "  {}  csync {sync}",
                    if *locked { "locked" } else { "lost" }
                ),
                paint.style(paint.pal.theme.paper),
            ),
        ]));
    }
    if !actions.is_empty() {
        out.push(Line::raw(""));
        let mut spans = vec![Span::raw("  ")];
        for a in actions {
            spans.push(Span::styled(
                format!("[{}]", a.key),
                paint.bold(paint.pal.theme.accent),
            ));
            spans.push(Span::styled(
                format!(" {}   ", a.what),
                paint.style(paint.pal.theme.paper),
            ));
        }
        out.push(Line::from(spans));
    }
    out.push(Line::raw(""));
    out
}

fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Write the report into the scrollback, where it stays.
fn report(paint: &Paint, etch: &Etch, checks: &[Check], extras: &Extras, actions: &[Action]) {
    let mut out = head_block(paint, etch, crate::assets::RETRACE.rest, None);
    let width = checks.iter().map(|c| c.label.len()).max().unwrap_or(10);
    let mut section = "";
    for c in checks {
        if c.section != section {
            section = c.section;
            out.push(Line::raw(""));
            out.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(section.to_string(), paint.bold(paint.pal.theme.accent)),
            ]));
        }
        out.push(Line::from(vec![
            Span::raw("  "),
            paint.tag(c.level),
            Span::raw("  "),
            Span::styled(
                format!("{:<width$}", c.label),
                paint.style(paint.pal.theme.paper),
            ),
            Span::raw("  "),
            Span::styled(
                c.note.clone(),
                paint.style(if c.level.ok() {
                    paint.pal.theme.dim
                } else {
                    paint.pal.theme.fg
                }),
            ),
        ]));
    }

    if let Some(t) = &extras.timing {
        out.push(Line::raw(""));
        out.extend(timing_lines(paint, t));
    }
    if !extras.outputs.is_empty() {
        out.push(Line::raw(""));
        out.extend(output_lines(paint, &extras.outputs));
    }
    if let Some((locked, sync)) = &extras.lock {
        out.push(Line::raw(""));
        out.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("DAC     ", paint.bold(paint.pal.theme.accent)),
            Span::styled(
                "███",
                paint.style(if *locked {
                    paint.pal.theme.bright_green
                } else {
                    paint.pal.theme.red
                }),
            ),
            Span::styled(
                format!(
                    "  {}  csync {sync}",
                    if *locked { "locked" } else { "lost" }
                ),
                paint.style(paint.pal.theme.paper),
            ),
        ]));
    }
    if !actions.is_empty() {
        out.push(Line::raw(""));
        let mut spans = vec![Span::raw("  ")];
        for a in actions {
            spans.push(Span::styled(
                format!("[{}]", a.key),
                paint.bold(paint.pal.theme.accent),
            ));
            spans.push(Span::styled(
                format!(" {}   ", a.what),
                paint.style(paint.pal.theme.paper),
            ));
        }
        out.push(Line::from(spans));
    }
    out.push(Line::raw(""));
    print_lines(&out);
}

/// The modeline, drawn: where the picture is, where the sync pulse sits, and
/// what is porch on either side of it. The numbers are the ones the tube is
/// configured with, so a diagram that disagrees with the television is a
/// diagram that is telling you something.
fn timing_lines(paint: &Paint, t: &Timing) -> Vec<Line<'static>> {
    let mut out = vec![Line::from(vec![
        Span::raw("  "),
        Span::styled("TIMING", paint.bold(paint.pal.theme.accent)),
        Span::styled(
            format!(
                "  {}  {:.0} MHz  {:.3} kHz  {:.2} Hz",
                t.name, t.clock_mhz, t.khz, t.hz
            ),
            paint.style(paint.pal.theme.dim),
        ),
    ])];
    for (label, v, unit) in [("H", t.h, "px"), ("V", t.v, "lines")] {
        let total = v[3].max(1) as f32;
        let bar = 46.0;
        // Clamped to the bar: these numbers come from a configuration file,
        // and a modeline whose active width is larger than its total would
        // otherwise ask for a string as long as it liked.
        let seg = |from: u32, to: u32| -> usize {
            (((to.saturating_sub(from) as f32 / total) * bar).round() as usize).min(bar as usize)
        };
        out.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{label}       "),
                paint.style(paint.pal.theme.paper),
            ),
            Span::styled(
                "█".repeat(seg(0, v[0]).max(1)),
                paint.style(paint.pal.theme.cyan),
            ),
            Span::styled(
                "▒".repeat(seg(v[0], v[1])),
                paint.style(paint.pal.theme.dim),
            ),
            Span::styled(
                "█".repeat(seg(v[1], v[2]).max(1)),
                paint.style(paint.pal.theme.magenta),
            ),
            Span::styled(
                "▒".repeat(seg(v[2], v[3])),
                paint.style(paint.pal.theme.dim),
            ),
            Span::styled(
                format!("  {} {unit}", v[0]),
                paint.style(paint.pal.theme.dim),
            ),
        ]));
        out.push(Line::from(vec![
            Span::raw("          "),
            Span::styled(
                format!(
                    "active {}   porch {}   sync {}   porch {}   total {}",
                    v[0],
                    v[1].saturating_sub(v[0]),
                    v[2].saturating_sub(v[1]),
                    v[3].saturating_sub(v[2]),
                    v[3]
                ),
                paint.style(paint.pal.theme.dim),
            ),
        ]));
    }
    out
}

/// The card's outputs, and what each is for.
fn output_lines(paint: &Paint, outs: &[Output]) -> Vec<Line<'static>> {
    let mut out = vec![Line::from(vec![
        Span::raw("  "),
        Span::styled("OUTPUTS", paint.bold(paint.pal.theme.accent)),
    ])];
    for o in outs {
        let mut spans = output_line(paint, o, false);
        spans.insert(0, Span::raw("  "));
        out.push(Line::from(spans));
    }
    out
}

/// Render lines to the scrollback through a one line buffer, so the styling
/// is ratatui's and the result is ordinary terminal output that scrolls.
fn print_lines(lines: &[Line<'static>]) {
    use std::io::Write;
    let mut stdout = stdout().lock();
    for line in lines {
        let _ = writeln!(stdout, "\r\x1b[2K{}", line_text(line));
    }
    let _ = stdout.flush();
}

/// How wide to render. A pty with no window size set reports zero, and a
/// report rendered into a buffer no cells wide is a page of blank lines.
fn columns() -> u16 {
    ratatui::crossterm::terminal::size()
        .map(|(c, _)| c)
        .ok()
        .filter(|c| *c > 20)
        .unwrap_or(100)
}

/// One styled line as the escape sequences for it, with the trailing blanks
/// left off so the text can be copied out of a terminal without a tail of
/// spaces on every line.
fn line_text(line: &Line<'static>) -> String {
    let cols = columns();
    let area = Rect::new(0, 0, cols, 1);
    let mut buf = Buffer::empty(area);
    line.clone().render(area, &mut buf);
    let mut text = String::new();
    let mut last: Option<Style> = None;
    let mut trailing = String::new();
    for x in 0..cols {
        let cell = &buf[(x, 0)];
        let symbol = cell.symbol();
        if symbol == " " && cell.fg == TColor::Reset {
            trailing.push(' ');
            continue;
        }
        text.push_str(&trailing);
        trailing.clear();
        let style = Style::default().fg(cell.fg).add_modifier(cell.modifier);
        if last != Some(style) {
            text.push_str(&ansi(style));
            last = Some(style);
        }
        text.push_str(symbol);
    }
    text.push_str("\x1b[0m");
    text
}

/// A ratatui style as the escape sequence for it.
fn ansi(style: Style) -> String {
    let mut s = String::from("\x1b[0m");
    if style.add_modifier.contains(Modifier::BOLD) {
        s.push_str("\x1b[1m");
    }
    match style.fg {
        Some(TColor::Rgb(r, g, b)) => s.push_str(&format!("\x1b[38;2;{r};{g};{b}m")),
        Some(TColor::Indexed(i)) => s.push_str(&format!("\x1b[38;5;{i}m")),
        Some(c) => s.push_str(named(c)),
        None => {}
    }
    s
}

fn named(c: TColor) -> &'static str {
    match c {
        TColor::Black => "\x1b[30m",
        TColor::Red => "\x1b[31m",
        TColor::Green => "\x1b[32m",
        TColor::Yellow => "\x1b[33m",
        TColor::Blue => "\x1b[34m",
        TColor::Magenta => "\x1b[35m",
        TColor::Cyan => "\x1b[36m",
        TColor::Gray => "\x1b[37m",
        TColor::DarkGray => "\x1b[90m",
        TColor::LightRed => "\x1b[91m",
        TColor::LightGreen => "\x1b[92m",
        TColor::LightYellow => "\x1b[93m",
        TColor::LightBlue => "\x1b[94m",
        TColor::LightMagenta => "\x1b[95m",
        TColor::LightCyan => "\x1b[96m",
        TColor::White => "\x1b[97m",
        _ => "",
    }
}

/// Wait for one of the offered keys, or for the reader to give up.
fn wait_for_key(actions: &[Action]) -> Option<char> {
    let raw = RawMode::on();
    if !raw.0 {
        return None;
    }
    let mut pressed = None;
    loop {
        match event::read() {
            Err(_) => break,
            Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => match k.code {
                KeyCode::Char('c') if k.modifiers.contains(event::KeyModifiers::CONTROL) => break,
                KeyCode::Char(c) if actions.iter().any(|a| a.key == c) => {
                    pressed = Some(c);
                    break;
                }
                KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => break,
                _ => {}
            },
            Ok(_) => {}
        }
    }
    drop(raw);
    println!();
    pressed
}

/// Choose an output by pointing at the picture rather than copying a name.
///
/// Returns the index chosen, or nothing when the reader gave up or there is
/// no terminal to ask. The rows are the same map the report draws, so
/// `setup` and `doctor` describe the machine the same way.
pub fn pick_output(outs: &[Output], preselect: usize) -> Option<usize> {
    if outs.is_empty() || !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return None;
    }
    let pal = Palette::load();
    let paint = Paint { pal };
    let mut sel = preselect.min(outs.len() - 1);
    let raw = RawMode::on();
    if !raw.0 {
        return None;
    }
    let height = outs.len() as u16 + 2;
    let mut first = true;
    let chosen = loop {
        let mut lines = vec![Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "which output is the television?",
                paint.bold(paint.pal.theme.accent),
            ),
        ])];
        for (i, o) in outs.iter().enumerate() {
            let mut spans = output_line(&paint, o, i == sel);
            if i == sel {
                spans.insert(0, Span::styled("▸ ", paint.bold(paint.pal.theme.accent)));
            } else {
                spans.insert(0, Span::raw("  "));
            }
            lines.push(Line::from(spans));
        }
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "↑↓ or j k to move   enter to choose   esc to leave it alone",
                paint.style(paint.pal.theme.dim),
            ),
        ]));
        draw_area(&lines, height, first);
        first = false;
        match event::read() {
            Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => match k.code {
                KeyCode::Up | KeyCode::Char('k') => sel = sel.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => sel = (sel + 1).min(outs.len() - 1),
                KeyCode::Enter => break Some(sel),
                KeyCode::Esc | KeyCode::Char('q') => break None,
                KeyCode::Char('c') if k.modifiers.contains(event::KeyModifiers::CONTROL) => {
                    break None;
                }
                _ => {}
            },
            Ok(_) => {}
            Err(_) => break None,
        }
    };
    drop(raw);
    print!("\x1b[{height}A");
    chosen
}

/// One row of the output map, as spans.
fn output_line(paint: &Paint, o: &Output, selected: bool) -> Vec<Span<'static>> {
    let (mark, colour, what) = match o.role {
        Role::Tube => ("▐█▌", paint.pal.theme.bright_green, "the television"),
        Role::Desktop => ("▐▓▌", paint.pal.theme.blue, "the desktop"),
        Role::Free => ("▐░▌", paint.pal.theme.dim, "free"),
        Role::Disconnected => ("▐ ▌", paint.pal.theme.dim, "nothing plugged in"),
    };
    let name = if selected {
        paint.bold(paint.pal.theme.paper)
    } else {
        paint.style(paint.pal.theme.paper)
    };
    vec![
        Span::styled(mark, paint.style(colour)),
        Span::styled(format!("  {:<12}", o.name), name),
        Span::styled(format!("{:<22}", o.edid), paint.style(paint.pal.theme.dim)),
        Span::styled(what, paint.style(colour)),
    ]
}

/// Draw a reserved area of `height` lines in place, the way the self test
/// draws its own.
fn draw_area(lines: &[Line<'static>], height: u16, first: bool) {
    use std::io::Write;
    let mut out = String::new();
    if !first {
        out.push_str(&format!("\x1b[{height}A"));
    }
    for i in 0..height as usize {
        out.push_str("\r\x1b[2K");
        if let Some(l) = lines.get(i) {
            out.push_str(&line_text(l));
        }
        out.push_str("\r\n");
    }
    let mut stdout = stdout().lock();
    let _ = stdout.write_all(out.as_bytes());
    let _ = stdout.flush();
}
