//! `omarchy-crt`: drive a 15 kHz CRT from the Omarchy desktop.
//!
//! One command turns the tube on (modeline, DAC composite sync, audio to the
//! TV, launcher fullscreen) and one turns it off. The rest is status,
//! diagnostics, the game library and BIOS files. The Omarchy bar plugin is
//! a thin face over `omarchy-crt status --json` and these commands.

use omarchy_crt_shell::crt::dac::{Csync, Dac, Lock};
use omarchy_crt_shell::crt::output::{self, Connector, Modeline};
use omarchy_crt_shell::crt::{self, Config, State, audio, bios, display, launcher, roms, watchdog};
use omarchy_crt_shell::library::{self, Library};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::exit;

const HELP: &str = "\
omarchy-crt: drive a 15 kHz CRT from the Omarchy desktop

  status [--json]          output, mode, DAC, audio, launcher, BIOS at a glance
  on [ntsc|pal]            15 kHz modeline, DAC csync, audio to the TV, launcher
  off                      launcher closed, audio back, output disabled
  boot                     login reset: CRT output off, audio back to the desktop
  watchdog                 put the display back if it dies; started by `on`, ends with `off`
  toggle
  mode [ntsc|pal|film|480i|576i] [--lines N] [--shift-x X] [--shift-y Y]
                           standard, active lines, picture shift; no args = full frame
  shell start|stop|restart|focus
  shell key <input>...           drive the launcher: home menu up down left right fire back fav alt
                                 search osk del next prev first last
  shell type <text>              type into the launcher's search bar
  game key <key> [ms]            press a key inside the running game (enter, rshift, or an
                                 evdev code), held for that many milliseconds
  watch <file|url> [--later [TITLE]]  play a video or a YouTube link on the tube, or keep it for later
  game menu|pause|save|load|reset|quit|cmd <CMD>   talk to the running emulator
  shot <file.png>                what the tube shows right now (leased output)
  monitor on|off                 desktop window: live preview of the tube, keyboard to the tube when focused
  record start <file.mp4>|stop   capture the tube, picture and sound, into a video
  focus                    keyboard focus to the launcher
  audio crt|desktop|all|apps  games audio to the TV or back; all = whole system
  audio volume N           TV sink volume in percent (up to 150), kept in the config
  dac status|reset|csync and|xor|separate|watch
  bios [--json]            BIOS files the cores expect
  bios import DIR [--all]  copy BIOS files from another collection
  bios discover [--json]   folders on the roots and disks that hold BIOS files
  library [--json]         systems, sources, game counts, cores
  library cores [--json]   the core each system needs, installed or not, and its package
  library set SYS core=X|dir=D   change a system's core or folder in systems.toml
  library covers [SYS...] [--limit N] [--force]   fetch box art for the collection, matching titles when names differ
  library scan [DIR...]    index every game under the roots (any layout)
  library discover [--json]  mounted places that look like collections
  library roots add|remove DIR
  library assign DIR SYS   tell the scan what a folder holds
  library unknown          files the scan could not place
  library systems          the systems catalogue
  library collections [import DIR]  curated lists (RePlayOS _favorites folders import)
  doctor                   checks with plain answers
  config                   config file path and contents
  config set KEY VALUE     change one setting (output.csync, output.standard, audio.volume, shell.autostart, ...)

Config: ~/.config/omarchy-crt/crt.toml (written with defaults on first run)";

/// Keyboard to the tube. With the connector leased the tube's clients have
/// no keyboard of their own: the desktop monitor window carries it.
fn focus_note() -> String {
    if display::running() {
        display::monitor_focus()
    } else {
        launcher::focus().1
    }
}

fn die(msg: &str) -> ! {
    eprintln!("omarchy-crt: {msg}");
    exit(1)
}

fn has(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

/// Arguments that are neither flags nor the value of a flag taking one.
fn positional(args: &[String]) -> Vec<&String> {
    let mut out = Vec::new();
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
            continue;
        }
        if a == "--lines" || a == "--shift-x" || a == "--shift-y" {
            skip = true;
            continue;
        }
        if !a.starts_with("--") {
            out.push(a);
        }
    }
    out
}

fn connector(cfg: &Config) -> Connector {
    output::pick(cfg)
        .unwrap_or_else(|| die("no CRT output found (connect the DAC or set output.connector)"))
}

fn open_dac(conn: &Connector) -> Result<Dac, String> {
    let bus = Dac::bus_of(&conn.path).ok_or("connector has no DDC bus")?;
    let dac = Dac::open(&bus).map_err(|e| format!("{bus}: {e}"))?;
    if !dac.present() {
        return Err(format!("no RGB-Pi 2 at 0x78 on {bus}"));
    }
    Ok(dac)
}

/// Sink the launcher should play on: the DAC's when audio routing is on.
fn crt_sink(cfg: &Config, conn: &Connector) -> Option<String> {
    if !cfg.audio.route {
        return None;
    }
    audio::target(conn).map(|t| t.sink)
}

fn library() -> Library {
    Library::load(&library::default_path())
}

// ------------------------------------------------------------------ status

fn status(cfg: &Config) -> Value {
    let state = State::load();
    let lib = library();
    let conn = output::pick(cfg);
    let mut st = json!({
        "config": Config::path(),
        "on": state.on,
        "standard": if state.standard.is_empty() { cfg.output.standard.clone() } else { state.standard.clone() },
        "active": false,
        "connector": Value::Null,
        "mode": Value::Null,
        "dac": { "present": false },
        "audio": Value::Null,
        "shell": Value::Null,
        "playing": launcher::playing(),
        "bios": Value::Null,
        "library": Value::Null,
    });
    if let Some(c) = &conn {
        let leased = display::leaseable(&c.name);
        st["connector"] = json!({
            "drm": c.drm, "name": c.name, "connected": c.connected,
            "edid_name": c.edid_name, "edid_audio": c.edid_audio, "rgbpi2": c.is_rgbpi2(),
            "leaseable": leased, "display": display::running(),
        });
        if leased {
            if display::running() {
                let standard = st["standard"].as_str().unwrap_or("ntsc").to_string();
                if let Some(ml) = cfg.modeline(&standard).and_then(Modeline::parse) {
                    let ml = if state.lines > 0 && state.lines != ml.height() {
                        ml.with_lines(state.lines)
                    } else {
                        ml
                    };
                    st["active"] = json!(true);
                    st["mode"] = json!({
                        "width": ml.width(), "height": ml.height(), "refresh_hz": ml.vfreq_hz(), "disabled": false,
                        "hfreq_khz": (ml.hfreq_khz() * 1000.0).round() / 1000.0,
                        "vfreq_hz": (ml.vfreq_hz() * 1000.0).round() / 1000.0,
                        "lines": format!("{}p", ml.height()),
                    });
                }
            }
        } else if let Some(m) = output::hypr_monitor(&c.name) {
            let w = m["width"].as_u64().unwrap_or(0) as u32;
            let h = m["height"].as_u64().unwrap_or(0) as u32;
            let disabled = m["disabled"].as_bool().unwrap_or(false);
            let mut mode = json!({ "width": w, "height": h, "refresh_hz": m["refreshRate"], "disabled": disabled });
            // ntsc and film share 3520x240; prefer the standard the state
            // records so the label matches what was applied, not whichever
            // modeline happens to be checked last.
            let saved = if state.standard.is_empty() {
                cfg.output.standard.as_str()
            } else {
                state.standard.as_str()
            };
            let order: [&str; 3] = match saved {
                "film" => ["film", "ntsc", "pal"],
                "pal" => ["pal", "ntsc", "film"],
                _ => ["ntsc", "film", "pal"],
            };
            for std in order {
                if st["active"] == json!(true) {
                    break;
                }
                if let Some(ml) = cfg.modeline(std).and_then(Modeline::parse)
                    && !disabled
                    && ml.width() == w
                    && (h == ml.height() || h == state.lines)
                {
                    st["active"] = json!(true);
                    st["standard"] = json!(std);
                    let ml = if h == ml.height() {
                        ml
                    } else {
                        ml.with_lines(h)
                    };
                    mode["hfreq_khz"] = json!((ml.hfreq_khz() * 1000.0).round() / 1000.0);
                    mode["vfreq_hz"] = json!((ml.vfreq_hz() * 1000.0).round() / 1000.0);
                    mode["lines"] = json!(format!("{h}p"));
                }
            }
            st["mode"] = mode;
        }
        if c.is_rgbpi2() && c.connected {
            st["dac"] = match open_dac(c) {
                Ok(d) => {
                    let lock = d
                        .lock()
                        .map(|l| l.label())
                        .unwrap_or_else(|e| e.to_string());
                    let cs = d
                        .csync()
                        .map(|v| {
                            Csync::from_value(v)
                                .map(|c| c.label().to_string())
                                .unwrap_or(format!("0x{v:02X}"))
                        })
                        .unwrap_or_default();
                    json!({ "present": true, "bus": d.bus, "lock": lock, "csync": cs })
                }
                Err(e) => json!({ "present": false, "error": e }),
            };
        }
        if let Some(t) = audio::target(c) {
            let prof = audio::active_profile(&t.card);
            st["audio"] = json!({
                "card": t.card, "profile": t.profile, "sink": t.sink, "pin": t.pin,
                "routed": prof.as_deref() == Some(t.profile.as_str()),
                "default": audio::default_sink().as_deref() == Some(t.sink.as_str()),
                "volume": cfg.audio.volume,
            });
        }
    }
    let pids = launcher::pids();
    st["shell"] = json!({
        "running": !pids.is_empty(),
        "pid": pids.first(),
        "binary": launcher::binary(cfg),
    });
    let names: Vec<String> = lib
        .systems
        .iter()
        .filter(|s| !s.is_video() && library::expand(&s.dir).is_dir())
        .map(|s| s.name.clone())
        .collect();
    let rep = bios::report(&names);
    st["bios"] = json!({
        "missing": rep.missing().len(),
        "required": rep.relevant_required(),
        "system_dir": rep.system_dir,
        "missing_files": rep.missing().iter().map(|i| i.file.clone()).collect::<Vec<_>>(),
    });
    let scan = roms::scan(&lib);
    st["library"] = json!({
        "systems": scan.iter().filter(|s| s.exists && s.games > 0).count(),
        "games": scan.iter().filter(|s| s.name != "videos").map(|s| s.games).sum::<usize>(),
        "videos": scan.iter().filter(|s| s.name == "videos").map(|s| s.games).sum::<usize>(),
        "missing_cores": scan.iter().filter(|s| s.exists && !s.core_present).map(|s| s.name.clone()).collect::<Vec<_>>(),
    });
    st
}

fn print_status(st: &Value) {
    match &st["connector"] {
        Value::Null => println!("CRT output: none found (connect the DAC or set output.connector)"),
        c => println!(
            "CRT output: {}  {}  EDID '{}'{}{}",
            c["name"].as_str().unwrap_or(""),
            if c["connected"].as_bool().unwrap_or(false) {
                "connected"
            } else {
                "disconnected"
            },
            c["edid_name"].as_str().unwrap_or(""),
            if c["edid_audio"].as_bool().unwrap_or(false) {
                "  audio"
            } else {
                ""
            },
            if c["rgbpi2"].as_bool().unwrap_or(false) {
                "  RGB-Pi 2"
            } else {
                ""
            },
        ),
    }
    if let Value::Object(m) = &st["mode"] {
        let extra = if st["active"].as_bool().unwrap_or(false) {
            format!(
                "  {} kHz  {} Hz  {}",
                m["hfreq_khz"],
                m["vfreq_hz"],
                st["standard"].as_str().unwrap_or("").to_uppercase()
            )
        } else if m["disabled"].as_bool().unwrap_or(false) {
            "  disabled".into()
        } else {
            "  not a 15 kHz mode".into()
        };
        println!(
            "Mode:       {}x{} @ {:.3} Hz{extra}",
            m["width"],
            m["height"],
            m["refresh_hz"].as_f64().unwrap_or(0.0)
        );
    }
    let d = &st["dac"];
    if d["present"].as_bool().unwrap_or(false) {
        println!(
            "DAC:        {}, csync {} ({})",
            d["lock"].as_str().unwrap_or(""),
            d["csync"].as_str().unwrap_or(""),
            d["bus"].as_str().unwrap_or("")
        );
    } else if st["connector"]["rgbpi2"].as_bool().unwrap_or(false) {
        println!(
            "DAC:        not reachable ({})",
            d["error"].as_str().unwrap_or("no answer")
        );
    }
    if let Value::Object(a) = &st["audio"] {
        println!(
            "Audio:      {}  {}{}",
            if a["routed"].as_bool().unwrap_or(false) {
                "to the TV"
            } else {
                "desktop"
            },
            a["profile"].as_str().unwrap_or(""),
            if a["default"].as_bool().unwrap_or(false) {
                "  default sink"
            } else {
                ""
            }
        );
    }
    let s = &st["shell"];
    println!(
        "Launcher:   {}{}",
        if s["running"].as_bool().unwrap_or(false) {
            format!("running pid {}", s["pid"])
        } else {
            "stopped".into()
        },
        if s["binary"].is_null() {
            "  (binary not found)"
        } else {
            ""
        }
    );
    if let Some(p) = st["playing"].as_str() {
        println!("Playing:    {p}");
    }
    let l = &st["library"];
    println!(
        "Library:    {} games in {} systems, {} videos",
        l["games"], l["systems"], l["videos"]
    );
    if let Some(mc) = l["missing_cores"].as_array().filter(|a| !a.is_empty()) {
        let names: Vec<&str> = mc.iter().filter_map(|v| v.as_str()).collect();
        println!("Cores:      missing for {}", names.join(", "));
    }
    let b = &st["bios"];
    println!(
        "BIOS:       {} missing of {} required ({})",
        b["missing"],
        b["required"],
        b["system_dir"].as_str().unwrap_or("")
    );
}

// ------------------------------------------------------------------ actions

fn apply_mode(
    cfg: &Config,
    conn: &Connector,
    standard: &str,
    lines: Option<u32>,
    shift: (i32, i32),
) -> Result<Modeline, String> {
    let base = cfg
        .modeline(standard)
        .and_then(Modeline::parse)
        .ok_or_else(|| format!("no modeline for {standard}"))?;
    let ml = match lines {
        Some(l) if l != base.height() => base.with_lines(l),
        _ => base,
    };
    // The TV profile's shift is global; the caller adds the system's own.
    // Shifts are in launcher pixels (320 wide), scaled to the mode's width.
    let profile = omarchy_crt_shell::profile::Profile::load(&omarchy_crt_shell::crt::config_dir());
    let sx = (ml.width() as f64 / 320.0).max(1.0);
    let dx = ((profile.h_shift + shift.0) as f64 * sx).round() as i32;
    let dy = profile.v_shift + shift.1;
    let ml = if dx != 0 || dy != 0 {
        ml.shifted(dx, dy)
    } else {
        ml
    };
    let (ok, out) = output::apply_modeline(&conn.name, &ml, &cfg.output.position);
    if !ok {
        return Err(format!("modeline refused: {out}"));
    }
    let mut state = State::load();
    state.standard = standard.into();
    state.lines = ml.height();
    state.shift_x = shift.0;
    state.shift_y = shift.1;
    state.save();
    Ok(ml)
}

fn set_csync(cfg: &Config, conn: &Connector) -> Result<String, String> {
    let mode =
        Csync::parse(&cfg.output.csync).ok_or("output.csync must be and, xor or separate")?;
    let dac = open_dac(conn)?;
    // The DAC re-initialises when the input signal appears and may ignore or
    // scramble the first write, so write, read back and retry for a while.
    let mut last = None;
    for _ in 0..8 {
        dac.set_csync(mode).map_err(|e| e.to_string())?;
        std::thread::sleep(std::time::Duration::from_millis(300));
        last = dac.csync().ok();
        if last == Some(mode.value()) {
            return Ok(mode.label().into());
        }
    }
    Err(format!(
        "csync {} not accepted, register reads {}",
        mode.label(),
        last.map(|v| format!("0x{v:02X}"))
            .unwrap_or("nothing".into())
    ))
}

/// Hyprland options that let RetroArch and mpv take the tube while they run
/// and hand it back to the launcher when they exit.
fn compositor_fullscreen_policy(on: bool) {
    // 0: a window that opens under a fullscreen one stays behind it. The
    // launcher keeps the tube while RetroArch starts; the launcher then moves
    // RetroArch to the game workspace and nothing else is ever composited.
    let code = if on {
        "hl.config({ misc = { on_focus_under_fullscreen = 0, exit_window_retains_fullscreen = true } })"
    } else {
        "hl.config({ misc = { on_focus_under_fullscreen = 0, exit_window_retains_fullscreen = false } })"
    };
    output::hypr_eval(code);
}

fn cmd_on(cfg: &Config, standard: Option<&str>) {
    let conn = connector(cfg);
    let state = State::load();
    let standard = standard.map(str::to_string).unwrap_or_else(|| {
        if state.standard.is_empty() {
            cfg.output.standard.clone()
        } else {
            state.standard.clone()
        }
    });
    if display::leaseable(&conn.name) {
        cmd_on_leased(cfg, &conn, &standard);
        return;
    }
    // Rules first: the output must come up already bound to the `crt`
    // workspace, or Hyprland hands it the next free numbered desktop one.
    compositor_fullscreen_policy(true);
    output::workspace_rule(&conn.name);
    output::window_rules(&conn.name);
    output::isolate(&conn.name);
    match apply_mode(cfg, &conn, &standard, None, (0, 0)) {
        Ok(ml) => println!(
            "mode:       {} {}x{} {:.2} kHz {:.2} Hz",
            standard.to_uppercase(),
            ml.width(),
            ml.height(),
            ml.hfreq_khz(),
            ml.vfreq_hz()
        ),
        Err(e) => die(&e),
    }
    std::thread::sleep(std::time::Duration::from_millis(1000));
    match set_csync(cfg, &conn) {
        Ok(m) => println!("dac:        csync {m}"),
        Err(e) => println!("dac:        {e}"),
    }
    if cfg.audio.route {
        match audio::target(&conn) {
            Some(t) => {
                let mut state = State::load();
                let note =
                    audio::route_to_crt(&t, cfg.audio.volume, cfg.audio.system_default, &mut state);
                state.save();
                println!("audio:      {note}");
            }
            None => println!("audio:      no HDMI audio pin for this output"),
        }
    }
    match launcher::start(cfg, &conn.name, crt_sink(cfg, &conn).as_deref()) {
        Ok(note) => {
            println!("launcher:   {note}");
            launcher::focus();
        }
        Err(e) => println!("launcher:   {e}"),
    }
    let mut state = State::load();
    state.on = true;
    state.save();
}

/// The tube is ours: the display process leases the connector, sets the
/// timing and hosts the launcher and the programs as Wayland clients. No
/// compositor rules, no workspace, nothing else on the output.
fn cmd_on_leased(cfg: &Config, conn: &Connector, standard: &str) {
    let mut state = State::load();
    state.standard = standard.into();
    state.lines = 0;
    state.save();
    match display::start_with_sink(&conn.name, crt_sink(cfg, conn).as_deref()) {
        Ok(note) => println!("display:    {note}"),
        Err(e) => die(&format!("display: {e}")),
    }
    if let Some(text) = cfg.modeline(standard)
        && let Some(ml) = Modeline::parse(text)
    {
        println!(
            "mode:       {} {}x{} {:.2} kHz {:.2} Hz",
            standard.to_uppercase(),
            ml.width(),
            ml.height(),
            ml.hfreq_khz(),
            ml.vfreq_hz()
        );
        display::mode(text);
    }
    match set_csync(cfg, conn) {
        Ok(m) => println!("dac:        csync {m}"),
        Err(e) => println!("dac:        {e}"),
    }
    if cfg.audio.route {
        // The HDMI audio pin exists only while a mode is up; give PipeWire a
        // moment to notice the sink.
        let mut routed = false;
        for _ in 0..30 {
            if let Some(t) = audio::target(conn) {
                let mut state = State::load();
                let note =
                    audio::route_to_crt(&t, cfg.audio.volume, cfg.audio.system_default, &mut state);
                state.save();
                println!("audio:      {note}");
                routed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        if !routed {
            println!("audio:      no HDMI audio pin for this output");
        }
    }
    match launcher::start(cfg, &conn.name, crt_sink(cfg, conn).as_deref()) {
        Ok(note) => println!("launcher:   {note}"),
        Err(e) => println!("launcher:   {e}"),
    }
    let mut state = State::load();
    state.on = true;
    state.save();
    match watchdog::start() {
        Ok(()) => println!("watchdog:   up"),
        Err(e) => println!("watchdog:   {e}"),
    }
}

/// The watchdog process: `omarchy-crt watchdog`. Started by `on` when the
/// tube is leased, it puts the display back when it dies with the state still
/// saying the television is on. See `crt::watchdog`.
fn cmd_watchdog(cfg: &Config) -> i32 {
    watchdog::claim();
    let t0 = std::time::Instant::now();
    let now = || t0.elapsed().as_secs_f64();
    let mut budget = watchdog::Budget::default();
    // Only what was up when the display died is put back: quitting the
    // launcher for the desktop leaves the tube on and must stay that way.
    let mut had_launcher = !launcher::pids().is_empty();
    let mut was_up = display::running();
    loop {
        std::thread::sleep(watchdog::TICK);
        // A newer `on` has started its own watchdog, or `off` has cleared us.
        if !watchdog::still_ours() {
            return 0;
        }
        if !State::load().on {
            eprintln!("the tube is off, standing down");
            let _ = std::fs::remove_file(watchdog::pid_path());
            return 0;
        }
        if display::running() {
            was_up = true;
            had_launcher = !launcher::pids().is_empty();
            continue;
        }
        if !was_up {
            continue;
        }
        was_up = false;
        eprintln!("the display process is gone, putting the tube back");
        if !budget.spend(now()) {
            eprintln!(
                "{} restarts inside {:.0} seconds: giving up, read {}",
                watchdog::MAX_RESTARTS,
                watchdog::WINDOW_SECS,
                display::log_path().display()
            );
            let _ = std::fs::remove_file(watchdog::pid_path());
            return 1;
        }
        let Some(conn) = output::pick(cfg) else {
            eprintln!("no CRT output to restart on");
            continue;
        };
        let state = State::load();
        let standard = if state.standard.is_empty() {
            cfg.output.standard.clone()
        } else {
            state.standard.clone()
        };
        match display::start_with_sink(&conn.name, crt_sink(cfg, &conn).as_deref()) {
            Ok(note) => eprintln!("display: {note}"),
            Err(e) => {
                eprintln!("display: {e}");
                continue;
            }
        }
        if let Some(text) = cfg.modeline(&standard) {
            display::mode(text);
        }
        match set_csync(cfg, &conn) {
            Ok(m) => eprintln!("dac: csync {m}"),
            Err(e) => eprintln!("dac: {e}"),
        }
        if had_launcher {
            match launcher::start(cfg, &conn.name, crt_sink(cfg, &conn).as_deref()) {
                Ok(note) => eprintln!("launcher: {note}"),
                Err(e) => eprintln!("launcher: {e}"),
            }
        }
        was_up = display::running();
    }
}

fn cmd_off(cfg: &Config) {
    // Before anything else: a deliberate shutdown must not look like a crash.
    watchdog::stop();
    println!("launcher:   {}", launcher::stop());
    let mut state = State::load();
    if cfg.audio.route {
        println!("audio:      {}", audio::route_back(&mut state));
    }
    if display::running() {
        println!("display:    {}", display::stop());
        state.on = false;
        state.save();
        return;
    }
    compositor_fullscreen_policy(false);
    output::unisolate();
    if let Some(conn) = output::pick(cfg) {
        output::disable(&conn.name);
        println!("output:     {} disabled", conn.name);
    }
    state.on = false;
    state.save();
}

/// Login time reset: the compositor may have lit the CRT output with its
/// fallback mode and PipeWire remembers the CRT sink as default. Put the
/// desktop back to normal without touching the launcher config.
fn cmd_boot(cfg: &Config) {
    let mut state = State::load();
    if cfg.audio.route && (!state.previous_sink.is_empty() || state.on) {
        println!("audio:      {}", audio::route_back(&mut state));
    } else if let Some(conn) = output::pick(cfg) {
        // Nothing saved but the CRT sink may still be the default from an
        // unclean shutdown: fall back to any non CRT sink.
        if let Some(t) = audio::target(&conn)
            && audio::default_sink().as_deref() == Some(t.sink.as_str())
            && let Some(other) = audio::other_sink(&t.sink)
        {
            omarchy_crt_shell::crt::run("pactl", &["set-default-sink", &other]);
            println!("audio:      {other}");
        }
    }
    if let Some(conn) = output::pick(cfg) {
        let lit = output::hypr_monitor(&conn.name)
            .map(|m| !m["disabled"].as_bool().unwrap_or(true))
            .unwrap_or(false);
        if lit {
            output::disable(&conn.name);
            println!("output:     {} disabled until `omarchy-crt on`", conn.name);
        }
    }
    state.on = false;
    state.save();
    if cfg.shell.autostart && output::pick(cfg).is_some_and(|c| c.connected) {
        println!("autostart: the DAC is connected, switching the tube on");
        cmd_on(cfg, None);
    }
}

fn cmd_doctor(cfg: &Config) -> i32 {
    let mut rows: Vec<(String, bool, String)> = Vec::new();
    let conn = output::pick(cfg);
    rows.push((
        "CRT connector found".into(),
        conn.is_some(),
        conn.as_ref()
            .map(|c| c.drm.clone())
            .unwrap_or("set output.connector".into()),
    ));
    if let Some(c) = &conn {
        rows.push((
            "EDID readable".into(),
            !c.edid_name.is_empty(),
            if c.edid_name.is_empty() {
                "empty EDID".into()
            } else {
                c.edid_name.clone()
            },
        ));
        rows.push((
            "EDID advertises audio".into(),
            c.edid_audio,
            if c.edid_audio {
                "HDMI audio pin available".into()
            } else {
                "no audio over this DAC".into()
            },
        ));
        let bus = Dac::bus_of(&c.path).unwrap_or_default();
        let writable = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&bus)
            .is_ok();
        rows.push((
            "I2C bus writable".into(),
            writable,
            if writable {
                bus.clone()
            } else {
                format!("{bus}: join the i2c group")
            },
        ));
        let dac = open_dac(c);
        rows.push((
            "RGB-Pi 2 answers at 0x78".into(),
            dac.is_ok(),
            dac.as_ref()
                .map(|d| d.lock().map(|l| l.label()).unwrap_or_default())
                .unwrap_or_else(|e| e.clone()),
        ));
        let t = audio::target(c);
        rows.push((
            "HDMI audio pin matched".into(),
            t.is_some(),
            t.map(|t| t.profile)
                .unwrap_or("no ELD pin with this EDID name".into()),
        ));
    }
    for (label, cmd) in [
        ("hyprctl", "hyprctl"),
        ("pactl", "pactl"),
        ("retroarch", "retroarch"),
        ("mpv", "mpv"),
        ("ffmpeg", "ffmpeg"),
        ("curl", "curl"),
        ("yt-dlp", "yt-dlp"),
        ("cliamp", "cliamp"),
    ] {
        let found = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).any(|d| d.join(cmd).is_file()))
            .unwrap_or(false);
        rows.push((
            format!("{label} installed"),
            found,
            if found {
                "ok".into()
            } else {
                format!("pacman -S {cmd}")
            },
        ));
    }
    // The piece that makes leasing possible at all: without the boot time
    // unit the connector stays a desktop monitor and nothing else works.
    let unit = std::path::Path::new("/etc/systemd/system/omarchy-crt-lease.service");
    let unit_enabled = std::process::Command::new("systemctl")
        .args(["is-enabled", "--quiet", "omarchy-crt-lease.service"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    rows.push((
        "lease unit installed".into(),
        unit.is_file(),
        if unit.is_file() {
            "/etc/systemd/system/omarchy-crt-lease.service".into()
        } else {
            "sudo bin/omarchy-crt-install --system".into()
        },
    ));
    rows.push((
        "lease unit enabled".into(),
        unit_enabled,
        if unit_enabled {
            "runs at boot".into()
        } else {
            "sudo systemctl enable --now omarchy-crt-lease.service".into()
        },
    ));
    // The bar plugin, and whether the shell has been told about it.
    let plugin_dir = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default())
        .join(".config/omarchy/plugins/io.github.stefanomainardi.omarchy-crt");
    rows.push((
        "bar plugin installed".into(),
        plugin_dir.join("manifest.json").is_file(),
        if plugin_dir.join("manifest.json").is_file() {
            plugin_dir.display().to_string()
        } else {
            "bin/omarchy-crt-install (unlocked session)".into()
        },
    ));
    // Somewhere to write: a read only config directory fails much later,
    // in the middle of saving something the player cares about.
    for (label, dir) in [
        ("config directory writable", crt::config_dir()),
        ("state directory writable", crt::state_dir()),
    ] {
        let ok = std::fs::create_dir_all(&dir).is_ok()
            && omarchy_crt_shell::store::save(&dir.join(".write-test"), b"ok").is_ok();
        let _ = std::fs::remove_file(dir.join(".write-test"));
        let _ = std::fs::remove_file(dir.join(".write-test.bak"));
        rows.push((
            label.into(),
            ok,
            if ok {
                dir.display().to_string()
            } else {
                format!("cannot write {}", dir.display())
            },
        ));
    }
    let bin = launcher::binary(cfg);
    rows.push((
        "launcher binary".into(),
        bin.is_some(),
        bin.map(|b| b.display().to_string())
            .unwrap_or(cfg.shell.bin.clone()),
    ));
    let lib = library();
    let scan = roms::scan(&lib);
    let with_games = scan
        .iter()
        .filter(|s| s.games > 0 && s.name != "videos")
        .count();
    rows.push((
        "ROM folders with games".into(),
        with_games > 0,
        format!("{with_games} systems"),
    ));
    let missing: Vec<&str> = scan
        .iter()
        .filter(|s| s.exists && !s.core_present)
        .map(|s| s.name.as_str())
        .collect();
    rows.push((
        "cores for every folder".into(),
        missing.is_empty(),
        if missing.is_empty() {
            "ok".into()
        } else {
            missing.join(", ")
        },
    ));
    let names: Vec<String> = lib
        .systems
        .iter()
        .filter(|s| !s.is_video() && library::expand(&s.dir).is_dir())
        .map(|s| s.name.clone())
        .collect();
    let rep = bios::report(&names);
    let miss = rep.missing();
    rows.push((
        "BIOS files".into(),
        miss.is_empty(),
        if miss.is_empty() {
            "all required present".into()
        } else {
            format!("{} missing", miss.len())
        },
    ));
    let width = rows.iter().map(|r| r.0.len()).max().unwrap_or(10);
    let mut bad = 0;
    for (label, ok, note) in &rows {
        if !ok {
            bad += 1;
        }
        println!(
            "{} {:<width$}  {}",
            if *ok { "OK  " } else { "FAIL" },
            label,
            note
        );
    }
    if bad == 0 { 0 } else { 1 }
}

fn cmd_bios(args: &[String]) {
    let lib = library();
    let names: Vec<String> = lib
        .systems
        .iter()
        .filter(|s| !s.is_video() && library::expand(&s.dir).is_dir())
        .map(|s| s.name.clone())
        .collect();
    let pos = positional(args);
    if pos.first().map(|s| s.as_str()) == Some("discover") {
        use omarchy_crt_shell::index::{self, LibraryConfig};
        let mut places = LibraryConfig::load().roots;
        for d in index::discover() {
            if !places.contains(&d) {
                places.push(d);
            }
        }
        let found = bios::discover(&places);
        if has(args, "--json") {
            let rows: Vec<Value> = found
                .iter()
                .map(|(p, n)| json!({ "path": p, "files": n }))
                .collect();
            println!("{}", Value::Array(rows));
            return;
        }
        if found.is_empty() {
            println!("no folder with known BIOS files under the roots or the mounted disks");
        }
        for (p, n) in &found {
            println!("{:>4} known file(s)  {}", n, p.display());
        }
        return;
    }
    if pos.first().map(|s| s.as_str()) == Some("import") {
        let Some(dir) = pos.get(1) else {
            die("bios import needs a directory")
        };
        let src = PathBuf::from(dir);
        if !src.is_dir() {
            die(&format!("{dir} is not a directory"));
        }
        match bios::import(&src, has(args, "--all")) {
            Ok((copied, skipped)) => println!(
                "{copied} file(s) copied into {}, {skipped} already there",
                bios::system_dir().display()
            ),
            Err(e) => die(&e.to_string()),
        }
        return;
    }
    let rep = bios::report(&names);
    if has(args, "--json") {
        let items: Vec<Value> = rep
            .items
            .iter()
            .map(|i| json!({ "system": i.system, "file": i.file, "required": i.required, "description": i.description, "present": i.present, "relevant": i.relevant }))
            .collect();
        println!(
            "{}",
            json!({ "system_dir": rep.system_dir, "missing": rep.missing().len(), "items": items })
        );
        return;
    }
    println!("RetroArch system directory: {}", rep.system_dir.display());
    for i in &rep.items {
        let mark = if i.present {
            "OK  "
        } else if i.required {
            "MISS"
        } else {
            "opt "
        };
        let rel = if i.relevant { "" } else { "  (no ROM folder)" };
        println!(
            "{mark} {:<12} {:<34} {}{rel}",
            i.system, i.file, i.description
        );
    }
    println!(
        "{} required file(s) missing for the systems you have.",
        rep.missing().len()
    );
}

fn cmd_library(args: &[String]) {
    use omarchy_crt_shell::index::{self, Index, LibraryConfig};
    let pos = positional(args);
    match pos.first().map(|s| s.as_str()) {
        Some("scan") => {
            let mut lc = LibraryConfig::load();
            let given: Vec<PathBuf> = pos[1..].iter().map(PathBuf::from).collect();
            for g in &given {
                if !g.is_dir() {
                    die(&format!("{} is not a directory", g.display()));
                }
                let canon = std::fs::canonicalize(g).unwrap_or(g.clone());
                if !lc.roots.contains(&canon) {
                    lc.roots.push(canon);
                }
            }
            if lc.roots.is_empty() {
                let found = index::discover();
                if found.is_empty() {
                    die("nothing to scan: pass a folder, e.g. omarchy-crt library scan ~/Games");
                }
                println!("no roots configured, using what looks like a collection:");
                for f in &found {
                    println!("  {}", f.display());
                }
                lc.roots = found;
            }
            lc.save().unwrap_or_else(|e| die(&e.to_string()));
            let quiet = has(args, "--quiet");
            // --progress: one plain line per folder on stdout, for a caller
            // that shows the progress itself (the library overlay does).
            let progress = has(args, "--progress");
            let started = std::time::Instant::now();
            let mut last = String::new();
            let ix = index::scan(&lc.roots, &lc.hints(), |dir| {
                if dir == last {
                    return;
                }
                last = dir.to_string();
                if progress {
                    println!("scanning {dir}");
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                } else if !quiet {
                    eprint!("\r\x1b[2K  {dir}");
                }
            });
            if !quiet && !progress {
                eprint!("\r\x1b[2K");
            }
            ix.save().unwrap_or_else(|e| die(&e.to_string()));
            let secs = started.elapsed().as_secs_f32();
            println!(
                "{} games in {} systems, {:.1} s",
                ix.items.len(),
                ix.systems().len(),
                secs
            );
            for (system, n) in ix.systems() {
                let label = index::catalog(&system)
                    .map(|(l, _, _)| l)
                    .unwrap_or("unknown to the catalogue");
                println!("  {:<12} {:>6}  {}", system, n, label);
            }
            if !ix.unknown.is_empty() {
                println!(
                    "{} file(s) with no system; folders involved:",
                    ix.unknown.len()
                );
                let mut dirs: std::collections::BTreeMap<PathBuf, usize> = Default::default();
                for u in &ix.unknown {
                    if let Some(d) = u.parent() {
                        *dirs.entry(d.to_path_buf()).or_default() += 1;
                    }
                }
                for (d, n) in dirs.iter().take(12) {
                    println!("  {:>5}  {}", n, d.display());
                }
                println!("assign one with: omarchy-crt library assign <folder> <system>");
            }
        }
        Some("discover") => {
            let roots = LibraryConfig::load().roots;
            let found = index::discover();
            if has(args, "--json") {
                let rows: Vec<Value> = found
                    .iter()
                    .map(|f| json!({ "path": f, "root": roots.contains(f) }))
                    .collect();
                println!("{}", Value::Array(rows));
                return;
            }
            for f in found {
                let note = if roots.contains(&f) {
                    "  (a root already)"
                } else {
                    ""
                };
                println!("{}{note}", f.display());
            }
        }
        Some("set") => {
            let (Some(system), Some(assign)) = (pos.get(1), pos.get(2)) else {
                die("library set needs a system and key=value, e.g. library set arcade core=fbneo");
            };
            let Some((key, value)) = assign.split_once('=') else {
                die("library set needs key=value (core=... or dir=...)");
            };
            library::set_system_field(system, key, value).unwrap_or_else(|e| die(&e));
            println!("{system}: {key} = {value}");
        }
        Some("covers") => {
            // Box art for the whole collection, exact names first, fuzzy after.
            use omarchy_crt_shell::covers;
            let lib = library();
            let settings = omarchy_crt_shell::settings::Settings::load(&lib.config_dir);
            let regions = covers::regions_for(&settings.music.country);
            let wanted: Vec<&String> = pos
                .iter()
                .skip(1)
                .filter(|s| !s.starts_with("--"))
                .copied()
                .collect();
            let limit: usize = args
                .iter()
                .position(|a| a == "--limit")
                .and_then(|i| args.get(i + 1))
                .and_then(|v| v.parse().ok())
                .unwrap_or(usize::MAX);
            let force = has(args, "--force");
            for system in lib.systems.iter().filter(|s| !s.is_video()) {
                if !wanted.is_empty() && !wanted.iter().any(|w| **w == system.name) {
                    continue;
                }
                let Some(label) = covers::label(&system.name) else {
                    continue;
                };
                let games = lib.games(system);
                if games.is_empty() {
                    continue;
                }
                let Some(index) = covers::NameIndex::load(label) else {
                    println!("{:<12} no thumbnail index reachable", system.name);
                    continue;
                };
                let (mut have, mut exact, mut fuzzy, mut none) = (0, 0, 0, 0);
                let mut done = 0;
                for g in &games {
                    if done >= limit {
                        break;
                    }
                    let stem = g.path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    let dest = covers::cache_path(&system.name, stem);
                    if dest.exists() && !force {
                        have += 1;
                        continue;
                    }
                    done += 1;
                    eprint!("\r\x1b[2K  {} {}", system.name, stem);
                    // Arcade files are named after the set, the repository by
                    // title: the databases RetroArch ships pair them.
                    let stem = covers::title_for(&system.name, stem);
                    match index.best(&stem, &regions) {
                        Some(name) => {
                            if covers::download(label, name, &dest) {
                                if name == covers::thumb_name(&stem) {
                                    exact += 1;
                                } else {
                                    fuzzy += 1;
                                }
                            } else {
                                none += 1;
                            }
                        }
                        None => {
                            let _ = std::fs::write(dest.with_extension("missing"), b"");
                            none += 1;
                        }
                    }
                }
                eprint!("\r\x1b[2K");
                println!(
                    "{:<12} {:>5} games: {have} had art, {exact} exact, {fuzzy} matched by title, {none} without",
                    system.name,
                    games.len()
                );
            }
        }
        Some("cores") => {
            let lib = library();
            let available = library::installed_cores(&lib.core_dir);
            let mut rows: Vec<Value> = Vec::new();
            for s in lib.systems.iter().filter(|s| !s.is_video()) {
                let path = lib.core_path(s);
                let (package, aur) = index::core_package(&s.core);
                let label = index::catalog(&s.name)
                    .map(|(l, _, _)| l.to_string())
                    .unwrap_or_else(|| s.name.clone());
                rows.push(json!({
                    "system": s.name, "label": label, "core": s.core,
                    "installed": path.is_file(), "path": path, "package": package, "aur": aur,
                }));
            }
            if has(args, "--json") {
                println!("{}", json!({ "systems": rows, "available": available }));
                return;
            }
            for r in &rows {
                let installed = r["installed"].as_bool().unwrap_or(false);
                println!(
                    "{:<4} {:<12} {:<20} {}{}",
                    if installed { "OK" } else { "MISS" },
                    r["system"].as_str().unwrap_or(""),
                    r["core"].as_str().unwrap_or(""),
                    r["package"].as_str().unwrap_or(""),
                    if r["aur"].as_bool().unwrap_or(false) {
                        "  (AUR)"
                    } else {
                        ""
                    },
                );
            }
        }
        Some("roots") => {
            let mut lc = LibraryConfig::load();
            match (pos.get(1).map(|s| s.as_str()), pos.get(2)) {
                (Some("add"), Some(d)) => {
                    let p =
                        std::fs::canonicalize(d).unwrap_or_else(|_| die(&format!("{d} not found")));
                    if !lc.roots.contains(&p) {
                        lc.roots.push(p);
                    }
                    lc.save().unwrap_or_else(|e| die(&e.to_string()));
                }
                (Some("remove"), Some(d)) => {
                    let p = std::fs::canonicalize(d).unwrap_or(PathBuf::from(d));
                    lc.roots.retain(|r| *r != p && r != Path::new(d));
                    lc.save().unwrap_or_else(|e| die(&e.to_string()));
                }
                _ => {}
            }
            for r in &lc.roots {
                println!(
                    "{}{}",
                    r.display(),
                    if r.is_dir() { "" } else { "  (not mounted)" }
                );
            }
        }
        Some("assign") => {
            let (Some(path), Some(system)) = (pos.get(1), pos.get(2)) else {
                die("library assign needs a folder (or file) and a system name");
            };
            if index::catalog(system).is_none() {
                eprintln!("note: {system} is not in the catalogue, it will list without a core");
            }
            let mut lc = LibraryConfig::load();
            let p = std::fs::canonicalize(path).unwrap_or(PathBuf::from(path));
            lc.hints.insert(p.display().to_string(), system.to_string());
            lc.save().unwrap_or_else(|e| die(&e.to_string()));
            println!(
                "{} -> {system}; run `omarchy-crt library scan` to apply",
                p.display()
            );
        }
        Some("unknown") => {
            let Some(ix) = Index::load() else {
                die("no index yet, run omarchy-crt library scan")
            };
            for u in &ix.unknown {
                println!("{}", u.display());
            }
        }
        Some("collections") => {
            let dir = omarchy_crt_shell::crt::config_dir().join("collections");
            match pos.get(1).map(|s| s.as_str()) {
                Some("import") => {
                    let Some(src) = pos.get(2) else {
                        die(
                            "collections import needs a folder of lists (a RePlayOS _favorites folder)",
                        )
                    };
                    let src = PathBuf::from(src);
                    let Some(ix) = Index::load() else {
                        die("no index yet, run omarchy-crt library scan")
                    };
                    std::fs::create_dir_all(&dir).unwrap_or_else(|e| die(&e.to_string()));
                    let mut lists = 0;
                    let mut games = 0;
                    let mut missing = 0;
                    let Ok(rd) = std::fs::read_dir(&src) else {
                        die(&format!("{} is not a directory", src.display()))
                    };
                    let mut folders: Vec<PathBuf> = rd
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| p.is_dir())
                        .collect();
                    folders.sort();
                    for folder in folders {
                        let name = folder
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let mut paths: Vec<String> = Vec::new();
                        let Ok(files) = std::fs::read_dir(&folder) else {
                            continue;
                        };
                        for f in files.flatten() {
                            let fp = f.path();
                            // RePlayOS: `<system>@<title>.fav` holding `/roms/<system>/<file>`.
                            let Ok(text) = std::fs::read_to_string(&fp) else {
                                continue;
                            };
                            let rel = text.trim().trim_start_matches('/');
                            let rel = rel.strip_prefix("roms/").unwrap_or(rel);
                            if rel.is_empty() {
                                continue;
                            }
                            let suffix = format!("/{rel}");
                            match ix
                                .items
                                .iter()
                                .find(|i| i.path.to_string_lossy().ends_with(&suffix))
                            {
                                Some(it) => paths.push(it.path.display().to_string()),
                                None => missing += 1,
                            }
                        }
                        if paths.is_empty() {
                            continue;
                        }
                        paths.sort();
                        paths.dedup();
                        games += paths.len();
                        lists += 1;
                        let pretty = name.replace(['-', '_'], " ");
                        std::fs::write(dir.join(format!("{pretty}.txt")), paths.join("\n") + "\n")
                            .unwrap_or_else(|e| die(&e.to_string()));
                    }
                    println!(
                        "{lists} collection(s), {games} games, {missing} entries not in the index, written to {}",
                        dir.display()
                    );
                }
                _ => {
                    let lib = library();
                    for (name, items) in lib.collections() {
                        println!("{:<32} {:>5}", name, items.len());
                    }
                }
            }
        }
        Some("systems") => {
            for (s, label, core, exts) in index::CATALOG {
                println!("{:<12} {:<28} {:<20} {}", s, label, core, exts.join(","));
            }
        }
        _ => {
            let lib = library();
            let scan = roms::scan(&lib);
            if has(args, "--json") {
                let rows: Vec<Value> = scan
                    .iter()
                    .map(|s| {
                        let sys = lib.systems.iter().find(|x| x.name == s.name);
                        let core = sys.map(|x| x.core.clone()).unwrap_or_default();
                        let (package, aur) = index::core_package(&core);
                        let label = index::catalog(&s.name)
                            .map(|(l, _, _)| l.to_string())
                            .unwrap_or_else(|| s.name.clone());
                        json!({
                            "name": s.name, "label": label, "dir": s.dir, "exists": s.exists,
                            "games": s.games, "unknown": s.unknown, "core": s.core_present,
                            "core_name": core, "package": package, "aur": aur,
                            "video": sys.map(|x| x.is_video()).unwrap_or(false),
                        })
                    })
                    .collect();
                let roots = LibraryConfig::load().roots;
                let out = json!({
                    "systems": rows,
                    "roots": roots.iter().map(|r| json!({ "path": r, "mounted": r.is_dir() })).collect::<Vec<_>>(),
                    "index": Index::load().map(|ix| json!({ "games": ix.items.len(), "scanned_at": ix.scanned_at, "unknown": ix.unknown.len() })).unwrap_or(Value::Null),
                });
                println!("{out}");
                return;
            }
            let ix = Index::load();
            if let Some(ix) = &ix {
                println!("index: {} games, scanned {}", ix.items.len(), ix.scanned_at);
                for r in &ix.roots {
                    println!("  root {}", r.display());
                }
            } else {
                println!("no index: run `omarchy-crt library scan <folder>`");
            }
            for s in &scan {
                let note = if !s.exists {
                    "folder missing".to_string()
                } else if !s.core_present {
                    "core not installed".to_string()
                } else if s.unknown > 0 {
                    format!("{} file(s) with unknown extension", s.unknown)
                } else {
                    String::new()
                };
                println!("{:<14} {:>6}  {}  {}", s.name, s.games, s.dir, note);
            }
        }
    }
}

fn main() {
    // `omarchy-crt library | head` must not panic when the reader goes away.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = argv.first().map(|s| s.as_str()) else {
        println!("{HELP}");
        return;
    };
    let args = &argv[1..];
    let cfg = Config::load();
    match cmd {
        "-h" | "--help" | "help" => println!("{HELP}"),
        "status" => {
            let st = status(&cfg);
            if has(args, "--json") {
                println!("{st}");
            } else {
                print_status(&st);
            }
        }
        "on" => cmd_on(&cfg, positional(args).first().map(|s| s.as_str())),
        "off" => cmd_off(&cfg),
        "watchdog" => exit(cmd_watchdog(&cfg)),
        "boot" => cmd_boot(&cfg),
        "toggle" => {
            if status(&cfg)["active"].as_bool().unwrap_or(false) {
                cmd_off(&cfg)
            } else {
                cmd_on(&cfg, None)
            }
        }
        "mode" => {
            let pos = positional(args);
            let state = State::load();
            let current = if state.standard.is_empty() {
                cfg.output.standard.clone()
            } else {
                state.standard.clone()
            };
            let std = pos.first().map(|s| s.as_str()).unwrap_or(current.as_str());
            if !matches!(std, "ntsc" | "pal" | "film" | "480i" | "576i") {
                die("mode needs ntsc, pal, film, 480i or 576i");
            }
            let flag = |name: &str| -> Option<i32> {
                args.iter()
                    .position(|a| a == name)
                    .and_then(|i| args.get(i + 1))
                    .and_then(|v| v.parse().ok())
            };
            let lines = flag("--lines").map(|v| v.max(0) as u32);
            let shift = (
                flag("--shift-x").unwrap_or(0),
                flag("--shift-y").unwrap_or(0),
            );
            let conn = connector(&cfg);
            if display::leaseable(&conn.name) && display::running() {
                let text = cfg
                    .modeline(std)
                    .unwrap_or_else(|| die("no modeline for that standard"));
                let mut ml = Modeline::parse(text).unwrap_or_else(|| die("bad modeline"));
                if let Some(l) = lines {
                    ml = ml.with_lines(l);
                }
                if shift != (0, 0) {
                    let scale = ml.width() as f32 / 320.0;
                    ml = ml.shifted((shift.0 as f32 * scale) as i32, shift.1);
                }
                display::mode(&ml.to_hypr());
                let mut state = State::load();
                state.standard = std.into();
                state.lines = lines.unwrap_or(0);
                state.shift_x = shift.0;
                state.shift_y = shift.1;
                state.save();
                println!(
                    "{} {}x{} {:.3} kHz {:.3} Hz",
                    std.to_uppercase(),
                    ml.width(),
                    ml.height(),
                    ml.hfreq_khz(),
                    ml.vfreq_hz()
                );
                return;
            }
            match apply_mode(&cfg, &conn, std, lines, shift) {
                Ok(ml) => println!(
                    "{} {}x{} {:.3} kHz {:.3} Hz",
                    std.to_uppercase(),
                    ml.width(),
                    ml.height(),
                    ml.hfreq_khz(),
                    ml.vfreq_hz()
                ),
                Err(e) => die(&e),
            }
            // The DAC keeps its csync selection across modesets; a quick
            // confirming write is enough and the launcher gets focus back.
            let _ = set_csync(&cfg, &conn);
            launcher::focus();
        }
        "monitor" => {
            let on = positional(args)
                .first()
                .map(|s| s.as_str() != "off")
                .unwrap_or(true);
            if !display::running() {
                die("the display process is not running");
            }
            display::send(&format!("monitor {}", if on { "on" } else { "off" }))
                .unwrap_or_else(|e| die(&e.to_string()));
            println!(
                "monitor {}",
                if on {
                    "on: a desktop window shows the tube; focus it to type on the tube"
                } else {
                    "off"
                }
            );
        }
        "record" => {
            if !display::running() {
                die("the display process is not running");
            }
            let sub = positional(args)
                .first()
                .map(|s| s.to_string())
                .unwrap_or_default();
            match sub.as_str() {
                "start" => {
                    let given = positional(args)
                        .get(1)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| die("record start needs a file path"));
                    let path = if given.starts_with('/') {
                        given
                    } else {
                        match std::env::current_dir() {
                            Ok(d) => d.join(&given).to_string_lossy().into_owned(),
                            Err(_) => given,
                        }
                    };
                    let conn = connector(&cfg);
                    let sink = crt_sink(&cfg, &conn);
                    display::record_start(&path, sink.as_deref())
                        .unwrap_or_else(|e| die(&e.to_string()));
                    println!(
                        "recording the tube to {path}{} (omarchy-crt record stop)",
                        sink.as_deref()
                            .map(|s| format!(" with audio from {s}"))
                            .unwrap_or_default()
                    );
                }
                "stop" => {
                    display::record_stop().unwrap_or_else(|e| die(&e.to_string()));
                    println!("recording stopped");
                }
                _ => die("record start <file.mp4> | stop"),
            }
        }
        "shot" => {
            let given: String = positional(args)
                .first()
                .map(|s| s.to_string())
                .unwrap_or_else(|| die("shot needs a file path"));
            let path: String = if given.starts_with('/') {
                given
            } else {
                match std::env::current_dir() {
                    Ok(d) => d.join(&given).to_string_lossy().into_owned(),
                    Err(_) => given,
                }
            };
            if !display::running() {
                die(
                    "the display process is not running (the desktop's own tools see the tube when it is not leased)",
                );
            }
            let _ = std::fs::remove_file(&path);
            display::send(&format!("shot {path}")).unwrap_or_else(|e| die(&e.to_string()));
            for _ in 0..40 {
                std::thread::sleep(std::time::Duration::from_millis(50));
                if std::path::Path::new(&path).exists() {
                    println!("{path}");
                    return;
                }
            }
            die("no screenshot written; see ~/.local/state/omarchy-crt/display.log");
        }
        "game" => {
            use omarchy_crt_shell::game;
            let sub = positional(args)
                .first()
                .map(|s| s.as_str())
                .unwrap_or("status")
                .to_string();
            let r = match sub.as_str() {
                "menu" => game::send("MENU_TOGGLE"),
                "pause" => game::pause_toggle(),
                "save" => game::save_state(),
                "load" => game::load_state(),
                "reset" => game::reset(),
                "quit" => game::quit(),
                // A key pressed inside the game, held as long as asked: the
                // compositor presses it, RetroArch reads its own bindings
                // (start is enter, select rshift, A x, B z).
                "key" => {
                    let pos = positional(args);
                    let name = pos
                        .get(1)
                        .map(|s| s.as_str())
                        .unwrap_or_else(|| die("game key needs a key name or an evdev code"));
                    let line = match pos.get(2) {
                        Some(ms) => format!("key {name} {ms}"),
                        None => format!("key {name}"),
                    };
                    crt::display::send(&line)
                }
                "cmd" => match positional(args).get(1) {
                    Some(c) => game::send(c),
                    None => die("game cmd needs a RetroArch command"),
                },
                _ => {
                    println!(
                        "{}",
                        launcher::playing().unwrap_or("nothing running on the tube")
                    );
                    Ok(())
                }
            };
            if let Err(e) = r {
                die(&format!("game: {e}"));
            }
        }
        "shell" => {
            let sub = positional(args)
                .first()
                .map(|s| s.as_str())
                .unwrap_or("status");
            match sub {
                "start" => {
                    let conn = connector(&cfg);
                    println!(
                        "{}",
                        launcher::start(&cfg, &conn.name, crt_sink(&cfg, &conn).as_deref())
                            .unwrap_or_else(|e| die(&e))
                    );
                    launcher::focus();
                }
                "stop" => println!("{}", launcher::stop()),
                "restart" => {
                    launcher::stop();
                    let conn = connector(&cfg);
                    println!(
                        "{}",
                        launcher::start(&cfg, &conn.name, crt_sink(&cfg, &conn).as_deref())
                            .unwrap_or_else(|e| die(&e))
                    );
                    launcher::focus();
                }
                "focus" => println!("{}", focus_note()),
                "key" => {
                    let names: Vec<&str> = positional(args)
                        .iter()
                        .skip(1)
                        .map(|n| {
                            crt::control::normalize(n).unwrap_or_else(|| {
                                die(&format!(
                                    "unknown input {n}; one of {}",
                                    crt::control::INPUTS.join(", ")
                                ))
                            })
                        })
                        .collect();
                    if names.is_empty() {
                        die("shell key needs at least one input name");
                    }
                    crt::control::send(&names).unwrap_or_else(|e| die(&e.to_string()));
                }
                "type" => {
                    let words: Vec<&str> = positional(args)
                        .iter()
                        .skip(1)
                        .map(|s| s.as_str())
                        .collect();
                    if words.is_empty() {
                        die("shell type needs the text to type");
                    }
                    crt::control::send_text(&words.join(" "))
                        .unwrap_or_else(|e| die(&e.to_string()));
                }
                _ => {
                    let pids = launcher::pids();
                    println!(
                        "{}",
                        if pids.is_empty() {
                            "stopped".to_string()
                        } else {
                            format!("running pid {}", pids[0])
                        }
                    );
                }
            }
        }
        "focus" => println!("{}", focus_note()),
        "audio" => {
            let conn = connector(&cfg);
            let target =
                audio::target(&conn).unwrap_or_else(|| die("no HDMI audio pin for this output"));
            let mut state = State::load();
            match positional(args).first().map(|s| s.as_str()) {
                Some("crt") => println!(
                    "{}",
                    audio::route_to_crt(
                        &target,
                        cfg.audio.volume,
                        cfg.audio.system_default,
                        &mut state
                    )
                ),
                Some("desktop") => println!("{}", audio::route_back(&mut state)),
                Some("volume") => {
                    let v: u32 = positional(args)
                        .get(1)
                        .and_then(|s| s.trim_end_matches('%').parse().ok())
                        .unwrap_or_else(|| die("audio volume needs a percent, 0 to 150"));
                    let v = v.min(150);
                    omarchy_crt_shell::crt::set_value("audio.volume", &v.to_string())
                        .unwrap_or_else(|e| die(&e));
                    if state.on
                        || audio::active_profile(&target.card).is_some_and(|p| p == target.profile)
                    {
                        omarchy_crt_shell::crt::run(
                            "pactl",
                            &["set-sink-volume", &target.sink, &format!("{v}%")],
                        );
                    }
                    println!("TV volume {v}%");
                }
                Some("all") => {
                    if state.previous_sink.is_empty()
                        && let Some(prev) = audio::default_sink()
                        && prev != target.sink
                    {
                        state.previous_sink = prev;
                    }
                    omarchy_crt_shell::crt::run("pactl", &["set-default-sink", &target.sink]);
                    println!("system default: {}", target.sink);
                }
                Some("apps") => {
                    if !state.previous_sink.is_empty() {
                        omarchy_crt_shell::crt::run(
                            "pactl",
                            &["set-default-sink", &state.previous_sink],
                        );
                        println!("system default: {}", state.previous_sink);
                        state.previous_sink.clear();
                    } else if let Some(other) = audio::other_sink(&target.sink) {
                        omarchy_crt_shell::crt::run("pactl", &["set-default-sink", &other]);
                        println!("system default: {other}");
                    }
                }
                _ => die("audio needs crt, desktop, all or apps"),
            }
            state.save();
        }
        "dac" => {
            let conn = connector(&cfg);
            let dac = open_dac(&conn).unwrap_or_else(|e| die(&e));
            let pos = positional(args);
            match pos.first().map(|s| s.as_str()).unwrap_or("status") {
                "status" => {
                    let lock = dac.lock().unwrap_or(Lock::Other(0));
                    let cs = dac.csync().unwrap_or(0);
                    println!(
                        "{} {}: {}, csync {}",
                        conn.name,
                        dac.bus,
                        lock.label(),
                        Csync::from_value(cs)
                            .map(|c| c.label().to_string())
                            .unwrap_or(format!("0x{cs:02X}"))
                    );
                }
                "reset" => {
                    let mode = Csync::parse(&cfg.output.csync);
                    dac.reset(mode).unwrap_or_else(|e| die(&e.to_string()));
                    println!(
                        "reset done, csync {}",
                        mode.map(|m| m.label()).unwrap_or("unchanged")
                    );
                }
                "csync" => {
                    let mode = pos
                        .get(1)
                        .and_then(|m| Csync::parse(m))
                        .unwrap_or_else(|| die("csync needs and, xor or separate"));
                    dac.set_csync(mode).unwrap_or_else(|e| die(&e.to_string()));
                    println!("csync {}", mode.label());
                }
                "watch" => {
                    let mode = Csync::parse(&cfg.output.csync).unwrap_or(Csync::Xor);
                    dac.watch(mode, |msg| println!("{msg}"))
                        .unwrap_or_else(|e| die(&e.to_string()));
                }
                other => die(&format!("unknown dac command {other}")),
            }
        }
        "bios" => cmd_bios(args),
        "library" => cmd_library(args),
        "watch" => {
            let pos = positional(args);
            let Some(target) = pos.first() else {
                die("watch needs a file or a URL");
            };
            let target = if target.starts_with("http://") || target.starts_with("https://") {
                target.to_string()
            } else {
                std::fs::canonicalize(target)
                    .unwrap_or_else(|_| die(&format!("{target} not found")))
                    .display()
                    .to_string()
            };
            if has(args, "--later") {
                let title = pos.get(1).map(|s| s.as_str()).unwrap_or("");
                let path = omarchy_crt_shell::crt::config_dir().join("watch-later.tsv");
                let mut text = std::fs::read_to_string(&path).unwrap_or_default();
                if text
                    .lines()
                    .any(|l| l.split('\t').next() == Some(target.as_str()))
                {
                    println!("already in the watch later list");
                    return;
                }
                text.push_str(&format!("{target}\t{title}\n"));
                std::fs::write(&path, text).unwrap_or_else(|e| die(&e.to_string()));
                println!("kept for later: {target}");
            } else {
                let line = format!("watch {target}");
                crt::control::send(&[line.as_str()]).unwrap_or_else(|e| die(&e.to_string()));
                println!("playing on the tube: {target}");
            }
        }
        "doctor" => exit(cmd_doctor(&cfg)),
        "config" => {
            let pos = positional(args);
            if pos.first().map(|s| s.as_str()) == Some("set") {
                let (Some(key), Some(value)) = (pos.get(1), pos.get(2)) else {
                    die("config set needs a key and a value, e.g. config set audio.volume 110")
                };
                omarchy_crt_shell::crt::set_value(key, value).unwrap_or_else(|e| die(&e));
                println!("{key} = {value}");
                return;
            }
            println!("{}", Config::path().display());
            if let Ok(t) = std::fs::read_to_string(Config::path()) {
                print!("{t}");
            }
        }
        other => die(&format!("unknown command {other}\n\n{HELP}")),
    }
}
