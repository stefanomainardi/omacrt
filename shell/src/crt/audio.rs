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

/// The name PulseAudio shows for a sink, which is the only name SDL knows.
///
/// SDL's PulseAudio backend enumerates sinks by their description and asks
/// the server for the default sink by name when it is told to open "the
/// default device". That explicit name beats `PULSE_SINK`, so the launcher
/// has to be told which device to open rather than which sink to prefer,
/// and the description is what it has to be told.
pub fn description(sink: &str) -> Option<String> {
    description_in(&run("pactl", &["list", "sinks"])?, sink)
}

/// The parsing on its own, so it can be tested without a sound server.
fn description_in(text: &str, sink: &str) -> Option<String> {
    let mut current = "";
    for line in text.lines() {
        let s = line.trim();
        if let Some(n) = s.strip_prefix("Name:") {
            current = n.trim();
        } else if let Some(d) = s.strip_prefix("Description:")
            && current == sink
        {
            return Some(d.trim().to_string());
        }
    }
    None
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

/// A sink's volume as a percentage, out of the first channel pactl prints:
/// `Volume: front-left: 39254 /  60% / -13.36 dB, ...`.
fn sink_volume(sink: &str) -> Option<String> {
    let text = run("pactl", &["get-sink-volume", sink])?;
    let pct = text.split('/').nth(1)?.trim().trim_end_matches('%');
    pct.parse::<u32>().ok().map(|v| v.to_string())
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
                "omacrt-shell"
                    | "RetroArch"
                    | "Supermodel"
                    | "omacrt-player"
                    | "mpv"
                    | "PipeWire ALSA [cliamp]"
            ) {
                out.push((id.clone(), app));
            }
        }
    }
    out
}

pub fn move_streams(sink: &str) -> usize {
    // RetroArch goes through the leveller when it is up and the streams are
    // going to the television: the leveller's own output is what goes on
    // to the television, and is not one of these.
    let level = (sink != LEVEL_SINK && level_ready()).then_some(LEVEL_SINK);
    let mut n = 0;
    for (id, app) in our_streams() {
        let to = match level {
            Some(l) if matches!(app.as_str(), "RetroArch" | "Supermodel") && is_crt_sink(sink) => l,
            _ => sink,
        };
        if run("pactl", &["move-sink-input", &id, to]).is_some() {
            n += 1;
        }
    }
    n
}

/// Wait for a program's stream to appear, up to a few seconds, then move the
/// streams to the sink. For a program whose SDL opens the default sink by
/// name and so ignores PULSE_SINK: Supermodel, and the launcher itself.
pub fn move_when_playing(app: &str, sink: &str) -> bool {
    for _ in 0..20 {
        if our_streams().iter().any(|(_, a)| a == app) {
            move_streams(sink);
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    false
}

/// Whether a sink is the television's: the DAC is on the GPU's HDMI audio.
fn is_crt_sink(sink: &str) -> bool {
    sink.contains("hdmi")
}

// ------------------------------------------------------------ the leveller

/// One game is mastered loud and the next one quiet, and on a television the
/// difference is somebody reaching for the remote. While the tube is on,
/// RetroArch plays into a PipeWire filter chain holding LSP's automatic gain
/// stage, which brings what passes through it to one loudness and hands it
/// on to the television. It runs as its own small `pipewire -c` process
/// rather than in the session's PipeWire, so nothing in the desktop's audio
/// configuration changes and nothing has to be restarted; `off` ends it.
pub const LEVEL_SINK: &str = "omacrt_level";
const LEVEL_PLUGIN: &str = "http://lsp-plug.in/plugins/lv2/autogain_stereo";

/// Where the LSP plugins are, when they are installed: the places PipeWire's
/// LV2 loader looks without `LV2_PATH`.
pub fn level_available() -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    [
        "/usr/lib/lv2",
        "/usr/local/lib/lv2",
        &format!("{home}/.lv2"),
    ]
    .iter()
    .any(|d| {
        std::path::Path::new(d)
            .join("lsp-plugins.lv2/autogain_stereo.ttl")
            .exists()
    })
}

fn level_conf() -> std::path::PathBuf {
    super::state_dir().join("level.conf")
}

/// The chain, sending to `target`. The level is -18 LUFS, nearer what games
/// are mastered at than the -23 broadcast figure the plugin starts from, and
/// the gain may rise by 12 dB at most, so a quiet passage or a silent menu is
/// not lifted by the plugin's own limit of 36. Everything else is the
/// plugin's own default.
fn level_config(target: &str) -> String {
    format!(
        r#"# Written by omacrt for the tube's session; `omacrt off` ends it.
context.properties = {{ log.level = 1 }}
context.spa-libs = {{
  audio.convert.* = audioconvert/libspa-audioconvert
  support.*       = support/libspa-support
}}
context.modules = [
  {{ name = libpipewire-module-rt flags = [ ifexists nofail ] }}
  {{ name = libpipewire-module-protocol-native }}
  {{ name = libpipewire-module-client-node }}
  {{ name = libpipewire-module-adapter }}
  {{ name = libpipewire-module-filter-chain
    args = {{
      node.description = "OmaCRT game level"
      media.name       = "OmaCRT game level"
      filter.graph = {{
        nodes = [
          {{ type = lv2 name = level plugin = "{LEVEL_PLUGIN}"
            control = {{ "level" = -18.0 "max_on" = 1.0 "max_amp" = 12.0 }} }}
        ]
        inputs  = [ "level:in_l" "level:in_r" ]
        outputs = [ "level:out_l" "level:out_r" ]
      }}
      audio.channels = 2
      audio.position = [ FL FR ]
      capture.props  = {{ node.name = "{LEVEL_SINK}" media.class = Audio/Sink }}
      playback.props = {{ node.name = "{LEVEL_SINK}.output" node.passive = true target.object = "{target}" }}
    }}
  }}
]
"#
    )
}

/// The leveller's processes: a `pipewire` whose command line names our file.
fn level_pids() -> Vec<i32> {
    let conf = level_conf();
    let conf = conf.to_string_lossy();
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    dir.flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<i32>().ok())
        .filter(|pid| {
            let cmd = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
            let args: Vec<&[u8]> = cmd.split(|b| *b == 0).collect();
            args.first().is_some_and(|a| a.ends_with(b"pipewire"))
                && args.iter().any(|a| *a == conf.as_bytes())
        })
        .collect()
}

/// Whether the leveller's sink is there to play into.
pub fn level_ready() -> bool {
    run("pactl", &["list", "short", "sinks"])
        .is_some_and(|t| t.lines().any(|l| l.split('\t').nth(1) == Some(LEVEL_SINK)))
}

/// Start the leveller in front of `target`, replacing one already running,
/// since the television's sink can have changed since. Detached from this
/// process: `on` is a command that exits, and the chain stays until `off`.
pub fn level_start(target: &str) -> Result<(), String> {
    if !level_available() {
        return Err("lsp-plugins is not installed".into());
    }
    level_stop();
    let conf = level_conf();
    if let Some(dir) = conf.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&conf, level_config(target)).map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new("pipewire");
    cmd.arg("-c")
        .arg(&conf)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid touches no memory of this process; it only moves
        // the child into a session of its own, out of the terminal's.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    cmd.spawn().map_err(|e| format!("pipewire: {e}"))?;
    for _ in 0..30 {
        if level_ready() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    level_stop();
    Err("the leveller's sink did not appear".into())
}

/// End the leveller, if it is running.
pub fn level_stop() {
    for pid in level_pids() {
        // SAFETY: a plain signal to a process found a moment ago by its
        // command line; a pid reused since then would not carry our file.
        unsafe {
            libc::kill(pid, libc::SIGTERM);
        }
    }
}

/// Card profile to the DAC pin, sink volume, our streams moved there. With
/// `system_default` the CRT also becomes the default sink for everything.
pub fn route_to_crt(
    t: &Target,
    volume: u32,
    system_default: bool,
    level: bool,
    state: &mut State,
) -> String {
    if let Some(prev) = active_profile(&t.card)
        && prev != t.profile
        && state.previous_profile.is_empty()
    {
        state.previous_profile = prev;
    }
    state.audio_card = t.card.clone();
    run("pactl", &["set-card-profile", &t.card, &t.profile]);
    std::thread::sleep(std::time::Duration::from_millis(400));
    if state.previous_volume.is_empty()
        && let Some(prev) = sink_volume(&t.sink)
    {
        state.previous_volume = prev;
        state.crt_sink = t.sink.clone();
    }
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
    // The leveller first, so the games' streams moved below land in it.
    let levelled = match (level, level && level_start(&t.sink).is_ok()) {
        (false, _) => {
            level_stop();
            ""
        }
        (true, true) => ", games levelled",
        (true, false) => ", games not levelled (lsp-plugins missing?)",
    };
    let moved = move_streams(&t.sink);
    format!(
        "{} at {volume}%, {moved} stream(s) moved{}{levelled}",
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
    level_stop();
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
    // The volume goes back before the profile does: once the card leaves the
    // profile the sink belongs to, the sink is gone and there is nothing left
    // to set.
    if !state.previous_volume.is_empty() && !state.crt_sink.is_empty() {
        run(
            "pactl",
            &[
                "set-sink-volume",
                &state.crt_sink,
                &format!("{}%", state.previous_volume),
            ],
        );
        state.previous_volume.clear();
        state.crt_sink.clear();
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

#[cfg(test)]
mod level_tests {
    #[test]
    fn the_chain_sends_to_the_television_and_names_the_plugin_controls() {
        let c = super::level_config("alsa_output.pci-0000_03_00.1.hdmi-stereo-extra3");
        assert!(c.contains(r#"target.object = "alsa_output.pci-0000_03_00.1.hdmi-stereo-extra3""#));
        assert!(c.contains(super::LEVEL_PLUGIN));
        // The three controls set, by the symbols the plugin's own TTL gives.
        for k in [
            r#""level" = -18.0"#,
            r#""max_on" = 1.0"#,
            r#""max_amp" = 12.0"#,
        ] {
            assert!(c.contains(k), "{k}");
        }
        assert!(c.contains(r#"node.name = "omacrt_level""#));
    }
}

#[cfg(test)]
mod description_tests {
    #[test]
    fn the_description_is_the_one_of_the_named_sink() {
        let text = "Sink #73\n\tName: alsa_output.usb-Focusrite\n\tDescription: Scarlett 2i2\n\
                    Sink #335655\n\tName: alsa_output.pci-0000_03_00.1.hdmi-stereo-extra3\n\
                    \tDescription: Navi 31 HDMI/DP Audio Digital Stereo (HDMI 4)\n";
        assert_eq!(
            super::description_in(text, "alsa_output.pci-0000_03_00.1.hdmi-stereo-extra3")
                .as_deref(),
            Some("Navi 31 HDMI/DP Audio Digital Stereo (HDMI 4)")
        );
        assert_eq!(
            super::description_in(text, "alsa_output.usb-Focusrite").as_deref(),
            Some("Scarlett 2i2")
        );
        assert_eq!(super::description_in(text, "nothing.like.this"), None);
    }
}
