//! `omarchy-crt`: drive a 15 kHz CRT from the Omarchy desktop.
//!
//! One command turns the tube on (modeline, DAC composite sync, audio to the
//! TV, launcher fullscreen) and one turns it off. The rest is status,
//! diagnostics, the game library and BIOS files. The Omarchy bar plugin is
//! a thin face over `omarchy-crt status --json` and these commands.

use omarchy_crt_shell::crt::dac::{Csync, Dac, Lock};
use omarchy_crt_shell::crt::output::{self, Connector, Modeline};
use omarchy_crt_shell::crt::{Config, State, audio, bios, launcher, roms};
use omarchy_crt_shell::library::{self, Library};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::exit;

const HELP: &str = "\
omarchy-crt: drive a 15 kHz CRT from the Omarchy desktop

  status [--json]          output, mode, DAC, audio, launcher, BIOS at a glance
  on [ntsc|pal]            15 kHz modeline, DAC csync, audio to the TV, launcher
  off                      launcher closed, audio back, output disabled
  toggle
  mode [ntsc|pal] [--lines N]  standard and active lines (224 for SNES); no args = full frame
  shell start|stop|restart|focus
  focus                    keyboard focus to the launcher
  audio crt|desktop
  dac status|reset|csync and|xor|separate|watch
  bios [--json]            BIOS files the cores expect
  bios import DIR [--all]  copy BIOS files from another collection
  library scan [--json]    systems, folders, game counts, cores
  library link DIR [--write]  adopt a RePlayOS or Batocera roms folder
  doctor                   checks with plain answers
  config                   config file path and contents

Config: ~/.config/omarchy-crt/crt.toml (written with defaults on first run)";

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
        if a == "--lines" {
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
        st["connector"] = json!({
            "drm": c.drm, "name": c.name, "connected": c.connected,
            "edid_name": c.edid_name, "edid_audio": c.edid_audio, "rgbpi2": c.is_rgbpi2(),
        });
        if let Some(m) = output::hypr_monitor(&c.name) {
            let w = m["width"].as_u64().unwrap_or(0) as u32;
            let h = m["height"].as_u64().unwrap_or(0) as u32;
            let disabled = m["disabled"].as_bool().unwrap_or(false);
            let mut mode = json!({ "width": w, "height": h, "refresh_hz": m["refreshRate"], "disabled": disabled });
            for std in ["ntsc", "pal"] {
                if let Some(ml) = cfg.modeline(std).and_then(Modeline::parse) {
                    if !disabled && ml.width() == w && (h == ml.height() || h == state.lines) {
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
) -> Result<Modeline, String> {
    let base = cfg
        .modeline(standard)
        .and_then(Modeline::parse)
        .ok_or_else(|| format!("no modeline for {standard}"))?;
    let ml = match lines {
        Some(l) if l != base.height() => base.with_lines(l),
        _ => base,
    };
    let (ok, out) = output::apply_modeline(&conn.name, &ml, &cfg.output.position);
    if !ok {
        return Err(format!("modeline refused: {out}"));
    }
    let mut state = State::load();
    state.standard = standard.into();
    state.lines = ml.height();
    state.save();
    Ok(ml)
}

fn set_csync(cfg: &Config, conn: &Connector) -> Result<String, String> {
    let mode =
        Csync::parse(&cfg.output.csync).ok_or("output.csync must be and, xor or separate")?;
    let dac = open_dac(conn)?;
    dac.set_csync(mode).map_err(|e| e.to_string())?;
    Ok(mode.label().into())
}

/// Hyprland options that let RetroArch and mpv take the tube while they run
/// and hand it back to the launcher when they exit.
fn compositor_fullscreen_policy(on: bool) {
    let code = if on {
        "hl.config({ misc = { on_focus_under_fullscreen = 1, exit_window_retains_fullscreen = true } })"
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
    match apply_mode(cfg, &conn, &standard, None) {
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
                let note = audio::route_to_crt(&t, cfg.audio.volume, &mut state);
                state.save();
                println!("audio:      {note}");
            }
            None => println!("audio:      no HDMI audio pin for this output"),
        }
    }
    compositor_fullscreen_policy(true);
    output::window_rules(&conn.name);
    match launcher::start(cfg, &conn.name) {
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

fn cmd_off(cfg: &Config) {
    println!("launcher:   {}", launcher::stop());
    let mut state = State::load();
    if cfg.audio.route {
        println!("audio:      {}", audio::route_back(&mut state));
    }
    compositor_fullscreen_policy(false);
    if let Some(conn) = output::pick(cfg) {
        output::disable(&conn.name);
        println!("output:     {} disabled", conn.name);
    }
    state.on = false;
    state.save();
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
    let lib = library();
    let pos = positional(args);
    match pos.first().map(|s| s.as_str()) {
        Some("link") => {
            let Some(dir) = pos.get(1) else {
                die("library link needs the roms directory")
            };
            let root = PathBuf::from(dir);
            if !root.is_dir() {
                die(&format!("{dir} is not a directory"));
            }
            let (systems, linked) = roms::link(&root, &lib.systems);
            if linked.is_empty() {
                die("no known system folder found there");
            }
            for l in &linked {
                println!("{:<16} -> {:<12} {} files", l.folder, l.system, l.files);
            }
            let text = roms::systems_toml(&systems, lib.switching);
            if has(args, "--write") {
                let path = library::default_path();
                if path.exists() {
                    let backup = path.with_extension("toml.bak");
                    let _ = std::fs::copy(&path, &backup);
                    println!("previous file kept as {}", backup.display());
                }
                if let Some(p) = path.parent() {
                    let _ = std::fs::create_dir_all(p);
                }
                std::fs::write(&path, text).unwrap_or_else(|e| die(&e.to_string()));
                println!("written {}", path.display());
            } else {
                println!(
                    "\n{text}\nRun again with --write to save it as {}.",
                    library::default_path().display()
                );
            }
        }
        _ => {
            let scan = roms::scan(&lib);
            if has(args, "--json") {
                let rows: Vec<Value> = scan
                    .iter()
                    .map(|s| json!({ "name": s.name, "dir": s.dir, "exists": s.exists, "games": s.games, "unknown": s.unknown, "core": s.core_present }))
                    .collect();
                println!("{}", Value::Array(rows));
                return;
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
            if std != "ntsc" && std != "pal" {
                die("mode needs ntsc or pal");
            }
            let lines = args
                .iter()
                .position(|a| a == "--lines")
                .and_then(|i| args.get(i + 1))
                .and_then(|v| v.parse().ok());
            let conn = connector(&cfg);
            match apply_mode(&cfg, &conn, std, lines) {
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
            std::thread::sleep(std::time::Duration::from_millis(800));
            let _ = set_csync(&cfg, &conn);
            launcher::focus();
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
                        launcher::start(&cfg, &conn.name).unwrap_or_else(|e| die(&e))
                    );
                    launcher::focus();
                }
                "stop" => println!("{}", launcher::stop()),
                "restart" => {
                    launcher::stop();
                    std::thread::sleep(std::time::Duration::from_millis(600));
                    let conn = connector(&cfg);
                    println!(
                        "{}",
                        launcher::start(&cfg, &conn.name).unwrap_or_else(|e| die(&e))
                    );
                    launcher::focus();
                }
                "focus" => println!("{}", launcher::focus().1),
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
        "focus" => println!("{}", launcher::focus().1),
        "audio" => {
            let conn = connector(&cfg);
            let target =
                audio::target(&conn).unwrap_or_else(|| die("no HDMI audio pin for this output"));
            let mut state = State::load();
            match positional(args).first().map(|s| s.as_str()) {
                Some("crt") => println!(
                    "{}",
                    audio::route_to_crt(&target, cfg.audio.volume, &mut state)
                ),
                Some("desktop") => println!("{}", audio::route_back(&mut state)),
                _ => die("audio needs crt or desktop"),
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
                    dac.reset().unwrap_or_else(|e| die(&e.to_string()));
                    println!("reset done");
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
                    dac.watch(|msg| println!("{msg}"))
                        .unwrap_or_else(|e| die(&e.to_string()));
                }
                other => die(&format!("unknown dac command {other}")),
            }
        }
        "bios" => cmd_bios(args),
        "library" => cmd_library(args),
        "doctor" => exit(cmd_doctor(&cfg)),
        "config" => {
            println!("{}", Config::path().display());
            if let Ok(t) = std::fs::read_to_string(Config::path()) {
                print!("{t}");
            }
        }
        other => die(&format!("unknown command {other}\n\n{HELP}")),
    }
}
