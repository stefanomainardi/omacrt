//! The pads, as four sockets on the screen.
//!
//! Four ports are drawn whether they are filled or not, so the player can see
//! at a glance which is which without counting. A pad the launcher remembers
//! but which is switched off keeps its socket, greyed: the order is a thing
//! the player arranged and it does not disappear when a battery does.
//!
//! The order in `pads.toml` is the only thing that decides who is P1, and the
//! ports are handed out when a game starts, skipping whoever is not there.

use super::*;
use crate::bt::Bluetooth;
use omacrt_shell::pads::{Pad, short_name};

/// The width of one socket, and the gap between two.
const TILE_W: i32 = 136;
const TILE_H: i32 = 58;
const GAP: i32 = 16;
/// Where the top row of sockets starts, under the header.
const TOP: i32 = 56;

/// How long a tile takes to slide between two sockets.
const MOVE_SECS: f64 = 0.22;

/// How far along a move is, 0 at the start and 1 at the end.
fn move_phase(started: f64, now: f64) -> f64 {
    if MOVE_SECS <= 0.0 {
        return 1.0;
    }
    ((now - started) / MOVE_SECS).clamp(0.0, 1.0)
}

/// Where a sliding tile sits between two points, easing out so it arrives
/// rather than stops.
fn slide(from: i32, to: i32, phase: f64) -> i32 {
    let eased = 1.0 - (1.0 - phase) * (1.0 - phase);
    from + ((to - from) as f64 * eased).round() as i32
}

/// Where a socket sits relative to the first one: two across, two down.
fn tile_offset(port: usize) -> (i32, i32) {
    let port = port.min(3) as i32;
    ((port % 2) * (TILE_W + GAP), (port / 2) * (TILE_H + 6))
}

/// What a socket is showing.
enum Slot<'a> {
    /// A pad on the list, and whether it is switched on now.
    Pad(&'a Pad, bool),
    Empty,
}

impl Scene {
    fn tile_at(&self, fb: &Framebuffer, port: usize) -> (i32, i32) {
        let left = (fb.w as f32 * 0.05) as i32 + self.slide();
        let (dx, dy) = tile_offset(port);
        let (mut x, mut y) = (left + dx, TOP + dy);
        // A socket taking part in a move is drawn on its way there, so the
        // two pads are seen to change places rather than appearing swapped.
        if let Some((from, to, started)) = self.pad_move {
            let phase = move_phase(started, self.now);
            let other = if port == from {
                Some(to)
            } else if port == to {
                Some(from)
            } else {
                None
            };
            if let Some(other) = other {
                let (ox, oy) = tile_offset(other);
                x = slide(left + ox, left + dx, phase);
                y = slide(TOP + oy, TOP + dy, phase);
            }
        }
        (x, y)
    }

    /// Which pad, if any, is meant to be in each of the four ports.
    ///
    /// The first four of the remembered order. A pad that is on but that the
    /// list has never seen goes into the first free socket, so a pad plugged
    /// in for the first time is visible before anything has been saved.
    fn slots(&self) -> Vec<Slot<'_>> {
        let mut out: Vec<Slot<'_>> = Vec::new();
        for p in self.pad_list.order.iter().take(4) {
            let on = self.pads_here.iter().any(|h| h.is(p));
            out.push(Slot::Pad(p, on));
        }
        while out.len() < 4 {
            out.push(Slot::Empty);
        }
        out
    }

    fn pad_art(name: &str) -> &'static [&'static str] {
        match PadKind::from_name(name) {
            PadKind::PlayStation => &icons::PAD_STICKS,
            PadKind::Nintendo => &icons::PAD_ROUND,
            _ => &icons::PAD_TWIN,
        }
    }

    /// One socket: its frame, its corners, the number astride the top edge.
    fn draw_socket(&self, fb: &mut Framebuffer, x: i32, y: i32, port: usize, c: Color, sel: bool) {
        if sel {
            fb.rect(x + 2, y + 2, TILE_W - 4, TILE_H - 4, self.theme.selection);
        }
        // Rectangles rather than a border: one pixel on each side, with the
        // corners two thick so the shape reads from the sofa.
        fb.rect(x, y, TILE_W, 1, c);
        fb.rect(x, y + TILE_H - 1, TILE_W, 1, c);
        fb.rect(x, y, 1, TILE_H, c);
        fb.rect(x + TILE_W - 1, y, 1, TILE_H, c);
        for (cx, cy) in [
            (x, y),
            (x + TILE_W - 5, y),
            (x, y + TILE_H - 2),
            (x + TILE_W - 5, y + TILE_H - 2),
        ] {
            fb.rect(cx, cy, 5, 2, c);
        }
        for (cx, cy) in [
            (x, y),
            (x + TILE_W - 2, y),
            (x, y + TILE_H - 5),
            (x + TILE_W - 2, y + TILE_H - 5),
        ] {
            fb.rect(cx, cy, 2, 5, c);
        }
        // Astride the top edge, clear of the corner bracket, on a patch of
        // background so the line does not run through the number.
        let label = format!("P{}", port + 1);
        let w = Framebuffer::text_width(&label, 1);
        fb.rect(x + 9, y - 4, w + 6, 10, self.theme.bg);
        fb.text(x + 12, y - 3, &label, c, 1);
    }

    fn draw_slot(&self, fb: &mut Framebuffer, port: usize, slot: &Slot<'_>, sel: bool) {
        let (x, y) = self.tile_at(fb, port);
        // A travelling tile covers what it passes over. Without this the two
        // crossing sockets printed their text through each other.
        if self
            .pad_move
            .is_some_and(|(f, t, _)| port == f || port == t)
        {
            fb.rect(x, y, TILE_W, TILE_H, self.theme.bg);
        }
        let (pad, on) = match slot {
            Slot::Empty => {
                self.draw_socket(fb, x, y, port, self.theme.selection, sel);
                fb.bitmap(
                    x + TILE_W / 2 - 4,
                    y + 18,
                    &icons::SOCKET[..],
                    scale(self.theme.dim, 0.7),
                    1,
                    8,
                );
                fb.text_centered(x + TILE_W / 2, y + 32, "empty", self.theme.dim, 1);
                return;
            }
            Slot::Pad(p, on) => (*p, *on),
        };
        let accent = if !on {
            self.theme.dim
        } else if port == 0 {
            self.theme.cyan
        } else {
            self.theme.green
        };
        let body = if on { self.theme.paper } else { self.theme.dim };
        self.draw_socket(fb, x, y, port, accent, sel);
        // The name across the whole socket, the drawing under it: a name cut
        // to the width left beside a picture says nothing.
        let room = ((TILE_W - 14) / 8).max(1) as usize;
        fb.text(x + 7, y + 8, &short_name(&pad.name, room), body, 1);
        let art = Self::pad_art(&pad.name);
        fb.bitmap(x + 6, y + 22, art, body, 1, art.len());
        // The buttons of the drawing, in the socket's own colour.
        let lit: Vec<String> = art
            .iter()
            .map(|r| r.replace('#', ".").replace('o', "#"))
            .collect();
        let lit: Vec<&str> = lit.iter().map(String::as_str).collect();
        fb.bitmap(x + 6, y + 22, &lit, accent, 1, lit.len());
        // Over the air or on a cable, and the battery when it is known.
        let wireless = pad.unit.contains(':');
        fb.bitmap(
            x + 34,
            y + 24,
            if wireless {
                &icons::BLUETOOTH[..]
            } else {
                &icons::USB[..]
            },
            if on { accent } else { self.theme.dim },
            1,
            8,
        );
        fb.text(
            x + 45,
            y + 24,
            if wireless { "wireless" } else { "cable" },
            self.theme.dim,
            1,
        );
        // The charge, when the kernel has one. Most pads report nothing, and
        // then nothing is drawn: a gauge invented out of no reading is worse
        // than no gauge.
        if on && let Some(bars) = omacrt_shell::pads::battery_of(&pad.unit) {
            fb.bitmap(x + 34, y + 38, &icons::BATTERY[..], self.theme.dim, 1, 6);
            let lit = icons::battery_bars(bars);
            let lit: Vec<&str> = lit.iter().map(String::as_str).collect();
            fb.bitmap(
                x + 34,
                y + 38,
                &lit,
                if bars > 1 {
                    self.theme.green
                } else {
                    self.theme.orange
                },
                1,
                6,
            );
        }
        if !on {
            let w = Framebuffer::text_width("off", 1);
            fb.text(
                x + TILE_W - 6 - w,
                y + TILE_H - 14,
                "off",
                self.theme.dim,
                1,
            );
        } else if self.identify_until > self.now {
            // Shaking: the only way to tell two of one model apart.
            let n = ((self.now * 8.0) as i32) % 2;
            fb.bitmap(
                x + TILE_W - 16 + n,
                y + TILE_H - 16,
                &icons::SHAKE[..],
                self.theme.accent,
                1,
                8,
            );
        }
    }

    /// The search for a new pad, over the two lower sockets.
    fn draw_search(&self, fb: &mut Framebuffer, sel: usize) {
        let left = (fb.w as f32 * 0.05) as i32 + self.slide();
        let y = TOP + TILE_H + 6;
        let w = TILE_W * 2 + GAP;
        let c = if self.bt.failed {
            self.theme.red
        } else {
            self.theme.cyan
        };
        fb.rect(left, y, w, 1, c);
        fb.rect(left, y + TILE_H - 1, w, 1, c);
        fb.rect(left, y, 1, TILE_H, c);
        fb.rect(left + w - 1, y, 1, TILE_H, c);
        let title = if self.bt.busy() {
            self.bt.phase().label()
        } else {
            "add a pad"
        };
        let tw = Framebuffer::text_width(title, 1);
        fb.rect(left + 7, y - 4, tw + 4, 10, self.theme.bg);
        fb.text(left + 9, y - 3, title, c, 1);
        // The bar creeps along while a step runs, so a screen that is waiting
        // does not look like a screen that has stopped.
        if self.bt.busy() {
            let run = (w as f32 * self.bt.progress()) as i32;
            fb.rect(left + 1, y + TILE_H - 3, run.max(1), 2, scale(c, 0.6));
            if let Some(step) = self.bt.phase().step() {
                let s = format!("step {step} of 3");
                let sw = Framebuffer::text_width(&s, 1);
                fb.text(left + w - 6 - sw, y - 3, &s, self.theme.dim, 1);
            }
        }
        let rows = ((TILE_H - 14) / 11).max(1) as usize;
        if self.bt.devices.is_empty() {
            if self.bt.failed {
                fb.bitmap(left + 8, y + 9, &icons::SPARK[..], self.theme.red, 1, 8);
            }
            let tx = if self.bt.failed { left + 20 } else { left + 8 };
            fb.text(tx, y + 10, &self.bt.status, self.theme.dim, 1);
            if !self.bt.detail.is_empty() {
                let room = ((w - 16) / 8) as usize;
                let d: String = self.bt.detail.chars().take(room).collect();
                fb.text(left + 8, y + 24, &d, self.theme.red, 1);
            }
            return;
        }
        let top = sel.saturating_sub(rows - 1);
        for (row, i) in (top..self.bt.devices.len().min(top + rows)).enumerate() {
            let (mac, name) = &self.bt.devices[i];
            let ty = y + 6 + row as i32 * 11;
            if i == sel {
                fb.rect(left + 3, ty - 1, w - 6, 10, self.theme.selection);
            }
            fb.bitmap(left + 6, ty, &icons::BLUETOOTH[..], c, 1, 8);
            let tail = mac.get(9..).unwrap_or(mac);
            let room = ((w - 30) / 8).saturating_sub(tail.len() as i32 + 1).max(1) as usize;
            let n: String = name.chars().take(room).collect();
            fb.text(left + 17, ty, &n, self.theme.paper, 1);
            let tw = Framebuffer::text_width(tail, 1);
            fb.text(left + w - 6 - tw, ty, tail, self.theme.dim, 1);
        }
    }

    pub(super) fn draw_pads(&mut self, fb: &mut Framebuffer, sel: usize, scan: bool) {
        let left = (fb.w as f32 * 0.05) as i32 + self.slide();
        let h = fb.h as i32;
        // A move that has arrived is over: the sockets are where they say.
        if self
            .pad_move
            .is_some_and(|(_, _, t)| move_phase(t, self.now) >= 1.0)
        {
            self.pad_move = None;
        }
        let here = self.pads_here.len();
        // The count goes in the title: the header's own right hand side is
        // the clock, and a second thing there is drawn on top of it.
        self.draw_header(fb, &format!("Pads   {here} of 4"));
        let slots = self.slots();
        let shown = if scan { 2 } else { 4 };
        // The socket a pad is arriving in goes last, so it passes in front of
        // the one it is changing places with rather than under it.
        let front = self.pad_move.map(|(_, to, _)| to);
        for (port, slot) in slots.iter().enumerate().take(shown) {
            if Some(port) == front {
                continue;
            }
            self.draw_slot(fb, port, slot, !scan && port == sel);
        }
        if let Some(port) = front
            && port < shown
            && let Some(slot) = slots.get(port)
        {
            self.draw_slot(fb, port, slot, !scan && port == sel);
        }
        if scan {
            self.draw_search(fb, sel);
        }
        // What the selection is, spelled out, because a highlighted rectangle
        // is not a sentence.
        let said = if scan {
            self.bt.status.clone()
        } else {
            match slots.get(sel) {
                Some(Slot::Pad(p, true)) => format!("P{}  {}", sel + 1, short_name(&p.name, 26)),
                Some(Slot::Pad(p, false)) => {
                    format!("P{}  {}  switched off", sel + 1, short_name(&p.name, 14))
                }
                _ => format!("P{}  nothing in this port", sel + 1),
            }
        };
        let room = ((fb.w as i32 - 2 * left - 6) / 8) as usize;
        fb.rect(left, 182, 2, 10, self.theme.accent);
        fb.text(
            left + 6,
            183,
            &said.chars().take(room).collect::<String>(),
            self.theme.accent,
            1,
        );
        if self.pad_list.ambiguous() {
            fb.text(
                left,
                198,
                "two of one model report no serial",
                self.theme.yellow,
                1,
            );
        }
        let filled = matches!(slots.get(sel), Some(Slot::Pad(_, true)));
        let hint = if scan {
            self.hint(&[("A", "pair"), ("X", "look again"), ("B", "back")])
        } else if filled {
            self.hint(&[("A", "identify"), ("X", "remap"), ("Y", "forget")])
        } else {
            self.hint(&[("A", "add a pad"), ("Y", "forget")])
        };
        fb.text(left, h - 26, &hint, scale(self.theme.dim, 0.7), 1);
        let hint2 = if scan {
            String::new()
        } else {
            self.hint(&[("L R", "move a port"), ("B", "back")])
        };
        fb.text(left, h - 14, &hint2, scale(self.theme.dim, 0.7), 1);
    }

    // ------------------------------------------------------------- the input

    pub(super) fn pads_nav(&mut self, sel: &mut usize, scan: &mut bool, nav: Nav) -> bool {
        if *scan {
            let n = self.bt.devices.len();
            return match nav {
                Nav::Up if *sel > 0 => {
                    *sel -= 1;
                    true
                }
                Nav::Down if *sel + 1 < n => {
                    *sel += 1;
                    true
                }
                Nav::Back => {
                    // B stops whatever bluetoothctl is doing rather than
                    // leaving it running behind a screen that has gone.
                    self.bt.cancel();
                    *scan = false;
                    *sel = 0;
                    true
                }
                _ => false,
            };
        }
        match nav {
            Nav::Up if *sel >= 2 => {
                *sel -= 2;
                true
            }
            Nav::Down if *sel + 2 < 4 => {
                *sel += 2;
                true
            }
            Nav::Left if !(*sel).is_multiple_of(2) => {
                *sel -= 1;
                true
            }
            Nav::Right if (*sel).is_multiple_of(2) => {
                *sel += 1;
                true
            }
            Nav::Back => {
                self.screen = Screen::Settings {
                    sel: settings_row(Page::Pads),
                };
                true
            }
            _ => false,
        }
    }

    /// The pad in the selected port, when there is one and it is switched on.
    fn selected_pad(&self, sel: usize) -> Option<Pad> {
        match self.slots().get(sel) {
            Some(Slot::Pad(p, true)) => Some((*p).clone()),
            _ => None,
        }
    }

    pub(super) fn pads_fire(&mut self, sel: usize, scan: bool) {
        if scan {
            self.pending.push(Sound::Select);
            self.bt.pair(sel);
            return;
        }
        match self.selected_pad(sel) {
            Some(p) => {
                self.pending.push(Sound::Lock);
                self.identify = Some(p.clone());
                self.identify_until = self.now + 1.2;
                self.message = Some((format!("{} is shaking", p.name), self.now + 3.0));
            }
            // An empty socket is where a new pad goes, so A on one opens the
            // search rather than refusing.
            None => {
                let mut s = sel;
                let mut sc = false;
                self.pads_search(&mut s, &mut sc);
                if let Screen::Pads { sel, scan } = &mut self.screen {
                    *sel = s;
                    *scan = sc;
                }
            }
        }
    }

    /// Y: stop remembering the pad in this port.
    pub(super) fn pads_forget(&mut self, sel: usize) {
        if self.pad_forget(sel) {
            self.pending.push(Sound::Lock);
        } else {
            self.pending.push(Sound::Crunch);
        }
    }

    /// Move the pad in port `i`, from anywhere: the screen's shoulders, or the
    /// desktop panel down the control pipe. Answers with the port it ended in.
    pub fn pad_reorder(&mut self, i: usize, dir: i32) -> Option<usize> {
        if i >= self.pad_list.order.len() {
            return None;
        }
        let to = self.pad_list.shift(i, dir);
        if to == i {
            return None;
        }
        self.pad_move = Some((i, to, self.now));
        if let Err(e) = self.pad_list.save() {
            self.message = Some((format!("cannot save: {e}"), self.now + 4.0));
        }
        Some(to)
    }

    /// Shake the pad in port `i`, from anywhere. False when that port is
    /// empty or its pad is switched off, because nothing would shake.
    pub fn pad_identify(&mut self, i: usize) -> bool {
        let Some(pad) = self.pad_list.order.get(i).cloned() else {
            return false;
        };
        if !self.pads_here.iter().any(|h| h.is(&pad)) {
            return false;
        }
        self.identify_until = self.now + 1.2;
        self.message = Some((
            format!("{} is shaking", short_name(&pad.name, 16)),
            self.now + 3.0,
        ));
        self.identify = Some(pad);
        true
    }

    /// Stop remembering the pad in port `i`, from anywhere.
    pub fn pad_forget(&mut self, i: usize) -> bool {
        if i >= self.pad_list.order.len() {
            return false;
        }
        let name = self.pad_list.order[i].name.clone();
        self.pad_list.forget(i);
        match self.pad_list.save() {
            Ok(()) => self.message = Some((format!("{name} forgotten"), self.now + 3.0)),
            Err(e) => self.message = Some((format!("cannot save: {e}"), self.now + 4.0)),
        }
        true
    }

    /// The shoulders move the selected pad one port along, which is the whole
    /// point of the screen: the order is the player's.
    pub(super) fn pads_shift(&mut self, sel: &mut usize, dir: i32) {
        if *sel >= self.pad_list.order.len() {
            self.pending.push(Sound::Crunch);
            self.message = Some(("nothing in that port to move".into(), self.now + 3.0));
            return;
        }
        let from = *sel;
        // Silence at the end of the list reads as a button that does not
        // work, which is what it was taken for.
        let Some(to) = self.pad_reorder(from, dir) else {
            self.pending.push(Sound::Crunch);
            self.message = Some((
                format!(
                    "{} is already P{}",
                    short_name(&self.pad_list.order[from].name, 16),
                    from + 1
                ),
                self.now + 3.0,
            ));
            return;
        };
        *sel = to;
        self.pending.push(Sound::Move);
        self.message = Some((
            format!(
                "{} is now P{}",
                short_name(&self.pad_list.order[to].name, 16),
                to + 1
            ),
            self.now + 3.0,
        ));
    }

    /// Start: open the search over the two lower sockets.
    pub(super) fn pads_search(&mut self, sel: &mut usize, scan: &mut bool) {
        if !Bluetooth::available() {
            self.pending.push(Sound::Crunch);
            self.message = Some(("bluetoothctl is not on this machine".into(), self.now + 4.0));
            return;
        }
        *scan = true;
        *sel = 0;
        self.pending.push(Sound::Select);
        self.bt.start_scan();
    }

    /// X: look again while the search is open, remap the pad otherwise.
    pub(super) fn pads_alt(&mut self, sel: usize, scan: bool) {
        if scan {
            self.pending.push(Sound::Select);
            self.bt.start_scan();
            return;
        }
        match self.selected_pad(sel) {
            Some(p) => {
                self.remap_request = true;
                self.message = Some((format!("mapping {}", p.name), self.now + 3.0));
            }
            None => {
                self.pending.push(Sound::Crunch);
                self.message = Some(("nothing in that port to map".into(), self.now + 3.0));
            }
        }
    }

    /// The pad the screen wants shaken, taken once.
    pub fn take_identify(&mut self) -> Option<Pad> {
        self.identify.take()
    }

    /// A pad the launcher has just met.
    ///
    /// The scene owns the order, because the screen reorders and forgets. A
    /// second copy in the main loop meant the two overwrote each other, and a
    /// pad connected while the screen was up landed in neither: the header
    /// counted two and the sockets drew one.
    pub fn pad_seen(&mut self, pad: &Pad) {
        if self.pad_list.seen(pad)
            && let Err(e) = self.pad_list.save()
        {
            self.message = Some((format!("cannot save the pad list: {e}"), self.now + 4.0));
        }
    }

    /// Which pads are switched on right now.
    pub fn set_pads_here(&mut self, here: Vec<Pad>) {
        self.pads_here = here;
    }

    /// Whether the search is open, so the main loop keeps polling bluetoothctl
    /// even when another screen is drawn.
    pub fn bt_poll(&mut self) -> bool {
        if self.bt.busy() {
            self.bt.poll()
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GAP, TILE_H, TILE_W, TOP, tile_offset};

    #[test]
    fn a_tile_crossing_starts_where_it_was_and_ends_where_it_goes() {
        use super::{MOVE_SECS, move_phase, slide};
        assert_eq!(move_phase(10.0, 10.0), 0.0);
        assert_eq!(move_phase(10.0, 10.0 + MOVE_SECS), 1.0);
        assert_eq!(move_phase(10.0, 99.0), 1.0, "and it does not overshoot");
        assert_eq!(move_phase(10.0, 9.0), 0.0, "nor go backwards");
        assert_eq!(slide(16, 168, 0.0), 16);
        assert_eq!(slide(16, 168, 1.0), 168);
        // Easing out: past halfway by the time half the seconds are gone.
        let half = slide(16, 168, 0.5);
        assert!(half > (16 + 168) / 2, "{half} is not past the middle");
        assert!(half < 168);
    }

    #[test]
    fn the_four_sockets_are_two_across_and_two_down() {
        assert_eq!(tile_offset(0), (0, 0));
        assert_eq!(tile_offset(1), (TILE_W + GAP, 0));
        assert_eq!(tile_offset(2), (0, TILE_H + 6));
        assert_eq!(tile_offset(3), (TILE_W + GAP, TILE_H + 6));
        // A port number from somewhere unexpected lands on the last socket
        // rather than off the screen.
        assert_eq!(tile_offset(9), tile_offset(3));
    }

    /// 320 across and 240 down with the launcher's own five percent margin:
    /// two sockets and a gap across, two rows ending above the line that says
    /// what is selected. Checked at compile time, since none of it varies.
    const _FITS: () = {
        assert!(2 * TILE_W + GAP <= 320 - 2 * 16);
        assert!(TOP + 2 * TILE_H + 6 < 182);
    };
}
