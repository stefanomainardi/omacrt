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
            name = b[5..18]
                .iter()
                .take_while(|&&c| c != b'\n' && c != 0)
                .map(|&c| c as char)
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
        let name = drm.splitn(2, '-').nth(1).unwrap_or(&drm).to_string();
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

/// The connector named in the config, else the first connected HDMI output
/// with a Mortaca EDID, else the first connected HDMI output.
pub fn pick(cfg: &Config) -> Option<Connector> {
    let all = connectors();
    let want = cfg.output.connector.trim();
    if !want.is_empty() {
        return all.into_iter().find(|c| c.drm == want || c.name == want);
    }
    all.iter()
        .find(|c| c.connected && c.name.contains("HDMI") && c.is_rgbpi2())
        .or_else(|| all.iter().find(|c| c.connected && c.name.contains("HDMI")))
        .cloned()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Modeline {
    pub clock_mhz: f64,
    pub h: [u32; 4],
    pub v: [u32; 4],
    pub flags: String,
}

impl Modeline {
    pub fn parse(text: &str) -> Option<Self> {
        let parts: Vec<&str> = text.split_whitespace().collect();
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
            flags: parts[9..].join(" "),
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
    pub fn with_lines(&self, lines: u32) -> Self {
        let active = self.v[0];
        let vsync = self.v[2] - self.v[1];
        let front = self.v[1] - self.v[0];
        let lines = lines.clamp(180, self.v[3] - vsync - 4);
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

pub fn hypr_monitor(name: &str) -> Option<serde_json::Value> {
    let text = run("hyprctl", &["monitors", "all", "-j"])?;
    let list: Vec<serde_json::Value> = serde_json::from_str(&text).ok()?;
    list.into_iter().find(|m| m["name"] == name)
}

pub fn apply_modeline(name: &str, ml: &Modeline, position: &str) -> (bool, String) {
    hypr_eval(&format!(
        "hl.monitor({{ output = \"{name}\", mode = \"{}\", position = \"{position}\", scale = 1, disabled = false }})",
        ml.to_hypr()
    ))
}

pub fn disable(name: &str) -> (bool, String) {
    hypr_eval(&format!(
        "hl.monitor({{ output = \"{name}\", disabled = true }})"
    ))
}

/// Pin the launcher to the CRT output and our video player to its own
/// workspace there. RetroArch has no way to carry an app id of ours, so the
/// launcher moves its window by pid after it maps (`place_child` in the
/// launcher); a class rule would drag every RetroArch on the desktop along.
/// No `fullscreen` rule: the programs ask for it themselves, and a rule-made
/// fullscreen leaves the bar above them.
pub fn window_rules(name: &str) {
    hypr_eval(&format!(
        "hl.window_rule({{ name = \"omarchy-crt-shell\", match = {{ class = \"{SHELL_CLASS}\" }}, monitor = \"{name}\" }})"
    ));
    hypr_eval(&format!(
        "hl.workspace_rule({{ workspace = \"name:{GAME_WORKSPACE}\", monitor = \"{name}\" }})"
    ));
    hypr_eval(&format!(
        "hl.window_rule({{ name = \"omarchy-crt-player\", match = {{ class = \"omarchy-crt-player\" }}, monitor = \"{name}\", workspace = \"name:{GAME_WORKSPACE}\" }})"
    ));
    // Undo the rule older versions installed for every RetroArch window.
    hypr_eval(
        "hl.window_rule({ name = \"omarchy-crt-retroarch\", match = { class = \"com.libretro.RetroArch\" }, enabled = false })",
    );
    hypr_eval(
        "hl.window_rule({ name = \"omarchy-crt-mpv\", match = { class = \"mpv\" }, enabled = false })",
    );
}

/// Keep the tube to ourselves. A Lua handler on `window.open` inside the
/// compositor moves anything that is not ours off the CRT workspaces to the
/// desktop (a video started from the desktop while the tube had focus), and
/// sends RetroArch straight to the game workspace the moment it maps, so the
/// launcher underneath never loses fullscreen.
pub fn isolate(name: &str) {
    let Some(text) = run("hyprctl", &["monitors", "-j"]) else {
        return;
    };
    let Ok(list) = serde_json::from_str::<Vec<serde_json::Value>>(&text) else {
        return;
    };
    // The focused desktop monitor, else the biggest one.
    let desk = list
        .iter()
        .filter(|m| m["name"] != name && !m["disabled"].as_bool().unwrap_or(false))
        .max_by_key(|m| {
            let focused = m["focused"].as_bool().unwrap_or(false) as i64;
            let area = m["width"].as_i64().unwrap_or(0) * m["height"].as_i64().unwrap_or(0);
            (focused, area)
        })
        .and_then(|m| m["activeWorkspace"]["id"].as_i64());
    let Some(desk) = desk else { return };
    let code = format!(
        r#"if omarchy_crt_isolate then omarchy_crt_isolate:remove() end
omarchy_crt_isolate = hl.on("window.open", function(w)
  if not w or not w.workspace then return end
  local ws = w.workspace.name
  if ws ~= "{WORKSPACE}" and ws ~= "{GAME_WORKSPACE}" then return end
  local class = tostring(w.class or "")
  if class == "{SHELL_CLASS}" or class == "omarchy-crt-player" then return end
  if class == "com.libretro.RetroArch" then
    hl.dispatch(hl.dsp.window.move({{ window = w, workspace = "name:{GAME_WORKSPACE}" }}))
    hl.dispatch(hl.dsp.focus({{ workspace = "name:{GAME_WORKSPACE}" }}))
    return
  end
  hl.dispatch(hl.dsp.window.move({{ window = w, workspace = "{desk}" }}))
end)
return "isolating"
"#
    );
    hypr_eval(&code);
}

/// Drop the isolation handler.
pub fn unisolate() {
    hypr_eval(
        "if omarchy_crt_isolate then omarchy_crt_isolate:remove(); omarchy_crt_isolate = nil end return \"ok\"",
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
    // No `workspace.move` here: moving a workspace that holds a fullscreen
    // window desynchronised Hyprland's fullscreen bookkeeping and the bar
    // came back above the launcher. The rule alone places new windows.
    hypr_eval(&format!(
        "hl.dispatch(hl.dsp.focus({{ workspace = \"name:{WORKSPACE}\" }}))"
    ));
}

/// Keyboard focus to the first window of a class.
pub fn focus_class(class: &str) -> (bool, String) {
    let (ok, out) = hypr_eval(&format!(
        "local w = hl.get_windows({{ class = \"{class}\" }})[1]; if not w then return \"no window\" end; hl.dispatch(hl.dsp.focus({{ window = w }})); return \"focused\""
    ));
    (ok && out.contains("focused"), out)
}
