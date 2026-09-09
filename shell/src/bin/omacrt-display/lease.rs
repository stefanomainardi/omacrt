//! Take a DRM lease of one connector from the desktop compositor.
//!
//! Hyprland offers non-desktop connectors through wp_drm_lease_v1. The
//! lease hands back a DRM file descriptor that is master for that connector,
//! its CRTC and planes. The Wayland connection must stay alive for as long
//! as the lease is used; `Lease::pump` services it.

use std::os::fd::OwnedFd;
use wayland_client::protocol::{wl_display, wl_registry};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, event_created_child};
use wayland_protocols::wp::drm_lease::v1::client::{
    wp_drm_lease_connector_v1 as lconn, wp_drm_lease_device_v1 as ldev,
    wp_drm_lease_request_v1 as lreq, wp_drm_lease_v1 as lease,
};

#[derive(Default)]
pub struct State {
    pub devices: Vec<ldev::WpDrmLeaseDeviceV1>,
    pub devices_done: usize,
    /// (connector object, owning device index, name, DRM id)
    pub connectors: Vec<(lconn::WpDrmLeaseConnectorV1, usize, String, u32)>,
    pub lease_fd: Option<OwnedFd>,
    pub lease_failed: bool,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        st: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = event
            && interface == "wp_drm_lease_device_v1"
        {
            let dev =
                registry.bind::<ldev::WpDrmLeaseDeviceV1, _, _>(name, 1, qh, st.devices.len());
            st.devices.push(dev);
        }
    }
}

impl Dispatch<ldev::WpDrmLeaseDeviceV1, usize> for State {
    fn event(
        st: &mut Self,
        _: &ldev::WpDrmLeaseDeviceV1,
        event: ldev::Event,
        idx: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ldev::Event::Connector { id } => st.connectors.push((id, *idx, String::new(), 0)),
            ldev::Event::Done => st.devices_done += 1,
            _ => {}
        }
    }
    event_created_child!(State, ldev::WpDrmLeaseDeviceV1, [
        ldev::EVT_CONNECTOR_OPCODE => (lconn::WpDrmLeaseConnectorV1, ()),
    ]);
}

impl Dispatch<lconn::WpDrmLeaseConnectorV1, ()> for State {
    fn event(
        st: &mut Self,
        c: &lconn::WpDrmLeaseConnectorV1,
        event: lconn::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(entry) = st.connectors.iter_mut().find(|(o, ..)| o == c) else {
            return;
        };
        match event {
            lconn::Event::Name { name } => entry.2 = name,
            lconn::Event::ConnectorId { connector_id } => entry.3 = connector_id,
            _ => {}
        }
    }
}

impl Dispatch<lreq::WpDrmLeaseRequestV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &lreq::WpDrmLeaseRequestV1,
        _: lreq::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<lease::WpDrmLeaseV1, ()> for State {
    fn event(
        st: &mut Self,
        _: &lease::WpDrmLeaseV1,
        event: lease::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            lease::Event::LeaseFd { leased_fd } => st.lease_fd = Some(leased_fd),
            lease::Event::Finished => st.lease_failed = true,
            _ => {}
        }
    }
}

impl Dispatch<wl_display::WlDisplay, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_display::WlDisplay,
        _: wl_display::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// A live lease: the file descriptor plus the connection that keeps it.
pub struct Lease {
    pub fd: Option<OwnedFd>,
    pub connector_id: u32,
    pub name: String,
    conn: Connection,
    queue: EventQueue<State>,
    state: State,
    _lease: lease::WpDrmLeaseV1,
}

impl Lease {
    /// Ask the compositor for `want` (a connector name such as `HDMI-A-1`).
    pub fn take(want: &str) -> Result<Lease, String> {
        let conn = Connection::connect_to_env().map_err(|e| format!("wayland: {e}"))?;
        let mut queue = conn.new_event_queue::<State>();
        let qh = queue.handle();
        let display = conn.display();
        let _registry = display.get_registry(&qh, ());
        let mut st = State::default();
        for _ in 0..6 {
            queue
                .roundtrip(&mut st)
                .map_err(|e| format!("roundtrip: {e}"))?;
            if !st.devices.is_empty() && st.devices_done >= st.devices.len() {
                break;
            }
        }
        let offered: Vec<String> = st.connectors.iter().map(|(_, _, n, _)| n.clone()).collect();
        let Some(target) = st.connectors.iter().find(|(_, _, n, _)| n == want).cloned() else {
            let offered = if offered.is_empty() {
                "none".to_string()
            } else {
                offered.join(" ")
            };
            // Two different failures used to read the same. Asking "is it
            // marked non-desktop?" is misleading when it is: the kernel has
            // the flag, and the compositor simply started before it did.
            // Hyprland decides which connectors it offers when it starts and
            // does not read the property again, not even after a real unplug
            // and plug, so there is nothing to do here but say so.
            return Err(if omacrt_shell::crt::display::leaseable(want) {
                format!(
                    "connector {want} is marked non-desktop but the compositor is not offering it (offered: {offered}). \
                     Almost always a monitor rule claiming it: a connector Hyprland has an hl.monitor for is a monitor to it, \
                     and a monitor is never offered for leasing. Look in ~/.config/hypr for a rule matching this output - it \
                     may match on desc: rather than on the name - remove it and reboot."
                )
            } else {
                format!(
                    "connector {want} is not offered for lease (offered: {offered}) and the kernel does not mark it non-desktop. \
                     Install the boot time override with `sudo bin/omacrt-install --system` and reboot."
                )
            });
        };
        let dev = st.devices[target.1].clone();
        let request = dev.create_lease_request(&qh, ());
        request.request_connector(&target.0);
        let lease_obj = request.submit(&qh, ());
        for _ in 0..10 {
            queue
                .roundtrip(&mut st)
                .map_err(|e| format!("roundtrip: {e}"))?;
            if st.lease_fd.is_some() || st.lease_failed {
                break;
            }
        }
        let fd = st
            .lease_fd
            .take()
            .ok_or("the compositor refused the lease")?;
        Ok(Lease {
            fd: Some(fd),
            connector_id: target.3,
            name: target.2.clone(),
            conn,
            queue,
            state: st,
            _lease: lease_obj,
        })
    }

    /// Service the compositor connection without blocking. Returns false once
    /// the lease is gone, which is either a polite `Finished` from the
    /// compositor or the connection to it breaking: a compositor that dies
    /// sends no event at all, so an error here has to count as revocation.
    /// Reporting it is what lets the caller shut down and the watchdog put
    /// the television back, instead of a live process holding a dead lease.
    pub fn pump(&mut self) -> bool {
        if let Err(e) = self.conn.flush()
            && fatal(&e)
        {
            eprintln!("lease: flush: {e}");
            self.state.lease_failed = true;
        }
        if let Some(guard) = self.conn.prepare_read()
            && let Err(e) = guard.read()
            && fatal(&e)
        {
            eprintln!("lease: read: {e}");
            self.state.lease_failed = true;
        }
        if let Err(e) = self.queue.dispatch_pending(&mut self.state) {
            eprintln!("lease: dispatch: {e}");
            self.state.lease_failed = true;
        }
        !self.state.lease_failed
    }
}

/// A read or a flush that would block is the normal state of a socket with
/// nothing on it. Anything else has ended the connection.
fn fatal(e: &wayland_client::backend::WaylandError) -> bool {
    !matches!(e, wayland_client::backend::WaylandError::Io(io)
        if io.kind() == std::io::ErrorKind::WouldBlock)
}
