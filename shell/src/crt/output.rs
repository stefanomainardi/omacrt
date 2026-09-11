//! The CRT output: DRM connector, EDID, Hyprland modelines and window rules.

use super::{Config, SHELL_CLASS, run, run_loose};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Connector {
    pub path: PathBuf,
    /// Kernel name, `card1-HDMI-A-1`.
    pub drm: String,
    /// Hyprland name, `HDMI-A-1`.
    pub name: String,
    pub connected: bool,
    pub edid_name: String,
    pub edid_audio: bool,
}

impl Connector {
    pub fn is_rgbpi2(&self) -> bool {
        self.edid_name.to_ascii_uppercase().contains("MORTACA")
    }
}

fn read_trim(p: &Path) -> String {
    std::fs::read_to_string(p)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Product name and audio flag from the EDID of a connector.
pub fn edid_info(path: &Path) -> (String, bool) {
    let Ok(edid) = std::fs::read(path.join("edid")) else {
        return (String::new(), false);
    };
    if edid.len() < 128 {
        return (String::new(), false);
    }
    let mut name = String::new();
    let mut off = 54;
    while off + 18 <= 126 {
        let b = &edid[off..off + 18];
        if b[0] == 0 && b[1] == 0 && b[2] == 0 && b[3] == 0xFC {
            // Thirteen bytes chosen by whatever is plugged in. The EDID
            // specification says printable ASCII; a device is free to say
            // otherwise, and this string is printed straight into a
            // terminal by `status`, `setup` and `doctor`. An escape
            // sequence in it can repaint the line it is on, which on a
            // report somebody trusts is worth more to an attacker than it
            // sounds. Anything that is not printable is dropped.
            name = b[5..18]
                .iter()
                .take_while(|&&c| c != b'\n' && c != 0)
                .map(|&c| c as char)
                .filter(|c| !c.is_control())
                .collect::<String>()
                .trim()
                .to_string();
        }
        off += 18;
    }
    let audio = edid.len() >= 256 && edid[128] == 0x02 && edid[131] & 0x40 != 0;
    (name, audio)
}

pub fn connectors() -> Vec<Connector> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir("/sys/class/drm") else {
        return out;
    };
    let mut paths: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        let Some(drm) = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        if !drm.starts_with("card") || !drm.contains('-') || drm.contains("Writeback") {
            continue;
        }
        if !path.join("status").exists() {
            continue;
        }
        let connected = read_trim(&path.join("status")) == "connected";
        let (edid_name, edid_audio) = edid_info(&path);
        let name = drm.split_once('-').map(|x| x.1).unwrap_or(&drm).to_string();
        out.push(Connector {
            path,
            drm,
            name,
            connected,
            edid_name,
            edid_audio,
        });
    }
    out
}

/// Known DACs, by the product name their EDID carries. The first field is
/// what shows up in `edid_name`, uppercased; the second is what to call it;
/// the third says whether the sync mode can be set over I2C, which is a
/// thing only the RGB-Pi 2 does so far.
///
/// The list is a convenience, not the way a DAC is found: [`choose`] finds
/// one of any make once its connector is marked non-desktop. What the list
/// buys is a name in `setup` and `status`, and a right answer before the
/// boot time override is installed.
pub const KNOWN_DACS: &[(&str, &str, bool)] = &[
    ("MORTACA", "RGB-Pi 2", true),
    ("RGB-PI", "RGB-Pi", false),
    ("RETROTINK", "RetroTINK", false),
    ("OSSC", "OSSC", false),
];

pub fn known_dac(edid_name: &str) -> Option<(&'static str, bool)> {
    let up = edid_name.to_ascii_uppercase();
    KNOWN_DACS
        .iter()
        .find(|(needle, _, _)| up.contains(needle))
        .map(|(_, label, csync)| (*label, *csync))
}

/// Which connector the television is on.
pub fn pick(cfg: &Config) -> Option<Connector> {
    choose(
        &connectors(),
        &desktop_monitor_names(),
        &cfg.output.connector,
    )
}

/// The names the compositor is currently using as desktop monitors.
///
/// Empty when there is no compositor to ask, which is a fact about the
/// question rather than an answer to it: [`choose`] treats it that way.
fn desktop_monitor_names() -> Vec<String> {
    let Some(text) = run("hyprctl", &["monitors", "all", "-j"]) else {
        return Vec::new();
    };
    let Ok(list) = serde_json::from_str::<Vec<serde_json::Value>>(&text) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|m| m["name"].as_str().map(|s| s.to_string()))
        .collect()
}

/// Pick the television's connector, in order of how much each signal is
/// worth knowing.
///
/// 1. What the configuration says, if it says anything. Nothing overrules a
///    person who has written the name down.
/// 2. A DAC this project knows by the name in its EDID. Works before the
///    boot time override is installed, which is when somebody most needs to
///    be told which connector to hand over.
/// 3. A connected HDMI output the compositor is not using as a monitor.
///    That is exactly what the override makes of the DAC's connector, and
///    it says nothing about who made the device: any DAC marked non-desktop
///    is found this way. With more than one such output the first is taken,
///    which is a guess, but a guess among outputs that are not somebody's
///    desktop, and `status` names the one it took so a wrong guess costs one
///    line of configuration.
/// 4. Failing all that, the first connected HDMI output.
pub fn choose(all: &[Connector], desktop: &[String], configured: &str) -> Option<Connector> {
    let want = configured.trim();
    if !want.is_empty() {
        return all
            .iter()
            .find(|c| c.drm == want || c.name == want)
            .cloned();
    }
    let hdmi: Vec<&Connector> = all
        .iter()
        .filter(|c| c.connected && c.name.contains("HDMI"))
        .collect();
    if let Some(c) = hdmi.iter().find(|c| known_dac(&c.edid_name).is_some()) {
        return Some((*c).clone());
    }
    if !desktop.is_empty()
        && let Some(spare) = hdmi
            .iter()
            .find(|c| !desktop.iter().any(|n| n == &c.name || n == &c.drm))
    {
        return Some((*spare).clone());
    }
    hdmi.first().map(|c| (*c).clone())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Modeline {
    pub clock_mhz: f64,
    pub h: [u32; 4],
    pub v: [u32; 4],
    pub flags: String,
}

/// What may follow the numbers in a modeline. Anything else is not a flag,
/// and this list is what keeps `crt.toml` from reaching the compositor's Lua.
const MODELINE_FLAGS: &[&str] = &[
    "+hsync",
    "-hsync",
    "+vsync",
    "-vsync",
    "+csync",
    "-csync",
    "interlace",
    "doublescan",
    "rgb",
];

impl Modeline {
    pub fn parse(text: &str) -> Option<Self> {
        // With or without Hyprland's leading "modeline" word.
        let parts: Vec<&str> = text
            .split_whitespace()
            .filter(|p| !p.eq_ignore_ascii_case("modeline"))
            .collect();
        if parts.len() < 9 {
            return None;
        }
        let clock_mhz: f64 = parts[0].parse().ok()?;
        let mut nums = [0u32; 8];
        for (i, p) in parts[1..9].iter().enumerate() {
            nums[i] = p.parse().ok()?;
        }
        Some(Self {
            clock_mhz,
            h: [nums[0], nums[1], nums[2], nums[3]],
            v: [nums[4], nums[5], nums[6], nums[7]],
            // Only the tokens a modeline can actually carry. Everything
            // after the numbers comes out of `crt.toml` and ends up inside a
            // Lua string handed to the compositor, so an unknown word is
            // dropped rather than passed on.
            flags: parts[9..]
                .iter()
                .filter(|f| MODELINE_FLAGS.contains(&f.to_ascii_lowercase().as_str()))
                .cloned()
                .collect::<Vec<_>>()
                .join(" "),
        })
    }

    pub fn width(&self) -> u32 {
        self.h[0]
    }

    pub fn height(&self) -> u32 {
        self.v[0]
    }

    pub fn hfreq_khz(&self) -> f64 {
        self.clock_mhz * 1000.0 / self.h[3] as f64
    }

    pub fn vfreq_hz(&self) -> f64 {
        self.hfreq_khz() * 1000.0 / self.v[3] as f64
    }

    /// The same timing with a different number of active lines, centred in
    /// the frame: a 224 line game on a 240 line standard keeps the line rate
    /// and refresh and gains blank lines above and below.
    /// True when the modeline draws two fields per frame.
    pub fn is_interlaced(&self) -> bool {
        self.flags.to_ascii_lowercase().contains("interlace")
    }

    /// What the television sees: the field rate, which for an interlaced mode
    /// is twice the frame rate. A 480i picture is 59.94 Hz on the tube even
    /// though its frames arrive at 29.97.
    pub fn field_hz(&self) -> f64 {
        if self.is_interlaced() {
            self.vfreq_hz() * 2.0
        } else {
            self.vfreq_hz()
        }
    }

    /// How the mode is written: `240p`, `480i`.
    pub fn label(&self) -> String {
        format!(
            "{}{}",
            self.height(),
            if self.is_interlaced() { "i" } else { "p" }
        )
    }

    pub fn with_lines(&self, lines: u32) -> Self {
        let active = self.v[0];
        let vsync = self.v[2] - self.v[1];
        let front = self.v[1] - self.v[0];
        // Never eat into the blanking: a frame with no front or back porch
        // left is one the television cannot lock onto.
        let lines = lines.clamp(180, self.v[3] - vsync - 8);
        let extra = active as i64 - lines as i64;
        let front = (front as i64 + extra / 2).max(1) as u32;
        let mut m = self.clone();
        m.v = [lines, lines + front, lines + front + vsync, self.v[3]];
        m
    }

    /// Move the picture on the tube: right by `dx` pixels (front porch
    /// shrinks, back porch grows) and down by `dy` lines. Porches keep a
    /// minimum so sync stays legal.
    pub fn shifted(&self, dx: i32, dy: i32) -> Self {
        let mut m = self.clone();
        let hfront = (self.h[1] - self.h[0]) as i32;
        let hsync = (self.h[2] - self.h[1]) as i32;
        let hback = (self.h[3] - self.h[2]) as i32;
        let dx = dx.clamp(-(hback - 16).max(0), (hfront - 8).max(0));
        let hfront = hfront - dx;
        m.h[1] = self.h[0] + hfront as u32;
        m.h[2] = m.h[1] + hsync as u32;
        // htotal unchanged, back porch absorbs the rest
        let vfront = (self.v[1] - self.v[0]) as i32;
        let vsync = (self.v[2] - self.v[1]) as i32;
        let vback = (self.v[3] - self.v[2]) as i32;
        let dy = dy.clamp(-(vback - 4).max(0), (vfront - 1).max(0));
        let vfront = vfront - dy;
        m.v[1] = self.v[0] + vfront as u32;
        m.v[2] = m.v[1] + vsync as u32;
        m
    }

    pub fn to_hypr(&self) -> String {
        format!(
            "modeline {} {} {} {} {} {} {} {} {} {}",
            self.clock_mhz,
            self.h[0],
            self.h[1],
            self.h[2],
            self.h[3],
            self.v[0],
            self.v[1],
            self.v[2],
            self.v[3],
            self.flags
        )
        .trim()
        .to_string()
    }
}

/// `hyprctl eval` with a Lua snippet; Hyprland 0.56 refuses `keyword` on
/// Lua configs. Returns (ok, output).
pub fn hypr_eval(code: &str) -> (bool, String) {
    let (ok, out) = run_loose("hyprctl", &["eval", code]);
    (ok && !out.to_ascii_lowercase().contains("error"), out)
}

/// One `hyprctl getoption` value, as the integer the compositor reports.
/// Booleans come back as `bool`, everything else as `int`.
pub fn option_int(name: &str) -> Option<i64> {
    let text = run("hyprctl", &["getoption", name, "-j"])?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.get("int")
        .and_then(serde_json::Value::as_i64)
        .or_else(|| {
            v.get("bool")
                .and_then(serde_json::Value::as_bool)
                .map(i64::from)
        })
}

pub fn hypr_monitor(name: &str) -> Option<serde_json::Value> {
    let text = run("hyprctl", &["monitors", "all", "-j"])?;
    let list: Vec<serde_json::Value> = serde_json::from_str(&text).ok()?;
    list.into_iter().find(|m| m["name"] == name)
}

/// `auto`, `auto-left`, or a pair of coordinates. Hyprland takes nothing
/// else, and the value is written straight into the Lua below.
pub fn hypr_position(position: &str) -> &str {
    let p = position.trim();
    let ok = p.starts_with("auto")
        && p.len() <= 16
        && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        || p.split_once('x').is_some_and(|(a, b)| {
            !a.is_empty()
                && !b.is_empty()
                && [a, b].iter().all(|n| {
                    n.strip_prefix('-')
                        .unwrap_or(n)
                        .chars()
                        .all(|c| c.is_ascii_digit())
                })
        });
    if ok { p } else { "auto" }
}

pub fn apply_modeline(name: &str, ml: &Modeline, position: &str) -> (bool, String) {
    hypr_eval(&format!(
        "hl.monitor({{ output = \"{name}\", mode = \"{}\", position = \"{}\", scale = 1, disabled = false }})",
        ml.to_hypr(),
        hypr_position(position)
    ))
}

pub fn disable(name: &str) -> (bool, String) {
    hypr_eval(&format!(
        "hl.monitor({{ output = \"{name}\", disabled = true }})"
    ))
}

/// Window rules for what we put on the tube.
///
/// Nothing on the tube is a compositor "fullscreen" window any more: a
/// fullscreen window hides everything else on its workspace, so showing the
/// pause menu meant hiding the game, and a hidden game stalls on its next
/// frame (no commands, then the not-responding dialog). Instead the
/// launcher, the emulator and the player are floating, pinned, output sized
/// windows on the `crt` workspace: all of them stay mapped and rendered, and
/// which one is seen is a matter of stacking order (`raise`).
pub fn window_rules(name: &str) {
    let (_, _, w, h) = monitor_geometry(name).unwrap_or((0, 0, 320, 240));
    let fields = tube_rule_fields(w, h);
    hypr_eval(&format!(
        "hl.window_rule({{ name = \"omacrt-shell\", match = {{ class = \"{SHELL_CLASS}\" }}, monitor = \"{name}\", workspace = \"name:{WORKSPACE}\", {fields} }})"
    ));
    hypr_eval(&format!(
        "hl.window_rule({{ name = \"omacrt-player\", match = {{ class = \"omacrt-player\" }}, monitor = \"{name}\", workspace = \"name:{WORKSPACE}\", {fields} }})"
    ));
    // RetroArch has no app id of ours; the window.open handler (`isolate`)
    // takes care of the one the launcher starts. Rules from older versions
    // go away.
    for old in ["omacrt-retroarch", "omacrt-mpv"] {
        hypr_eval(&format!(
            "hl.window_rule({{ name = \"{old}\", match = {{ class = \"{old}\" }}, enabled = false }})"
        ));
    }
}

/// Keep the tube to ourselves. A Lua handler on `window.open` inside the
/// compositor moves anything that is not ours off the CRT workspaces to the
/// desktop (a video started from the desktop while the tube had focus), and
/// sends RetroArch straight to the game workspace the moment it maps, so the
/// launcher underneath never loses fullscreen.
/// Position and size of a monitor, from the compositor.
pub fn monitor_geometry(name: &str) -> Option<(i64, i64, i64, i64)> {
    let text = run("hyprctl", &["monitors", "-j"])?;
    let list = serde_json::from_str::<Vec<serde_json::Value>>(&text).ok()?;
    let m = list.iter().find(|m| m["name"] == name)?;
    Some((
        m["x"].as_i64()?,
        m["y"].as_i64()?,
        m["width"].as_i64()?,
        m["height"].as_i64()?,
    ))
}

/// Rule fields that make a window a bare, full output, always-on-top
/// surface on the tube: floating and pinned (drawn above everything on its
/// monitor, whatever workspace is active), sized to the output and at its
/// corner (rule positions are monitor relative), without border, rounding,
/// shadow, dim or animation.
fn tube_rule_fields(w: i64, h: i64) -> String {
    format!(
        "float = true, pin = true, size = \"{w} {h}\", move = \"0 0\", border_size = 0, rounding = 0, no_shadow = true, no_anim = true, no_dim = true, no_blur = true, decorate = false"
    )
}

/// The desktop monitor: the focused one that is not the CRT, else the
/// biggest. Returns (active workspace id, centre x, centre y).
pub fn desktop_monitor(crt: &str) -> Option<(i64, i64, i64)> {
    let text = run("hyprctl", &["monitors", "-j"])?;
    let list = serde_json::from_str::<Vec<serde_json::Value>>(&text).ok()?;
    let m = list
        .iter()
        .filter(|m| m["name"] != crt && !m["disabled"].as_bool().unwrap_or(false))
        .filter(|m| {
            let ws = m["activeWorkspace"]["name"].as_str().unwrap_or("");
            ws != WORKSPACE && ws != GAME_WORKSPACE
        })
        .max_by_key(|m| {
            let focused = m["focused"].as_bool().unwrap_or(false) as i64;
            let area = m["width"].as_i64().unwrap_or(0) * m["height"].as_i64().unwrap_or(0);
            (focused, area)
        })?;
    let ws = m["activeWorkspace"]["id"].as_i64()?;
    let cx = m["x"].as_i64()? + m["width"].as_i64()? / 2;
    let cy = m["y"].as_i64()? + m["height"].as_i64()? / 2;
    Some((ws, cx, cy))
}

/// Put the pointer back on the desktop. Focusing a window warps the
/// pointer onto it, and a pointer parked on the tube is drawn there as a
/// stray blob until someone moves it.
pub fn park_cursor(crt: &str) {
    if let Some((_, cx, cy)) = desktop_monitor(crt) {
        hypr_eval(&format!(
            "hl.dispatch(hl.dsp.cursor.move({{ x = {cx}, y = {cy} }}))"
        ));
    }
}

pub fn isolate(name: &str) {
    let Some((desk, cx, cy)) = desktop_monitor(name) else {
        return;
    };
    let (mx, my, _, _) = monitor_geometry(name).unwrap_or((0, 0, 0, 0));
    let code = format!(
        r#"if omacrt_isolate then omacrt_isolate:remove() end
omacrt_isolate = hl.on("window.open", function(w)
  if not w or not w.workspace then return end
  local ws = w.workspace.name
  local class = tostring(w.class or "")
  local ours = class == "{SHELL_CLASS}" or class == "omacrt-player"
  local game = class == "com.libretro.RetroArch" and (ws == "{WORKSPACE}" or ws == "{GAME_WORKSPACE}" or omacrt_expect_game)
  if game then
    omacrt_expect_game = false
    hl.dispatch(hl.dsp.window.move({{ window = w, workspace = "name:{WORKSPACE}" }}))
    hl.dispatch(hl.dsp.window.fullscreen({{ window = w, enable = false }}))
    hl.dispatch(hl.dsp.window.float({{ window = w, enable = true }}))
    hl.dispatch(hl.dsp.window.pin({{ window = w, enable = true }}))
    hl.dispatch(hl.dsp.window.move({{ window = w, x = {mx}, y = {my} }}))
    hl.dispatch(hl.dsp.window.bring_to_top({{ window = w }}))
    hl.dispatch(hl.dsp.focus({{ window = w }}))
    hl.dispatch(hl.dsp.cursor.move({{ x = {cx}, y = {cy} }}))
    return
  end
  if ours then return end
  if ws ~= "{WORKSPACE}" and ws ~= "{GAME_WORKSPACE}" then return end
  hl.dispatch(hl.dsp.window.move({{ window = w, workspace = "{desk}" }}))
end)
return "isolating"
"#
    );
    hypr_eval(&code);
}

/// Tell the compositor the next RetroArch window is ours, wherever focus
/// happens to be when it maps. The launcher calls this right before it
/// spawns a game; the handler clears the flag when the window arrives, and
/// it expires on its own so a game that never starts cannot capture a
/// RetroArch opened on the desktop later.
pub fn expect_game() {
    hypr_eval("omacrt_expect_game = true; return \"ok\"");
}

/// Drop the isolation handler.
pub fn unisolate() {
    hypr_eval(
        "if omacrt_isolate then omacrt_isolate:remove(); omacrt_isolate = nil end return \"ok\"",
    );
}

/// Workspace games and videos run on while the launcher waits underneath.
pub const GAME_WORKSPACE: &str = "crtgame";

/// Name of the workspace the CRT output owns while on, so it never takes a
/// numbered desktop workspace and new terminals never land on the tube.
pub const WORKSPACE: &str = "crt";

/// Bind the `crt` workspace to the CRT output and show it there.
pub fn workspace_rule(name: &str) {
    hypr_eval(&format!(
        "hl.workspace_rule({{ workspace = \"name:{WORKSPACE}\", monitor = \"{name}\", default = true, persistent = true }})"
    ));
    // No `workspace.move` and no focus dispatch here: moving a workspace
    // that holds a fullscreen window desynchronised Hyprland's fullscreen
    // bookkeeping, and focusing before the output exists lands on the
    // desktop. The rule alone (default = true) gives the output `crt` when
    // it comes up; the launcher window then gets focus explicitly.
}

/// Bring one of our windows to the front of the tube and give it focus.
///
/// Stacking among pinned windows is fixed, so raising is done with the pin
/// itself: the game unpinned sits below the pinned launcher (still mapped,
/// rendered and answering), pinned again it is back above. Focus follows.
pub fn raise(class: &str) -> bool {
    let game = "com.libretro.RetroArch";
    let pin_game = class != SHELL_CLASS;
    park_cursor("");
    let (ok, out) = hypr_eval(&format!(
        "local g = hl.get_windows({{ class = \"{game}\" }})[1]; if g then hl.dispatch(hl.dsp.window.pin({{ window = g, enable = {pin_game} }})) end; local w = hl.get_windows({{ class = \"{class}\" }})[1]; if not w then return \"no window\" end; hl.dispatch(hl.dsp.focus({{ window = w }})); return \"raised\""
    ));
    ok && !out.contains("no window")
}

/// Forget an expected game that never showed up.
pub fn expect_game_clear() {
    hypr_eval("omacrt_expect_game = false; return \"ok\"");
}

/// Keyboard focus to the first window of a class.
pub fn focus_class(class: &str) -> (bool, String) {
    // The pointer moves first: this desktop focuses whatever is under the
    // cursor (input:follow_mouse), so parking it after focusing would hand
    // the keyboard straight back to whatever window sits there.
    park_cursor("");
    let (ok, out) = hypr_eval(&format!(
        "local w = hl.get_windows({{ class = \"{class}\" }})[1]; if not w then return \"no window\" end; hl.dispatch(hl.dsp.focus({{ window = w }})); return \"focused\""
    ));
    (ok && out.contains("focused"), out)
}

#[cfg(test)]
mod choose_tests {
    use super::*;

    fn conn(name: &str, connected: bool, edid: &str) -> Connector {
        Connector {
            path: PathBuf::from(format!("/sys/class/drm/card1-{name}")),
            drm: format!("card1-{name}"),
            name: name.to_string(),
            connected,
            edid_name: edid.to_string(),
            edid_audio: true,
        }
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_configuration_wins() {
        let all = [
            conn("HDMI-A-1", true, "MORTACA DEV00"),
            conn("HDMI-A-2", true, "PRISM"),
        ];
        let picked = choose(&all, &names(&["HDMI-A-1", "HDMI-A-2"]), "HDMI-A-2").expect("picked");
        assert_eq!(picked.name, "HDMI-A-2");
        // The kernel name works too, since that is what sysfs calls it.
        let picked = choose(&all, &[], "card1-HDMI-A-2").expect("picked");
        assert_eq!(picked.name, "HDMI-A-2");
    }

    /// Before the boot time override there is nothing to tell the DAC from a
    /// monitor except the name in its EDID, so the one name this project
    /// knows is still worth checking first.
    #[test]
    fn a_dac_known_by_name_is_found_among_monitors() {
        let all = [
            conn("HDMI-A-1", true, "Samsung C34J79x"),
            conn("HDMI-A-2", true, "MORTACA DEV00"),
        ];
        let picked = choose(&all, &names(&["HDMI-A-1", "HDMI-A-2"]), "").expect("picked");
        assert_eq!(picked.name, "HDMI-A-2");
    }

    /// The vendor neutral case: any DAC at all, once its connector is marked
    /// non-desktop, is the connected HDMI output the compositor is not using.
    #[test]
    fn a_dac_of_any_make_is_found_by_the_desktop_not_using_it() {
        let all = [
            conn("HDMI-A-1", true, "Fujitsu B24W-7"),
            conn("HDMI-A-2", true, "Reflex Prism"),
        ];
        let picked = choose(&all, &names(&["HDMI-A-1"]), "").expect("picked");
        assert_eq!(picked.name, "HDMI-A-2");
    }

    /// With two candidates the first is taken, but never one the desktop is
    /// on: putting somebody's monitor at 15 kHz is the one wrong answer
    /// here, and a wrong guess between the other two costs a line of
    /// configuration.
    #[test]
    fn a_monitor_in_use_is_never_taken_for_the_television() {
        let all = [
            conn("HDMI-A-1", true, "Samsung C34J79x"),
            conn("HDMI-A-2", true, "two"),
            conn("HDMI-A-3", true, "three"),
        ];
        let picked = choose(&all, &names(&["HDMI-A-1"]), "").expect("picked");
        assert_eq!(picked.name, "HDMI-A-2");
    }

    #[test]
    fn a_single_hdmi_output_is_it() {
        let all = [
            conn("DP-2", true, "Samsung C34J79x"),
            conn("HDMI-A-1", true, "Reflex Prism"),
            conn("HDMI-A-2", false, ""),
        ];
        let picked = choose(&all, &names(&["DP-2", "HDMI-A-1"]), "").expect("picked");
        assert_eq!(picked.name, "HDMI-A-1");
    }

    #[test]
    fn nothing_connected_is_nothing() {
        let all = [conn("HDMI-A-1", false, "")];
        assert!(choose(&all, &[], "").is_none());
    }
}

#[cfg(test)]
mod edid_tests {
    use super::*;
    use std::io::Write;

    /// Build an EDID with `name` in its product name descriptor.
    fn edid_with(name: &[u8]) -> Vec<u8> {
        let mut e = vec![0u8; 128];
        e[54] = 0;
        e[55] = 0;
        e[56] = 0;
        e[57] = 0xFC;
        e[58] = 0;
        for (i, b) in name.iter().take(13).enumerate() {
            e[59 + i] = *b;
        }
        e
    }

    fn name_of(tag: &str, bytes: &[u8]) -> String {
        // A directory of its own: these run in parallel.
        let dir = std::env::temp_dir().join(format!("omacrt-edid-{}-{tag}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut f = std::fs::File::create(dir.join("edid")).expect("write edid");
        f.write_all(bytes).expect("write");
        drop(f);
        let (name, _) = edid_info(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        name
    }

    #[test]
    fn an_ordinary_product_name_is_read() {
        assert_eq!(
            name_of("plain", &edid_with(b"MORTACA DEV00")),
            "MORTACA DEV00"
        );
    }

    /// Thirteen bytes chosen by whatever is plugged in, printed into a
    /// terminal by three commands. A device that puts an escape sequence
    /// there could repaint the line of a report somebody is trusting.
    #[test]
    fn a_device_cannot_write_escapes_into_the_terminal() {
        let hostile = b"\x1b[2K\rOK   fine";
        let got = name_of("hostile", &edid_with(hostile));
        assert!(!got.contains('\x1b'), "escape survived: {got:?}");
        assert!(!got.contains('\r'), "carriage return survived: {got:?}");
        assert!(
            !got.chars().any(|c| c.is_control()),
            "a control character survived: {got:?}"
        );
        // Thirteen bytes is all a descriptor holds, so the lie is cut
        // short as well as disarmed.
        assert_eq!(got, "[2KOK   fin");
    }
}
