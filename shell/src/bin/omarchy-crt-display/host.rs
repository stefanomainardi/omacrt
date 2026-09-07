//! The desktop side of the tube: an ordinary window on the desktop
//! compositor that shows a live preview of what the tube displays and, while
//! it has keyboard focus, forwards every key to the program on the tube.
//!
//! Opened with `omarchy-crt monitor on`. The connection is a second Wayland
//! client connection to the desktop (the first one holds the lease); its
//! events are dispatched on the compositor's own event loop.

use crate::comp::Crt;
use smithay::backend::input::KeyState;
use smithay::input::keyboard::Keycode;

/// F1 in the Wayland numbering: evdev 59 plus the eight of the X11 offset.
const F1: u32 = 67;
use smithay::reexports::calloop::LoopHandle;
use wayland_client::protocol::{
    wl_buffer, wl_callback, wl_compositor, wl_keyboard, wl_registry, wl_seat, wl_shm, wl_shm_pool,
    wl_surface,
};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

pub const APP_ID: &str = "omarchy-crt-monitor";

pub struct Host {
    pub conn: Connection,
    pub qh: QueueHandle<Crt>,
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    seat: Option<wl_seat::WlSeat>,
    surface: Option<wl_surface::WlSurface>,
    xdg_surface: Option<xdg_surface::XdgSurface>,
    toplevel: Option<xdg_toplevel::XdgToplevel>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    /// Size the desktop compositor gave us; 0 until the first configure.
    size: (i32, i32),
    configured: bool,
    pub focused: bool,
    pool: Option<Pool>,
    buffer: Option<wl_buffer::WlBuffer>,
    buffer_busy: bool,
    frame_pending: bool,
    pub closed: bool,
}

struct Pool {
    pool: wl_shm_pool::WlShmPool,
    map: *mut u8,
    len: usize,
    size: (i32, i32),
}

impl Drop for Pool {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.map as *mut libc::c_void, self.len) };
        self.pool.destroy();
    }
}

impl Host {
    /// Connect to the desktop and start binding; the window appears once the
    /// globals have arrived (event driven, on the compositor's loop).
    pub fn open(handle: &LoopHandle<'static, Crt>) -> Result<Host, String> {
        let conn = Connection::connect_to_env().map_err(|e| format!("desktop wayland: {e}"))?;
        let queue = conn.new_event_queue::<Crt>();
        let qh = queue.handle();
        let display = conn.display();
        let _registry = display.get_registry(&qh, ());
        calloop_wayland_source::WaylandSource::new(conn.clone(), queue)
            .insert(handle.clone())
            .map_err(|e| format!("desktop source: {e}"))?;
        Ok(Host {
            conn,
            qh,
            compositor: None,
            shm: None,
            wm_base: None,
            seat: None,
            surface: None,
            xdg_surface: None,
            toplevel: None,
            keyboard: None,
            size: (0, 0),
            configured: false,
            focused: false,
            pool: None,
            buffer: None,
            buffer_busy: false,
            frame_pending: false,
            closed: false,
        })
    }

    fn try_create_window(&mut self) {
        if self.surface.is_some() {
            return;
        }
        let (Some(comp), Some(wm)) = (&self.compositor, &self.wm_base) else {
            return;
        };
        let surface = comp.create_surface(&self.qh, ());
        let xdg = wm.get_xdg_surface(&surface, &self.qh, ());
        let top = xdg.get_toplevel(&self.qh, ());
        top.set_app_id(APP_ID.into());
        top.set_title("Omarchy CRT".into());
        top.set_min_size(320, 240);
        surface.commit();
        self.surface = Some(surface);
        self.xdg_surface = Some(xdg);
        self.toplevel = Some(top);
        let _ = self.conn.flush();
    }

    /// True when a new preview frame can be pushed.
    pub fn ready(&self) -> bool {
        self.configured
            && self.surface.is_some()
            && !self.frame_pending
            && !self.buffer_busy
            && self.size.0 > 0
    }

    /// Push a frame: `bgra` is the tube's picture, top-down, `w` x `h`; it is
    /// scaled (nearest) into the window.
    pub fn present(&mut self, bgra: &[u8], w: usize, h: usize) {
        let (Some(shm), Some(surface)) = (&self.shm, &self.surface) else {
            return;
        };
        let (cw, ch) = self.size;
        if cw <= 0 || ch <= 0 {
            return;
        }
        let stride = cw * 4;
        let len = (stride * ch) as usize;
        let need_new = self
            .pool
            .as_ref()
            .map(|p| p.size != (cw, ch))
            .unwrap_or(true);
        if need_new {
            if let Some(b) = self.buffer.take() {
                b.destroy();
            }
            self.pool = None;
            let fd =
                unsafe { libc::memfd_create(c"omarchy-crt-monitor".as_ptr(), libc::MFD_CLOEXEC) };
            if fd < 0 {
                return;
            }
            if unsafe { libc::ftruncate(fd, len as libc::off_t) } < 0 {
                unsafe { libc::close(fd) };
                return;
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
                unsafe { libc::close(fd) };
                return;
            }
            let owned = unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) };
            let pool = shm.create_pool(std::os::fd::AsFd::as_fd(&owned), len as i32, &self.qh, ());
            let buffer =
                pool.create_buffer(0, cw, ch, stride, wl_shm::Format::Xrgb8888, &self.qh, ());
            self.pool = Some(Pool {
                pool,
                map: map as *mut u8,
                len,
                size: (cw, ch),
            });
            self.buffer = Some(buffer);
        }
        let Some(pool) = &self.pool else { return };
        let dst = unsafe { std::slice::from_raw_parts_mut(pool.map, pool.len) };
        // Nearest neighbour into a 4:3 letterbox of the window.
        let (cw, ch) = (cw as usize, ch as usize);
        let (pw, ph) = if cw * 3 >= ch * 4 {
            (ch * 4 / 3, ch)
        } else {
            (cw, cw * 3 / 4)
        };
        let (ox, oy) = ((cw - pw) / 2, (ch - ph) / 2);
        for y in 0..ch {
            for x in 0..cw {
                let d = (y * cw + x) * 4;
                if x < ox || y < oy || x >= ox + pw || y >= oy + ph {
                    dst[d..d + 4].copy_from_slice(&[0x14, 0x0d, 0x0b, 0xff]);
                    continue;
                }
                let sx = (x - ox) * w / pw;
                let sy = (y - oy) * h / ph;
                let s = (sy * w + sx) * 4;
                dst[d..d + 4].copy_from_slice(&bgra[s..s + 4]);
            }
        }
        if let Some(b) = &self.buffer {
            surface.attach(Some(b), 0, 0);
            surface.damage_buffer(0, 0, cw as i32, ch as i32);
            surface.frame(&self.qh, ());
            surface.commit();
            self.buffer_busy = true;
            self.frame_pending = true;
            let _ = self.conn.flush();
        }
    }

    pub fn close(&mut self) {
        if let Some(k) = self.keyboard.take() {
            k.release();
        }
        if let Some(t) = self.toplevel.take() {
            t.destroy();
        }
        if let Some(x) = self.xdg_surface.take() {
            x.destroy();
        }
        if let Some(s) = self.surface.take() {
            s.destroy();
        }
        if let Some(b) = self.buffer.take() {
            b.destroy();
        }
        self.pool = None;
        self.closed = true;
        let _ = self.conn.flush();
    }
}

use std::os::fd::FromRawFd;

impl Dispatch<wl_registry::WlRegistry, ()> for Crt {
    fn event(
        st: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let Some(host) = st.host.as_mut() else { return };
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => {
                    host.compositor = Some(registry.bind(name, version.min(4), qh, ()))
                }
                "wl_shm" => host.shm = Some(registry.bind(name, 1, qh, ())),
                "xdg_wm_base" => host.wm_base = Some(registry.bind(name, version.min(2), qh, ())),
                "wl_seat" => host.seat = Some(registry.bind(name, version.min(7), qh, ())),
                _ => {}
            }
            host.try_create_window();
        }
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for Crt {
    fn event(
        _: &mut Self,
        _: &wl_compositor::WlCompositor,
        _: wl_compositor::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_shm::WlShm, ()> for Crt {
    fn event(
        _: &mut Self,
        _: &wl_shm::WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_shm_pool::WlShmPool, ()> for Crt {
    fn event(
        _: &mut Self,
        _: &wl_shm_pool::WlShmPool,
        _: wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_surface::WlSurface, ()> for Crt {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_buffer::WlBuffer, ()> for Crt {
    fn event(
        st: &mut Self,
        _: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let (wl_buffer::Event::Release, Some(host)) = (event, st.host.as_mut()) {
            host.buffer_busy = false;
        }
    }
}
impl Dispatch<wl_callback::WlCallback, ()> for Crt {
    fn event(
        st: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let (wl_callback::Event::Done { .. }, Some(host)) = (event, st.host.as_mut()) {
            host.frame_pending = false;
        }
    }
}
impl Dispatch<xdg_wm_base::XdgWmBase, ()> for Crt {
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
impl Dispatch<xdg_surface::XdgSurface, ()> for Crt {
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
            if let Some(host) = st.host.as_mut() {
                if host.size.0 <= 0 {
                    host.size = (800, 600);
                }
                host.configured = true;
                host.buffer_busy = false;
                host.frame_pending = false;
            }
            st.preview();
        }
    }
}
impl Dispatch<xdg_toplevel::XdgToplevel, ()> for Crt {
    fn event(
        st: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(host) = st.host.as_mut() else { return };
        match event {
            xdg_toplevel::Event::Configure { width, height, .. } => {
                if width > 0 && height > 0 {
                    host.size = (width, height);
                }
            }
            xdg_toplevel::Event::Close => host.close(),
            _ => {}
        }
    }
}
impl Dispatch<wl_seat::WlSeat, ()> for Crt {
    fn event(
        st: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let Some(host) = st.host.as_mut() else { return };
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(caps),
        } = event
            && caps.contains(wl_seat::Capability::Keyboard)
            && host.keyboard.is_none()
        {
            host.keyboard = Some(seat.get_keyboard(qh, ()));
        }
    }
}
impl Dispatch<wl_keyboard::WlKeyboard, ()> for Crt {
    fn event(
        st: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Enter { .. } => {
                if let Some(h) = st.host.as_mut() {
                    h.focused = true;
                }
                st.host_title();
            }
            wl_keyboard::Event::Leave { .. } => {
                if let Some(h) = st.host.as_mut() {
                    h.focused = false;
                }
                st.host_title();
            }
            wl_keyboard::Event::Key {
                key,
                state: WEnum::Value(state),
                ..
            } => {
                let pressed = matches!(state, wl_keyboard::KeyState::Pressed);
                st.forward_key(
                    Keycode::new(key + 8),
                    if pressed {
                        KeyState::Pressed
                    } else {
                        KeyState::Released
                    },
                );
            }
            _ => {}
        }
    }
}

impl Crt {
    /// Window title reflects whether typing reaches the tube.
    pub fn host_title(&mut self) {
        let Some(host) = self.host.as_ref() else {
            return;
        };
        if let Some(t) = &host.toplevel {
            t.set_title(if host.focused {
                "Omarchy CRT · keyboard on the tube".into()
            } else {
                "Omarchy CRT".into()
            });
            let _ = host.conn.flush();
        }
    }

    /// Keys typed into the desktop window go to the program on the tube.
    ///
    /// No check on our own `focused` flag: the desktop only delivers key
    /// events to a window that holds its keyboard, and that flag has been
    /// seen to stay false when the enter event was missed, which swallowed
    /// every key typed in the window (Escape over a video, for one).
    fn forward_key(&mut self, code: Keycode, state: KeyState) {
        // F1 belongs to the launcher, not to the program on the tube: while a
        // game runs the keyboard goes to RetroArch, so a launcher shortcut
        // would never arrive. It is turned into the launcher's own `menu`
        // input, which is what the pad's Select + Start sends.
        if code.raw() == F1 {
            if matches!(state, KeyState::Pressed) {
                let _ = omarchy_crt_shell::crt::control::send(&["menu"]);
            }
            return;
        }
        self.focus_top();
        self.key_event(code, state);
    }
}

// Unused Proxy import guard.
#[allow(dead_code)]
fn _proxy_marker<P: Proxy>(_: &P) {}
