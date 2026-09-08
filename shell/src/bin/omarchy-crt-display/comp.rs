//! A Wayland compositor for the tube.
//!
//! One leased DRM output, a handful of clients (the launcher, RetroArch,
//! mpv), every toplevel fullscreen at the output's size. The stacking order
//! decides what is seen; every mapped surface keeps receiving frame
//! callbacks, so a program under the pause overlay keeps running and keeps
//! answering. Clients reach us through `WAYLAND_DISPLAY=wayland-crt`.

use crate::drm_mode;
use crate::lease::Lease;
use omarchy_crt_shell::crt::output::Modeline;
use omarchy_crt_shell::crt::{Config, dac, display, output};
use smithay::backend::allocator::Fourcc;
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::drm::compositor::FrameFlags;
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
use smithay::backend::drm::output::{DrmOutput, DrmOutputManager, DrmOutputRenderElements};
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent};
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
use smithay::wayland::socket::ListeningSocketSource;
use smithay::{
    delegate_compositor, delegate_dmabuf, delegate_output, delegate_seat, delegate_shm,
    delegate_xdg_shell,
};
use std::ffi::OsString;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const SOCKET: &str = "wayland-crt";

type Allocator = GbmAllocator<DrmDeviceFd>;
type Exporter = GbmFramebufferExporter<DrmDeviceFd>;
type Element = SpaceRenderElements<GlesRenderer, WaylandSurfaceRenderElement<GlesRenderer>>;

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
    _drm: DrmOutputManager<Allocator, Exporter, (), DrmDeviceFd>,
    drm_output: Option<DrmOutput<Allocator, Exporter, (), DrmDeviceFd>>,
    frame_queued: bool,
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
    thread: Option<std::thread::JoinHandle<()>>,
    frames: u64,
    path: String,
}

const REC_W: usize = 1280;
const REC_H: usize = 960;

impl Recorder {
    fn start(path: &str, sink: Option<String>) -> Result<Recorder, String> {
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
        cmd.arg(path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit());
        let mut child = cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;
        let mut stdin = child.stdin.take().ok_or("ffmpeg stdin")?;
        let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(8);
        let thread = std::thread::spawn(move || {
            use std::io::Write;
            for frame in rx {
                if stdin.write_all(&frame).is_err() {
                    break;
                }
            }
            let _ = stdin.flush();
        });
        Ok(Recorder {
            child,
            tx,
            thread: Some(thread),
            frames: 0,
            path: path.to_string(),
        })
    }

    /// Scale the tube's frame into the recording size (nearest: the wide
    /// super resolution becomes 4:3, each of the tube's lines four pixels).
    fn push(&mut self, bgra: &[u8], w: usize, h: usize) {
        let mut out = vec![0u8; REC_W * REC_H * 4];
        for y in 0..REC_H {
            let sy = y * h / REC_H;
            let src_row = &bgra[sy * w * 4..(sy + 1) * w * 4];
            let dst_row = &mut out[y * REC_W * 4..(y + 1) * REC_W * 4];
            for x in 0..REC_W {
                let sx = x * w / REC_W;
                dst_row[x * 4..x * 4 + 4].copy_from_slice(&src_row[sx * 4..sx * 4 + 4]);
            }
        }
        // Drop the frame rather than stall the compositor when ffmpeg lags.
        if self.tx.try_send(out).is_ok() {
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
            DrmEvent::VBlank(_) => st.vblank(meta.as_ref().map(|m| m.sequence).unwrap_or(0)),
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
    crt.render();
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
    let _ = std::fs::remove_file(display::ctl_path());
    Ok(())
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
                    self.render();
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
                            std::env::var("OMARCHY_CRT_SINK")
                                .ok()
                                .filter(|s| !s.is_empty())
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
                            "hl.window_rule({{ name = \"omarchy-crt-monitor\", match = {{ class = \"{}\" }}, float = true, size = \"880 660\", center = true }})",
                            crate::host::APP_ID
                        ));
                        match crate::host::Host::open(&self.handle) {
                            Ok(h) => self.host = Some(h),
                            Err(e) => eprintln!("monitor: {e}"),
                        }
                    }
                }
                _ => {
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
        let program = ["com.libretro.RetroArch", "omarchy-crt-player"]
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
    fn capture(&mut self) -> Result<(Vec<u8>, usize, usize), String> {
        let size = self
            .output
            .current_mode()
            .map(|m| m.size)
            .ok_or("no mode")?;
        let (w, h) = (size.w, size.h);
        let mut target: GlesRenderbuffer = self
            .renderer
            .create_buffer(Fourcc::Argb8888, (w, h).into())
            .map_err(|e| format!("offscreen buffer: {e}"))?;
        let elements = space_render_elements(&mut self.renderer, [&self.space], &self.output, 1.0)
            .map_err(|e| format!("elements: {e}"))?;
        let mut tracker = OutputDamageTracker::from_output(&self.output);
        let mut fb = self
            .renderer
            .bind(&mut target)
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
        let (w, h) = (w as usize, h as usize);
        let row = w * 4;
        let mut out = vec![0u8; row * h];
        for y in 0..h {
            // GL framebuffers read bottom-up unless the mapping says otherwise.
            let src_y = if flipped { y } else { h - 1 - y };
            out[y * row..(y + 1) * row].copy_from_slice(&bytes[src_y * row..(src_y + 1) * row]);
        }
        Ok((out, w, h))
    }

    /// Write the current frame as a PNG. The desktop's screenshot tools
    /// cannot see a leased output, this can.
    fn screenshot(&mut self, path: &str) -> Result<(), String> {
        if path.is_empty() {
            return Err("shot needs a file path".into());
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
        self.render();
    }

    fn render(&mut self) {
        if self.frame_queued {
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
                    // Nothing changed: look again in a frame's time.
                    let _ = self.handle.insert_source(
                        Timer::from_duration(Duration::from_millis(16)),
                        |_, _, st: &mut Crt| {
                            st.render();
                            TimeoutAction::Drop
                        },
                    );
                } else if let Err(e) = out.queue_frame(()) {
                    eprintln!("queue_frame: {e}");
                } else {
                    self.frame_queued = true;
                }
            }
            Err(e) => {
                eprintln!("render_frame: {e}");
                out.reset_buffers();
            }
        }
    }

    fn vblank(&mut self, _seq: u32) {
        if let Some(out) = self.drm_output.as_mut() {
            let _ = out.frame_submitted();
        }
        self.frame_queued = false;
        self.frames += 1;
        if self.recorder.is_some()
            && self.frames.is_multiple_of(2)
            && let Ok((bgra, w, h)) = self.capture()
            && let Some(r) = self.recorder.as_mut()
        {
            r.push(&bgra, w, h);
        }
        if self.last_stats.elapsed() >= Duration::from_secs(5) {
            self.last_stats = Instant::now();
            let commits: Vec<String> = self
                .commits
                .iter()
                .map(|(k, v)| format!("{k}:{v}"))
                .collect();
            self.commits.clear();
            if omarchy_crt_shell::logfile::debug_enabled() {
                println!(
                    "{} frames so far, {} client window(s) mapped, commits in 5 s: {}",
                    self.frames,
                    self.space.elements().count(),
                    commits.join(" ")
                );
            }
            // While it runs, keep the file from growing without end.
            omarchy_crt_shell::logfile::rotate_if_big(
                &display::log_path(),
                omarchy_crt_shell::logfile::CAP_BYTES,
            );
        }
        let t = self.start.elapsed();
        let output = self.output.clone();
        for w in self.space.elements() {
            w.send_frame(&output, t, Some(Duration::from_secs(1)), |_, _| {
                Some(output.clone())
            });
        }
        self.render();
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
        self.render();
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
        self.render();
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
