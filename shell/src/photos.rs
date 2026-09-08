//! The photo frame's supply of pictures, kept coming on a thread.
//!
//! Everything slow lives here: asking the server what to show, fetching each
//! picture, handing it to ffmpeg, decoding the result. The scene takes what
//! is ready and never waits. Two pictures are prepared ahead and then the
//! thread blocks, so the cache fills at the speed the frame shows them
//! rather than all at once.

use crate::art::{self, Image};
use omarchy_crt_shell::{ambient, immich};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};

/// A picture ready to go up, with what should be written under it.
pub struct Shown {
    pub image: Image,
    /// Where and when, as one line: "Etterbeek, Belgium".
    pub place: String,
    /// The date, spoken: "10 September 2025".
    pub when: String,
    /// "a year ago today", for a picture that came from a memory.
    pub ago: String,
    /// Who is in it, at most three names.
    pub people: Vec<String>,
}

impl Shown {
    /// The caption line: where, when, and who is in it.
    pub fn caption(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if !self.place.is_empty() {
            parts.push(self.place.clone());
        }
        if !self.when.is_empty() {
            parts.push(self.when.clone());
        }
        if !self.people.is_empty() {
            parts.push(self.people.join(", "));
        }
        parts.join("  ")
    }
}

enum Msg {
    Photo(Box<Shown>),
    Ambient(ambient::Info),
    /// Why there are no pictures, for the screen to say out loud.
    Trouble(String),
}

/// The supply. Dropping it stops the thread at its next send.
pub struct Feed {
    rx: Option<Receiver<Msg>>,
    /// Pictures received and not yet shown.
    queue: Vec<Shown>,
    /// The last thing the ambient screen was told about the outside.
    pub info: ambient::Info,
    /// What went wrong, when nothing is coming.
    pub trouble: Option<String>,
    started: bool,
    size: (usize, usize),
}

impl Feed {
    pub fn new() -> Self {
        Self {
            rx: None,
            queue: Vec::new(),
            info: ambient::Info::default(),
            trouble: None,
            started: false,
            size: (0, 0),
        }
    }

    /// Start the thread, or start it again for a different screen size or a
    /// different source. Doing nothing when it is already up for this shape
    /// is what lets the scene call it on every frame.
    pub fn start(
        &mut self,
        config_dir: &Path,
        source: immich::Source,
        album: String,
        place: String,
        calendar: String,
        w: usize,
        h: usize,
    ) {
        if self.started && self.size == (w, h) {
            return;
        }
        self.started = true;
        self.size = (w, h);
        self.queue.clear();
        self.trouble = None;
        let (tx, rx) = sync_channel::<Msg>(2);
        self.rx = Some(rx);
        let dir = config_dir.to_path_buf();
        std::thread::spawn(move || work(tx, dir, source, album, place, calendar, w, h));
    }

    /// Collect whatever the thread has sent. Cheap; call it every frame.
    pub fn poll(&mut self) {
        let Some(rx) = &self.rx else { return };
        loop {
            match rx.try_recv() {
                Ok(Msg::Photo(p)) => self.queue.push(*p),
                Ok(Msg::Ambient(i)) => self.info = i,
                Ok(Msg::Trouble(why)) => self.trouble = Some(why),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.rx = None;
                    break;
                }
            }
        }
    }

    /// The next picture to put up, if one has arrived.
    pub fn take(&mut self) -> Option<Shown> {
        if self.queue.is_empty() {
            return None;
        }
        Some(self.queue.remove(0))
    }
}

impl Default for Feed {
    fn default() -> Self {
        Self::new()
    }
}

/// The thread: a list of pictures, then each one prepared and sent, and the
/// list asked for again when it runs out.
fn work(
    tx: SyncSender<Msg>,
    config_dir: PathBuf,
    source: immich::Source,
    album: String,
    place: String,
    calendar: String,
    w: usize,
    h: usize,
) {
    let Some(cfg) = immich::Config::load(&config_dir) else {
        let _ = tx.send(Msg::Trouble(format!(
            "no immich.toml in {}",
            config_dir.display()
        )));
        return;
    };
    // The weather and the calendar first: they are one request each and the
    // screen has somewhere to put them straight away.
    let _ = tx.send(Msg::Ambient(ambient::info(&place, &calendar)));
    let mut refreshed = std::time::Instant::now();

    // Whatever is already prepared goes up first: the screen fills at once,
    // and it fills even with the server switched off.
    let mut ready = immich::cached(w as u32, h as u32);
    shuffle(&mut ready);
    for path in ready {
        let Some(image) = art::decode(&path) else {
            continue;
        };
        let note = immich::Note::read(&path);
        let shown = Shown {
            image,
            place: note.place,
            when: note.when,
            ago: note.ago,
            people: note.people,
        };
        if tx.send(Msg::Photo(Box::new(shown))).is_err() {
            return;
        }
    }

    let mut shots = immich::list(&cfg, source, &album, 120);
    if shots.is_empty() {
        let _ = tx.send(Msg::Trouble(format!(
            "the {} source has no photographs",
            source.label()
        )));
        return;
    }
    shuffle(&mut shots);
    let mut at = 0usize;
    loop {
        if at >= shots.len() {
            let fresh = immich::list(&cfg, source, &album, 120);
            if fresh.is_empty() {
                // Nothing new: go round the ones already known rather than
                // leaving the screen empty.
                at = 0;
            } else {
                shots = fresh;
                shuffle(&mut shots);
                at = 0;
            }
        }
        let shot = shots[at].clone();
        at += 1;

        if refreshed.elapsed().as_secs() > 900 {
            refreshed = std::time::Instant::now();
            if tx
                .send(Msg::Ambient(ambient::info(&place, &calendar)))
                .is_err()
            {
                return;
            }
        }

        let already = immich::prepared_path(&shot, w as u32, h as u32);
        if already.as_ref().map(|p| p.exists()).unwrap_or(false) {
            // Shown already, out of the cache, at the top of this thread.
            continue;
        }
        let Some(path) = immich::prepare(&cfg, &shot, w as u32, h as u32) else {
            continue;
        };
        let Some(image) = art::decode(&path) else {
            let _ = std::fs::remove_file(&path);
            continue;
        };
        let details = immich::details(&cfg, &shot.id);
        let when = if details.taken.is_empty() {
            immich::spoken_date(&shot.taken)
        } else {
            immich::spoken_date(&details.taken)
        };
        let ago = match shot.years_ago {
            Some(1) => "a year ago today".to_string(),
            Some(n) if n > 1 => format!("{n} years ago today"),
            _ => String::new(),
        };
        immich::Note {
            place: details.place.clone(),
            when: when.clone(),
            ago: ago.clone(),
            people: details.people.clone(),
        }
        .write(&path);
        let shown = Shown {
            image,
            place: details.place,
            when,
            ago,
            people: details.people,
        };
        if tx.send(Msg::Photo(Box::new(shown))).is_err() {
            return;
        }
    }
}

/// A shuffle with the clock as its seed: a frame that shows the same
/// photographs in the same order every evening stops being interesting.
fn shuffle<T>(items: &mut [T]) {
    let mut state = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x2545_F491)
        | 1;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for i in (1..items.len()).rev() {
        let j = (next() % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}
