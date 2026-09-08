//! Music through cliamp, Omarchy's music player, running as a daemon. The
//! launcher is only a face on it: a 240p one with the pad language of the
//! games. cliamp speaks newline delimited JSON on a Unix socket; the radio
//! directory it ships (Radio Browser) is also queried directly for the
//! country and genre lists its socket does not expose yet.
//!
//! Everything slow runs on one worker thread. The scene posts requests, polls
//! the replies once per frame and keeps drawing.

use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::Duration;

const RADIO_BROWSER: &str = "https://all.api.radio-browser.info/json";
/// Stations per country or genre list, most voted first.
const STATIONS: usize = 100;

/// Silence the daemon from outside the launcher (the launcher stopping, the
/// tube going off). Nothing happens when no daemon listens.
pub fn stop_now() {
    let _ = call(json!({ "cmd": "stop" }));
}

fn socket_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config/cliamp/cliamp.sock")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Stopped,
    Playing,
    Paused,
}

/// A playable thing as cliamp describes it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub path: String,
    pub stream: bool,
    pub realtime: bool,
    /// Station name once a radio stream replaced the title with its ICY tag.
    pub station: String,
    /// Bitrate and codec of a directory station, for the row's right side.
    pub note: String,
    pub duration_secs: u32,
    /// Album art URL a provider gave, when any.
    pub art: String,
    /// The object as cliamp sent it, handed back untouched on play so a
    /// provider track keeps its identity.
    pub raw: Value,
}

impl Track {
    fn from_json(v: &Value) -> Self {
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        Self {
            title: s("title"),
            artist: s("artist"),
            album: s("album"),
            path: s("path"),
            stream: v.get("stream").and_then(Value::as_bool).unwrap_or(false),
            realtime: v.get("realtime").and_then(Value::as_bool).unwrap_or(false),
            station: s("station"),
            note: String::new(),
            duration_secs: v.get("duration_secs").and_then(Value::as_u64).unwrap_or(0) as u32,
            art: s("album_art_url"),
            raw: v.clone(),
        }
    }

    fn to_json(&self) -> Value {
        if self.raw.is_object() {
            return self.raw.clone();
        }
        let mut m = json!({ "title": self.title, "path": self.path });
        if !self.artist.is_empty() {
            m["artist"] = json!(self.artist);
        }
        if !self.album.is_empty() {
            m["album"] = json!(self.album);
        }
        if self.stream {
            m["stream"] = json!(true);
        }
        if self.realtime {
            m["realtime"] = json!(true);
        }
        if !self.station.is_empty() {
            m["station"] = json!(self.station);
        }
        m
    }

    /// One line for a row or the now playing screen.
    pub fn label(&self) -> String {
        fold(&if !self.artist.is_empty() && !self.title.is_empty() {
            format!("{} - {}", self.artist, self.title)
        } else if !self.title.is_empty() {
            self.title.clone()
        } else {
            self.path
                .rsplit('/')
                .next()
                .unwrap_or(&self.path)
                .to_string()
        })
    }
}

/// The shell's font is 8x8 ASCII: fold the Latin accents station names carry
/// ("Stereocittà") onto their base letters instead of showing `?`.
pub fn fold(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'À' | 'Á' | 'Â' | 'Ä' | 'Ã' | 'Å' => 'A',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'È' | 'É' | 'Ê' | 'Ë' => 'E',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'Ì' | 'Í' | 'Î' | 'Ï' => 'I',
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' | 'ø' => 'o',
            'Ò' | 'Ó' | 'Ô' | 'Ö' | 'Õ' | 'Ø' => 'O',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'Ù' | 'Ú' | 'Û' | 'Ü' => 'U',
            'ç' => 'c',
            'Ç' => 'C',
            'ñ' => 'n',
            'Ñ' => 'N',
            'ß' => 's',
            '’' | '‘' => '\'',
            '“' | '”' => '"',
            '–' | '—' => '-',
            '…' => '.',
            c if c.is_ascii() => c,
            _ => '?',
        })
        .collect()
}

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub state: Option<State>,
    pub track: Option<Track>,
    pub position: f64,
    pub duration: f64,
    /// dB, cliamp's own scale: -30 to +6.
    pub volume: f64,
    pub index: usize,
    pub total: usize,
    /// Name of the equaliser preset in force, "Custom" once a band moved.
    pub eq_preset: String,
    /// Gain of the ten bands in dB, as cliamp reports them.
    pub eq_bands: Vec<f64>,
}

impl Status {
    pub fn playing(&self) -> bool {
        self.state == Some(State::Playing)
    }

    pub fn active(&self) -> bool {
        matches!(self.state, Some(State::Playing) | Some(State::Paused))
    }
}

/// The equaliser: ten bands, cliamp's own gain range, and the presets it
/// knows by name (it answers "Custom" once a band was moved by hand).
pub const EQ_BANDS: usize = 10;
pub const EQ_MIN: f64 = -12.0;
pub const EQ_MAX: f64 = 12.0;
pub const EQ_FREQS: [&str; EQ_BANDS] = [
    "31", "62", "125", "250", "500", "1k", "2k", "4k", "8k", "16k",
];
pub const EQ_PRESETS: [&str; 4] = ["Flat", "Rock", "Pop", "Jazz"];

/// Where a list comes from. Each opens a list of items; playable ones are
/// tracks, the others are sources again.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// Radio Browser stations of one country (ISO code, display name).
    Country(String, String),
    /// Radio Browser stations carrying one tag.
    Tag(String),
    /// Countries by station count.
    Countries,
    /// Tags by station count.
    Tags,
    /// cliamp's own radio catalog and any list a provider offers.
    ProviderPlaylists(String, String),
    /// One playlist of a provider: (provider key, playlist id, name).
    ProviderPlaylist(String, String, String),
    /// cliamp's live queue.
    Queue,
    /// Tracks played past the scrobble threshold.
    History,
    /// Stations starred in the launcher (`radio-favorites.tsv`).
    Favorites,
    /// A provider's search: (provider key, query).
    ProviderSearch(String, String),
}

impl Source {
    pub fn title(&self) -> String {
        match self {
            Source::Country(_, name) => fold(name),
            Source::Tag(t) => fold(t),
            Source::Countries => "Countries".into(),
            Source::Tags => "Genres".into(),
            Source::ProviderPlaylists(_, name) => fold(name),
            Source::ProviderPlaylist(_, _, name) => fold(name),
            Source::Queue => "Queue".into(),
            Source::History => "Recently played".into(),
            Source::Favorites => "Favourite stations".into(),
            Source::ProviderSearch(_, q) => {
                if q.is_empty() {
                    "Search".into()
                } else {
                    fold(q)
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum Item {
    Track(Track),
    Source(Source, String),
}

impl Item {
    pub fn label(&self) -> String {
        match self {
            Item::Track(t) => t.label(),
            Item::Source(s, _) => s.title(),
        }
    }

    pub fn right(&self) -> String {
        match self {
            Item::Track(t) => {
                if !t.note.is_empty() {
                    t.note.clone()
                } else if t.duration_secs > 0 {
                    crate::player::clock(t.duration_secs as f64)
                } else if t.stream {
                    "live".into()
                } else {
                    String::new()
                }
            }
            Item::Source(_, right) => right.clone(),
        }
    }
}

/// A provider cliamp has configured: key and display name.
#[derive(Clone, Debug)]
pub struct Provider {
    pub key: String,
    pub name: String,
}

enum Request {
    Ensure,
    Status,
    Bands,
    Providers,
    List(Source),
    Play(Track),
    PlayIndex(usize),
    Lyrics,
    Cover(String),
    Load(String, String),
    Toggle,
    Pause,
    Next,
    Prev,
    Stop,
    Volume(f64),
    EqPreset(String),
    EqBand(usize, f64),
}

enum Reply {
    Ready(Result<(), String>),
    Status(Status),
    Bands(Vec<f32>),
    Providers(Vec<Provider>),
    List(Source, Result<Vec<Item>, String>),
    Error(String),
    Played,
    Lyrics(Vec<(f64, String)>),
    Cover(Option<PathBuf>),
}

/// The scene's handle: state mirrors plus the worker channel.
pub struct Music {
    tx: Sender<Request>,
    rx: Receiver<Reply>,
    pub status: Status,
    pub bands: Vec<f32>,
    pub providers: Vec<Provider>,
    pub lists: HashMap<Source, Result<Vec<Item>, String>>,
    pub loading: Option<Source>,
    pub ready: Option<Result<(), String>>,
    pub error: Option<String>,
    /// Set for one poll after a play or load request completed.
    pub played: bool,
    last_status: f64,
    last_bands: f64,
    /// Pulse sink the launcher's own audio goes to; cliamp follows it.
    sink: Option<String>,
    /// Stream URLs starred by the listener.
    pub favorites: std::collections::HashSet<String>,
    /// Synced lyrics of what plays, `(seconds, line)`, and which song they are for.
    pub lyrics: Vec<(f64, String)>,
    lyrics_for: String,
    /// Album art of what plays as a 96 px PNG in the cache, once fetched.
    pub cover: Option<PathBuf>,
    cover_for: String,
    /// When the engine was last looked for, before it is known to be there.
    last_probe: f64,
    /// Art of what we asked to play: a station's logo, which cliamp's status
    /// does not carry back.
    played_art: (String, String),
}

impl Music {
    pub fn new(sink: Option<String>) -> Self {
        let (tx, req_rx) = channel::<Request>();
        let (rep_tx, rx) = channel::<Reply>();
        let worker_sink = sink.clone();
        std::thread::Builder::new()
            .name("music".into())
            .spawn(move || worker(req_rx, rep_tx, worker_sink))
            .expect("music worker");
        Self {
            tx,
            rx,
            status: Status::default(),
            bands: vec![0.0; 10],
            providers: Vec::new(),
            lists: HashMap::new(),
            loading: None,
            ready: None,
            error: None,
            played: false,
            last_status: 0.0,
            last_bands: 0.0,
            sink,
            favorites: load_favorites().iter().map(|t| t.path.clone()).collect(),
            lyrics: Vec::new(),
            lyrics_for: String::new(),
            cover: None,
            cover_for: String::new(),
            last_probe: 0.0,
            played_art: (String::new(), String::new()),
        }
    }

    /// Ask for the art of what plays when the status carries none: a station's
    /// favicon from the tuned list, or a Spotify track by its URI.
    pub fn want_cover(&mut self, art: &str) {
        if art.is_empty() || art == self.cover_for {
            return;
        }
        self.cover_for = art.to_string();
        self.cover = None;
        let _ = self.tx.send(Request::Cover(art.to_string()));
    }

    /// The ten band gains, always ten long even before the first status.
    pub fn eq_bands(&self) -> Vec<f64> {
        let mut b = self.status.eq_bands.clone();
        b.resize(EQ_BANDS, 0.0);
        b
    }

    /// One of cliamp's presets, by name.
    pub fn eq_set_preset(&mut self, name: &str) {
        self.status.eq_preset = name.to_string();
        self.status.eq_bands.clear();
        let _ = self.tx.send(Request::EqPreset(name.to_string()));
    }

    /// Move one band; the preset becomes Custom, as cliamp reports it.
    pub fn eq_set_band(&mut self, i: usize, db: f64) {
        let db = db.clamp(EQ_MIN, EQ_MAX);
        let mut bands = self.eq_bands();
        if i >= bands.len() {
            return;
        }
        bands[i] = db;
        self.status.eq_bands = bands;
        self.status.eq_preset = "Custom".into();
        let _ = self.tx.send(Request::EqBand(i, db));
    }

    /// Absolute volume in dB.
    pub fn volume_set(&mut self, v: f64) {
        let v = v.clamp(-30.0, 6.0);
        self.status.volume = v;
        let _ = self.tx.send(Request::Volume(v));
    }

    pub fn is_favorite(&self, t: &Track) -> bool {
        self.favorites.contains(&t.path)
    }

    /// Star or unstar a station; the favourites list is refetched next time.
    pub fn toggle_favorite(&mut self, t: &Track) -> bool {
        let mut list = load_favorites();
        let now_fav = if let Some(i) = list.iter().position(|x| x.path == t.path) {
            list.remove(i);
            self.favorites.remove(&t.path);
            false
        } else {
            list.push(t.clone());
            self.favorites.insert(t.path.clone());
            true
        };
        if let Err(e) = save_favorites(&list) {
            self.error = Some(format!("favourites: {e}"));
        }
        self.lists.remove(&Source::Favorites);
        now_fav
    }

    pub fn available() -> bool {
        which("cliamp")
    }

    /// Start the daemon when it is not running and read the providers again:
    /// a `cliamp setup` done meanwhile shows up on the next open.
    pub fn ensure(&mut self) {
        if self.ready.is_none() {
            let _ = self.tx.send(Request::Ensure);
        }
        let _ = self.tx.send(Request::Providers);
    }

    /// Called every frame: polls status at 2 Hz, the spectrum at 20 Hz while
    /// something visualises it, and drains the worker's replies.
    pub fn tick(&mut self, now: f64, visualising: bool) {
        // Before anything opened the music screens: if the engine is already
        // running, follow what it plays, so the home screen can show it. No
        // daemon is started for this, that waits for ensure().
        if self.ready.is_none() && now - self.last_probe > 3.0 && socket_path().exists() {
            self.last_probe = now;
            let _ = self.tx.send(Request::Status);
        }
        if self.ready.as_ref().is_some_and(|r| r.is_ok()) {
            if now - self.last_status > 0.5 {
                self.last_status = now;
                let _ = self.tx.send(Request::Status);
            }
            // The spectrum: fast while it is on screen, a trickle otherwise so
            // the lists can breathe with the beat.
            let period = if visualising { 0.05 } else { 0.125 };
            if self.status.playing() && now - self.last_bands > period {
                self.last_bands = now;
                let _ = self.tx.send(Request::Bands);
            }
        }
        self.played = false;
        loop {
            match self.rx.try_recv() {
                Ok(Reply::Ready(r)) => self.ready = Some(r),
                Ok(Reply::Status(s)) => {
                    if !s.playing() {
                        for b in self.bands.iter_mut() {
                            *b *= 0.8;
                        }
                    }
                    self.status = s;
                    // A new song (or a new ICY title on a stream): lyrics and art.
                    if let Some(t) = &self.status.track {
                        let key = format!("{}|{}|{}", t.path, t.artist, t.title);
                        if key != self.lyrics_for && self.status.active() {
                            self.lyrics_for = key.clone();
                            self.lyrics.clear();
                            let _ = self.tx.send(Request::Lyrics);
                        }
                        // Spotify gives no art over the socket; its public
                        // oEmbed does. A station's logo came with the list we
                        // played from and is remembered here.
                        let art = if !t.art.is_empty() {
                            t.art.clone()
                        } else if t.path.starts_with("spotify:track:") {
                            t.path.clone()
                        } else if self.played_art.0 == t.path {
                            self.played_art.1.clone()
                        } else {
                            String::new()
                        };
                        if !art.is_empty() && art != self.cover_for {
                            self.cover_for = art.clone();
                            self.cover = None;
                            let _ = self.tx.send(Request::Cover(art));
                        }
                    }
                }
                Ok(Reply::Lyrics(l)) => self.lyrics = l,
                Ok(Reply::Cover(c)) => self.cover = c,
                Ok(Reply::Bands(b)) => self.bands = b,
                Ok(Reply::Providers(p)) => self.providers = p,
                Ok(Reply::List(src, items)) => {
                    if self.loading.as_ref() == Some(&src) {
                        self.loading = None;
                    }
                    self.lists.insert(src, items);
                }
                Ok(Reply::Error(e)) => {
                    // The daemon went away: start it again on the next open.
                    if e.contains("Connection refused") || e.contains("No such file") {
                        self.ready = None;
                        self.status = Status::default();
                    } else {
                        self.error = Some(e);
                    }
                }
                Ok(Reply::Played) => {
                    self.played = true;
                    self.last_status = 0.0;
                }
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
    }

    /// Ask for a list unless it is cached or already loading.
    pub fn open(&mut self, src: &Source) {
        if self.lists.contains_key(src) || self.loading.as_ref() == Some(src) {
            return;
        }
        self.loading = Some(src.clone());
        let _ = self.tx.send(Request::List(src.clone()));
    }

    /// Drop a cached list so the next open fetches it again.
    pub fn refresh(&mut self, src: &Source) {
        self.lists.remove(src);
    }

    pub fn play(&mut self, t: &Track) {
        self.played_art = (t.path.clone(), t.art.clone());
        self.status.track = Some(t.clone());
        self.status.state = Some(State::Playing);
        self.status.position = 0.0;
        let _ = self.tx.send(Request::Play(t.clone()));
    }

    /// Play the queue's `i`th track instead of appending it again.
    pub fn play_index(&mut self, i: usize) {
        self.status.state = Some(State::Playing);
        let _ = self.tx.send(Request::PlayIndex(i));
    }

    pub fn load(&mut self, provider: &str, playlist: &str) {
        let _ = self
            .tx
            .send(Request::Load(provider.into(), playlist.into()));
    }

    pub fn toggle(&mut self) {
        self.status.state = match self.status.state {
            Some(State::Playing) => Some(State::Paused),
            Some(State::Paused) => Some(State::Playing),
            ref other => other.clone(),
        };
        let _ = self.tx.send(Request::Toggle);
    }

    /// Pause when something else is about to use the speakers.
    pub fn hush(&mut self) {
        if self.status.playing() {
            self.status.state = Some(State::Paused);
            let _ = self.tx.send(Request::Pause);
        }
    }

    pub fn next(&mut self) {
        let _ = self.tx.send(Request::Next);
    }

    pub fn prev(&mut self) {
        let _ = self.tx.send(Request::Prev);
    }

    pub fn stop(&mut self) {
        self.status.state = Some(State::Stopped);
        let _ = self.tx.send(Request::Stop);
    }

    /// Volume step in dB within cliamp's range.
    pub fn volume_step(&mut self, delta: f64) {
        let v = (self.status.volume + delta).clamp(-30.0, 6.0);
        self.status.volume = v;
        let _ = self.tx.send(Request::Volume(v));
    }

    pub fn sink(&self) -> Option<&str> {
        self.sink.as_deref()
    }
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

// ------------------------------------------------------------------ worker

fn worker(rx: Receiver<Request>, tx: Sender<Reply>, sink: Option<String>) {
    while let Ok(req) = rx.recv() {
        let reply = match req {
            Request::Ensure => Reply::Ready(ensure_daemon()),
            Request::Status => match call(json!({ "cmd": "status" })) {
                Ok(v) => Reply::Status(parse_status(&v)),
                Err(e) => Reply::Error(e),
            },
            Request::Bands => match call(json!({ "cmd": "bands" })) {
                Ok(v) => Reply::Bands(
                    v.get("bands")
                        .and_then(Value::as_array)
                        .map(|a| a.iter().map(|b| b.as_f64().unwrap_or(0.0) as f32).collect())
                        .unwrap_or_default(),
                ),
                Err(e) => Reply::Error(e),
            },
            Request::Providers => match call(json!({ "cmd": "provider.list" })) {
                Ok(v) => Reply::Providers(
                    v.get("providers")
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .map(|p| Provider {
                                    key: p.get("key").and_then(Value::as_str).unwrap_or("").into(),
                                    name: p
                                        .get("name")
                                        .and_then(Value::as_str)
                                        .unwrap_or("")
                                        .into(),
                                })
                                .filter(|p| !p.key.is_empty())
                                .collect()
                        })
                        .unwrap_or_default(),
                ),
                Err(e) => Reply::Error(e),
            },
            Request::List(src) => {
                let items = list(&src);
                Reply::List(src, items)
            }
            Request::Play(t) => {
                let r = call(json!({ "cmd": "track.play", "track": t.to_json() }));
                follow_sink(&sink);
                match r {
                    Ok(_) => Reply::Played,
                    Err(e) => Reply::Error(e),
                }
            }
            Request::PlayIndex(i) => {
                let r = call(json!({ "cmd": "queue.play", "index": i }));
                follow_sink(&sink);
                match r {
                    Ok(_) => Reply::Played,
                    Err(e) => Reply::Error(e),
                }
            }
            Request::Load(provider, playlist) => {
                let r = call(json!({
                    "cmd": "provider.load", "provider": provider, "playlist": playlist, "play": true
                }));
                follow_sink(&sink);
                match r {
                    Ok(_) => Reply::Played,
                    Err(e) => Reply::Error(e),
                }
            }
            Request::Lyrics => {
                // Lyrics arrive a little after the song starts; ask twice.
                let mut lines = fetch_lyrics();
                if lines.is_empty() {
                    std::thread::sleep(Duration::from_millis(2500));
                    lines = fetch_lyrics();
                }
                Reply::Lyrics(lines)
            }
            Request::Cover(url) => Reply::Cover(fetch_cover(&url)),
            Request::Toggle => simple("toggle"),
            Request::Pause => simple("pause"),
            Request::Next => simple("next"),
            Request::Prev => simple("prev"),
            Request::Stop => simple("stop"),
            Request::EqPreset(name) => match call(json!({ "cmd": "eq", "name": name })) {
                Ok(_) => Reply::Played,
                Err(e) => Reply::Error(e),
            },
            Request::EqBand(i, db) => match call(json!({ "cmd": "eq", "band": i, "value": db })) {
                Ok(_) => Reply::Played,
                Err(e) => Reply::Error(e),
            },
            Request::Volume(v) => match call(json!({ "cmd": "volume", "value": v })) {
                Ok(_) => Reply::Played,
                Err(e) => Reply::Error(e),
            },
        };
        if tx.send(reply).is_err() {
            return;
        }
    }
}

fn fetch_lyrics() -> Vec<(f64, String)> {
    match call(json!({ "cmd": "lyrics" })) {
        Ok(v) => v
            .get("lyrics")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|l| {
                        let start = l.get("start").and_then(Value::as_f64)?;
                        let text = l.get("text").and_then(Value::as_str)?.trim().to_string();
                        if text.is_empty() {
                            return None;
                        }
                        Some((start, text))
                    })
                    .collect()
            })
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Album art for the cassette label: downloaded once per URL into the cache,
/// scaled by ffmpeg to a 96 px PNG (the launcher only decodes PNG).
fn fetch_cover(url: &str) -> Option<PathBuf> {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cache"))
        .join("omarchy-crt")
        .join("music-art");
    std::fs::create_dir_all(&cache).ok()?;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in url.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    let png = cache.join(format!("{hash:016x}.png"));
    if !png.is_file() {
        // A Spotify URI: the oEmbed endpoint answers without a key and
        // names the cover image.
        let mut url = url.to_string();
        if let Some(id) = url.strip_prefix("spotify:track:") {
            let out = crate::net::curl(15, 26_214_400)
                .arg(format!(
                    "https://open.spotify.com/oembed?url=spotify:track:{id}"
                ))
                .output()
                .ok()?;
            let v: Value = serde_json::from_slice(&out.stdout).ok()?;
            url = v.get("thumbnail_url")?.as_str()?.to_string();
        }
        let raw = cache.join(format!("{hash:016x}.tmp"));
        let ok = crate::net::curl(15, 26_214_400)
            .arg("-o")
            .arg(&raw)
            .arg(&url)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            return None;
        }
        let ok = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-i"])
            .arg(&raw)
            .args([
                "-vf",
                "scale=96:96:force_original_aspect_ratio=decrease",
                "-frames:v",
                "1",
            ])
            .arg(&png)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        let _ = std::fs::remove_file(&raw);
        if !ok {
            return None;
        }
    }
    Some(png)
}

fn simple(cmd: &str) -> Reply {
    match call(json!({ "cmd": cmd })) {
        Ok(_) => Reply::Played,
        Err(e) => Reply::Error(e),
    }
}

/// cliamp opens its own ALSA stream through PipeWire; when the launcher's
/// audio is routed to the tube, that stream goes there too.
fn follow_sink(sink: &Option<String>) {
    if let Some(s) = sink {
        std::thread::sleep(Duration::from_millis(600));
        crate::crt::audio::move_streams(s);
    }
}

/// Connect to the daemon, starting it when nothing listens.
fn ensure_daemon() -> Result<(), String> {
    if call(json!({ "cmd": "status" })).is_ok() {
        return Ok(());
    }
    if !which("cliamp") {
        return Err("cliamp is not installed".into());
    }
    let mut cmd = std::process::Command::new("cliamp");
    cmd.arg("--daemon")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    {
        use std::os::unix::process::CommandExt;
        // Its own session: the daemon outlives a launcher restart.
        cmd.process_group(0);
    }
    cmd.spawn().map_err(|e| format!("cliamp --daemon: {e}"))?;
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(100));
        if call(json!({ "cmd": "status" })).is_ok() {
            return Ok(());
        }
    }
    Err("cliamp did not answer".into())
}

/// One request, one JSON line back. Errors carry cliamp's message.
fn call(req: Value) -> Result<Value, String> {
    let mut s = UnixStream::connect(socket_path()).map_err(|e| format!("cliamp: {e}"))?;
    s.set_read_timeout(Some(Duration::from_secs(30))).ok();
    s.set_write_timeout(Some(Duration::from_secs(5))).ok();
    let mut line = serde_json::to_string(&req).map_err(|e| e.to_string())?;
    line.push('\n');
    s.write_all(line.as_bytes())
        .map_err(|e| format!("cliamp: {e}"))?;
    let mut reader = BufReader::new(s);
    let mut out = String::new();
    reader
        .read_line(&mut out)
        .map_err(|e| format!("cliamp: {e}"))?;
    let v: Value = serde_json::from_str(out.trim()).map_err(|e| format!("cliamp: {e}"))?;
    if v.get("ok").and_then(Value::as_bool) == Some(false) {
        return Err(v
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("cliamp refused")
            .to_string());
    }
    Ok(v)
}

fn parse_status(v: &Value) -> Status {
    let state = match v.get("state").and_then(Value::as_str) {
        Some("playing") => Some(State::Playing),
        Some("paused") => Some(State::Paused),
        Some(_) => Some(State::Stopped),
        None => None,
    };
    let track = v
        .get("track")
        .filter(|t| t.is_object())
        .map(Track::from_json);
    let num = |k: &str| v.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    Status {
        state,
        track,
        position: num("position"),
        duration: num("duration"),
        volume: num("volume"),
        index: v.get("index").and_then(Value::as_u64).unwrap_or(0) as usize,
        total: v.get("total").and_then(Value::as_u64).unwrap_or(0) as usize,
        eq_preset: v
            .get("eq_preset")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        eq_bands: v
            .get("eq_bands")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_f64).collect())
            .unwrap_or_default(),
    }
}

fn tracks_of(v: &Value) -> Vec<Item> {
    v.get("tracks")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(|t| Item::Track(Track::from_json(t))).collect())
        .unwrap_or_default()
}

fn favorites_path() -> PathBuf {
    crate::crt::config_dir().join("radio-favorites.tsv")
}

/// `title<TAB>url<TAB>note` per line.
fn load_favorites() -> Vec<Track> {
    std::fs::read_to_string(favorites_path())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut parts = l.split('\t');
            let title = parts.next()?.trim();
            let path = parts.next()?.trim();
            if title.is_empty() || path.is_empty() {
                return None;
            }
            Some(Track {
                title: title.to_string(),
                path: path.to_string(),
                stream: true,
                realtime: true,
                station: title.to_string(),
                note: parts.next().unwrap_or("").trim().to_string(),
                ..Track::default()
            })
        })
        .collect()
}

fn save_favorites(list: &[Track]) -> std::io::Result<()> {
    let path = favorites_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text: String = list
        .iter()
        .map(|t| {
            let title = if t.station.is_empty() {
                &t.title
            } else {
                &t.station
            };
            format!("{}\t{}\t{}\n", title.replace('\t', " "), t.path, t.note)
        })
        .collect();
    crate::store::save(&path, text)
}

fn list(src: &Source) -> Result<Vec<Item>, String> {
    match src {
        Source::Favorites => Ok(load_favorites().into_iter().map(Item::Track).collect()),
        Source::ProviderSearch(provider, query) => {
            if query.trim().is_empty() {
                return Ok(Vec::new());
            }
            let v = call(
                json!({ "cmd": "provider.search", "provider": provider, "query": query, "limit": 10 }),
            )?;
            Ok(tracks_of(&v))
        }
        Source::Country(code, _) => stations(&format!(
            "{RADIO_BROWSER}/stations/bycountrycodeexact/{code}?order=votes&reverse=true&hidebroken=true&limit={STATIONS}"
        )),
        Source::Tag(tag) => stations(&format!(
            "{RADIO_BROWSER}/stations/bytagexact/{}?order=votes&reverse=true&hidebroken=true&limit={STATIONS}",
            urlencode(tag)
        )),
        Source::Countries => {
            let v = fetch(&format!(
                "{RADIO_BROWSER}/countries?order=stationcount&reverse=true&limit=120"
            ))?;
            let mut out = Vec::new();
            for c in v.as_array().ok_or("countries: not a list")? {
                let name = c.get("name").and_then(Value::as_str).unwrap_or("");
                let code = c.get("iso_3166_1").and_then(Value::as_str).unwrap_or("");
                let n = c.get("stationcount").and_then(Value::as_u64).unwrap_or(0);
                if name.is_empty() || code.is_empty() || n < 20 {
                    continue;
                }
                out.push(Item::Source(
                    Source::Country(code.to_string(), short_country(name)),
                    format!("{n:>5}"),
                ));
            }
            Ok(out)
        }
        Source::Tags => {
            let v = fetch(&format!(
                "{RADIO_BROWSER}/tags?order=stationcount&reverse=true&limit=400"
            ))?;
            let mut out = Vec::new();
            for t in v.as_array().ok_or("tags: not a list")? {
                let name = t.get("name").and_then(Value::as_str).unwrap_or("");
                let n = t.get("stationcount").and_then(Value::as_u64).unwrap_or(0);
                if !genre_like(name) {
                    continue;
                }
                out.push(Item::Source(
                    Source::Tag(name.to_string()),
                    format!("{n:>5}"),
                ));
                if out.len() >= 60 {
                    break;
                }
            }
            Ok(out)
        }
        Source::ProviderPlaylists(provider, _) => {
            let v = call(json!({ "cmd": "provider.playlists", "provider": provider }))?;
            let mut out = Vec::new();
            for p in v
                .get("playlists")
                .and_then(Value::as_array)
                .unwrap_or(&Vec::new())
            {
                let id = p.get("id").and_then(Value::as_str).unwrap_or("");
                let name = p.get("name").and_then(Value::as_str).unwrap_or("");
                if id.is_empty() {
                    continue;
                }
                out.push(Item::Source(
                    Source::ProviderPlaylist(provider.clone(), id.to_string(), name.to_string()),
                    String::new(),
                ));
            }
            Ok(out)
        }
        Source::ProviderPlaylist(provider, id, _) => {
            let v =
                call(json!({ "cmd": "provider.tracks", "provider": provider, "playlist": id }))?;
            Ok(tracks_of(&v))
        }
        Source::Queue => Ok(tracks_of(&call(json!({ "cmd": "queue.list" }))?)),
        Source::History => {
            let v = call(json!({ "cmd": "history", "limit": 100 }))?;
            let mut out = Vec::new();
            for h in v
                .get("history")
                .and_then(Value::as_array)
                .unwrap_or(&Vec::new())
            {
                let mut t = Track::from_json(h);
                if t.path.is_empty() {
                    continue;
                }
                t.stream = t.path.starts_with("http");
                out.push(Item::Track(t));
            }
            Ok(out)
        }
    }
}

/// Radio Browser stations to tracks, deduplicated by stream URL.
fn stations(url: &str) -> Result<Vec<Item>, String> {
    let v = fetch(url)?;
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for s in v.as_array().ok_or("stations: not a list")? {
        let name = s.get("name").and_then(Value::as_str).unwrap_or("").trim();
        let stream = s
            .get("url_resolved")
            .and_then(Value::as_str)
            .filter(|u| !u.is_empty())
            .or_else(|| s.get("url").and_then(Value::as_str))
            .unwrap_or("");
        if name.is_empty()
            || !(stream.starts_with("http://") || stream.starts_with("https://"))
            || !seen.insert(stream.to_string())
        {
            continue;
        }
        let favicon = s
            .get("favicon")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let codec = s.get("codec").and_then(Value::as_str).unwrap_or("");
        let kbps = s.get("bitrate").and_then(Value::as_u64).unwrap_or(0);
        let note = match (kbps, codec.is_empty()) {
            (0, true) => String::new(),
            (0, false) => codec.to_string(),
            (k, true) => format!("{k}k"),
            (k, false) => format!("{k}k {codec}"),
        };
        out.push(Item::Track(Track {
            title: name.to_string(),
            path: stream.to_string(),
            stream: true,
            realtime: true,
            station: name.to_string(),
            note,
            art: if favicon.starts_with("http") {
                favicon
            } else {
                String::new()
            },
            ..Track::default()
        }));
    }
    Ok(out)
}

/// GET JSON with curl: the launcher has no HTTP client of its own and the
/// directory is HTTPS.
fn fetch(url: &str) -> Result<Value, String> {
    let out = crate::net::curl(12, 26_214_400)
        .arg(url)
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    if !out.status.success() {
        return Err("radio directory unreachable".into());
    }
    serde_json::from_slice(&out.stdout).map_err(|_| "radio directory: bad answer".to_string())
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Radio Browser's tag index is community written: keep short ASCII words
/// that read as a genre or a format, drop the noise.
fn genre_like(tag: &str) -> bool {
    const NOISE: [&str; 20] = [
        "music",
        "radio",
        "fm",
        "am",
        "estación",
        "entretenimiento",
        "misc",
        "various",
        "variety",
        "local",
        "community",
        "regional",
        "online",
        "web",
        "internet",
        "hits",
        "station",
        "musica",
        "música",
        "moi merino",
    ];
    let t = tag.trim();
    t.len() >= 3
        && t.len() <= 18
        && t.is_ascii()
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '&')
        && !NOISE.contains(&t)
}

/// Directory names are long ("The United States Of America"); menus want short.
fn short_country(name: &str) -> String {
    match name {
        "The United States Of America" => "United States".into(),
        "The United Kingdom Of Great Britain And Northern Ireland" => "United Kingdom".into(),
        "The Russian Federation" => "Russia".into(),
        "The Netherlands" => "Netherlands".into(),
        "The Republic Of Korea" => "South Korea".into(),
        "Iran, Islamic Republic Of" => "Iran".into(),
        "Taiwan, Republic Of China" => "Taiwan".into(),
        "The Czech Republic" | "Czechia" => "Czechia".into(),
        other => other.strip_prefix("The ").unwrap_or(other).to_string(),
    }
}

/// Country the listener most likely wants first: the settings, else the
/// region of the locale (`it_IT` gives IT), else nothing.
pub fn home_country(setting: &str) -> Option<(String, String)> {
    let code = if !setting.trim().is_empty() {
        setting.trim().to_ascii_uppercase()
    } else {
        let lang = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .find(|v| !v.is_empty())?;
        let region = lang.split('.').next()?.split('_').nth(1)?;
        region.to_ascii_uppercase()
    };
    if code.len() != 2 {
        return None;
    }
    let name = country_name(&code);
    Some((code, name))
}

fn country_name(code: &str) -> String {
    match code {
        "IT" => "Italy",
        "US" => "United States",
        "GB" => "United Kingdom",
        "DE" => "Germany",
        "FR" => "France",
        "ES" => "Spain",
        "PT" => "Portugal",
        "NL" => "Netherlands",
        "BE" => "Belgium",
        "CH" => "Switzerland",
        "AT" => "Austria",
        "SE" => "Sweden",
        "NO" => "Norway",
        "DK" => "Denmark",
        "FI" => "Finland",
        "PL" => "Poland",
        "CZ" => "Czechia",
        "GR" => "Greece",
        "IE" => "Ireland",
        "CA" => "Canada",
        "MX" => "Mexico",
        "BR" => "Brazil",
        "AR" => "Argentina",
        "JP" => "Japan",
        "KR" => "South Korea",
        "AU" => "Australia",
        "NZ" => "New Zealand",
        "IN" => "India",
        "RU" => "Russia",
        "TR" => "Turkey",
        other => other,
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_region() {
        assert_eq!(home_country("it"), Some(("IT".into(), "Italy".into())));
        assert_eq!(home_country(" de ").map(|c| c.0), Some("DE".into()));
        assert_eq!(home_country("xyz"), None);
    }

    #[test]
    fn tag_filter() {
        assert!(genre_like("jazz"));
        assert!(genre_like("classic rock"));
        assert!(!genre_like("music"));
        assert!(!genre_like("méxico"));
        assert!(!genre_like("a"));
    }

    #[test]
    fn labels() {
        let t = Track {
            title: "Song".into(),
            artist: "Band".into(),
            ..Track::default()
        };
        assert_eq!(t.label(), "Band - Song");
        assert_eq!(urlencode("classic rock"), "classic%20rock");
        assert_eq!(fold("Stereocittà è qui"), "Stereocitta e qui");
    }
}
