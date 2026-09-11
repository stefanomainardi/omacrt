//! How long a frame takes to get from a client's hands onto the tube.
//!
//! This is a measuring instrument, not part of the product: it is built only
//! with `--features latency` and never installed. It answers one question
//! that nobody has published for Linux into a real 15 kHz television, and
//! that decides where any further work on latency should go.
//!
//! # What it measures
//!
//! An ordinary Wayland client on `wayland-crt`. It commits a frame at a
//! *random phase* inside the frame period, asks `wp_presentation` when that
//! frame reached the screen, and subtracts. On a CRT the answer is very
//! nearly press-to-photon for the top of the picture: there is no panel
//! buffer, no scaler and no frame store between the connector and the
//! phosphor, so the beam starts drawing the moment scanout starts, which is
//! the timestamp the kernel hands back.
//!
//! The random phase matters. A client that commits on its frame callback
//! always lands at the same point of the window, and the distribution it
//! measures is a fiction: every sample sits at the same offset from the
//! vblank. Sleeping a random fraction of a frame between commits spreads the
//! samples across the whole window, which is what a program that reacts to
//! input does.
//!
//! # What it does not measure
//!
//! The half in front of the commit: a pad's own polling and the kernel's
//! evdev delivery. Those need an input device, which needs membership of the
//! `input` group, and they are the same on every Linux machine. What is
//! measured here is the half that belongs to this project.
//!
//! # Reading the answer
//!
//! The floor is the time from a commit to the next vblank, which for a
//! commit at a random phase averages half a frame. Anything on top of that
//! is the compositor's own: rendering, the flip, and whether a commit that
//! arrives while a flip is already queued has to wait for the one after.
//!
//!   - a median near half a frame: the compositor is not in the way, and
//!     work on latency belongs in the emulator's own settings;
//!   - a median near a frame and a half: a commit is waiting for a flip that
//!     is already in the air, and the compositor's scheduling is where the
//!     work goes.
//!
//! Run it with the tube already up:
//!
//!     flyback run &
//!     cargo run --release --features latency --bin latency -- 500

use std::collections::HashMap;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::time::Duration;

use wayland_client::backend::ObjectId;
use wayland_client::protocol::{
    wl_buffer, wl_callback, wl_compositor, wl_registry, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::presentation_time::client::{wp_presentation, wp_presentation_feedback};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

/// The default number of commits to time. Five hundred takes about twenty
/// seconds and is enough for a 5th and 95th percentile to stop moving.
const SAMPLES: usize = 500;

/// Buffers in the ring. The surface is two pixels of flat colour stretched
/// over the whole screen by the viewporter, so a frame costs the client
/// nothing and what is left in the measurement is the compositor's.
const BUFFERS: usize = 4;

/// CLOCK_MONOTONIC, the clock `wp_presentation` advertises.
fn now() -> Duration {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: the kernel writes the two fields of a struct we own.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}

/// Enough randomness to scatter a commit inside a frame. A cryptographic
/// generator would be the same numbers for this purpose and another
/// dependency.
struct Rng(u64);

impl Rng {
    fn new() -> Rng {
        Rng(now().as_nanos() as u64 | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A number in `0..n`.
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }
}

/// One timed frame.
struct Sample {
    /// Commit to the start of scanout.
    latency: Duration,
    /// What the compositor said about the timestamp. Without
    /// `HwClock | HwCompletion` the number is an estimate and says so.
    flags: u32,
}

struct Probe {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    presentation: Option<wp_presentation::WpPresentation>,
    viewporter: Option<wp_viewporter::WpViewporter>,
    surface: Option<wl_surface::WlSurface>,
    viewport: Option<wp_viewport::WpViewport>,
    buffers: Vec<wl_buffer::WlBuffer>,
    size: (i32, i32),
    configured: bool,
    /// Commits that have been timed but not yet answered, by the feedback
    /// object that will answer them.
    inflight: HashMap<ObjectId, Duration>,
    samples: Vec<Sample>,
    /// The refresh the compositor reports with each frame, in nanoseconds.
    refresh: u64,
    /// A feedback the compositor threw away rather than answered: the frame
    /// was superseded before it reached the screen.
    discarded: usize,
    /// Frame callbacks answered, counted by the check below.
    callbacks: usize,
    closed: bool,
}

impl Probe {
    /// A pool of `BUFFERS` two-pixel buffers, alternating black and white, so
    /// consecutive frames differ on the glass as well as in the protocol.
    fn make_buffers(&mut self, qh: &QueueHandle<Probe>) -> Result<(), String> {
        let shm = self.shm.as_ref().ok_or("the compositor offers no wl_shm")?;
        let stride = 4;
        let len = stride * BUFFERS;
        // SAFETY: a fresh anonymous file, sized and mapped before use.
        let fd = unsafe { libc::memfd_create(c"omacrt-latency".as_ptr(), libc::MFD_CLOEXEC) };
        if fd < 0 {
            return Err("memfd_create".into());
        }
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };
        if unsafe { libc::ftruncate(fd, len as libc::off_t) } < 0 {
            return Err("ftruncate".into());
        }
        let map = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        if map == libc::MAP_FAILED {
            return Err("mmap".into());
        }
        let px = unsafe { std::slice::from_raw_parts_mut(map as *mut u32, BUFFERS) };
        for (i, p) in px.iter_mut().enumerate() {
            *p = if i.is_multiple_of(2) {
                0xff00_0000
            } else {
                0xffff_ffff
            };
        }
        let pool = shm.create_pool(owned.as_fd(), len as i32, qh, ());
        for i in 0..BUFFERS {
            self.buffers.push(pool.create_buffer(
                (i * stride) as i32,
                1,
                1,
                stride as i32,
                wl_shm::Format::Xrgb8888,
                qh,
                (),
            ));
        }
        pool.destroy();
        Ok(())
    }

    /// Commit one frame and start its clock. With `callback`, ask for a
    /// frame callback in the same commit, which is how a real client paces
    /// itself.
    fn commit(&mut self, qh: &QueueHandle<Probe>, i: usize, callback: bool) {
        let (Some(surface), Some(pres)) = (&self.surface, &self.presentation) else {
            return;
        };
        if callback {
            surface.frame(qh, ());
        }
        surface.attach(Some(&self.buffers[i % BUFFERS]), 0, 0);
        surface.damage_buffer(0, 0, 1, 1);
        // The clock starts as late as it can: everything after this line is
        // the compositor's, the kernel's and the television's.
        let t = now();
        let feedback = pres.feedback(surface, qh, ());
        surface.commit();
        self.inflight.insert(feedback.id(), t);
    }
}

/// Wait for `d`, reading and answering the compositor the whole time.
///
/// Sleeping instead is what a first attempt does, and it fails at about two
/// hundred frames: the events the compositor sends back fill its side of the
/// socket, it cannot write, and it drops the connection. The wait is also
/// where the random phase comes from, so it is timed in nanoseconds rather
/// than the milliseconds `poll` takes.
fn pump(
    conn: &Connection,
    queue: &mut wayland_client::EventQueue<Probe>,
    probe: &mut Probe,
    d: Duration,
) -> Result<(), String> {
    pump_until(conn, queue, probe, d, |_| false)
}

/// The same, but stopping the moment `stop` is true: waiting out the rest of
/// a timeout after the compositor has already answered would be counted as
/// the client's own drawing time and would move the deadline for nothing.
fn pump_until(
    conn: &Connection,
    queue: &mut wayland_client::EventQueue<Probe>,
    probe: &mut Probe,
    d: Duration,
    stop: impl Fn(&Probe) -> bool,
) -> Result<(), String> {
    let until = std::time::Instant::now() + d;
    loop {
        queue
            .dispatch_pending(probe)
            .map_err(|e| format!("dispatch: {e}"))?;
        if stop(probe) {
            return Ok(());
        }
        let now = std::time::Instant::now();
        if now >= until {
            return Ok(());
        }
        let left = until - now;
        let Some(guard) = conn.prepare_read() else {
            continue;
        };
        conn.flush().map_err(|e| format!("flush: {e}"))?;
        let ts = libc::timespec {
            tv_sec: left.as_secs() as libc::time_t,
            tv_nsec: left.subsec_nanos() as i64,
        };
        let mut pfd = libc::pollfd {
            fd: conn.as_fd().as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one descriptor we own and a timeout we own.
        let ready = unsafe { libc::ppoll(&mut pfd, 1, &ts, std::ptr::null()) };
        if ready > 0 {
            guard.read().map_err(|e| format!("read: {e}"))?;
        }
    }
}

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let i = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[i]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!(
            "usage: latency [frames] [--display NAME] [--paced [--draw MS]]\n\n  \
             by default a frame is committed at a random point of every frame,\n  \
             which measures the whole window a client could commit in.\n  \
             --paced instead draws on the frame callback like a real client,\n  \
             taking MS milliseconds over it, which is the number a game sees."
        );
        return;
    }
    let n: usize = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .and_then(|s| s.parse().ok())
        .unwrap_or(SAMPLES);
    // The tube, not the desktop. This runs from a terminal on the desktop
    // session, where WAYLAND_DISPLAY names the desktop compositor, and
    // measuring that instead would be a number about Hyprland.
    let display = args
        .iter()
        .position(|a| a == "--display")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "wayland-crt".into());
    // SAFETY: single threaded, before the connection reads the environment.
    unsafe { std::env::set_var("WAYLAND_DISPLAY", &display) };
    let paced = args.iter().any(|a| a == "--paced");
    let draw = args
        .iter()
        .position(|a| a == "--draw")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(1);
    if let Err(e) = run(n, paced, Duration::from_millis(draw)) {
        eprintln!("latency: {e}");
        std::process::exit(1);
    }
}

fn run(n: usize, paced: bool, draw: Duration) -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|e| {
        format!(
            "no compositor on {:?}: {e}",
            std::env::var("WAYLAND_DISPLAY")
        )
    })?;
    let mut queue = conn.new_event_queue::<Probe>();
    let qh = queue.handle();
    let _registry = conn.display().get_registry(&qh, ());

    let mut probe = Probe {
        compositor: None,
        shm: None,
        wm_base: None,
        presentation: None,
        viewporter: None,
        surface: None,
        viewport: None,
        buffers: Vec::new(),
        size: (0, 0),
        configured: false,
        inflight: HashMap::new(),
        samples: Vec::new(),
        refresh: 16_666_666,
        discarded: 0,
        callbacks: 0,
        closed: false,
    };
    queue
        .roundtrip(&mut probe)
        .map_err(|e| format!("registry: {e}"))?;

    let comp = probe
        .compositor
        .clone()
        .ok_or("the compositor offers no wl_compositor")?;
    let wm = probe
        .wm_base
        .clone()
        .ok_or("the compositor offers no xdg_wm_base")?;
    probe
        .presentation
        .as_ref()
        .ok_or("the compositor offers no wp_presentation: nothing can be timed")?;
    let vp = probe
        .viewporter
        .clone()
        .ok_or("the compositor offers no wp_viewporter")?;

    let surface = comp.create_surface(&qh, ());
    let xdg = wm.get_xdg_surface(&surface, &qh, ());
    let top = xdg.get_toplevel(&qh, ());
    top.set_app_id("omacrt-latency".into());
    top.set_title("latency".into());
    surface.commit();
    probe.surface = Some(surface);
    probe.viewport = Some(vp.get_viewport(probe.surface.as_ref().unwrap(), &qh, ()));

    // Wait for the size the compositor wants, so the one pixel can be
    // stretched over exactly the picture.
    for _ in 0..100 {
        queue
            .roundtrip(&mut probe)
            .map_err(|e| format!("configure: {e}"))?;
        if probe.configured && probe.size.0 > 0 {
            break;
        }
    }
    if !probe.configured {
        return Err("the compositor never configured the window".into());
    }
    probe.make_buffers(&qh)?;
    if let (Some(v), Some(s)) = (&probe.viewport, &probe.surface) {
        v.set_destination(probe.size.0, probe.size.1);
        s.attach(Some(&probe.buffers[0]), 0, 0);
        s.damage_buffer(0, 0, 1, 1);
        s.commit();
    }
    queue
        .roundtrip(&mut probe)
        .map_err(|e| format!("first frame: {e}"))?;
    println!(
        "timing {n} frames on a {}x{} surface, {}",
        probe.size.0,
        probe.size.1,
        if paced {
            format!(
                "drawing on the frame callback, {} ms a frame",
                draw.as_millis()
            )
        } else {
            "committing at a random point of the frame".into()
        }
    );

    if paced {
        // The loop below waits for the callback the previous commit asked
        // for; the first one has to come from somewhere.
        probe.commit(&qh, 0, true);
        conn.flush().map_err(|e| format!("flush: {e}"))?;
        probe.samples.clear();
        probe.inflight.clear();
        probe.discarded = 0;
    }
    let mut rng = Rng::new();
    let frame = probe.refresh;
    for i in 0..n {
        if paced {
            // What a game does: it is told it may draw, it draws, it commits.
            // Everything after the commit is the compositor's.
            let seen = probe.callbacks;
            pump_until(
                &conn,
                &mut queue,
                &mut probe,
                Duration::from_millis(200),
                |p| p.callbacks > seen,
            )?;
            pump(&conn, &mut queue, &mut probe, draw)?;
        } else {
            // One frame, plus a random fraction of another: the commit lands
            // at a different point of every window and the samples cover it.
            pump(
                &conn,
                &mut queue,
                &mut probe,
                Duration::from_nanos(frame + rng.below(frame)),
            )?;
        }
        probe.commit(&qh, i, paced);
        conn.flush().map_err(|e| format!("flush: {e}"))?;
        if probe.closed {
            return Err("the compositor closed the window".into());
        }
    }
    // Let the last frames land.
    for _ in 0..10 {
        if probe.inflight.is_empty() {
            break;
        }
        pump(&conn, &mut queue, &mut probe, Duration::from_millis(20))?;
    }
    report(&probe, paced);
    pacer(&conn, &mut queue, &mut probe, &qh)?;
    Ok(())
}

fn report(probe: &Probe, paced: bool) {
    let mut us: Vec<u64> = probe
        .samples
        .iter()
        .map(|s| s.latency.as_nanos() as u64 / 1000)
        .collect();
    if us.is_empty() {
        println!("no frame was ever answered: the compositor kept nothing");
        return;
    }
    us.sort_unstable();
    let frame_us = probe.refresh / 1000;
    let mean = us.iter().sum::<u64>() / us.len() as u64;
    // Whether the numbers are the display hardware's or an estimate.
    // wp_presentation_feedback: 1 = vsync, 2 = hw clock, 4 = hw completion.
    let hw = probe
        .samples
        .iter()
        .filter(|s| s.flags & 0b110 == 0b110)
        .count();
    let row = |name: &str, v: u64| {
        println!(
            "  {name:<10} {:>7.2} ms   {:>5.2} frames",
            v as f64 / 1000.0,
            v as f64 / frame_us as f64
        );
    };
    println!(
        "\n{} frames answered, {} discarded, one frame is {:.3} ms",
        us.len(),
        probe.discarded,
        frame_us as f64 / 1000.0
    );
    println!("commit to the start of scanout:");
    row("best", us[0]);
    row("5%", percentile(&us, 0.05));
    row("median", percentile(&us, 0.50));
    row("mean", mean);
    row("95%", percentile(&us, 0.95));
    row("worst", us[us.len() - 1]);
    // A frame that took half a frame longer than the quickest twentieth
    // slipped a vblank: it was shown one flip later than its neighbours,
    // which on a television is judder rather than latency. It is counted
    // rather than averaged away, because an average hides exactly this.
    //
    // Only when the frames were paced. Committing at a random point of the
    // frame is meant to spread the samples over a whole frame, so counting
    // the spread as judder there would flag half of them.
    if paced {
        let late = percentile(&us, 0.05) + frame_us / 2;
        let slipped = us.iter().filter(|&&v| v > late).count();
        println!(
            "\n{slipped} of {} frames slipped a vblank ({:.1}%)",
            us.len(),
            slipped as f64 * 100.0 / us.len() as f64
        );
    }
    println!(
        "\n{hw} of {} timestamps came from the display hardware",
        us.len()
    );
    // The floor a client cannot do anything about: a commit at a uniformly
    // random phase waits on average half a frame for the next vblank. What
    // the median has on top of that is the compositor's own. It says nothing
    // about a paced run, where the phase is the compositor's choice.
    if !paced {
        let over = percentile(&us, 0.50) as f64 - frame_us as f64 / 2.0;
        println!(
            "the median is {:+.2} ms either side of half a frame, which is what a \
             commit at a random phase\nhas to wait for the next vblank no matter \
             who is compositing",
            over / 1000.0
        );
    }
}

/// A client that asks for a frame callback and then commits nothing new must
/// still be told it may draw again.
///
/// This is not a detail: the compositor stops flipping when nothing has
/// changed, which is where most of the latency above went, and the vblank is
/// what used to carry the frame callbacks. A tube that answers no callbacks
/// is a tube where every client stops for ever, so it is checked here rather
/// than discovered later.
fn pacer(
    conn: &Connection,
    queue: &mut wayland_client::EventQueue<Probe>,
    probe: &mut Probe,
    qh: &QueueHandle<Probe>,
) -> Result<(), String> {
    let Some(surface) = probe.surface.clone() else {
        return Ok(());
    };
    probe.callbacks = 0;
    let rounds = 5;
    for _ in 0..rounds {
        surface.frame(qh, ());
        // No attach and no damage: nothing for the compositor to show.
        surface.commit();
        conn.flush().map_err(|e| format!("flush: {e}"))?;
        let was = probe.callbacks;
        for _ in 0..25 {
            pump(conn, queue, probe, Duration::from_millis(20))?;
            if probe.callbacks > was {
                break;
            }
        }
    }
    if probe.callbacks == rounds {
        println!("\nan idle tube still answers frame callbacks: {rounds} of {rounds}");
    } else {
        println!(
            "\nAN IDLE TUBE STOPS ANSWERING FRAME CALLBACKS: {} of {rounds}. Every client on \nit will freeze.",
            probe.callbacks
        );
    }
    Ok(())
}

impl Dispatch<wl_registry::WlRegistry, ()> for Probe {
    fn event(
        st: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };
        match interface.as_str() {
            "wl_compositor" => {
                st.compositor = Some(registry.bind(name, version.min(4), qh, ()));
            }
            "wl_shm" => st.shm = Some(registry.bind(name, 1, qh, ())),
            "xdg_wm_base" => st.wm_base = Some(registry.bind(name, version.min(3), qh, ())),
            "wp_presentation" => st.presentation = Some(registry.bind(name, 1, qh, ())),
            "wp_viewporter" => st.viewporter = Some(registry.bind(name, 1, qh, ())),
            _ => {}
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for Probe {
    fn event(
        _: &mut Self,
        wm: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for Probe {
    fn event(
        st: &mut Self,
        xdg: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            xdg.ack_configure(serial);
            st.configured = true;
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for Probe {
    fn event(
        st: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            xdg_toplevel::Event::Configure { width, height, .. } => {
                if width > 0 && height > 0 {
                    st.size = (width, height);
                }
            }
            xdg_toplevel::Event::Close => st.closed = true,
            _ => {}
        }
    }
}

impl Dispatch<wp_presentation::WpPresentation, ()> for Probe {
    fn event(
        _: &mut Self,
        _: &wp_presentation::WpPresentation,
        _: wp_presentation::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wp_presentation_feedback::WpPresentationFeedback, ()> for Probe {
    fn event(
        st: &mut Self,
        obj: &wp_presentation_feedback::WpPresentationFeedback,
        event: wp_presentation_feedback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wp_presentation_feedback::Event::Presented {
                tv_sec_hi,
                tv_sec_lo,
                tv_nsec,
                refresh,
                flags,
                ..
            } => {
                let Some(commit) = st.inflight.remove(&obj.id()) else {
                    return;
                };
                let secs = ((tv_sec_hi as u64) << 32) | tv_sec_lo as u64;
                let shown = Duration::new(secs, tv_nsec);
                if refresh > 0 {
                    st.refresh = refresh as u64;
                }
                // A presentation earlier than its own commit is a clock that
                // is not the one we were promised; drop it rather than
                // report a negative latency as a very large positive one.
                if let Some(latency) = shown.checked_sub(commit) {
                    st.samples.push(Sample {
                        latency,
                        flags: flags.into(),
                    });
                }
            }
            wp_presentation_feedback::Event::Discarded => {
                st.inflight.remove(&obj.id());
                st.discarded += 1;
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for Probe {
    fn event(
        st: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            st.callbacks += 1;
        }
    }
}

macro_rules! ignore {
    ($($t:ty),*) => {$(
        impl Dispatch<$t, ()> for Probe {
            fn event(
                _: &mut Self,
                _: &$t,
                _: <$t as Proxy>::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
            }
        }
    )*};
}

ignore!(
    wl_compositor::WlCompositor,
    wl_surface::WlSurface,
    wl_shm::WlShm,
    wl_shm_pool::WlShmPool,
    wl_buffer::WlBuffer,
    wp_viewporter::WpViewporter,
    wp_viewport::WpViewport
);
