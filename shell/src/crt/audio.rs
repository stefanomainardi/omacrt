//! HDMI audio to the DAC through PipeWire: the ELD pin of the connector, its
//! card profile and sink, default sink switching and stream migration.

use super::output::Connector;
use super::{State, run};

#[derive(Clone, Debug)]
pub struct Target {
    pub card: String,
    pub profile: String,
    pub sink: String,
    pub pin: u32,
}

/// The GPU's audio function is PCI function 1 of the display device; each
/// connector with a sink is one ELD pin, matched here by the EDID name.
pub fn target(conn: &Connector) -> Option<Target> {
    let gpu = std::fs::canonicalize(conn.path.join("device/device")).ok()?;
    let gpu = gpu.file_name()?.to_str()?.to_string();
    let audio_pci = format!("{}.1", gpu.rsplit_once('.')?.0);
    let cards = std::fs::read_dir(format!("/sys/bus/pci/devices/{audio_pci}/sound")).ok()?;
    let alsa = cards
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|n| n.starts_with("card"))?;
    let mut pin = None;
    for entry in std::fs::read_dir(format!("/proc/asound/{alsa}"))
        .ok()?
        .flatten()
    {
        let fname = entry.file_name().to_string_lossy().into_owned();
        let Some(idx) = fname.strip_prefix("eld#0.") else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let present = text
            .lines()
            .any(|l| l.starts_with("monitor_present") && l.trim_end().ends_with('1'));
        if !present {
            continue;
        }
        let name = text
            .lines()
            .find(|l| l.starts_with("monitor_name"))
            .map(|l| l["monitor_name".len()..].trim().to_string())
            .unwrap_or_default();
        if !conn.edid_name.is_empty() && name == conn.edid_name {
            pin = idx.parse::<u32>().ok();
        }
    }
    let pin = pin?;
    let pci_id = audio_pci.replace(':', "_");
    let profile = if pin == 0 {
        "output:hdmi-stereo".to_string()
    } else {
        format!("output:hdmi-stereo-extra{pin}")
    };
    Some(Target {
        card: format!("alsa_card.pci-{pci_id}"),
        sink: format!("alsa_output.pci-{pci_id}.{}", &profile["output:".len()..]),
        profile,
        pin,
    })
}

pub fn active_profile(card: &str) -> Option<String> {
    let text = run("pactl", &["list", "cards"])?;
    let mut current = "";
    for line in text.lines() {
        let s = line.trim();
        if let Some(n) = s.strip_prefix("Name:") {
            current = n.trim();
        } else if let Some(p) = s.strip_prefix("Active Profile:")
            && current == card
        {
            return Some(p.trim().to_string());
        }
    }
    None
}

/// Any sink that is not the CRT one, preferring non HDMI outputs.
pub fn other_sink(crt: &str) -> Option<String> {
    let text = run("pactl", &["list", "short", "sinks"])?;
    let names: Vec<String> = text
        .lines()
        .filter_map(|l| l.split_whitespace().nth(1))
        .filter(|n| *n != crt)
        .map(str::to_string)
        .collect();
    names
        .iter()
        .find(|n| !n.contains("hdmi"))
        .or(names.first())
        .cloned()
}

pub fn default_sink() -> Option<String> {
    run("pactl", &["get-default-sink"]).map(|s| s.trim().to_string())
}

/// Sink inputs of the launcher, RetroArch, mpv and cliamp: (id, application).
fn our_streams() -> Vec<(String, String)> {
    let Some(text) = run("pactl", &["list", "sink-inputs"]) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut id = String::new();
    for line in text.lines() {
        let s = line.trim();
        if let Some(rest) = s.strip_prefix("Sink Input #") {
            id = rest.trim().to_string();
        } else if let Some(rest) = s.strip_prefix("application.name = ") {
            let app = rest.trim_matches('"').to_string();
            // Our own player names itself, so PipeWire remembers a sink for
            // it alone: a desktop mpv keeps whatever output the desktop uses.
            // Plain "mpv" stays in the list for players started before this.
            if matches!(
                app.as_str(),
                "omacrt-shell" | "RetroArch" | "omacrt-player" | "mpv" | "PipeWire ALSA [cliamp]"
            ) {
                out.push((id.clone(), app));
            }
        }
    }
    out
}

pub fn move_streams(sink: &str) -> usize {
    let mut n = 0;
    for (id, _) in our_streams() {
        if run("pactl", &["move-sink-input", &id, sink]).is_some() {
            n += 1;
        }
    }
    n
}

/// Card profile to the DAC pin, sink volume, our streams moved there. With
/// `system_default` the CRT also becomes the default sink for everything.
pub fn route_to_crt(t: &Target, volume: u32, system_default: bool, state: &mut State) -> String {
    if let Some(prev) = active_profile(&t.card)
        && prev != t.profile
        && state.previous_profile.is_empty()
    {
        state.previous_profile = prev;
    }
    state.audio_card = t.card.clone();
    run("pactl", &["set-card-profile", &t.card, &t.profile]);
    std::thread::sleep(std::time::Duration::from_millis(400));
    run(
        "pactl",
        &["set-sink-volume", &t.sink, &format!("{volume}%")],
    );
    run("pactl", &["set-sink-mute", &t.sink, "0"]);
    if system_default {
        if state.previous_sink.is_empty()
            && let Some(prev) = default_sink()
            && prev != t.sink
        {
            state.previous_sink = prev;
        }
        run("pactl", &["set-default-sink", &t.sink]);
    }
    let moved = move_streams(&t.sink);
    format!(
        "{} at {volume}%, {moved} stream(s) moved{}",
        t.sink,
        if system_default {
            ", system default"
        } else {
            ""
        }
    )
}

/// Undo `route_to_crt` from the saved state. Our streams go back to the
/// desktop default sink; the system default is restored only when `on`
/// changed it.
pub fn route_back(state: &mut State) -> String {
    let mut note = String::from("desktop");
    if !state.previous_sink.is_empty() {
        run("pactl", &["set-default-sink", &state.previous_sink]);
        note = state.previous_sink.clone();
    }
    if let Some(def) = default_sink() {
        move_streams(&def);
        if state.previous_sink.is_empty() {
            note = def;
        }
    }
    if !state.audio_card.is_empty() && !state.previous_profile.is_empty() {
        run(
            "pactl",
            &[
                "set-card-profile",
                &state.audio_card,
                &state.previous_profile,
            ],
        );
    }
    state.previous_profile.clear();
    state.previous_sink.clear();
    state.audio_card.clear();
    note
}
