//! Drive the tube directly: lease its DRM connector from the desktop
//! compositor and set the mode ourselves.
//!
//! With the connector marked non-desktop (see scripts/crt-lease-setup.sh),
//! Hyprland stops configuring it and offers it through wp_drm_lease_v1. The
//! lease hands us a DRM file descriptor that is master for that connector and
//! its CRTC: any modeline, interlace included, no compositor layers, no
//! pointer, no window rules. This is the probe stage: take the lease, set the
//! configured timing and show a test card for a while.
//!
//!   omarchy-crt-display probe [connector] [seconds]

use drm::Device;
use drm::control::{Device as ControlDevice, Mode, framebuffer};
use omarchy_crt_shell::crt::Config;
use omarchy_crt_shell::crt::output::Modeline;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use wayland_client::protocol::{wl_display, wl_registry};
use wayland_client::{Connection, Dispatch, QueueHandle, event_created_child};
use wayland_protocols::wp::drm_lease::v1::client::{
    wp_drm_lease_connector_v1 as lconn, wp_drm_lease_device_v1 as ldev,
    wp_drm_lease_request_v1 as lreq, wp_drm_lease_v1 as lease,
};

struct Leased(OwnedFd);
impl AsFd for Leased {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}
impl Device for Leased {}
impl ControlDevice for Leased {}

#[derive(Default)]
struct State {
    devices: Vec<ldev::WpDrmLeaseDeviceV1>,
    devices_done: usize,
    /// (connector object, owning device index, name, DRM id)
    connectors: Vec<(lconn::WpDrmLeaseConnectorV1, usize, String, u32)>,
    lease_fd: Option<OwnedFd>,
    lease_failed: bool,
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
        {
            if interface == "wp_drm_lease_device_v1" {
                let dev =
                    registry.bind::<ldev::WpDrmLeaseDeviceV1, _, _>(name, 1, qh, st.devices.len());
                st.devices.push(dev);
            }
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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(|s| s.as_str()) == Some("props") {
        props(args.get(1).map(|s| s.as_str()).unwrap_or("HDMI-A-1"));
        return;
    }
    if args.first().map(|s| s.as_str()) != Some("probe") {
        eprintln!("usage: omarchy-crt-display probe|props [connector] [seconds]");
        std::process::exit(2);
    }
    let cfg = Config::load();
    let want = args.get(1).cloned().unwrap_or_else(|| {
        if cfg.output.connector.is_empty() {
            "HDMI-A-1".into()
        } else {
            cfg.output
                .connector
                .trim_start_matches("card1-")
                .to_string()
        }
    });
    let secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(15);

    let conn = Connection::connect_to_env().unwrap_or_else(|e| die(&format!("wayland: {e}")));
    let mut queue = conn.new_event_queue::<State>();
    let qh = queue.handle();
    let display = conn.display();
    let _registry = display.get_registry(&qh, ());
    let mut st = State::default();
    for _ in 0..6 {
        queue
            .roundtrip(&mut st)
            .unwrap_or_else(|e| die(&format!("roundtrip: {e}")));
        if !st.devices.is_empty() && st.devices_done >= st.devices.len() {
            break;
        }
    }
    println!(
        "lease devices: {}, connectors offered: {}",
        st.devices.len(),
        st.connectors
            .iter()
            .map(|(_, d, n, id)| format!("{n}(id {id}, device {d})"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let Some(target) = st
        .connectors
        .iter()
        .find(|(_, _, n, _)| n == &want)
        .cloned()
    else {
        die(&format!(
            "connector {want} is not offered for lease; is it marked non-desktop? (scripts/crt-lease-setup.sh)"
        ));
    };
    let dev = st.devices[target.1].clone();
    let request = dev.create_lease_request(&qh, ());
    request.request_connector(&target.0);
    let _lease = request.submit(&qh, ());
    for _ in 0..10 {
        queue
            .roundtrip(&mut st)
            .unwrap_or_else(|e| die(&format!("roundtrip: {e}")));
        if st.lease_fd.is_some() || st.lease_failed {
            break;
        }
    }
    let Some(fd) = st.lease_fd.take() else {
        die("the compositor refused the lease");
    };
    println!("leased {want} (DRM connector {})", target.3);

    let card = Leased(fd);
    let res = card
        .resource_handles()
        .unwrap_or_else(|e| die(&format!("resources: {e}")));
    let conn_handle = res
        .connectors()
        .iter()
        .copied()
        .find(|h| u32::from(*h) == target.3)
        .or_else(|| res.connectors().first().copied())
        .unwrap_or_else(|| die("lease has no connector"));
    let info = card
        .get_connector(conn_handle, false)
        .unwrap_or_else(|e| die(&format!("connector: {e}")));
    let crtc_handle = info
        .current_encoder()
        .and_then(|e| card.get_encoder(e).ok())
        .and_then(|e| e.crtc())
        .or_else(|| res.crtcs().first().copied())
        .unwrap_or_else(|| die("lease has no crtc"));
    println!(
        "connector {:?} state {:?}, crtc {:?}, {} native modes",
        conn_handle,
        info.state(),
        crtc_handle,
        info.modes().len()
    );

    let text = cfg
        .modeline("ntsc")
        .unwrap_or_else(|| die("no ntsc modeline in crt.toml"));
    let ml = Modeline::parse(&text).unwrap_or_else(|| die("bad modeline"));
    let mode = drm_mode(&ml);
    let (w, h) = (ml.width(), ml.height());
    println!(
        "setting {}x{} clock {} kHz ({:.3} kHz / {:.3} Hz)",
        w,
        h,
        (ml.clock_mhz * 1000.0) as u32,
        ml.hfreq_khz(),
        ml.vfreq_hz()
    );

    let mut db = card
        .create_dumb_buffer((w, h), drm::buffer::DrmFourcc::Xrgb8888, 32)
        .unwrap_or_else(|e| die(&format!("dumb buffer: {e}")));
    {
        let mut map = card
            .map_dumb_buffer(&mut db)
            .unwrap_or_else(|e| die(&format!("map: {e}")));
        test_card(map.as_mut(), w as usize, h as usize);
    }
    let fb: framebuffer::Handle = card
        .add_framebuffer(&db, 24, 32)
        .unwrap_or_else(|e| die(&format!("framebuffer: {e}")));
    card.set_crtc(crtc_handle, Some(fb), (0, 0), &[conn_handle], Some(mode))
        .unwrap_or_else(|e| die(&format!("set_crtc: {e}")));
    println!("mode set, test card on the tube for {secs} s");
    std::thread::sleep(std::time::Duration::from_secs(secs));
    let _ = card.destroy_framebuffer(fb);
    let _ = card.destroy_dumb_buffer(db);
    println!("done, releasing the lease");
}

/// Print the kernel's view of a connector (no master needed): status,
/// EDID size and the non-desktop property the lease path depends on.
fn props(want: &str) {
    for card in ["/dev/dri/card1", "/dev/dri/card0", "/dev/dri/card2"] {
        let Ok(f) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(card)
        else {
            continue;
        };
        let dev = Leased(OwnedFd::from(f));
        let Ok(res) = dev.resource_handles() else {
            continue;
        };
        for h in res.connectors() {
            let Ok(info) = dev.get_connector(*h, false) else {
                continue;
            };
            let name = format!("{:?}-{}", info.interface(), info.interface_id());
            if !name.contains(
                want.trim_start_matches("HDMI-A-")
                    .trim_start_matches(|c: char| !c.is_ascii_digit()),
            ) && !name.starts_with(want.split('-').next().unwrap_or(""))
            {
                continue;
            }
            let Ok(props) = dev.get_properties(*h) else {
                continue;
            };
            let mut nd = String::from("?");
            for (pid, val) in props.iter() {
                if let Ok(pi) = dev.get_property(*pid) {
                    if pi.name().to_str().unwrap_or("") == "non-desktop" {
                        nd = val.to_string();
                    }
                }
            }
            println!(
                "{card} {name}: state {:?}, {} modes, non-desktop = {nd}",
                info.state(),
                info.modes().len()
            );
        }
    }
}

/// Our modeline as the kernel wants it.
fn drm_mode(ml: &Modeline) -> Mode {
    let mut raw = drm_ffi::drm_mode_modeinfo {
        clock: (ml.clock_mhz * 1000.0).round() as u32,
        hdisplay: ml.h[0] as u16,
        hsync_start: ml.h[1] as u16,
        hsync_end: ml.h[2] as u16,
        htotal: ml.h[3] as u16,
        hskew: 0,
        vdisplay: ml.v[0] as u16,
        vsync_start: ml.v[1] as u16,
        vsync_end: ml.v[2] as u16,
        vtotal: ml.v[3] as u16,
        vscan: 0,
        vrefresh: ml.vfreq_hz().round() as u32,
        flags: 0,
        type_: drm_ffi::DRM_MODE_TYPE_USERDEF,
        name: [0; 32],
    };
    let f = ml.flags.to_ascii_lowercase();
    raw.flags |= if f.contains("+hsync") {
        drm_ffi::DRM_MODE_FLAG_PHSYNC
    } else {
        drm_ffi::DRM_MODE_FLAG_NHSYNC
    };
    raw.flags |= if f.contains("+vsync") {
        drm_ffi::DRM_MODE_FLAG_PVSYNC
    } else {
        drm_ffi::DRM_MODE_FLAG_NVSYNC
    };
    if f.contains("interlace") {
        raw.flags |= drm_ffi::DRM_MODE_FLAG_INTERLACE;
    }
    let name = format!("{}x{}crt", ml.h[0], ml.v[0]);
    for (i, b) in name.bytes().take(31).enumerate() {
        raw.name[i] = b as _;
    }
    Mode::from(raw)
}

/// SMPTE-ish bars, a white frame two pixels in, and a centre cross.
fn test_card(px: &mut [u8], w: usize, h: usize) {
    let bars: [u32; 8] = [
        0xffffff, 0xffff00, 0x00ffff, 0x00ff00, 0xff00ff, 0xff0000, 0x0000ff, 0x202020,
    ];
    for y in 0..h {
        for x in 0..w {
            let mut c = bars[(x * 8 / w).min(7)];
            if y >= h * 3 / 4 {
                let g = (x * 255 / w) as u32;
                c = (g << 16) | (g << 8) | g;
            }
            let edge = x < 2 || y < 2 || x >= w - 2 || y >= h - 2;
            let cross = (x == w / 2 || x == w / 2 + 1) && y > h / 3 && y < h * 2 / 3
                || (y == h / 2) && x > w / 3 && x < w * 2 / 3;
            if edge || cross {
                c = 0xffffff;
            }
            let o = (y * w + x) * 4;
            px[o] = (c & 0xff) as u8;
            px[o + 1] = ((c >> 8) & 0xff) as u8;
            px[o + 2] = ((c >> 16) & 0xff) as u8;
            px[o + 3] = 0;
        }
    }
}

fn die(msg: &str) -> ! {
    eprintln!("omarchy-crt-display: {msg}");
    std::process::exit(1);
}
