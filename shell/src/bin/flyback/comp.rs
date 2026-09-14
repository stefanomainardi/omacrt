//! Flyback: a Wayland compositor for a television.
//!
//! Named after what a tube does between two lines, which is also what the
//! project's mark draws: the beam running back with the gun switched off.
//!
//! What makes it unlike every other compositor is the output it takes. It
//! does not ask for a screen the desktop is using; it takes the one the
//! desktop has been told to leave alone, through `wp_drm_lease_v1`, and
//! programs a fifteen kilohertz modeline on it. A kiosk compositor puts one
//! window on a monitor. This one owns the scanout of a television.
//!
//! One leased DRM output, a handful of clients (the launcher, RetroArch,
//! mpv), every toplevel fullscreen at the output's size. The stacking order
//! decides what is seen; every mapped surface keeps receiving frame
//! callbacks, so a program under the pause overlay keeps running and keeps
//! answering. Clients reach us through `WAYLAND_DISPLAY=wayland-crt`.
//!
//! # Frame scheduling
//!
//! Three questions, once a frame: when to tell the clients they may draw,
//! when to draw, and when to put the result in the air. What the television
//! will accept decides the answers, and there are two regimes.
//!
//! **At a fixed refresh** a flip has to be queued before the vblank or the
//! picture waits a whole frame, so there is a deadline and everything is
//! arranged around not missing it. The compositor draws at the last safe
//! moment - one frame less a margin taken from what recent frames cost - and
//! tells the clients early enough that their commit arrives before that.
//! Drawing at the start of the frame instead, which is what it used to do,
//! throws away almost all of a frame: the flip goes out before the client has
//! committed, and the client's picture then waits for the one after.
//!
//! **At a variable refresh** there is no deadline at all. A flip that arrives
//! after the frame's minimum length simply makes that frame longer, and
//! nothing is dropped. So nothing is held back: the compositor draws the
//! moment a client commits, and tells the clients as late as it dares.
//!
//! The one thing it must not do with a variable rate is draw at the vblank.
//! Anything committed while the last flip was in flight is about to be
//! superseded by the frame the client is drawing now, and flipping it puts a
//! stale picture in the air that the fresh one then waits behind. That single
//! mistake cost a frame and a half.
//!
//! Two estimates feed all of this, both measured rather than assumed, because
//! between telling a client to draw and seeing its picture scanned out sit a
//! wake-up, a draw, a flip and a hardware latch that no constant would get
//! right. `render_cost` is what this compositor takes to draw and queue, a
//! decaying maximum so a heavy scene moves the deadline earlier by itself.
//! `client_cost` is what a client takes between being told and committing,
//! kept per window and read from the one on top: the callbacks all go out
//! together, so a single estimate would be the slowest client on the tube,
//! and with the launcher mapped under a game that is the launcher. When a
//! rate has been asked for by name, a third one closes the loop: `rate_trim`
//! is measured error, a quarter of it corrected each frame.
//!
//! A client cannot choose its own rate, which is worth knowing before reading
//! the rest: paced by frame callbacks it runs at the cadence it is given, and
//! the cadence is taken from the cadence it runs at. Whatever it settles on
//! is where it stays. That is why the rate is asked for - `rate 59.92` on the
//! control pipe - rather than discovered.
//!
//! # Notes
//!
//! The `lock().unwrap()` on smithay's own surface data, which appears a few
//! times below, panics only on a poisoned mutex: another thread panicked
//! while holding it. There is no compositor left to run at that point.

use crate::drm_mode;
use crate::lease::Lease;
use omacrt_shell::crt::output::Modeline;
use omacrt_shell::crt::{Config, dac, display, output};
use smithay::backend::allocator::Fourcc;
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::drm::VrrSupport;
use smithay::backend::drm::compositor::{FrameFlags, PrimaryPlaneElement};
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
use smithay::backend::drm::output::{DrmOutput, DrmOutputManager, DrmOutputRenderElements};
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent};
use smithay::backend::drm::{DrmEventMetadata, DrmEventTime};
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::input::KeyState;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::default_primary_scanout_output_compare;
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::gles::GlesRenderbuffer;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::utils::on_commit_buffer_handler;
use smithay::backend::renderer::{
    Bind, ExportMem, ImportDma, ImportEgl, Offscreen, TextureMapping,
};
use smithay::desktop::space::{SpaceRenderElements, space_render_elements};
use smithay::desktop::utils::{
    OutputPresentationFeedback, surface_presentation_feedback_flags_from_states,
    surface_primary_scanout_output, update_surface_primary_scanout_output,
};
use smithay::desktop::{Space, Window};
use smithay::input::keyboard::{FilterResult, Keycode};
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::output::{Mode as WlMode, Output, PhysicalProperties, Scale, Subpixel};
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{
    EventLoop, Interest, LoopHandle, Mode, PostAction, generic::Generic,
};
use smithay::reexports::drm::control::Device as _;
use smithay::reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::reexports::wayland_server::protocol::{wl_buffer, wl_seat, wl_surface::WlSurface};
use smithay::reexports::wayland_server::{Client, Display, DisplayHandle};
use smithay::utils::Rectangle;
use smithay::utils::{DeviceFd, Serial, Transform};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    CompositorClientState, CompositorHandler, CompositorState, get_parent, is_sync_subsurface,
    with_states,
};
use smithay::wayland::dmabuf::{
    DmabufFeedbackBuilder, DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier,
};
use smithay::wayland::output::{OutputHandler, OutputManagerState};
use smithay::wayland::presentation::{PresentationState, Refresh};
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
    XdgToplevelSurfaceData,
};
use smithay::wayland::shm::{ShmHandler, ShmState};
use smithay::wayland::socket::ListeningSocketSource;
use smithay::wayland::viewporter::ViewporterState;
use smithay::{
    delegate_compositor, delegate_dmabuf, delegate_output, delegate_presentation, delegate_seat,
    delegate_shm, delegate_viewporter, delegate_xdg_shell,
};
use std::ffi::OsString;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const SOCKET: &str = "wayland-crt";

type Allocator = GbmAllocator<DrmDeviceFd>;
type Exporter = GbmFramebufferExporter<DrmDeviceFd>;
type Element = SpaceRenderElements<GlesRenderer, WaylandSurfaceRenderElement<GlesRenderer>>;

/// What a queued frame carries until the flip completes: the promise made to
/// every client whose surface is in it. On vblank the promise is kept with
/// the time the flip actually happened, which is the only way a client can
/// know when its picture reached the screen rather than guess.
type Frame = Option<OutputPresentationFeedback>;

pub struct Crt {
    start: Instant,
    dh: DisplayHandle,
    handle: LoopHandle<'static, Crt>,
    pub socket_name: OsString,
    space: Space<Window>,
    output: Output,
    compositor_state: CompositorState,
    xdg_shell_state: XdgShellState,
    shm_state: ShmState,
    _output_manager_state: OutputManagerState,
    seat_state: SeatState<Crt>,
    seat: Seat<Crt>,
    dmabuf_state: DmabufState,
    _dmabuf_global: DmabufGlobal,
    renderer: GlesRenderer,
    _drm: DrmOutputManager<Allocator, Exporter, Frame, DrmDeviceFd>,
    drm_output: Option<DrmOutput<Allocator, Exporter, Frame, DrmDeviceFd>>,
    frame_queued: bool,
    /// When the frame now in flight was queued. A page flip that is accepted
    /// is always followed by a vblank, so one that is not means the device
    /// has stopped answering and no error was reported anywhere.
    queued_at: Option<Instant>,
    /// The offscreen buffer the recording and the screenshot render into,
    /// and the mode it was made for. Kept between frames.
    capture_target: Option<GlesRenderbuffer>,
    capture_size: Option<(i32, i32)>,
    /// Page flips refused in a row. A driver that will not take a frame
    /// takes none of them, so the count only ever runs away.
    flips_failed: u32,
    /// Whether anything has changed since the last flip was queued. See
    /// `damaged`.
    dirty: bool,
    /// Whether a pacer tick is already on its way, so that the several paths
    /// that can ask for one do not stack up a timer each.
    pacer_armed: bool,
    /// Whether the television is following our flips rather than its own
    /// clock. With a variable refresh rate there is nothing to be late for:
    /// the picture is scanned out when it is given, so the compositor stops
    /// pacing itself and draws the moment a client commits.
    vrr_on: bool,
    /// The connector the lease gave us, kept so the refresh behaviour can be
    /// asked about after start-up.
    conn_handle: smithay::reexports::drm::control::connector::Handle,
    /// When a modeline was last asked for, until the first vblank in it.
    /// `use_mode` only tests the timing and stores it; the modeset itself
    /// happens on the next commit, so the time the television is dark is the
    /// distance from there to that vblank.
    mode_at: Option<Instant>,
    /// When the last vblank arrived, so the interval between two of them can
    /// be reported: that interval is the refresh rate the television is
    /// actually being given, whatever the mode says.
    last_vblank: Option<Instant>,
    /// The driver's own timestamp for the previous vblank, which is when the
    /// hardware flipped rather than when this process woke up.
    last_vblank_hw: Option<Duration>,
    /// Whether this frame's frame callbacks are already on their way.
    callback_armed: bool,
    /// When the clients were last told they may draw, until the first of
    /// them commits, and how long that took: a decaying maximum, the same
    /// shape as `render_cost` and for the same reason.
    told_at: Option<Instant>,
    /// The window on top has committed since the last flip, and the last
    /// time it did. While it is answering, it owns the flip cadence: see
    /// `damaged_by`.
    dirty_top: bool,
    last_top_commit: Option<Instant>,
    /// The timing currently on the connector, so that asking for the one
    /// that is already set can be answered with nothing. See `switch_mode`.
    modeline: Option<Modeline>,
    /// Which telling `told_at` belongs to, so each window counts a telling
    /// once. See `ClientCost`.
    telling: u64,
    /// Commits that arrived too late for the frame they were meant for.
    late: u32,
    /// The commit this frame is showing, from the moment it was queued until
    /// the vblank tells us the picture is on the glass.
    showing: Option<Instant>,
    /// How long the last few hundred frames took from a client's commit to
    /// the start of their scanout, and how long the frames themselves were.
    /// Written out every few seconds so the rest of the program can say both:
    /// with a variable refresh rate the second is no longer the mode's own
    /// and is the more interesting of the two.
    latencies: std::collections::VecDeque<Duration>,
    intervals: std::collections::VecDeque<Duration>,
    /// `FLYBACK_LATE_DRAW=off`: tell clients they may draw at the vblank,
    /// the way every other compositor does, rather than just in time.
    /// See `arm_callbacks`.
    late_draw: bool,
    /// `FLYBACK_SLACK_US`: how much a client is given on top of twice its
    /// own measured drawing time.
    client_slack: Duration,
    /// Whether this frame's drawing deadline is already on its way.
    deadline_armed: bool,
    /// Whether a timer is out waiting to see whether this field is going to
    /// run past what the television follows.
    floor_armed: bool,
    /// Bumped on every vblank, so a floor timer that was armed for a field
    /// already over can tell and do nothing.
    field_seq: u64,
    /// How many fields have been ended with a repeat rather than a fresh
    /// picture, for the log and for anybody asking whether the floor is
    /// doing anything.
    fields_repeated: u64,
    /// How long the last few frames took to draw and queue, decaying, so the
    /// deadline is set from what this machine and this scene actually cost
    /// rather than from a guess.
    render_cost: Duration,
    /// `FLYBACK_MARGIN_US`: a fixed margin in microseconds, or `off` to draw
    /// as soon as a client commits instead of waiting for the deadline.
    margin_override: Option<Duration>,
    frame_delay: bool,
    /// A frame length asked for by name rather than guessed from what a
    /// client happens to be doing. A program paced by frame callbacks runs
    /// at the cadence it is given and the cadence is taken from the cadence
    /// it runs at, so it can never choose one by itself: whatever rate it
    /// settles on is where it stays. Whoever started it knows better - an
    /// emulator's refresh is a property of the machine it is imitating - and
    /// this is where that is said.
    target_period: Option<Duration>,
    /// The band of line rates this display may be given, from
    /// `output.hfreq_khz`. See `Modeline::fault`.
    hfreq_band: [f64; 2],
    /// How far past its own frame this television will follow a stretched
    /// one, as a ratio, from `output.vrr_min_hz` against the frame that
    /// setting was measured on.
    ///
    /// A ratio and not a rate, because the rate alone belongs to one
    /// standard. Measured as 55 Hz on a 60.04 Hz frame, which is nine per
    /// cent; read as an absolute 55 it is *faster* than a PAL frame's own
    /// 50.08, so the floor came out shorter than the mode itself, the two
    /// ends of the range met, and the variable rate was off in PAL without
    /// anybody turning it off. A European game asking for 49.70 was held at
    /// 50.06 and dropped a frame every two and a half seconds.
    stretch: f64,
    /// Microseconds added to or taken off the moment a client is told to
    /// draw, so the frame comes out the length that was asked for.
    ///
    /// Aiming the commit at the target and hoping is not enough: between the
    /// telling and the scanout sit the client's wake-up, our own drawing,
    /// the flip and the hardware's latch, each a few hundred microseconds no
    /// estimate here would get right. So the error is measured instead - the
    /// frame that came out against the frame that was wanted - and a quarter
    /// of it corrected each time.
    rate_trim: i64,
    /// How long a client leaves between its own commits, and the recent
    /// ones it is taken from. Under a variable refresh rate that interval is
    /// the length of the frame the television should be given: the mode's
    /// own period is only the shortest frame the hardware will make, and
    /// pacing a program to it when it wants a longer one makes the two beat
    /// against each other, which on a tube is a picture that pulses.
    client_period: Duration,
    periods: std::collections::VecDeque<Duration>,
    /// When a client last committed, and how many commits arrived while a
    /// page flip was already in the air and so could not be drawn at once.
    /// Only kept when `FLYBACK_TRACE` is set: this is a measuring aid.
    trace: bool,
    last_commit: Option<Instant>,
    commits_blocked: u32,
    /// Whether the last frame reached the plane as the client's own buffer,
    /// or `None` before the first one. Kept only so the log says it once
    /// rather than sixty times a second.
    scanout_direct: Option<bool>,
    lease: Lease,
    running: bool,
    frames: u64,
    last_stats: Instant,
    /// Surface commits since the last statistics line, by app id.
    commits: std::collections::BTreeMap<String, u64>,
    /// The desktop side window (preview + keyboard), when opened.
    pub host: Option<crate::host::Host>,
    /// Video capture of the tube through ffmpeg, when recording.
    recorder: Option<Recorder>,
}

/// ffmpeg fed with raw frames of the tube (scaled to a 4:3 picture, each
/// line four pixels tall) and the HDMI sink's monitor as the audio track.
struct Recorder {
    child: std::process::Child,
    tx: std::sync::mpsc::SyncSender<Vec<u8>>,
    /// Buffers the writer has finished with, to be filled again.
    spent: std::sync::mpsc::Receiver<Vec<u8>>,
    thread: Option<std::thread::JoinHandle<()>>,
    frames: u64,
    path: String,
}

/// What one client takes between being told it may draw and committing.
///
/// Kept per window rather than once for the compositor, and it matters which.
/// The frame callbacks all go out together, so a single estimate is really
/// the slowest client on the tube; with the launcher still mapped underneath
/// a game, that is the launcher, and it is not the one being watched. A
/// window nobody can see answering late costs nothing. The window on top
/// answering late costs a frame. So the pacing follows the top window, and
/// the rest keep up as they can.
///
/// Measured: with the launcher mapped under it, a client that draws in a
/// millisecond saw 18.7 ms from commit to scanout, and 3.6 ms with the
/// launcher gone. Both numbers are the same compositor.
struct ClientCost {
    /// A decaying maximum, the same shape as `render_cost`: the deadline has
    /// to clear the worst of the last second rather than the average.
    cost: std::cell::Cell<Duration>,
    /// The telling this window has already been counted for. A client that
    /// commits twice after one callback is drawing more than once in a
    /// frame, not drawing slowly.
    counted: std::cell::Cell<u64>,
    /// Whether this window has ever been measured. Until it has, the first
    /// reading replaces the assumption rather than losing to it: a decaying
    /// maximum starting at a whole frame takes about fifty frames to come
    /// down to a millisecond, and for that half second a client that draws
    /// in a millisecond is told at the vblank and shown a frame late. It is
    /// the only half second of a program's life that anybody watches.
    measured: std::cell::Cell<bool>,
}

impl ClientCost {
    /// Assume the worst until a client has shown otherwise: a whole frame to
    /// draw in, which is where every other compositor leaves it.
    fn new() -> Self {
        ClientCost {
            cost: std::cell::Cell::new(Duration::from_millis(16)),
            counted: std::cell::Cell::new(0),
            measured: std::cell::Cell::new(false),
        }
    }
}

/// This window's estimate, made on first sight.
fn cost_of(window: &Window) -> &ClientCost {
    let data = window.user_data();
    data.insert_if_missing(ClientCost::new);
    // Just inserted if it was missing, so it is there.
    data.get::<ClientCost>().unwrap()
}

/// Page flips the connector may refuse in a row before the display process
/// gives up. Ten is about a sixth of a second of a picture that is not
/// arriving, long enough to ride out a mode change and short enough that the
/// watchdog puts the television back while somebody is still looking at it.
const FLIP_FAILURES_ALLOWED: u32 = 10;

/// How long a queued frame may go without its vblank. Two seconds is far
/// beyond any mode change and short enough that somebody watching sees the
/// television come back rather than wonder.
const FRAME_DEADLINE: Duration = Duration::from_secs(2);

const REC_W: usize = 1280;
const REC_H: usize = 960;

impl Recorder {
    fn start(path: &str, sink: Option<String>) -> Result<Recorder, String> {
        // Same reasoning as the screenshot: the recording goes to a film, and
        // a name that is not one is a way to truncate something else. The
        // extension check also keeps a leading dash from ever being the
        // output argument, which ffmpeg would read as an option.
        let name = path.to_ascii_lowercase();
        if !(name.ends_with(".mp4") || name.ends_with(".mkv") || name.ends_with(".webm")) {
            return Err(format!(
                "{path}: a recording is written to .mp4, .mkv or .webm"
            ));
        }
        let mut cmd = std::process::Command::new("ffmpeg");
        cmd.args(["-hide_banner", "-loglevel", "error", "-y"])
            .args(["-f", "rawvideo", "-pix_fmt", "bgra"])
            // Frames are stamped as they arrive and the output is forced to a
            // constant 30 fps: a frame dropped because the encoder was busy is
            // then filled in by holding the previous one, instead of shortening
            // the film. Without this a long recording plays faster than it was
            // shot and drifts away from its own sound, seconds of it over a
            // quarter of an hour.
            .args(["-use_wallclock_as_timestamps", "1"])
            .args([
                "-s",
                &format!("{REC_W}x{REC_H}"),
                "-r",
                "30",
                "-i",
                "pipe:0",
            ]);
        if let Some(sink) = &sink {
            cmd.args(["-f", "pulse", "-i", &format!("{sink}.monitor")]);
        }
        cmd.args([
            "-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-pix_fmt", "yuv420p",
        ])
        .args(["-fps_mode", "cfr", "-r", "30"]);
        if sink.is_some() {
            cmd.args(["-c:a", "aac", "-b:a", "192k", "-shortest"]);
        }
        // `--` first: the output is the last argument, and without it a path
        // beginning with a dash is an ffmpeg option. `player.rs` does the same
        // for mpv's target.
        cmd.arg("--")
            .arg(path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit());
        let mut child = cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;
        let mut stdin = child.stdin.take().ok_or("ffmpeg stdin")?;
        let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(8);
        // Written frames come back to be filled again: at 4.9 MB each, thirty
        // times a second, allocating one per frame is 147 MB a second through
        // the allocator while the compositor is trying to keep time.
        let (spent_tx, spent_rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let thread = std::thread::spawn(move || {
            use std::io::Write;
            for frame in rx {
                if stdin.write_all(&frame).is_err() {
                    break;
                }
                if spent_tx.send(frame).is_err() {
                    break;
                }
            }
            let _ = stdin.flush();
        });
        Ok(Recorder {
            child,
            tx,
            spent: spent_rx,
            thread: Some(thread),
            frames: 0,
            path: path.to_string(),
        })
    }

    /// A buffer to scale the next frame into: one the writer has finished
    /// with, or a new one the first few times.
    fn buffer(&mut self) -> Vec<u8> {
        let mut buf = self.spent.try_recv().unwrap_or_default();
        buf.clear();
        buf.resize(REC_W * REC_H * 4, 0);
        buf
    }

    /// Hand a filled buffer to the writer, dropping the frame rather than
    /// stalling the compositor when ffmpeg lags.
    fn push(&mut self, frame: Vec<u8>) {
        if self.tx.try_send(frame).is_ok() {
            self.frames += 1;
        }
    }

    fn stop(mut self) -> String {
        drop(self.tx);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        let _ = self.child.wait();
        format!("{} frames to {}", self.frames, self.path)
    }
}

#[derive(Default)]
pub struct ClientState {
    compositor_state: CompositorClientState,
}
impl ClientData for ClientState {
    fn initialized(&self, _: ClientId) {}
    fn disconnected(&self, _: ClientId, _: DisconnectReason) {}
}

pub fn run(connector: Option<&str>) -> Result<(), String> {
    let cfg = Config::load();
    let want = connector.map(str::to_string).unwrap_or_else(|| {
        if cfg.output.connector.is_empty() {
            "HDMI-A-1".into()
        } else {
            cfg.output
                .connector
                .trim_start_matches("card1-")
                .to_string()
        }
    });
    let standard = if cfg.output.standard.is_empty() {
        "ntsc"
    } else {
        cfg.output.standard.as_str()
    };
    let text = cfg
        .modeline(standard)
        .ok_or("no modeline for the standard in crt.toml")?;
    let ml = Modeline::parse(text).ok_or("bad modeline in crt.toml")?;
    // Nothing reaches the connector without passing here. A television's
    // horizontal deflection is tuned for one line rate, and this is the last
    // place a timing that would drive it somewhere else can be stopped.
    if let Some(why) = ml.fault(cfg.output.hfreq_khz) {
        return Err(format!(
            "refusing the {standard} modeline in crt.toml: it asks the \
             television for {why}"
        ));
    }

    let mut lease = Lease::take(&want)?;
    println!(
        "leased {} (DRM connector {})",
        lease.name, lease.connector_id
    );
    // A granted lease always carries the descriptor; without it there is no
    // output to drive, and the display process saying so beats a panic that
    // the watchdog would restart in a loop.
    let Some(fd) = lease.fd.take() else {
        return Err("the lease arrived without a file descriptor".into());
    };

    // DRM side: device, buffers, renderer.
    let dev_id = smithay::reexports::rustix::fs::fstat(fd.as_fd())
        .map(|st| st.st_rdev)
        .map_err(|e| format!("fstat: {e}"))?;
    let drm_fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let (drm, notifier) = DrmDevice::new(drm_fd.clone(), true).map_err(|e| format!("drm: {e}"))?;
    let gbm = GbmDevice::new(drm_fd.clone()).map_err(|e| format!("gbm: {e}"))?;
    let egl = unsafe { EGLDisplay::new(gbm.clone()) }.map_err(|e| format!("egl display: {e}"))?;
    let context = EGLContext::new(&egl).map_err(|e| format!("egl context: {e}"))?;
    let mut renderer = unsafe { GlesRenderer::new(context) }.map_err(|e| format!("gles: {e}"))?;
    let allocator = GbmAllocator::new(
        gbm.clone(),
        GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
    );
    let exporter = GbmFramebufferExporter::new(gbm.clone(), None);
    let render_formats = renderer.dmabuf_formats();
    let mut drm = DrmOutputManager::new(
        drm,
        allocator,
        exporter,
        Some(gbm),
        [Fourcc::Argb8888, Fourcc::Xrgb8888],
        render_formats.clone(),
    );

    // The connector and CRTC the lease gave us.
    let res = drm
        .device()
        .resource_handles()
        .map_err(|e| format!("resources: {e}"))?;
    let conn_handle = res
        .connectors()
        .iter()
        .copied()
        .find(|h| u32::from(*h) == lease.connector_id)
        .or_else(|| res.connectors().first().copied())
        .ok_or("lease has no connector")?;
    let crtc = res.crtcs().first().copied().ok_or("lease has no crtc")?;
    let mode = drm_mode(&ml);
    let (w, h) = (ml.width() as i32, ml.height() as i32);
    println!(
        "{}x{} at {:.3} kHz / {:.3} Hz on crtc {:?}",
        w,
        h,
        ml.hfreq_khz(),
        ml.vfreq_hz(),
        crtc
    );
    publish_mode(&ml);

    // Wayland side.
    let mut event_loop: EventLoop<'static, Crt> =
        EventLoop::try_new().map_err(|e| e.to_string())?;
    let display: Display<Crt> = Display::new().map_err(|e| e.to_string())?;
    let dh = display.handle();
    let handle = event_loop.handle();
    // Legacy wl_drm for EGL clients that still ask for it; dmabuf is the
    // main road.
    if let Err(e) = renderer.bind_wl_display(&dh) {
        eprintln!("egl wl_drm binding skipped: {e}");
    }

    let output = Output::new(
        "CRT".into(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Omarchy".into(),
            model: "CRT".into(),
        },
    );
    let wl_mode = WlMode {
        size: (w, h).into(),
        refresh: (ml.vfreq_hz() * 1000.0).round() as i32,
    };
    output.set_preferred(wl_mode);
    output.change_current_state(
        Some(wl_mode),
        Some(Transform::Normal),
        Some(Scale::Integer(1)),
        Some((0, 0).into()),
    );
    let _output_global = output.create_global::<Crt>(&dh);

    let compositor_state = CompositorState::new::<Crt>(&dh);
    let xdg_shell_state = XdgShellState::new::<Crt>(&dh);
    let shm_state = ShmState::new::<Crt>(&dh, vec![]);
    // wp_viewporter: a client says which part of its buffer to show and at
    // what size, and the compositor does the scaling. mpv asks for it at
    // every start and said so in the log a hundred and eighty-four times;
    // without it a player has to scale into a buffer of the right size
    // itself. smithay's surface elements read the viewport, so this is the
    // whole of it.
    let _viewporter_state = ViewporterState::new::<Crt>(&dh);
    // wp_presentation: the compositor tells a client the exact time its frame
    // reached the screen, on the same clock the kernel gives us the vblank
    // on. Without it a client that cares about timing - a player, an
    // emulator - can only guess, and both RetroArch and mpv ask for it.
    let _presentation_state = PresentationState::new::<Crt>(&dh, libc::CLOCK_MONOTONIC as u32);
    let output_manager_state = OutputManagerState::new_with_xdg_output::<Crt>(&dh);
    let mut seat_state = SeatState::new();
    let mut seat: Seat<Crt> = seat_state.new_wl_seat(&dh, "crt");
    // A keyboard with no keys behind it yet: clients such as RetroArch only
    // run when a keyboard has entered their surface.
    seat.add_keyboard(Default::default(), 200, 25)
        .map_err(|e| format!("keyboard: {e}"))?;
    let mut dmabuf_state = DmabufState::new();
    let feedback = DmabufFeedbackBuilder::new(dev_id, render_formats)
        .build()
        .map_err(|e| format!("dmabuf feedback: {e}"))?;
    let dmabuf_global = dmabuf_state.create_global_with_default_feedback::<Crt>(&dh, &feedback);

    let mut space: Space<Window> = Space::default();
    space.map_output(&output, (0, 0));

    let drm_output = drm
        .initialize_output::<GlesRenderer, Element>(
            crtc,
            mode,
            &[conn_handle],
            &output,
            None,
            &mut renderer,
            &DrmOutputRenderElements::default(),
        )
        .map_err(|e| format!("output: {e}"))?;

    // What the kernel thinks this television can do about its refresh rate.
    // `Supported` means the vertical blanking can be stretched frame by
    // frame without a mode change, which is the only way to follow a
    // program's own rate on a set whose horizontal rate must not move.
    // Asking for it in the EDID is the switch: the FreeSync range is only
    // there because the operator put it there, and there is nothing else a
    // television leased to this compositor would want a variable refresh
    // rate for. So if the kernel says the connector can, it does.
    let vrr_on = match drm_output.with_compositor(|c| c.vrr_supported(conn_handle)) {
        Ok(VrrSupport::NotSupported) => {
            println!("vrr: the connector is not capable of it");
            false
        }
        Ok(v) => match drm_output.with_compositor(|c| c.use_vrr(true)) {
            Ok(()) => {
                let on = drm_output.with_compositor(|c| c.vrr_enabled());
                println!("vrr: on ({v:?})");
                on
            }
            Err(e) => {
                println!("vrr: refused ({e})");
                false
            }
        },
        Err(e) => {
            println!("vrr: cannot tell ({e})");
            false
        }
    };

    // The DAC wants composite sync once the signal is up.
    if let Some(conn) = output::connectors()
        .into_iter()
        .find(|c| c.name == lease.name)
        && let Some(bus) = dac::Dac::bus_of(&conn.path)
    {
        match dac::Dac::open(&bus) {
            Ok(d) => {
                let cs = dac::Csync::parse(&cfg.output.csync).unwrap_or(dac::Csync::Xor);
                if let Err(e) = d.set_csync(cs) {
                    eprintln!("dac csync: {e}");
                }
            }
            Err(e) => eprintln!("dac: {e}"),
        }
    }

    // Sockets and sources.
    let listener =
        ListeningSocketSource::with_name(SOCKET).map_err(|e| format!("socket {SOCKET}: {e}"))?;
    let socket_name = listener.socket_name().to_os_string();
    handle
        .insert_source(listener, |stream, _, st: &mut Crt| {
            let _ = st
                .dh
                .insert_client(stream, Arc::new(ClientState::default()));
        })
        .map_err(|e| e.to_string())?;
    handle
        .insert_source(
            Generic::new(display, Interest::READ, Mode::Level),
            |_, display, st: &mut Crt| {
                unsafe {
                    let _ = display.get_mut().dispatch_clients(st);
                }
                Ok(PostAction::Continue)
            },
        )
        .map_err(|e| e.to_string())?;
    handle
        .insert_source(notifier, |event, meta, st: &mut Crt| match event {
            DrmEvent::VBlank(_) => st.vblank(meta.take()),
            DrmEvent::Error(e) => eprintln!("drm: {e}"),
        })
        .map_err(|e| e.to_string())?;
    handle
        .insert_source(
            Timer::from_duration(Duration::from_millis(250)),
            |_, _, st: &mut Crt| {
                if !st.lease.pump() {
                    eprintln!("the compositor revoked the lease");
                    st.running = false;
                }
                // A frame was accepted and its vblank never came. Every other
                // part of the process is healthy, so nothing else notices:
                // the picture is simply gone, and `render` returns at its
                // first line for ever because a frame is still in flight.
                if let Some(at) = st.queued_at
                    && at.elapsed() >= FRAME_DEADLINE
                {
                    eprintln!(
                        "no vblank for {} s with a frame in flight: the device has stopped",
                        FRAME_DEADLINE.as_secs()
                    );
                    st.running = false;
                }
                TimeoutAction::ToDuration(Duration::from_millis(250))
            },
        )
        .map_err(|e| e.to_string())?;

    // Control pipe: `top <app_id>`, `mode <modeline>`, `quit`. Opened
    // read-write so it never reports end of file between writers, and made
    // 0600 so only the user this runs as can say anything to the tube.
    //
    // A line longer than the buffer below would arrive in two pieces and
    // neither would parse, which is the right failure: every command here is
    // a few dozen bytes, and a half command is refused rather than guessed
    // at. `mode` in particular goes through `Modeline::fault` before it
    // reaches the connector.
    let ctl = display::ctl_path();
    let _ = std::fs::remove_file(&ctl);
    let cpath =
        std::ffi::CString::new(ctl.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    if unsafe { libc::mkfifo(cpath.as_ptr(), 0o600) } < 0 {
        return Err(format!("control pipe: {}", std::io::Error::last_os_error()));
    }
    let raw = unsafe {
        libc::open(
            cpath.as_ptr(),
            libc::O_RDWR | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if raw < 0 {
        return Err(format!(
            "control pipe open: {}",
            std::io::Error::last_os_error()
        ));
    }
    let ctl_fd = unsafe { OwnedFd::from_raw_fd(raw) };
    handle
        .insert_source(
            Generic::new(ctl_fd, Interest::READ, Mode::Level),
            |_, fd, st: &mut Crt| {
                let mut buf = [0u8; 4096];
                let n = unsafe {
                    libc::read(
                        fd.as_fd().as_raw_fd(),
                        buf.as_mut_ptr() as *mut libc::c_void,
                        buf.len(),
                    )
                };
                if n > 0 {
                    let text = String::from_utf8_lossy(&buf[..n as usize]).to_string();
                    for line in text
                        .lines()
                        .map(|l| l.trim_matches(|c: char| c.is_whitespace() || c == '\0'))
                        .filter(|l| !l.is_empty())
                    {
                        st.control(line);
                    }
                }
                Ok(PostAction::Continue)
            },
        )
        .map_err(|e| e.to_string())?;
    let _ = std::fs::write(display::pid_path(), std::process::id().to_string());

    let mut crt = Crt {
        start: Instant::now(),
        dh,
        handle,
        socket_name,
        space,
        output,
        compositor_state,
        xdg_shell_state,
        shm_state,
        _output_manager_state: output_manager_state,
        seat_state,
        seat,
        dmabuf_state,
        _dmabuf_global: dmabuf_global,
        renderer,
        _drm: drm,
        drm_output: Some(drm_output),
        frame_queued: false,
        queued_at: None,
        capture_target: None,
        capture_size: None,
        flips_failed: 0,
        dirty: true,
        pacer_armed: false,
        vrr_on,
        conn_handle,
        mode_at: None,
        last_vblank: None,
        last_vblank_hw: None,
        callback_armed: false,
        told_at: None,
        modeline: Some(ml.clone()),
        dirty_top: false,
        last_top_commit: None,
        telling: 0,
        hfreq_band: cfg.output.hfreq_khz,
        stretch: {
            // The calibration was taken on the standard this machine is set
            // to, so that frame is what the rate has to be read against.
            let nominal = cfg
                .modeline(&cfg.output.standard)
                .and_then(Modeline::parse)
                .map(|m| m.field_hz())
                .filter(|hz| (40.0..=90.0).contains(hz))
                .unwrap_or(60.0);
            (nominal / cfg.output.vrr_min_hz.clamp(20.0, 200.0)).clamp(1.0, 1.5)
        },
        target_period: None,
        rate_trim: 0,
        client_period: Duration::from_millis(16),
        periods: Default::default(),
        late: 0,
        showing: None,
        latencies: Default::default(),
        intervals: Default::default(),
        late_draw: std::env::var("FLYBACK_LATE_DRAW").as_deref() != Ok("off"),
        client_slack: std::env::var("FLYBACK_SLACK_US")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_micros)
            .unwrap_or(Duration::from_millis(3)),
        deadline_armed: false,
        floor_armed: false,
        field_seq: 0,
        fields_repeated: 0,
        render_cost: Duration::from_micros(500),
        margin_override: std::env::var("FLYBACK_MARGIN_US")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_micros),
        frame_delay: std::env::var("FLYBACK_MARGIN_US").as_deref() != Ok("off"),
        trace: std::env::var_os("FLYBACK_TRACE").is_some(),
        last_commit: None,
        commits_blocked: 0,
        scanout_direct: None,
        lease,
        running: true,
        frames: 0,
        last_stats: Instant::now(),
        commits: Default::default(),
        host: None,
        recorder: None,
    };
    crt.handle
        .insert_source(
            Timer::from_duration(Duration::from_millis(100)),
            |_, _, st: &mut Crt| {
                st.preview();
                TimeoutAction::ToDuration(Duration::from_millis(100))
            },
        )
        .map_err(|e| e.to_string())?;
    println!(
        "compositor up: WAYLAND_DISPLAY={}",
        crt.socket_name.to_string_lossy()
    );
    crt.damaged();
    crt.arm_pacer();
    let signal = event_loop.get_signal();
    event_loop
        .run(Duration::from_millis(50), &mut crt, |st| {
            // Send what the dispatch above produced: globals, configures,
            // frame callbacks. Without this the clients wait forever.
            let _ = st.dh.flush_clients();
            if !st.running {
                signal.stop();
            }
        })
        .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(display::pid_path());
    let _ = std::fs::remove_file(display::monitor_path());
    let _ = std::fs::remove_file(display::ctl_path());
    let _ = std::fs::remove_file(display::latency_path());
    let _ = std::fs::remove_file(display::mode_path());
    Ok(())
}

/// Say what the television is being given, for anything that wants to know.
///
/// Written after the modeset has landed, never before: this file is the one
/// answer in the system that is not a reconstruction, and it is worth nothing
/// if it says what was asked for rather than what happened.
fn publish_mode(ml: &Modeline) {
    let path = display::mode_path();
    let tmp = path.with_extension("mode.new");
    if std::fs::write(&tmp, format!("{}\n", ml.to_hypr())).is_ok()
        && std::fs::rename(&tmp, &path).is_err()
    {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// CLOCK_MONOTONIC, the clock wp_presentation was told about.
///
/// `SystemTime` is the wrong clock: it is the wall clock, and it steps when
/// the machine is corrected. A client comparing a presentation time with an
/// input event's timestamp is comparing two readings of *this* one.
fn monotonic() -> Duration {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: the kernel writes the two fields of a struct we own.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}

/// The frame length to aim at under a variable refresh rate.
///
/// The one asked for by name, or failing that the one the client has settled
/// into. Never shorter than the mode's own frame, because no hardware makes
/// a frame shorter than its timing; never longer than `slowest`, which is
/// where this television stops following a stretched blanking and the
/// picture starts losing height.
fn frame_room(
    target: Option<Duration>,
    client: Duration,
    shortest: Duration,
    slowest: Duration,
) -> Duration {
    target
        .unwrap_or(client)
        .clamp(shortest, slowest.max(shortest))
}

/// The correction to carry into the next frame, from the error in the last.
///
/// A quarter of the error, which settles in a handful of frames without
/// ringing, and never further out than one whole frame in either direction:
/// past that something else is wrong and winding the correction up would
/// only make it worse.
fn trim_step(trim: i64, interval: Duration, target: Duration, period: Duration) -> i64 {
    let err = interval.as_micros() as i64 - target.as_micros() as i64;
    let limit = period.as_micros() as i64;
    (trim - err / 4).clamp(-limit, limit)
}

/// How long before the vblank the drawing has to start, given a frame period
/// and what recent frames cost to draw.
///
/// Never less than a millisecond and a half, which is what it takes to reach
/// the kernel and back on an idle machine, and never more than half a frame,
/// because past that the picture is older than the frame it saves.
fn margin_for(period: Duration, cost: Duration, forced: Option<Duration>) -> Duration {
    forced
        .unwrap_or(cost * 2 + Duration::from_micros(1500))
        .clamp(Duration::from_micros(1500), period / 2)
}

/// How long after the vblank to tell a client it may draw, given the room
/// there is before the drawing deadline and what the client costs.
///
/// Zero when there is not enough room to be clever, which is the safe answer
/// and the one every other compositor gives.
fn callback_wait(room: Duration, client: Duration, slack: Duration) -> Duration {
    room.saturating_sub(client * 2 + slack)
}

#[cfg(test)]
mod cost {
    use super::ClientCost;
    use std::time::Duration;

    const FRAME: Duration = Duration::from_micros(16_655);

    /// A window starts out assumed to want the whole frame, so that a client
    /// nobody has measured yet is never squeezed.
    #[test]
    fn a_window_nobody_has_measured_is_given_the_frame() {
        assert_eq!(ClientCost::new().cost.get(), Duration::from_millis(16));
    }

    /// The first reading replaces that assumption instead of losing to it.
    /// A decaying maximum starting at a frame takes about fifty frames to
    /// come down to a millisecond, and for that half second a client that
    /// draws in a millisecond is told at the vblank and shown a frame late.
    /// Measured before this: 34 of the first frames of every run two frames
    /// late, and none after.
    #[test]
    fn the_first_reading_replaces_the_assumption() {
        let c = ClientCost::new();
        let ms = Duration::from_millis(1);
        if c.measured.replace(true) {
            c.cost.set(c.cost.get().max(ms));
        } else {
            c.cost.set(ms);
        }
        assert_eq!(c.cost.get(), ms);
    }

    /// After that it is a maximum again: one slow frame moves the deadline
    /// for the frames that follow it, which is the whole point of holding a
    /// worst case rather than an average.
    #[test]
    fn after_that_it_holds_the_worst() {
        let c = ClientCost::new();
        c.measured.set(true);
        c.cost.set(Duration::from_millis(1));
        for reading in [Duration::from_micros(900), Duration::from_millis(4)] {
            if c.measured.replace(true) {
                c.cost.set(c.cost.get().max(reading));
            }
        }
        assert_eq!(c.cost.get(), Duration::from_millis(4));
        // And it comes down on its own, a thirty-second of itself a frame.
        c.cost.set(c.cost.get() * 31 / 32);
        assert!(c.cost.get() < Duration::from_millis(4));
        assert!(c.cost.get() > Duration::from_millis(3));
        let _ = FRAME;
    }
}

#[cfg(test)]
mod timing {
    use super::{callback_wait, margin_for};
    use std::time::Duration;

    const FRAME: Duration = Duration::from_micros(16_655);

    #[test]
    fn a_rate_asked_for_wins_over_the_one_the_client_settled_into() {
        let asked = Duration::from_micros(18_000);
        let settled = Duration::from_micros(16_700);
        assert_eq!(
            super::frame_room(Some(asked), settled, FRAME, Duration::from_micros(20_000)),
            asked
        );
        assert_eq!(
            super::frame_room(None, settled, FRAME, Duration::from_micros(20_000)),
            settled
        );
    }

    #[test]
    fn nothing_asks_the_tube_for_a_frame_it_cannot_make() {
        // Shorter than the mode: the hardware has no such frame.
        assert_eq!(
            super::frame_room(
                Some(Duration::from_micros(10_000)),
                FRAME,
                FRAME,
                Duration::from_micros(20_000)
            ),
            FRAME
        );
        // Longer than this set follows: held where the picture still holds.
        let slowest = Duration::from_micros(18_180);
        assert_eq!(
            super::frame_room(Some(Duration::from_micros(25_000)), FRAME, FRAME, slowest),
            slowest
        );
    }

    /// The calibration is nine per cent past the frame, not an absolute rate,
    /// and this is the case that proves why it has to be.
    ///
    /// `output.vrr_min_hz` is 55 on this set, measured against a 60.04 Hz
    /// frame. A PAL frame is 50.08, already slower than 55: read as an
    /// absolute rate the floor lands *inside* the mode, `frame_room` clamps
    /// between two ends the wrong way round, and a European game asking for
    /// 49.70 gets the mode's own 50.08 instead. It then drops a frame every
    /// two and a half seconds, which is what a person sees.
    /// The floor is armed early enough that the flip is in the kernel's
    /// hands by the time the field would leave the set's window, not started
    /// at it.
    #[test]
    fn the_floor_is_armed_before_the_field_runs_out() {
        let stretch = 60.041 / 55.0;
        let period = Duration::from_micros(16_655);
        let floor = period.mul_f64(stretch);
        let render_cost = Duration::from_micros(500);
        let wait = floor.saturating_sub(render_cost + Duration::from_micros(500));
        assert!(wait < floor, "the timer fires before the floor");
        assert!(
            wait > period,
            "and after the frame the tube would have made anyway, or every \
             field would be repeated"
        );
        // A millisecond of room between the two, which is twice what a flip
        // and a composite pass were measured at.
        assert!((floor - wait).as_micros() >= 1_000);
    }

    #[test]
    fn the_floor_follows_the_mode_and_not_a_rate_from_another_standard() {
        let stretch = 60.041 / 55.0;
        let want_ntsc = |period: Duration| period.mul_f64(stretch).max(period);

        // NTSC: nothing moves. The floor is still 55 Hz, to a microsecond of
        // rounding either way.
        let ntsc = Duration::from_micros(16_655);
        let floor = want_ntsc(ntsc).as_micros() as i64;
        assert!((floor - 18_182).abs() <= 2, "{floor} us is not 55 Hz");

        // PAL: the frame is 19_968 us and a game wants 20_120. The old
        // absolute floor of 18_180 is shorter than the frame itself, so the
        // room collapses onto the mode and the game is refused its rate.
        let pal = Duration::from_micros(19_968);
        let asked = Duration::from_micros(20_120);
        let old = Duration::from_micros(18_180);
        assert_eq!(
            super::frame_room(Some(asked), pal, pal, old),
            pal,
            "the absolute floor gives the game the mode instead of its rate"
        );

        // As a ratio the floor is past the frame, and the game gets what it
        // asked for.
        assert_eq!(
            super::frame_room(Some(asked), pal, pal, want_ntsc(pal)),
            asked
        );
        assert!(want_ntsc(pal) > pal, "the floor is past the frame");
    }

    #[test]
    fn a_frame_that_came_out_long_pulls_the_telling_earlier() {
        let target = Duration::from_micros(18_000);
        let late = super::trim_step(0, Duration::from_micros(18_400), target, FRAME);
        assert_eq!(late, -100, "a quarter of 400 us, the other way");
        let early = super::trim_step(0, Duration::from_micros(17_600), target, FRAME);
        assert_eq!(early, 100);
    }

    #[test]
    fn the_correction_never_runs_away() {
        let target = Duration::from_micros(18_000);
        let mut t = 0;
        for _ in 0..200 {
            t = super::trim_step(t, Duration::from_secs(1), target, FRAME);
        }
        assert_eq!(t, -(FRAME.as_micros() as i64));
    }

    #[test]
    fn it_settles_on_the_frame_that_was_asked_for() {
        // A tube whose frame comes out a millisecond longer than the telling
        // implies. Later telling, longer frame, so the frame that comes out
        // is the target plus the correction plus that millisecond; the loop
        // should find the millisecond and settle on cancelling it.
        let target = Duration::from_micros(18_000);
        let overhead = 1_000i64;
        let mut t = 0;
        let mut interval = 0;
        for _ in 0..40 {
            interval = (target.as_micros() as i64 + t + overhead).max(0);
            t = super::trim_step(t, Duration::from_micros(interval as u64), target, FRAME);
        }
        let err = (interval - target.as_micros() as i64).abs();
        assert!(
            err < 20,
            "settled {interval} us against {target:?}, off by {err}"
        );
    }

    #[test]
    fn a_cheap_frame_still_leaves_the_kernel_time_to_answer() {
        // A tenth of a millisecond to draw would put the deadline 1.7 ms
        // out, and the floor holds it at 1.5.
        let m = margin_for(FRAME, Duration::from_micros(10), None);
        assert!(m >= Duration::from_micros(1500), "{m:?}");
    }

    #[test]
    fn an_expensive_frame_moves_the_deadline_earlier() {
        let cheap = margin_for(FRAME, Duration::from_micros(200), None);
        let dear = margin_for(FRAME, Duration::from_micros(3000), None);
        assert!(dear > cheap, "{dear:?} should be later than {cheap:?}");
    }

    #[test]
    fn nothing_takes_more_than_half_the_frame() {
        // A frame that costs more than the frame it is drawn in cannot be
        // helped by starting even earlier, and holding half the period back
        // would make every picture older for nothing.
        let m = margin_for(FRAME, Duration::from_millis(30), None);
        assert_eq!(m, FRAME / 2);
        assert_eq!(margin_for(FRAME, Duration::ZERO, Some(FRAME)), FRAME / 2);
    }

    #[test]
    fn a_client_is_told_early_enough_to_be_late_once() {
        // Twice its own drawing time plus the slack: a client that takes a
        // millisecond is told 4.5 ms before the deadline.
        let room = FRAME - Duration::from_micros(1900);
        let wait = callback_wait(room, Duration::from_millis(1), Duration::from_millis(3));
        assert_eq!(room - wait, Duration::from_millis(5));
    }

    #[test]
    fn a_client_slower_than_the_frame_is_told_at_the_vblank() {
        let room = FRAME - Duration::from_micros(1900);
        let wait = callback_wait(room, FRAME, Duration::from_millis(3));
        assert_eq!(wait, Duration::ZERO);
    }
}

impl Crt {
    /// One control line from the pipe.
    fn control(&mut self, line: &str) {
        let (cmd, arg) = line.split_once(' ').unwrap_or((line, ""));
        match cmd {
            "quit" => self.running = false,
            "top" => {
                let target = arg.trim();
                if let Some(w) = self.window_with_app_id(target) {
                    self.space.raise_element(&w, true);
                    self.focus_top();
                    self.damaged();
                } else {
                    eprintln!("top: no window with app id {target}");
                }
            }
            "key" => self.inject_key(arg.trim()),
            "record" => {
                let (what, rest) = arg.trim().split_once(' ').unwrap_or((arg.trim(), ""));
                // `record start <file> [sink]`: the sink's monitor is the audio track.
                let (path, sink_arg) = rest.trim().rsplit_once(' ').unwrap_or((rest.trim(), ""));
                let (path, sink_arg) = if sink_arg.starts_with("alsa_")
                    || sink_arg.contains('.') && !sink_arg.contains('/')
                {
                    (path, Some(sink_arg.to_string()))
                } else {
                    (rest.trim(), None)
                };
                match what {
                    "start" if !path.is_empty() => {
                        if let Some(r) = self.recorder.take() {
                            println!("record: {}", r.stop());
                        }
                        let sink = sink_arg.or_else(|| {
                            std::env::var("OMACRT_SINK").ok().filter(|s| !s.is_empty())
                        });
                        match Recorder::start(path.trim(), sink) {
                            Ok(r) => {
                                println!("record: started {}", path.trim());
                                self.recorder = Some(r);
                            }
                            Err(e) => eprintln!("record: {e}"),
                        }
                    }
                    "stop" => {
                        if let Some(r) = self.recorder.take() {
                            println!("record: {}", r.stop());
                        }
                    }
                    _ => eprintln!("record: use `record start <file.mp4>` or `record stop`"),
                }
            }
            "monitor" => match arg.trim() {
                "on" => {
                    if self.host.is_none() {
                        output::hypr_eval(&format!(
                            "hl.window_rule({{ name = \"omacrt-monitor\", match = {{ class = \"{}\" }}, float = true, size = \"880 660\", center = true }})",
                            crate::host::APP_ID
                        ));
                        match crate::host::Host::open(&self.handle) {
                            Ok(h) => {
                                let _ = std::fs::write(display::monitor_path(), "");
                                self.host = Some(h);
                            }
                            Err(e) => eprintln!("monitor: {e}"),
                        }
                    }
                }
                _ => {
                    let _ = std::fs::remove_file(display::monitor_path());
                    if let Some(mut h) = self.host.take() {
                        h.close();
                    }
                }
            },
            "shot" => {
                if let Err(e) = self.screenshot(arg.trim()) {
                    eprintln!("shot: {e}");
                }
            }
            // `vrr on|off`: ask the kernel to stretch the vertical blanking
            // frame by frame instead of holding the mode's own rate. It only
            // does anything when the connector is vrr_capable, which on this
            // chain means the EDID we inject carries a FreeSync range.
            "vrr" => {
                let want = arg.trim() != "off";
                let conn = self.conn_handle;
                let Some(out) = self.drm_output.as_mut() else {
                    eprintln!("vrr: no output");
                    return;
                };
                let sup = out.with_compositor(|c| c.vrr_supported(conn));
                match out.with_compositor(|c| c.use_vrr(want)) {
                    Ok(()) => {
                        let on = out.with_compositor(|c| c.vrr_enabled());
                        println!("vrr: asked for {want}, now {on}, support {sup:?}");
                        self.vrr_on = on;
                        self.damaged();
                    }
                    Err(e) => eprintln!("vrr: {e}"),
                }
            }
            // `rate 59.92` or `rate off`: the refresh the program on the tube
            // wants, which the television follows without a mode change for
            // as long as it stays inside the range the EDID declared.
            "rate" => {
                let arg = arg.trim();
                if arg.is_empty() || arg == "off" || arg == "0" {
                    self.target_period = None;
                    println!("rate: back to following the program's own pace");
                } else if let Ok(hz) = arg.parse::<f64>() {
                    let period = self.period();
                    let want = Duration::from_secs_f64(1.0 / hz.clamp(1.0, 1000.0));
                    let held = want.clamp(period, self.slowest());
                    self.target_period = Some(held);
                    self.rate_trim = 0;
                    println!(
                        "rate: {hz:.3} Hz asked for, {:.3} Hz given{}",
                        1.0 / held.as_secs_f64(),
                        // These are periods, not frequencies, so the
                        // comparisons read backwards: a longer period is a
                        // slower rate. Asking for something slower than the
                        // floor gives a period longer than the one held, and
                        // asking for something faster than the mode gives a
                        // shorter one. They were the wrong way round, so
                        // `rate 50` on a set calibrated to stop at 55 said
                        // the mode was not fast enough, which is the
                        // opposite of what had happened.
                        if !self.vrr_on {
                            " (the television is not following: no variable refresh rate)"
                        } else if want > held {
                            " (held at output.vrr_min_hz, where this set stops following)"
                        } else if want < held {
                            " (the mode itself is no faster than that)"
                        } else {
                            ""
                        }
                    );
                    self.damaged();
                } else {
                    eprintln!("rate: not a number of hertz: {arg:?}");
                }
            }
            "mode" => match Modeline::parse(arg) {
                Some(ml) => self.switch_mode(&ml),
                None => eprintln!("mode: bad modeline {arg:?}"),
            },
            _ => eprintln!("control: unknown command {line:?}"),
        }
    }

    /// Keyboard focus: the running program (emulator or player) when there
    /// is one, else the window on top. The launcher takes the pad in the
    /// background and its control pipe, it never needs the keyboard while a
    /// program runs; the program needs it to count as focused.
    pub fn focus_top(&mut self) {
        let program = ["com.libretro.RetroArch", "omacrt-player"]
            .iter()
            .find_map(|id| self.window_with_app_id(id));
        let target = program.or_else(|| self.space.elements().last().cloned());
        let surface = target.and_then(|w| w.toplevel().map(|t| t.wl_surface().clone()));
        if let Some(kbd) = self.seat.get_keyboard() {
            let serial = smithay::utils::SERIAL_COUNTER.next_serial();
            kbd.set_focus(self, surface, serial);
        }
    }

    /// Press a key on the tube's keyboard and release it a frame or two
    /// later, so the program's per frame input sampling cannot miss it. This
    /// is how the launcher drives RetroArch's hotkeys (pause, save, load,
    /// reset, quit): real key events, no network command interface.
    fn inject_key(&mut self, arg: &str) {
        // `key r 2000`: the key stays down for that many milliseconds.
        let (name, hold_ms) = match arg.split_once(' ') {
            Some((n, ms)) => (
                n.trim(),
                ms.trim().parse::<u64>().unwrap_or(45).clamp(20, 10_000),
            ),
            None => (arg, 45),
        };
        let evdev: u32 = match name {
            "rewind" | "r" => 19,
            "slow" | "e" => 18,
            "pause" | "p" => 25,
            "save" | "f2" => 60,
            "load" | "f4" => 62,
            "reset" | "h" => 35,
            "quit" | "esc" | "escape" => 1,
            "ff" | "space" => 57,
            "menu" | "f1" => 59,
            "enter" | "return" => 28,
            "slot+" | "f7" => 65,
            "slot-" | "f6" => 64,
            // A raw evdev code, for a key with no name here. KEY_MAX is
            // 0x2ff, and anything above it is not a key: refusing it keeps
            // the `+ 8` below from overflowing on a typo, which would take
            // the compositor down and the television with it.
            other => match other.parse::<u32>() {
                Ok(code) if code <= 0x2ff => code,
                Ok(code) => {
                    eprintln!("key: {code} is not an evdev key code");
                    return;
                }
                Err(_) => {
                    eprintln!("key: unknown key {other:?}");
                    return;
                }
            },
        };
        self.focus_top();
        let code = Keycode::new(evdev + 8);
        self.key_event(code, KeyState::Pressed);
        let _ = self.handle.insert_source(
            Timer::from_duration(Duration::from_millis(hold_ms)),
            move |_, _, st: &mut Crt| {
                st.key_event(code, KeyState::Released);
                TimeoutAction::Drop
            },
        );
    }

    pub fn key_event(&mut self, code: Keycode, state: KeyState) {
        let Some(kbd) = self.seat.get_keyboard() else {
            return;
        };
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        let time = self.start.elapsed().as_millis() as u32;
        kbd.input::<(), _>(self, code, state, serial, time, |_, _, _| {
            FilterResult::Forward
        });
    }

    /// Render the current frame off screen: top-down BGRA bytes, width, height.
    /// Render the tube's current picture off screen and hand the raw pixels
    /// to `take`, along with which way up they are and how big they are. The
    /// offscreen buffer is kept between calls: this runs thirty times a
    /// second while recording, and allocating a full frame of video memory
    /// each time is work the compositor does not have to do.
    fn with_frame<R>(
        &mut self,
        take: impl FnOnce(&[u8], bool, usize, usize) -> R,
    ) -> Result<R, String> {
        let size = self
            .output
            .current_mode()
            .map(|m| m.size)
            .ok_or("no mode")?;
        let (w, h) = (size.w, size.h);
        if self.capture_size != Some((w, h)) {
            self.capture_target = None;
        }
        if self.capture_target.is_none() {
            self.capture_target = Some(
                self.renderer
                    .create_buffer(Fourcc::Argb8888, (w, h).into())
                    .map_err(|e| format!("offscreen buffer: {e}"))?,
            );
            self.capture_size = Some((w, h));
        }
        let target = self
            .capture_target
            .as_mut()
            .ok_or("offscreen buffer went away")?;
        let elements = space_render_elements(&mut self.renderer, [&self.space], &self.output, 1.0)
            .map_err(|e| format!("elements: {e}"))?;
        let mut tracker = OutputDamageTracker::from_output(&self.output);
        let mut fb = self
            .renderer
            .bind(target)
            .map_err(|e| format!("bind: {e}"))?;
        tracker
            .render_output(
                &mut self.renderer,
                &mut fb,
                0,
                &elements,
                [0.02, 0.03, 0.06, 1.0],
            )
            .map_err(|e| format!("render: {e}"))?;
        let mapping = self
            .renderer
            .copy_framebuffer(&fb, Rectangle::from_size((w, h).into()), Fourcc::Argb8888)
            .map_err(|e| format!("copy: {e}"))?;
        let flipped = mapping.flipped();
        let bytes = self
            .renderer
            .map_texture(&mapping)
            .map_err(|e| format!("map: {e}"))?;
        Ok(take(bytes, flipped, w as usize, h as usize))
    }

    /// The picture the right way up, in a buffer of its own. For a still.
    fn capture(&mut self) -> Result<(Vec<u8>, usize, usize), String> {
        self.with_frame(|bytes, flipped, w, h| {
            let row = w * 4;
            let mut out = vec![0u8; row * h];
            for y in 0..h {
                // GL framebuffers read bottom-up unless the mapping says otherwise.
                let src_y = if flipped { y } else { h - 1 - y };
                out[y * row..(y + 1) * row].copy_from_slice(&bytes[src_y * row..(src_y + 1) * row]);
            }
            (out, w, h)
        })
    }

    /// Write the current frame as a PNG. The desktop's screenshot tools
    /// cannot see a leased output, this can.
    fn screenshot(&mut self, path: &str) -> Result<(), String> {
        if path.is_empty() {
            return Err("shot needs a file path".into());
        }
        // The pipe is 0600, so this is not somebody else's business - but any
        // program running as you can write a line into it, and this one
        // truncates whatever it is pointed at. Writing a PNG to a name that
        // does not end in .png is never what was meant, and refusing it is
        // what stops the command being a way to empty an arbitrary file.
        if !path.to_ascii_lowercase().ends_with(".png") {
            return Err(format!("{path}: a shot is written to a .png"));
        }
        let (bgra, w, h) = self.capture()?;
        let mut rgb = vec![0u8; w * h * 3];
        for i in 0..w * h {
            rgb[i * 3] = bgra[i * 4 + 2];
            rgb[i * 3 + 1] = bgra[i * 4 + 1];
            rgb[i * 3 + 2] = bgra[i * 4];
        }
        let file = std::fs::File::create(path).map_err(|e| format!("{path}: {e}"))?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&rgb).map_err(|e| e.to_string())?;
        println!("shot: {path}");
        Ok(())
    }

    /// A frame for the desktop preview window, when it is open and ready.
    pub fn preview(&mut self) {
        let closed = self.host.as_ref().map(|h| h.closed).unwrap_or(false);
        if closed {
            // Closed from the desktop, by the window's own button.
            let _ = std::fs::remove_file(display::monitor_path());
            self.host = None;
            return;
        }
        let ready = self.host.as_ref().map(|h| h.ready()).unwrap_or(false);
        if !ready {
            return;
        }
        match self.capture() {
            Ok((bgra, w, h)) => {
                if let Some(host) = self.host.as_mut() {
                    host.present(&bgra, w, h);
                }
            }
            Err(e) => eprintln!("preview: {e}"),
        }
    }

    fn window_with_app_id(&self, app_id: &str) -> Option<Window> {
        self.space
            .elements()
            .find(|w| {
                w.toplevel()
                    .map(|t| {
                        with_states(t.wl_surface(), |s| {
                            s.data_map
                                .get::<XdgToplevelSurfaceData>()
                                .and_then(|d| d.lock().unwrap().app_id.clone())
                        })
                        .as_deref()
                            == Some(app_id)
                    })
                    .unwrap_or(false)
            })
            .cloned()
    }

    /// Live modeline change: the CRTC, the advertised output mode and the
    /// size every client is told to use.
    fn switch_mode(&mut self, ml: &Modeline) {
        // The control pipe is a named pipe in the user's own state folder,
        // so a modeline can arrive here from anywhere that can write a line
        // to a file: this project, a mistake in `crt.toml`, or something
        // else entirely. Programming a line rate a television is not built
        // for is the one thing here that breaks hardware rather than a
        // picture, so it is checked on the way in and not on the way out.
        if let Some(why) = ml.fault(self.hfreq_band) {
            eprintln!("mode: refused, it asks the television for {why}");
            return;
        }
        // Asking for the timing that is already on the connector is not a
        // free request: the commit goes through, the connector relocks, and
        // the first vblank after it is 4 to 16 ms away with the television
        // dark for it. The launcher asks on every game start, every game
        // end, every pause and every time it follows the core's line count,
        // and most of those resolve to the mode that is already set: 41 of
        // them in one recent stretch of the log, every one to the same
        // vtotal. Each is a small hitch of the whole picture for nothing.
        if self.modeline.as_ref() == Some(ml) {
            println!("mode: already set, nothing done");
            return;
        }
        let Some(out) = self.drm_output.as_mut() else {
            return;
        };
        let mode = drm_mode(ml);
        // Two clocks, because they measure different things and only the
        // second one is what a person sees.
        //
        // `use_mode` below is a TEST_ONLY atomic commit plus the bookkeeping
        // that follows it: it asks the kernel whether the timing is
        // acceptable and costs a millisecond or two. The modeset itself
        // happens on the next real commit, and `mode_at` is that second
        // clock, read in the vblank handler.
        //
        // It used to say the television was dark from here until that vblank.
        // Sampling the converter's lock every millisecond says otherwise: the
        // signal survives the first 110 ms of a call that blocks for 200, and
        // then the converter has nothing to lock to for 280 ms, a third of it
        // after this call has already returned.
        //
        // It used to be declared and never set, so the line it feeds was
        // never printed and the figure that was published for the cost of a
        // mode change could not be reproduced from any log. Do not remove it
        // without removing that number too.
        let began = Instant::now();
        self.mode_at = Some(began);
        if let Err(e) = out.use_mode(
            mode,
            &mut self.renderer,
            &DrmOutputRenderElements::<GlesRenderer, Element>::default(),
        ) {
            eprintln!("mode: {e}");
            return;
        }
        let took = began.elapsed();
        let (w, h) = (ml.width() as i32, ml.height() as i32);
        let wl_mode = WlMode {
            size: (w, h).into(),
            refresh: (ml.vfreq_hz() * 1000.0).round() as i32,
        };
        self.output
            .change_current_state(Some(wl_mode), None, None, None);
        self.output.set_preferred(wl_mode);
        for win in self.space.elements() {
            if let Some(t) = win.toplevel() {
                t.with_pending_state(|s| s.size = Some((w, h).into()));
                t.send_pending_configure();
            }
        }
        println!(
            "mode: {}x{} {:.3} kHz {:.3} Hz, vtotal {}, set in {:.1} ms",
            w,
            h,
            ml.hfreq_khz(),
            ml.vfreq_hz(),
            ml.v[3],
            took.as_secs_f64() * 1000.0
        );
        self.modeline = Some(ml.clone());
        publish_mode(ml);
        self.frame_queued = false;
        self.queued_at = None;
        self.damaged();
    }

    /// Something on the tube changed, so the next frame has to be drawn and
    /// shown. Every caller that is not the vblank or its own retry goes
    /// through here: that is what tells the compositor a flip is worth
    /// making, and what keeps it from holding one in the air when nothing
    /// has changed.
    fn damaged(&mut self) {
        self.damaged_by(true);
    }

    /// The same, for a commit that can say whether it came from the window
    /// on top.
    ///
    /// Under a variable refresh rate a commit draws at once, which is the
    /// whole point of it - but only for the window being looked at. A window
    /// underneath committing on its own pace would otherwise put a flip in
    /// the air between the top window's callback and its commit, and the
    /// fresh picture then waits behind a frame nobody can see. Measured with
    /// the launcher mapped under a client that draws in a millisecond: 20.5
    /// ms from commit to scanout, one frame in six missing its vblank
    /// altogether. The hidden window is not ignored - the frame is marked
    /// dirty and the pacer draws it within the frame - it simply does not
    /// get to decide when the flip goes out.
    fn damaged_by(&mut self, top: bool) {
        self.dirty = true;
        if top {
            self.dirty_top = true;
            self.last_top_commit = Some(Instant::now());
        } else if self.vrr_on {
            return;
        }
        // With a variable refresh rate the flip is what starts the next
        // scanout, so there is nothing to wait for: draw now and the picture
        // is on the glass at once. Waiting for a deadline would be the one
        // thing that puts the delay back.
        if self.deadline_armed && !self.vrr_on {
            return;
        }
        self.render();
    }

    /// Is this surface's window the one on top, the one being looked at?
    fn is_top(&self, surface: &WlSurface) -> bool {
        let Some(top) = self.space.elements().next_back() else {
            return true;
        };
        self.window_for(surface).as_ref() == Some(top)
    }

    /// Put the measured latency where the rest of the program can read it:
    /// the median of the last few hundred frames, in milliseconds and in
    /// frames, and how many frames that was.
    ///
    /// The median rather than the mean, because one frame that waited for a
    /// mode change or a game starting is not what the tube feels like.
    fn write_latency(&self) {
        if self.latencies.is_empty() {
            return;
        }
        // A median and two points of the tail. The median alone was the
        // whole of this file for a day, and it cannot show what a viewer
        // actually complains about: a frame that arrives late once every few
        // seconds is three or four samples in three hundred and does not move
        // the middle at all. What is reported here is the same mistake this
        // project spent a day finding in its other instruments, which is an
        // instrument that answers a question nobody asked.
        let at = |q: &std::collections::VecDeque<Duration>, p: f64| -> Option<f64> {
            if q.is_empty() {
                return None;
            }
            let mut v: Vec<u128> = q.iter().map(|d| d.as_micros()).collect();
            v.sort_unstable();
            let i = ((v.len() - 1) as f64 * p).round() as usize;
            Some(v[i] as f64 / 1000.0)
        };
        let median = |q: &std::collections::VecDeque<Duration>| at(q, 0.5);
        let Some(commit_to_scanout) = median(&self.latencies) else {
            return;
        };
        // What the television is actually being given, which under a variable
        // refresh rate is not the mode's own rate and is the number worth
        // showing. Falling back to the mode's when nothing has been measured.
        let frame = median(&self.intervals).unwrap_or(self.period().as_micros() as f64 / 1000.0);
        // Written beside and renamed: a reader that arrives in the middle
        // of a plain write finds half a line, and half a measurement is
        // worse than none.
        let path = display::latency_path();
        let tmp = path.with_extension("latency.new");
        // Four numbers as before, so every reader that only wants those
        // keeps working, and then the tail: the 95th and the worst of the
        // commit to scanout, and the shortest and longest frame the tube was
        // actually given. The last two are the pair that says whether the
        // frame length is steady, which under a variable refresh rate is the
        // thing a television reacts to.
        // And two more, in the unit the television's own circuitry counts in.
        //
        // A set's vertical countdown, once locked, accepts sync inside a
        // narrow window: a Philips jungle datasheet gives 261 to 264 lines a
        // field for the 60 Hz standard, and a pulse outside it starts the
        // retrace at the edge of the window rather than on the sync that
        // arrived. Two lines over nominal is therefore the number that
        // predicts what somebody sees, and a median in milliseconds cannot
        // show it: three fields in three thousand six hundred left that
        // window on the afternoon this was written, and the row said
        // "16.66 to 16.66 ms" throughout.
        //
        // The window belongs to sets of that design and has not been measured
        // on any particular television, so what is written here is the count
        // and the longest field, and the reading of them is left to whoever
        // knows which set is in the room.
        let vtotal = self.modeline.as_ref().map(|m| m.v[3]).unwrap_or(262).max(1) as f64;
        let nominal = self.period().as_micros() as f64;
        let lines_of = |us: f64| us * vtotal / nominal;
        let over = self
            .intervals
            .iter()
            .filter(|d| lines_of(d.as_micros() as f64) > vtotal + 2.0)
            .count();
        let longest_lines = self
            .intervals
            .iter()
            .map(|d| lines_of(d.as_micros() as f64))
            .fold(0.0f64, f64::max)
            .max(vtotal);
        let line = format!(
            "{commit_to_scanout:.2} {:.2} {} {:.3} {:.2} {:.2} {:.3} {:.3} {over} {longest_lines:.1}\n",
            commit_to_scanout / frame,
            self.latencies.len(),
            1000.0 / frame,
            at(&self.latencies, 0.95).unwrap_or(commit_to_scanout),
            at(&self.latencies, 1.0).unwrap_or(commit_to_scanout),
            at(&self.intervals, 0.0).unwrap_or(frame),
            at(&self.intervals, 1.0).unwrap_or(frame),
        );
        if std::fs::write(&tmp, line).is_ok() && std::fs::rename(&tmp, &path).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }

    /// The longest frame this television will hold its picture at, in the
    /// mode it is in now.
    ///
    /// Never shorter than the mode's own frame: hardware cannot make a frame
    /// shorter than its timing, and a floor below the mode would leave the
    /// scheduler no room at all.
    fn slowest(&self) -> Duration {
        let period = self.period();
        period.mul_f64(self.stretch).max(period)
    }

    /// The period of one frame on the tube.
    fn period(&self) -> Duration {
        self.output
            .current_mode()
            .map(|m| Duration::from_nanos(1_000_000_000_000u64 / (m.refresh.max(1) as u64)))
            .unwrap_or(Duration::from_nanos(16_666_666))
    }

    /// How long before the vblank the drawing has to start.
    ///
    /// Twice what the last few frames cost, plus a millisecond and a half.
    /// Too small and the frame misses its flip and arrives a whole frame
    /// late, which is the thing this is trying to avoid; too large and the
    /// picture is older than it needs to be by the difference.
    fn margin(&self) -> Duration {
        margin_for(self.period(), self.render_cost, self.margin_override)
    }

    /// Draw at the last safe moment of this frame rather than at its start.
    ///
    /// A flip queued anywhere inside a frame is shown at the next vblank, so
    /// there is nothing to gain by queueing early and a whole frame to lose:
    /// a client told at the vblank that it may draw commits a millisecond
    /// later, and if the flip has already gone its picture waits for the
    /// frame after. Waiting until the deadline catches that commit instead.
    fn arm_deadline(&mut self) {
        if self.deadline_armed || !self.frame_delay {
            return;
        }
        let wait = self.period().saturating_sub(self.margin());
        self.deadline_armed = self
            .handle
            .insert_source(Timer::from_duration(wait), |_, _, st: &mut Crt| {
                st.deadline_armed = false;
                st.render();
                if !st.frame_queued {
                    st.arm_pacer();
                }
                TimeoutAction::Drop
            })
            .is_ok();
    }

    /// A frame's worth of work that does not depend on a flip: tell every
    /// client it may draw, then draw. Called from the vblank while the tube
    /// is flipping, and from the pacer while it is not.
    ///
    /// A client asks for a frame callback and waits for it before drawing
    /// again. Once the compositor stops flipping there are no vblanks, so
    /// without a pacer of its own an idle tube would never let anybody draw
    /// again and would stay idle for ever.
    fn tick(&mut self) {
        // Unless this frame's telling is already in hand: told twice, a
        // client draws twice and throws one away. The pacer is only for a
        // tube that has stopped flipping altogether.
        if !self.deadline_armed && !self.callback_armed && !self.frame_queued {
            self.send_frames();
        }
        self.render();
        if !self.frame_queued {
            self.arm_pacer();
        }
    }

    /// Tell every client it may draw.
    fn send_frames(&mut self) {
        let t = self.start.elapsed();
        let output = self.output.clone();
        for w in self.space.elements() {
            w.send_frame(&output, t, Some(Duration::from_secs(1)), |_, _| {
                Some(output.clone())
            });
        }
        self.told_at = Some(Instant::now());
        self.telling = self.telling.wrapping_add(1);
    }

    /// The drawing time the callbacks are paced by: the top window's, which
    /// is the one being looked at. A window with no reading yet is assumed
    /// to want a whole frame, and so is a tube with nothing mapped on it.
    fn client_cost(&self) -> Duration {
        self.space
            .elements()
            .next_back()
            .map(|w| cost_of(w).cost.get())
            .unwrap_or(Duration::from_millis(16))
            .min(self.period())
    }

    /// Tell the clients they may draw at the moment that leaves their
    /// picture as new as possible when it is scanned out.
    ///
    /// Told at the vblank, a program draws at once and its finished picture
    /// then sits waiting most of a frame for the deadline. Told twice its
    /// own drawing time before the deadline instead, it finishes just in
    /// time and the picture on the tube is that much fresher. This is what
    /// GroovyMAME calls frame delay, done here for every client at once
    /// rather than inside one emulator.
    ///
    /// The cost of getting it wrong is a frame: a program that is late
    /// misses the deadline and is shown one frame later than it would have
    /// been. So the estimate is a decaying maximum with a factor of two and
    /// three milliseconds on top, and a commit that does arrive late pushes
    /// it out at once. It starts at a whole frame, which is where every
    /// other compositor leaves it, and comes down over about a second.
    ///
    /// It is on, and this is what says it is safe. The launcher commits
    /// 0.23 ms after being told, RetroArch about the same, and neither was
    /// late for a single frame in thirty seconds, quiet or with every core
    /// on the machine busy; the launcher's own frame rate under that load
    /// fell from 60.2 to 59 either way. What it buys the launcher is the
    /// commit to the start of scanout falling from 16.6 ms to 5.3.
    ///
    /// The one client measured slipping is an artificial one that sleeps a
    /// millisecond in `ppoll` and commits: one frame in eight arrives late,
    /// and more slack does not buy it back, because what it is losing to is
    /// the jitter of its own wakeup rather than a want of room. A real
    /// program drawing a real frame does not behave that way.
    /// `FLYBACK_LATE_DRAW=off` goes back to telling clients at the vblank,
    /// `FLYBACK_SLACK_US` tunes the room they are given.
    fn arm_callbacks(&mut self) {
        if self.callback_armed {
            return;
        }
        // With a fixed refresh the compositor has to be drawing by the
        // margin or the frame is shown a whole frame late, so the room a
        // client gets stops there. With a variable one it does not: a flip
        // that arrives after the frame's minimum length simply makes that
        // frame longer, and nothing is dropped. So the whole frame is room,
        // and a client is given as much of it as its own drawing time
        // allows, which is what puts its picture on the glass at once.
        let room = if self.vrr_on {
            frame_room(
                self.target_period,
                self.client_period,
                self.period(),
                self.slowest(),
            )
        } else {
            self.period().saturating_sub(self.margin())
        };
        let client_cost = self.client_cost();
        let slack = if self.vrr_on {
            self.client_slack / 2
        } else {
            self.client_slack
        };
        let wait = if self.target_period.is_some() && self.vrr_on {
            // A rate was asked for, so the commit is aimed at it rather than
            // held clear of a deadline: told its own drawing time before the
            // frame should end, a program commits as that frame ends and the
            // television is given exactly the length that was asked for.
            // Early would make the frame short, and short is the one thing
            // the hardware cannot do.
            let aim = room.saturating_sub(client_cost).as_micros() as i64;
            Duration::from_micros((aim + self.rate_trim).max(0) as u64)
        } else if self.late_draw || self.vrr_on {
            callback_wait(room, client_cost, slack)
        } else {
            Duration::ZERO
        };
        if wait.is_zero() {
            self.send_frames();
            return;
        }
        self.callback_armed = self
            .handle
            .insert_source(Timer::from_duration(wait), |_, _, st: &mut Crt| {
                st.callback_armed = false;
                st.send_frames();
                TimeoutAction::Drop
            })
            .is_ok();
    }

    /// Stop the field before it runs past what this television follows.
    ///
    /// Under a variable refresh rate a field ends when a flip lands, and if
    /// none does it runs to whatever the hardware allows: `V_TOTAL_MAX`, set
    /// from the FreeSync range in the EDID this project writes, which is 328
    /// lines here. This set gives up at 291. Measured with the launcher alone
    /// on the tube and a terminal busy on the desktop, twenty-six fields in
    /// every three hundred left the set's window and the worst ran the whole
    /// way to 328: it does not take a game, it takes the desk being used.
    ///
    /// Nobody else has this problem because on a panel the driver's own
    /// below-the-range handling does the same job, and it is off here for a
    /// reason we cannot change: it wants the declared maximum to be at least
    /// twice the declared minimum, and 62 over 48 is not. So the compositor
    /// does it: if no real frame has arrived by the time the field reaches
    /// the floor, the last one is sent again. A repeated field keeps the set
    /// locked. A field that runs off the end does not.
    fn arm_floor(&mut self) {
        if self.floor_armed || !self.vrr_on {
            return;
        }
        // Less the cost of drawing and queueing, because the flip has to be
        // in the kernel's hands by the floor and not started at it.
        let wait = self
            .slowest()
            .saturating_sub(self.render_cost + Duration::from_micros(500));
        let seq = self.field_seq;
        self.floor_armed = self
            .handle
            .insert_source(Timer::from_duration(wait), move |_, _, st: &mut Crt| {
                st.floor_armed = false;
                st.floor_tick(seq);
                TimeoutAction::Drop
            })
            .is_ok();
    }

    /// The floor came up. Send the last picture again unless a real one is
    /// already on its way.
    fn floor_tick(&mut self, seq: u64) {
        // A vblank since this was armed means the field it belonged to is
        // over and somebody else is keeping time.
        if seq != self.field_seq || self.frame_queued || !self.vrr_on {
            return;
        }
        // smithay will not flip a frame with nothing new in it, which is
        // usually what keeps a still picture from costing anything. Here the
        // whole point is to flip a frame with nothing new in it, so the
        // buffer ages are reset and every part of the picture counts as
        // damaged again.
        if let Some(out) = self.drm_output.as_ref() {
            out.with_compositor(|c| c.reset_buffer_ages());
        }
        self.fields_repeated += 1;
        if self.trace {
            eprintln!(
                "trace: the field reached the floor with no frame, repeating ({} so far)",
                self.fields_repeated
            );
        }
        self.dirty = true;
        self.dirty_top = true;
        self.render();
    }

    /// One tick in a frame's time, unless one is already on its way.
    fn arm_pacer(&mut self) {
        if self.pacer_armed {
            return;
        }
        let period = self
            .output
            .current_mode()
            .map(|m| Duration::from_nanos(1_000_000_000_000u64 / (m.refresh.max(1) as u64)))
            .unwrap_or(Duration::from_nanos(16_666_666));
        self.pacer_armed = self
            .handle
            .insert_source(Timer::from_duration(period), |_, _, st: &mut Crt| {
                st.pacer_armed = false;
                st.tick();
                TimeoutAction::Drop
            })
            .is_ok();
    }

    fn render(&mut self) {
        // Nothing has changed since the last flip, so do not make another
        // one. A flip in the air is a frame of latency for whatever is
        // committed next: the client's picture cannot be queued until the
        // one already queued has landed. Staying idle means the next commit
        // is queued the moment it arrives and reaches the screen a whole
        // frame sooner. While recording, keep flipping: the recorder counts
        // frames on the vblank, and a vblank only arrives for a flip.
        if !self.dirty && self.recorder.is_none() {
            self.arm_pacer();
            return;
        }
        // Only a window underneath has changed, and the window on top is
        // still answering its callbacks. Flipping for the one nobody can see
        // would take the slot the top window's next commit needs: its
        // picture would then wait for this flip to land and be scanned out a
        // frame late. Measured, that happened to one frame in eight and cost
        // it two frames. So leave the damage marked and let it go out with
        // the top window's next frame - which is at most one frame away,
        // because that is what "still answering" means. When the top window
        // falls quiet for two frames it stops owning the cadence and
        // everything else is drawn again.
        if self.vrr_on
            && !self.dirty_top
            && self.recorder.is_none()
            && self
                .last_top_commit
                .is_some_and(|t| t.elapsed() < self.period() * 2)
        {
            self.arm_pacer();
            return;
        }
        if self.frame_queued {
            // A flip is already in the air. Whatever was just committed
            // cannot be drawn until it lands, which costs the client a whole
            // frame; how often that happens is the one number that says
            // whether the compositor is in the way.
            self.commits_blocked += 1;
            if self.trace {
                let top = self
                    .space
                    .elements()
                    .next_back()
                    .and_then(|w| w.toplevel().cloned())
                    .and_then(|t| {
                        with_states(t.wl_surface(), |s| {
                            s.data_map
                                .get::<XdgToplevelSurfaceData>()
                                .and_then(|d| d.lock().unwrap().app_id.clone())
                        })
                    })
                    .unwrap_or_default();
                eprintln!(
                    "trace: blocked, {} us since the vblank, {} us since the flip was queued, top {top}",
                    self.last_vblank
                        .map(|t| t.elapsed().as_micros())
                        .unwrap_or(0),
                    self.queued_at.map(|t| t.elapsed().as_micros()).unwrap_or(0),
                );
            }
            return;
        }
        let Some(out) = self.drm_output.as_mut() else {
            return;
        };
        let began = Instant::now();
        self.space.refresh();
        let elements =
            match space_render_elements(&mut self.renderer, [&self.space], &self.output, 1.0) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("render elements: {e}");
                    return;
                }
            };
        match out.render_frame(
            &mut self.renderer,
            &elements,
            [0.02, 0.03, 0.06, 1.0],
            // Scanning a client's own buffer straight out to the plane is
            // allowed here and never happens, which was worth finding out:
            // every program on the tube draws at its own size, 320x240 for
            // the launcher and the core's own geometry for an emulator, and
            // a plane cannot scale that up to a 3520 sample line. So each
            // frame costs one composite pass, measured at two to five tenths
            // of a millisecond, and it is that pass which does the widening.
            FrameFlags::DEFAULT,
        ) {
            Ok(res) => {
                if res.is_empty {
                    // Nothing to show after all, so the damage is spent. No
                    // feedback is taken off the surfaces here, because taking
                    // it would owe a client an answer that this frame is
                    // never going to give.
                    self.dirty = false;
                    return;
                }
                // Whether the client's own buffer went straight to the plane
                // or had to be drawn into ours. On this television the
                // difference is a whole copy of the frame, so it is worth a
                // line in the log the first time it changes.
                let direct = matches!(res.primary_element, PrimaryPlaneElement::Element(_));
                if self.scanout_direct != Some(direct) {
                    self.scanout_direct = Some(direct);
                    eprintln!(
                        "primary plane: {}",
                        if direct {
                            "the client's own buffer, scanned out directly"
                        } else {
                            "composited into ours"
                        }
                    );
                }
                // Which output each surface was drawn on. There is only one
                // here, but the record is what `surface_primary_scanout_output`
                // reads a moment later, and without it every surface looks
                // like it was drawn nowhere and nobody is ever told anything.
                for window in self.space.elements() {
                    window.with_surfaces(|surface, states| {
                        update_surface_primary_scanout_output(
                            surface,
                            &self.output,
                            states,
                            &res.states,
                            default_primary_scanout_output_compare,
                        );
                    });
                }
                // Who to tell, and when. Collected before the flip is queued
                // because the states belong to this frame's render, and kept
                // with the frame until the vblank that shows it.
                let mut feedback = OutputPresentationFeedback::new(&self.output);
                for window in self.space.elements() {
                    window.take_presentation_feedback(
                        &mut feedback,
                        surface_primary_scanout_output,
                        |surface, _| {
                            surface_presentation_feedback_flags_from_states(surface, &res.states)
                        },
                    );
                }
                let feedback = Some(feedback);
                let queued = {
                    let q = Instant::now();
                    let r = out.queue_frame(feedback);
                    let spent = q.elapsed();
                    // A flip is a register write and takes microseconds. One
                    // that takes milliseconds is carrying a modeset, which is
                    // the only thing here that makes the television dark.
                    //
                    // Not while a mode change is already being reported: the
                    // two lines either side of it say the same thing with
                    // more in them, and fifty of anything in a log is read as
                    // a condition rather than an event. An evening of fifty
                    // games would have turned the self test red for doing
                    // exactly what it is supposed to do.
                    if spent > Duration::from_millis(2) && self.mode_at.is_none() {
                        println!("queue_frame: {:.1} ms", spent.as_secs_f64() * 1000.0);
                    }
                    r
                };
                if let Err(e) = queued {
                    // The flip was refused, so no vblank is coming and
                    // `frame_queued` stays false: every client commit tries
                    // again. That is a picture that never arrives and a log
                    // line per commit, so stop and let the watchdog restart
                    // the display rather than spin here for ever.
                    self.flips_failed += 1;
                    eprintln!("queue_frame: {e} ({} in a row)", self.flips_failed);
                    if self.flips_failed >= FLIP_FAILURES_ALLOWED {
                        eprintln!(
                            "the connector has refused {FLIP_FAILURES_ALLOWED} page flips in a row: giving up the lease"
                        );
                        self.running = false;
                    }
                } else {
                    self.flips_failed = 0;
                    self.dirty = false;
                    self.dirty_top = false;
                    // What this frame cost, kept as a decaying maximum: the
                    // deadline has to clear the worst of the last second,
                    // not the average, or every heavy frame arrives late.
                    let cost = began.elapsed();
                    self.render_cost = cost.max(self.render_cost * 15 / 16);
                    self.showing = self.last_commit;
                    self.frame_queued = true;
                    self.queued_at = Some(Instant::now());
                    if self.trace {
                        let since = self
                            .last_commit
                            .map(|t| t.elapsed().as_micros())
                            .unwrap_or(0);
                        eprintln!(
                            "trace: commit to flip queued {since} us, client {} us, period {} us, render {} us",
                            self.client_cost().as_micros(),
                            self.client_period.as_micros(),
                            self.render_cost.as_micros()
                        );
                    }
                }
            }
            Err(e) => {
                eprintln!("render_frame: {e}");
                out.reset_buffers();
            }
        }
    }

    fn vblank(&mut self, meta: Option<DrmEventMetadata>) {
        // The flip has happened, and the kernel says when. Every client whose
        // surface was in that frame is told, on CLOCK_MONOTONIC, which is
        // what wp_presentation promised at bind time.
        let seq = meta.as_ref().map(|m| m.sequence).unwrap_or(0);
        let when = meta.as_ref().and_then(|m| match m.time {
            DrmEventTime::Monotonic(t) => Some(t),
            // A driver that timestamps on the realtime clock cannot be
            // compared with a monotonic one; our own reading of the same
            // clock is closer to the truth than a conversion would be.
            DrmEventTime::Realtime(_) => None,
        });
        // Only the driver's own stamp earns the hardware flags: saying a time
        // came from the display hardware when it came from a call to the
        // clock a moment later is worse than admitting the estimate, because
        // a client uses those flags to decide how much to trust the number.
        let mut flags = wp_presentation_feedback::Kind::Vsync;
        if when.is_some() {
            flags |= wp_presentation_feedback::Kind::HwClock
                | wp_presentation_feedback::Kind::HwCompletion;
        }
        let now = when.unwrap_or_else(monotonic);
        let refresh = self
            .output
            .current_mode()
            .map(|m| Duration::from_nanos(1_000_000_000_000u64 / (m.refresh.max(1) as u64)))
            .unwrap_or(Duration::from_nanos(16_666_666));
        if let Some(out) = self.drm_output.as_mut()
            && let Ok(Some(Some(mut feedback))) = out.frame_submitted()
        {
            feedback.presented::<Duration, smithay::utils::Monotonic>(
                now,
                Refresh::fixed(refresh),
                seq as u64,
                flags,
            );
        }
        if let Some(t) = self.mode_at.take() {
            println!(
                "mode: first vblank {:.1} ms after the modeline was asked for",
                t.elapsed().as_secs_f64() * 1000.0
            );
        }
        // The interval between two vblanks is the refresh the television is
        // actually being given, whatever the mode says it is. With a
        // stretched vertical blanking they stop being equal, and that is the
        // only way to see from here that it worked.
        // Measured on the driver's own stamp, which is when the hardware
        // flipped. `Instant::now()` here is when this process woke up, and
        // the two differ by however late the wakeup was.
        //
        // This mattered more than it looks. Reading the second and calling it
        // the first made a variable refresh rate and a fixed one produce the
        // same figure, because at a fixed rate the hardware's frame length
        // cannot vary at all and every bit of the spread was our own wakeup,
        // while under a variable rate the frame really does end when the flip
        // lands. The two are the same size here, so the instrument could not
        // tell a frame that was longer from a wakeup that was late.
        // A new field starts here, so any floor timer still out belongs to
        // the one that just ended.
        self.field_seq = self.field_seq.wrapping_add(1);
        let woke = self
            .last_vblank
            .replace(Instant::now())
            .map(|t| t.elapsed());
        let hw = when.and_then(|now| {
            let prev = self.last_vblank_hw.replace(now);
            prev.filter(|p| now > *p).map(|p| now - p)
        });
        if let Some(interval) = hw.or(woke) {
            if self.trace {
                match (hw, woke) {
                    (Some(h), Some(w)) => eprintln!(
                        "trace: vblank interval {} us on the driver's clock, {} us on ours",
                        h.as_micros(),
                        w.as_micros()
                    ),
                    _ => eprintln!("trace: vblank interval {} us", interval.as_micros()),
                }
            }
            // A vblank event only arrives for a flip, so this is the time
            // between two flips and not the length of a frame. They are the
            // same thing only while the compositor is flipping every frame.
            // A still picture flips for nothing, and the gap then reads as a
            // frame of two or ten or a thousand, which is how this made a
            // fixed refresh rate look like a tube being asked for frames of
            // different lengths: two frames exactly, on a menu nobody was
            // touching.
            //
            // Half a frame of slack over the mode's own period is enough to
            // keep every frame a variable rate can legally stretch to, since
            // nothing is ever asked for a rate below output.vrr_min_hz, and
            // to drop anything that skipped a flip.
            if interval * 2 < self.period() * 3 {
                self.intervals.push_back(interval);
                if self.intervals.len() > 300 {
                    self.intervals.pop_front();
                }
            }
            // Close the loop on the frame length that was asked for. Only on
            // frames of a plausible length: the first one after an idle tube
            // is seconds long and would throw the correction across its
            // whole range.
            if let Some(target) = self.target_period
                && self.vrr_on
                && interval < self.period() * 3
            {
                self.rate_trim = trim_step(self.rate_trim, interval, target, self.period());
            }
        }
        // The estimates come down on their own, so a client that was slow
        // once is not given the whole frame for ever. A floor, because
        // nothing is woken and drawn in less than that.
        for w in self.space.elements() {
            let c = cost_of(w);
            c.cost
                .set((c.cost.get() * 31 / 32).max(Duration::from_micros(300)));
        }
        if let Some(t) = self.showing.take() {
            self.latencies.push_back(t.elapsed());
            if self.latencies.len() > 300 {
                self.latencies.pop_front();
            }
        }
        if self.trace
            && let Some(t) = self.queued_at
        {
            eprintln!(
                "trace: flip queued to vblank {} us",
                t.elapsed().as_micros()
            );
        }
        self.frame_queued = false;
        self.queued_at = None;
        self.frames += 1;
        if self.recorder.is_some() && self.frames.is_multiple_of(2) {
            // Scale straight out of the mapped pixels into a buffer the
            // writer thread has finished with: no full sized copy of the
            // frame in between, and no allocation per frame.
            let mut frame = self
                .recorder
                .as_mut()
                .map(|r| r.buffer())
                .unwrap_or_default();
            let scaled = self.with_frame(|bytes, flipped, w, h| {
                for y in 0..REC_H {
                    let sy = y * h / REC_H;
                    // GL reads bottom-up unless the mapping says otherwise.
                    let src_y = if flipped { sy } else { h - 1 - sy };
                    let src_row = &bytes[src_y * w * 4..(src_y + 1) * w * 4];
                    let dst_row = &mut frame[y * REC_W * 4..(y + 1) * REC_W * 4];
                    for x in 0..REC_W {
                        let sx = x * w / REC_W;
                        dst_row[x * 4..x * 4 + 4].copy_from_slice(&src_row[sx * 4..sx * 4 + 4]);
                    }
                }
            });
            if let (Ok(()), Some(r)) = (scaled, self.recorder.as_mut()) {
                r.push(frame);
            }
        }
        if self.last_stats.elapsed() >= Duration::from_secs(5) {
            self.last_stats = Instant::now();
            let commits: Vec<String> = self
                .commits
                .iter()
                .map(|(k, v)| format!("{k}:{v}"))
                .collect();
            self.commits.clear();
            if omacrt_shell::logfile::debug_enabled() {
                println!(
                    "{} frames so far, {} client window(s) mapped, commits in 5 s: {}{}",
                    self.frames,
                    self.space.elements().count(),
                    commits.join(" "),
                    if self.commits_blocked > 0 {
                        format!(
                            ", {} commit(s) waited for a flip already in the air, {} of them \
                             too late for their own frame (a client is given {} us to draw in)",
                            std::mem::take(&mut self.commits_blocked),
                            std::mem::take(&mut self.late),
                            self.client_cost().as_micros()
                        )
                    } else {
                        String::new()
                    }
                );
            }
            self.write_latency();
            // While it runs, keep the file from growing without end.
            omacrt_shell::logfile::rotate_if_big(
                &display::log_path(),
                omacrt_shell::logfile::CAP_BYTES,
            );
        }
        if self.vrr_on {
            // With a variable refresh rate there is no deadline to miss, so
            // the only thing to do at the vblank is decide when to let the
            // clients draw. Drawing here would be the mistake: the frame a
            // client is about to commit would find a flip already in the air
            // and wait a whole frame for the next one.
            self.arm_callbacks();
            // And nothing else. Anything committed while the last flip was
            // in flight is about to be superseded by the frame the client is
            // drawing now, so flipping it here would only put a stale
            // picture in the air and make the fresh one wait for the frame
            // after.
            //
            // This used to draw when no telling was pending, on the reasoning
            // that nothing newer was coming. It is not so: a telling that has
            // just gone out - which is what a client asking for the whole
            // frame produces - means a commit is on its way in a millisecond
            // or two, and the flip queued here takes the slot it needed. That
            // commit is then blocked for the whole frame, its own estimate
            // rises because it looks slow, the next telling goes out at the
            // vblank too, and the thing latches: measured, one frame in five
            // arriving two frames late, for ever. The pacer draws whatever is
            // still dirty a frame later, which is where damage that no client
            // is about to supersede belongs.
            if !self.frame_queued {
                self.arm_pacer();
            }
        } else if self.frame_delay {
            self.arm_callbacks();
            self.arm_deadline();
        } else {
            self.tick();
        }
        // Whatever else this field is waiting for, it must not run past what
        // the set follows.
        self.arm_floor();
    }

    fn window_for(&self, surface: &WlSurface) -> Option<Window> {
        let mut root = surface.clone();
        while let Some(p) = get_parent(&root) {
            root = p;
        }
        self.space
            .elements()
            .find(|w| {
                w.toplevel()
                    .map(|t| t.wl_surface() == &root)
                    .unwrap_or(false)
            })
            .cloned()
    }
}

impl CompositorHandler for Crt {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }
    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        // Every client is inserted with a `ClientState`, by the only code
        // that inserts one, a few hundred lines below.
        &client.get_data::<ClientState>().unwrap().compositor_state
    }
    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        if !is_sync_subsurface(surface)
            && let Some(window) = self.window_for(surface)
        {
            window.on_commit();
            if let Some(t) = window.toplevel() {
                let app = with_states(t.wl_surface(), |s| {
                    s.data_map
                        .get::<XdgToplevelSurfaceData>()
                        .and_then(|d| d.lock().unwrap().app_id.clone())
                })
                .unwrap_or_default();
                *self.commits.entry(app).or_insert(0) += 1;
            }
            if let Some(t) = window.toplevel()
                && t.wl_surface() == surface
            {
                let sent = with_states(surface, |s| {
                    s.data_map
                        .get::<XdgToplevelSurfaceData>()
                        .map(|d| d.lock().unwrap().initial_configure_sent)
                        .unwrap_or(true)
                });
                if !sent {
                    t.send_configure();
                }
            }
        }
        let at = Instant::now();
        // How long this client took between being told it may draw and
        // committing, charged to the window that committed. Only the first
        // commit after a telling counts: the rest are a client drawing more
        // than once in a frame.
        if let Some(told) = self.told_at
            && let Some(w) = self.window_for(surface)
        {
            let c = cost_of(&w);
            if c.counted.get() != self.telling {
                c.counted.set(self.telling);
                let cost = at.saturating_duration_since(told);
                // A client that answers a whole frame later was not waiting
                // on this telling: the callback went out when it had not
                // asked for one, and the wait that follows is the next
                // frame's, not its drawing time. Taken as a reading it pegs
                // the estimate at a frame for ever, and a client given no
                // room draws at the vblank again, which is the thing this is
                // trying to avoid.
                if cost * 2 < self.period() {
                    if c.measured.replace(true) {
                        c.cost.set(c.cost.get().max(cost));
                    } else {
                        c.cost.set(cost);
                    }
                }
            }
        }
        // Too late for the frame it was meant for: the deadline has gone and
        // the flip with it. Give this client more room next time rather than
        // let it miss every frame.
        //
        // Not under a variable refresh rate, where nothing is ever late: a
        // commit that arrives with a flip in the air is shown as soon as
        // that one has had its minimum, and the frame is a little longer.
        // Counting it as late walks the estimate up to a whole frame, the
        // client is then told to draw at the vblank, and its picture waits
        // for the flip it has just missed - which is the very thing the
        // variable rate is there to avoid.
        if !self.vrr_on && !self.deadline_armed && self.frame_queued {
            self.late += 1;
            if let Some(w) = self.window_for(surface) {
                let c = cost_of(&w);
                c.cost.set((c.cost.get() * 5 / 4).min(self.period()));
            }
        }
        // The pace the tube is asked to keep, and the commit the next frame
        // will be showing: both are the top window's alone. With a second
        // client mapped underneath, the interval between any two commits is
        // not any client's own period - measured with the launcher under a
        // client running at 60 Hz, it read 15.6 ms for a 16.65 ms frame, and
        // a frame of room that is a millisecond short is a millisecond of
        // the client's picture thrown away.
        //
        // The median of the last sixteen, so one long gap - a game loading,
        // a menu opening - does not move the estimate.
        let top = self.is_top(surface);
        if top && let Some(prev) = self.last_commit.replace(at) {
            let d = at.saturating_duration_since(prev);
            if d > Duration::from_millis(4) && d < Duration::from_millis(100) {
                self.periods.push_back(d);
                if self.periods.len() > 16 {
                    self.periods.pop_front();
                }
                let mut v: Vec<Duration> = self.periods.iter().copied().collect();
                v.sort_unstable();
                self.client_period = v[v.len() / 2];
            }
        }
        self.damaged_by(top);
    }
}

impl BufferHandler for Crt {
    fn buffer_destroyed(&mut self, _: &wl_buffer::WlBuffer) {}
}

impl ShmHandler for Crt {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl DmabufHandler for Crt {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf_state
    }
    fn dmabuf_imported(&mut self, _: &DmabufGlobal, dmabuf: Dmabuf, notifier: ImportNotifier) {
        if self.renderer.import_dmabuf(&dmabuf, None).is_ok() {
            let _ = notifier.successful::<Crt>();
        } else {
            notifier.failed();
        }
    }
}

impl XdgShellHandler for Crt {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }
    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let size = self
            .output
            .current_mode()
            .map(|m| m.size)
            .unwrap_or((320, 240).into());
        surface.with_pending_state(|s| {
            s.size = Some((size.w, size.h).into());
            s.states.set(xdg_toplevel::State::Fullscreen);
            s.states.set(xdg_toplevel::State::Activated);
        });
        let app = with_states(surface.wl_surface(), |s| {
            s.data_map
                .get::<XdgToplevelSurfaceData>()
                .and_then(|d| d.lock().unwrap().app_id.clone())
        });
        println!("client window: {}", app.unwrap_or_default());
        let window = Window::new_wayland_window(surface);
        self.space.map_element(window, (0, 0), true);
        self.focus_top();
    }
    fn new_popup(&mut self, surface: PopupSurface, _: PositionerState) {
        let _ = surface.send_configure();
    }
    fn grab(&mut self, _: PopupSurface, _: wl_seat::WlSeat, _: Serial) {}
    fn reposition_request(&mut self, _: PopupSurface, _: PositionerState, _: u32) {}
    fn resize_request(
        &mut self,
        _: ToplevelSurface,
        _: wl_seat::WlSeat,
        _: Serial,
        _: xdg_toplevel::ResizeEdge,
    ) {
    }
    fn show_window_menu(
        &mut self,
        _: ToplevelSurface,
        _: wl_seat::WlSeat,
        _: Serial,
        _: smithay::utils::Point<i32, smithay::utils::Logical>,
    ) {
    }
    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        if let Some(w) = self.window_for(surface.wl_surface()) {
            self.space.unmap_elem(&w);
        }
        self.focus_top();
        self.damaged();
    }
}

impl SeatHandler for Crt {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;
    fn seat_state(&mut self) -> &mut SeatState<Crt> {
        &mut self.seat_state
    }
    fn cursor_image(&mut self, _: &Seat<Self>, _: smithay::input::pointer::CursorImageStatus) {}
    fn focus_changed(&mut self, _: &Seat<Self>, _: Option<&WlSurface>) {}
}

impl OutputHandler for Crt {}

delegate_compositor!(Crt);
delegate_xdg_shell!(Crt);
delegate_shm!(Crt);
delegate_seat!(Crt);
delegate_output!(Crt);
delegate_dmabuf!(Crt);
delegate_viewporter!(Crt);
delegate_presentation!(Crt);
