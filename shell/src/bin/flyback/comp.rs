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
use smithay::backend::drm::compositor::{FrameFlags, PrimaryPlaneElement};
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
use smithay::backend::drm::output::{DrmOutput, DrmOutputManager, DrmOutputRenderElements};
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent};
use smithay::backend::drm::{DrmEventMetadata, DrmEventTime};
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::input::KeyState;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::gles::GlesRenderbuffer;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::utils::on_commit_buffer_handler;
use smithay::backend::renderer::{
    Bind, ExportMem, ImportDma, ImportEgl, Offscreen, TextureMapping,
};
use smithay::desktop::space::{SpaceRenderElements, space_render_elements};
use smithay::desktop::{Space, Window};
use smithay::input::keyboard::{FilterResult, Keycode};
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::output::{Mode as WlMode, Output, PhysicalProperties, Scale, Subpixel};
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{
    EventLoop, Interest, LoopHandle, Mode, PostAction, generic::Generic,
};
use smithay::reexports::drm::control::Device as _;
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
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
    XdgToplevelSurfaceData,
};
use smithay::wayland::shm::{ShmHandler, ShmState};
use smithay::desktop::utils::{
    OutputPresentationFeedback, surface_presentation_feedback_flags_from_states,
    surface_primary_scanout_output, update_surface_primary_scanout_output,
};
use smithay::backend::renderer::element::default_primary_scanout_output_compare;
use smithay::reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;
use smithay::wayland::socket::ListeningSocketSource;
use smithay::wayland::presentation::{PresentationState, Refresh};
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
    let _presentation_state =
        PresentationState::new::<Crt>(&dh, libc::CLOCK_MONOTONIC as u32);
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
    // read-write so it never reports end of file between writers.
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
    Ok(())
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
            other => match other.parse::<u32>() {
                Ok(code) => code,
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
        let Some(out) = self.drm_output.as_mut() else {
            return;
        };
        let mode = drm_mode(ml);
        if let Err(e) = out.use_mode(
            mode,
            &mut self.renderer,
            &DrmOutputRenderElements::<GlesRenderer, Element>::default(),
        ) {
            eprintln!("mode: {e}");
            return;
        }
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
            "mode: {}x{} {:.3} kHz {:.3} Hz",
            w,
            h,
            ml.hfreq_khz(),
            ml.vfreq_hz()
        );
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
        self.dirty = true;
        self.render();
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
        let t = self.start.elapsed();
        let output = self.output.clone();
        for w in self.space.elements() {
            w.send_frame(&output, t, Some(Duration::from_secs(1)), |_, _| {
                Some(output.clone())
            });
        }
        self.render();
        if !self.frame_queued {
            self.arm_pacer();
        }
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
        if self.frame_queued {
            // A flip is already in the air. Whatever was just committed
            // cannot be drawn until it lands, which costs the client a whole
            // frame; how often that happens is the one number that says
            // whether the compositor is in the way.
            self.commits_blocked += 1;
            return;
        }
        let Some(out) = self.drm_output.as_mut() else {
            return;
        };
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
                if let Err(e) = out.queue_frame(feedback) {
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
                    self.frame_queued = true;
                    self.queued_at = Some(Instant::now());
                    if self.trace {
                        let since = self
                            .last_commit
                            .map(|t| t.elapsed().as_micros())
                            .unwrap_or(0);
                        eprintln!("trace: commit to flip queued {since} us");
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
        if self.trace && let Some(t) = self.queued_at {
            eprintln!("trace: flip queued to vblank {} us", t.elapsed().as_micros());
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
                            ", {} commit(s) waited for a flip already in the air",
                            std::mem::take(&mut self.commits_blocked)
                        )
                    } else {
                        String::new()
                    }
                );
            }
            // While it runs, keep the file from growing without end.
            omacrt_shell::logfile::rotate_if_big(
                &display::log_path(),
                omacrt_shell::logfile::CAP_BYTES,
            );
        }
        self.tick();
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
        self.last_commit = Some(Instant::now());
        self.damaged();
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
