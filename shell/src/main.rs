//! omarchy-crt-shell: native boot screen and launcher for a 15 kHz CRT.
//!
//! Renders a low-resolution framebuffer (320x240 by default) and shows it
//! through SDL2, either in a scaled window for development or fullscreen on
//! the CRT output. No fake scanlines: the tube provides them.

mod art;
mod assets;
mod audio;
mod bt;
mod deck;
mod crt_tag;
mod effects;
mod etch;
mod fb;
mod font8x8;
mod icons;
mod menu;
mod pad;
mod scene;
mod theme;
use omarchy_crt_shell::padmap::Raw;
use omarchy_crt_shell::{index, library, music, padmap, player, profile, settings, states, videofit};

use audio::Audio;
use fb::Framebuffer;
use pad::Stick;
use scene::{Action, Geometry, Nav, PauseOutcome, Scene, SysInfo};
use sdl2::controller::Button;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use std::path::PathBuf;
use std::time::Instant;

struct Args {
    w: usize,
    h: usize,
    hz: u32,
    scale: u32,
    fullscreen: bool,
    stretch: bool,
    no_audio: bool,
    auto_boot: bool,
    headless: bool,
    theme: Option<PathBuf>,
    systems: Option<PathBuf>,
    config_dir: Option<PathBuf>,
    browse: Option<Option<String>>,
    dump: Vec<f64>,
    dump_dir: PathBuf,
    idle: f32,
    screensaver: Option<Option<effects::Kind>>,
    dump_audio: Option<PathBuf>,
    record: Option<PathBuf>,
    record_secs: f32,
    script: Option<PathBuf>,
}

const USAGE: &str = "usage: omarchy-crt-shell [options]
  --size WxH        framebuffer size (default 320x240)
  --hz N            refresh label shown in POST (default 60)
  --scale N         window scale for desktop testing (default 3)
  --fullscreen      fullscreen on the current output
  --stretch         ignore aspect ratio, fill the output (for wide 15 kHz modes)
  --no-audio        skip audio
  --auto-boot       skip the PRESS START gate
  --theme PATH      Omarchy colors.toml (default ~/.config/omarchy/current/colors.toml)
  --systems PATH    systems.toml (default ~/.config/omarchy-crt/systems.toml)
  --config-dir DIR  where settings, profile, recent and RetroArch configs live
  --browse [SYSTEM] boot straight into the game browser (or settings, saver, diag, about, power, profile, pair)
  --headless        render without a window; use with --dump
  --dump T1,T2,...  write frame_<T>.ppm at these seconds after boot
  --dump-dir DIR    where dumps go (default .)
  --idle SECONDS    start the screensaver after this much idle time (default 60, 0 = never)
  --screensaver [NAME]  start directly in the screensaver; NAME picks an effect
  --dump-audio DIR  write every synthesized sound as WAV into DIR and exit
  --record DIR      offline render: one PPM per frame at 60 fps plus audio.wav
  --record-secs N   length of the recording (default 30)
  --script FILE     scripted input for --record: lines of '<seconds> <action>'
                    with start, up, down, left, right, back, fire, fav, saver [NAME]";

type ArgIter = std::iter::Peekable<std::iter::Skip<std::env::Args>>;

fn take(it: &mut ArgIter, arg: &str) -> Result<String, String> {
    it.next().ok_or_else(|| format!("{arg} needs a value"))
}

/// An optional value: consumed only when the next word is not another flag.
fn optional(it: &mut ArgIter) -> Option<String> {
    match it.peek() {
        Some(next) if !next.starts_with("--") => it.next(),
        _ => None,
    }
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        w: 320,
        h: 240,
        hz: 60,
        scale: 3,
        fullscreen: false,
        stretch: false,
        no_audio: false,
        auto_boot: false,
        headless: false,
        theme: None,
        systems: None,
        config_dir: None,
        browse: None,
        dump: Vec::new(),
        dump_dir: PathBuf::from("."),
        idle: 60.0,
        screensaver: None,
        dump_audio: None,
        record: None,
        record_secs: 30.0,
        script: None,
    };
    let mut it = std::env::args().skip(1).peekable();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--size" => {
                let v = take(&mut it, &arg)?;
                let (w, h) = v.split_once('x').ok_or("size must be WxH")?;
                a.w = w.parse().map_err(|_| "bad width")?;
                a.h = h.parse().map_err(|_| "bad height")?;
            }
            "--hz" => a.hz = take(&mut it, &arg)?.parse().map_err(|_| "bad hz")?,
            "--scale" => a.scale = take(&mut it, &arg)?.parse().map_err(|_| "bad scale")?,
            "--fullscreen" => a.fullscreen = true,
            "--stretch" => a.stretch = true,
            "--no-audio" => a.no_audio = true,
            "--auto-boot" => a.auto_boot = true,
            "--headless" => a.headless = true,
            "--theme" => a.theme = Some(PathBuf::from(take(&mut it, &arg)?)),
            "--systems" => a.systems = Some(PathBuf::from(take(&mut it, &arg)?)),
            "--config-dir" => a.config_dir = Some(PathBuf::from(take(&mut it, &arg)?)),
            "--browse" => a.browse = Some(optional(&mut it)),
            "--dump" => {
                a.dump = take(&mut it, &arg)?
                    .split(',')
                    .filter_map(|s| s.trim().parse().ok())
                    .collect();
            }
            "--dump-dir" => a.dump_dir = PathBuf::from(take(&mut it, &arg)?),
            "--idle" => a.idle = take(&mut it, &arg)?.parse().map_err(|_| "bad idle")?,
            "--dump-audio" => a.dump_audio = Some(PathBuf::from(take(&mut it, &arg)?)),
            "--record" => a.record = Some(PathBuf::from(take(&mut it, &arg)?)),
            "--record-secs" => {
                a.record_secs = take(&mut it, &arg)?
                    .parse()
                    .map_err(|_| "bad record-secs")?
            }
            "--script" => a.script = Some(PathBuf::from(take(&mut it, &arg)?)),
            "--screensaver" => {
                let name = optional(&mut it);
                let kind = match name.as_deref() {
                    None => None,
                    Some(n) => Some(
                        effects::ALL
                            .iter()
                            .copied()
                            .find(|k| k.name() == n)
                            .ok_or_else(|| {
                                format!(
                                    "unknown effect {n}; one of: {}",
                                    effects::ALL
                                        .iter()
                                        .map(|k| k.name())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                )
                            })?,
                    ),
                };
                a.screensaver = Some(kind);
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("unknown option {other}\n{USAGE}")),
        }
    }
    Ok(a)
}

fn build_scene(args: &Args) -> Scene {
    let theme_path = args.theme.clone().or_else(theme::Theme::default_path);
    let theme = theme_path
        .and_then(|p| theme::Theme::load(&p))
        .unwrap_or_else(theme::Theme::tokyo_night);
    let info = SysInfo::probe(args.w, args.h, args.hz);
    let systems_path = args
        .systems
        .clone()
        .unwrap_or_else(|| match &args.config_dir {
            Some(d) => d.join("systems.toml"),
            None => library::default_path(),
        });
    let library = library::Library::load(&systems_path);
    Scene::new(theme, info, args.idle, library)
}

fn run_headless(args: &Args) -> Result<(), String> {
    let mut scene = build_scene(args);
    let mut fb = Framebuffer::new(args.w, args.h);
    std::fs::create_dir_all(&args.dump_dir).map_err(|e| e.to_string())?;
    scene.start_boot(0.0);
    if let Some(b) = &args.browse {
        scene.debug_browse(b.as_deref());
    }
    if let Some(kind) = args.screensaver {
        scene.start_screensaver(0.0, kind);
    }
    let mut dumps = args.dump.clone();
    dumps.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let end = dumps.last().copied().unwrap_or(8.0) + 0.02;
    let dt = 1.0 / 60.0;
    let mut t = 0.0;
    let mut next = 0;
    while t <= end {
        scene.draw(&mut fb, t);
        fb.roll(scene.roll(), t as f32);
        fb.apply_gain(scene.power());
        while next < dumps.len() && t + dt / 2.0 >= dumps[next] {
            let path = args.dump_dir.join(format!("frame_{:.2}.ppm", dumps[next]));
            fb.write_ppm(&path).map_err(|e| e.to_string())?;
            println!("{}", path.display());
            next += 1;
        }
        scene.take_sounds();
        t += dt;
    }
    Ok(())
}

/// Offline render for demo videos: every frame as PPM, all sounds mixed into
/// one WAV at the exact frame times, inputs replayed from a script.
fn run_record(args: &Args, dir: &PathBuf) -> Result<(), String> {
    let mut scene = build_scene(args);
    let mut fb = Framebuffer::new(args.w, args.h);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let bank = audio::render_bank();
    let total = (args.record_secs * audio::RATE as f32) as usize;
    let mut master = vec![0f32; total];
    let mut script: Vec<(f32, String, Option<String>)> = Vec::new();
    if let Some(path) = &args.script {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split_whitespace();
            let at: f32 = parts
                .next()
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| format!("bad script line: {line}"))?;
            let action = parts
                .next()
                .ok_or_else(|| format!("bad script line: {line}"))?;
            script.push((at, action.to_string(), parts.next().map(str::to_string)));
        }
        script.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    } else {
        script.push((0.5, "start".into(), None));
    }
    if let Some(b) = &args.browse {
        scene.start_boot(0.0);
        scene.debug_browse(b.as_deref());
    }
    let dt = 1.0 / 60.0;
    let mut next = 0;
    let mut frame = 0usize;
    let mut t = 0.0f32;
    while t <= args.record_secs {
        while next < script.len() && script[next].0 <= t {
            let (_, action, arg) = &script[next];
            let now = t as f64;
            match action.as_str() {
                "start" => scene.start_boot(now),
                "saver" => {
                    let kind = arg
                        .as_deref()
                        .and_then(|n| effects::ALL.iter().copied().find(|k| k.name() == n));
                    scene.start_screensaver(now, kind);
                }
                other => {
                    if !scene.touch(now) {
                        match other {
                            "up" => scene.navigate(Nav::Up),
                            "down" => scene.navigate(Nav::Down),
                            "left" => scene.navigate(Nav::Left),
                            "right" => scene.navigate(Nav::Right),
                            "back" => scene.navigate(Nav::Back),
                            "fav" => scene.toggle_favorite(),
                            "convert" => scene.convert_selected(),
                            "fire" => match scene.activate() {
                                Action::Quit | Action::Restart => break,
                                _ => {}
                            },
                            "finish" => scene.game_finished(true),
                            _ => return Err(format!("unknown script action {other}")),
                        }
                    }
                }
            }
            next += 1;
        }
        if let Some((_, title, _)) = scene.take_launch() {
            eprintln!("script: not launching {title} while recording");
        }
        scene.draw(&mut fb, t as f64);
        fb.roll(scene.roll(), t);
        fb.apply_gain(scene.power());
        let offset = (t * audio::RATE as f32) as usize;
        for s in scene.take_sounds() {
            if let Some((_, data)) = bank.iter().find(|(k, _)| *k == s) {
                for (i, v) in data.iter().enumerate() {
                    if let Some(m) = master.get_mut(offset + i) {
                        *m += v;
                    }
                }
            }
        }
        for data in scene.take_samples() {
            for (i, v) in data.iter().enumerate() {
                if let Some(m) = master.get_mut(offset + i) {
                    *m += v;
                }
            }
        }
        fb.write_ppm(&dir.join(format!("frame_{frame:05}.ppm")))
            .map_err(|e| e.to_string())?;
        frame += 1;
        t += dt;
    }
    for m in master.iter_mut() {
        *m = m.clamp(-1.0, 1.0);
    }
    audio::write_wav(&dir.join("audio.wav"), &master).map_err(|e| e.to_string())?;
    println!("{frame} frames and audio.wav in {}", dir.display());
    Ok(())
}

fn run(args: &Args) -> Result<(), String> {
    // Keep reading the pad while mpv or RetroArch own the focused window.
    sdl2::hint::set("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS", "1");
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let gcs = sdl.game_controller()?;
    let audio = if args.no_audio {
        Audio::silent()
    } else {
        Audio::open(&sdl.audio()?)?
    };
    // SDL's PulseAudio backend opens the default device by name, so the
    // PULSE_SINK the CLI set for us is ignored; move the stream ourselves
    // once it exists.
    if let Ok(sink) = std::env::var("PULSE_SINK") {
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            omarchy_crt_shell::crt::audio::move_streams(&sink);
        });
    }

    let mut builder = video.window(
        "omarchy-crt",
        args.w as u32 * args.scale,
        args.h as u32 * args.scale,
    );
    builder.position_centered();
    // On the tube the compositor floats, pins and sizes the window through
    // a rule; never ask for fullscreen (see crt::output::window_rules).
    builder.resizable();
    let window = builder.build().map_err(|e| e.to_string())?;
    let mut canvas = window
        .into_canvas()
        .present_vsync()
        .build()
        .map_err(|e| e.to_string())?;
    if !args.stretch {
        canvas
            .set_logical_size(args.w as u32, args.h as u32)
            .map_err(|e| e.to_string())?;
    }
    let creator = canvas.texture_creator();
    let mut tex = creator
        .create_texture_streaming(PixelFormatEnum::ARGB8888, args.w as u32, args.h as u32)
        .map_err(|e| e.to_string())?;
    sdl.mouse().show_cursor(false);

    // Extra mappings for pads SDL does not know (SDL_GameControllerDB format).
    if let Some(dir) = library::default_path().parent() {
        let db = dir.join("gamecontrollerdb.txt");
        if db.exists() {
            match gcs.load_mappings(&db) {
                Ok(n) => eprintln!("loaded {n} controller mappings"),
                Err(e) => eprintln!("gamecontrollerdb: {e}"),
            }
        }
    }
    let js = sdl.joystick()?;
    let mut controllers = Vec::new();
    // Pads without a mapping stay open as bare joysticks so the wizard can
    // read their buttons.
    let mut raw_joys: Vec<sdl2::joystick::Joystick> = Vec::new();
    for i in 0..gcs.num_joysticks()? {
        if gcs.is_game_controller(i) {
            if let Ok(c) = gcs.open(i) {
                controllers.push(c);
            }
        } else if let Ok(j) = js.open(i) {
            raw_joys.push(j);
        }
    }
    let mut stick = Stick::new();

    let mut scene = build_scene(args);
    if let Some(c) = controllers.last() {
        scene.set_pad(Some(&c.name()));
    }
    // Pads plugged in before the start that SDL does not know: the wizard
    // shows once the menu is up.
    for j in &raw_joys {
        scene.pad_wizard_start(&j.name(), &j.guid().string(), j.instance_id());
    }
    let mut fb = Framebuffer::new(args.w, args.h);
    let mut bytes = Vec::with_capacity(args.w * args.h * 4);
    let clock = Instant::now();
    let now = || clock.elapsed().as_secs_f64();
    if args.auto_boot || args.screensaver.is_some() || args.browse.is_some() {
        scene.start_boot(now());
    }
    if let Some(b) = &args.browse {
        scene.debug_browse(b.as_deref());
    }
    if let Some(kind) = args.screensaver {
        scene.start_screensaver(now(), kind);
    }

    let mut dumps = args.dump.clone();
    dumps.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut next_dump = 0;
    if !dumps.is_empty() {
        std::fs::create_dir_all(&args.dump_dir).map_err(|e| e.to_string())?;
    }

    let mut pump = sdl.event_pump()?;
    // Typed characters arrive as text input events; the search bar uses them.
    video.text_input().start();
    // The "/" that opens the search also arrives as text: swallowed once.
    let mut swallow_slash = false;
    let mut trigger_held = false;
    let control = match omarchy_crt_shell::crt::control::listen() {
        Ok(rx) => Some(rx),
        Err(e) => {
            eprintln!("control pipe: {e}");
            None
        }
    };
    let mut child: Option<std::process::Child> = None;
    let mut lines_changed = false;
    // Pad buttons held, for the Select + Start pause combo.
    let mut held_back = false;
    let mut held_start = false;
    'main: loop {
        if let Some(c) = child.as_mut() {
            match c.try_wait() {
                Ok(Some(status)) => {
                    child = None;
                    eprintln!(
                        "game exited: {status} (its output: {})",
                        library::game_log_path().display()
                    );
                    // A crash inside RetroArch's own shutdown is a normal
                    // end of play as far as the launcher is concerned.
                    scene.game_finished(status.success() || library::exited_after_unload());
                    omarchy_crt_shell::crt::output::expect_game_clear();
                    if lines_changed {
                        crt_mode(None);
                        lines_changed = false;
                    }
                    if args.fullscreen {
                        fit_output(canvas.window_mut());
                        omarchy_crt_shell::crt::output::raise(omarchy_crt_shell::crt::SHELL_CLASS);
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("wait failed: {e}");
                    child = None;
                    scene.game_finished(false);
                }
            }
        }
        // Real events and control-pipe lines share one handling path.
        let mut inputs: Vec<Input> = Vec::new();
        for ev in pump.poll_iter() {
            // While a pad is being mapped its buttons answer the wizard and
            // nothing else; a remapped pad still emits controller events.
            if scene.pad_wizard_active()
                && matches!(
                    ev,
                    Event::ControllerButtonDown { .. }
                        | Event::ControllerButtonUp { .. }
                        | Event::ControllerAxisMotion { .. }
                )
            {
                continue;
            }
            let mut mapped: Option<String> = None;
            let mut inp = Input::default();
            // While the search bar takes text, letters type instead of
            // acting as vim keys; arrows, Enter and Escape keep working.
            let typing = scene.search_active() && !scene.osk_active();
            match ev {
                Event::Quit { .. } => inp.quit = true,
                Event::TextInput { text, .. } => {
                    if swallow_slash && text == "/" {
                        swallow_slash = false;
                    } else if typing {
                        inp.text = Some(text);
                    }
                }
                Event::KeyDown {
                    keycode: Some(k),
                    repeat: false,
                    ..
                } => match k {
                    Keycode::Q if !typing => break 'main,
                    Keycode::Slash if !typing => {
                        inp.search = true;
                        swallow_slash = true;
                    }
                    Keycode::Escape => inp.nav = Some(Nav::Back),
                    Keycode::Backspace => inp.backspace = true,
                    Keycode::Return => {
                        inp.start = true;
                        inp.fire = true;
                    }
                    Keycode::Space if !typing => {
                        inp.start = true;
                        inp.fire = true;
                    }
                    Keycode::Up => inp.nav = Some(Nav::Up),
                    Keycode::Down => inp.nav = Some(Nav::Down),
                    Keycode::Left => inp.nav = Some(Nav::Left),
                    Keycode::Right => inp.nav = Some(Nav::Right),
                    Keycode::K if !typing => inp.nav = Some(Nav::Up),
                    Keycode::J if !typing => inp.nav = Some(Nav::Down),
                    Keycode::H if !typing => inp.nav = Some(Nav::Left),
                    Keycode::L if !typing => inp.nav = Some(Nav::Right),
                    Keycode::F if !typing => inp.fav = true,
                    Keycode::X if !typing => inp.alt = true,
                    Keycode::PageUp => inp.jump = -1,
                    Keycode::PageDown => inp.jump = 1,
                    Keycode::Home => inp.edge = Some(false),
                    Keycode::End => inp.edge = Some(true),
                    _ => {}
                },
                Event::MouseButtonDown { .. } => inp.start = true,
                Event::ControllerDeviceAdded { which, .. } => {
                    if let Ok(c) = gcs.open(which) {
                        scene.set_pad(Some(&c.name()));
                        controllers.push(c);
                    }
                }
                Event::JoyDeviceAdded { which, .. } => {
                    if !gcs.is_game_controller(which) {
                        if let Ok(j) = js.open(which) {
                            scene.pad_wizard_start(&j.name(), &j.guid().string(), j.instance_id());
                            raw_joys.push(j);
                        }
                    }
                }
                Event::JoyDeviceRemoved { which, .. } => {
                    raw_joys.retain(|j| j.instance_id() != which);
                }
                Event::JoyButtonDown { which, button_idx, .. } if scene.pad_wizard_active() => {
                    mapped = scene.pad_wizard_raw(which, Raw::Button(button_idx));
                }
                Event::JoyAxisMotion { which, axis_idx, value, .. }
                    if scene.pad_wizard_active() && value.unsigned_abs() > 20000 =>
                {
                    mapped = scene.pad_wizard_raw(which, Raw::Axis(axis_idx, value > 0));
                }
                Event::JoyHatMotion { which, hat_idx, state, .. }
                    if scene.pad_wizard_active() && state != sdl2::joystick::HatState::Centered =>
                {
                    mapped = scene.pad_wizard_raw(which, Raw::Hat(hat_idx, state as u8));
                }
                Event::ControllerDeviceRemoved { which, .. } => {
                    controllers.retain(|c| c.instance_id() != which);
                    match controllers.last() {
                        Some(c) => scene.set_pad(Some(&c.name())),
                        None => scene.set_pad(None),
                    }
                }
                Event::ControllerAxisMotion { axis, value, .. } => {
                    if axis == sdl2::controller::Axis::TriggerLeft {
                        // Left trigger: the search with the on screen keyboard.
                        let on = value > 16000;
                        if on && !trigger_held {
                            inp.osk = true;
                        }
                        trigger_held = on;
                    } else {
                        stick.set(axis, value);
                    }
                }
                Event::ControllerButtonUp { button, .. } => match button {
                    Button::Back => held_back = false,
                    Button::Start => held_start = false,
                    _ => {}
                },
                Event::ControllerButtonDown { button, .. } => match button {
                    // Select + Start (either order) or the home button: the
                    // pause menu over a running game.
                    Button::Guide => inp.menu = true,
                    Button::Start if held_back => {
                        held_start = true;
                        inp.menu = true;
                    }
                    Button::Back if held_start => {
                        held_back = true;
                        inp.menu = true;
                    }
                    Button::A | Button::Start => {
                        held_start = button == Button::Start;
                        inp.start = true;
                        inp.fire = true;
                    }
                    Button::DPadUp => inp.nav = Some(Nav::Up),
                    Button::DPadDown => inp.nav = Some(Nav::Down),
                    Button::DPadLeft => inp.nav = Some(Nav::Left),
                    Button::DPadRight => inp.nav = Some(Nav::Right),
                    Button::B | Button::Back => {
                        held_back = button == Button::Back;
                        inp.nav = Some(Nav::Back);
                    }
                    Button::Y => inp.fav = true,
                    Button::X => inp.alt = true,
                    Button::LeftShoulder => inp.jump = -1,
                    Button::RightShoulder => inp.jump = 1,
                    _ => {}
                },
                _ => {}
            }
            if let Some(mapping) = mapped {
                // SDL learns the pad now; open it as a controller like any other.
                match gcs.add_mapping(&mapping) {
                    Ok(_) => {
                        for i in 0..gcs.num_joysticks().unwrap_or(0) {
                            if !gcs.is_game_controller(i) {
                                continue;
                            }
                            if let Ok(c) = gcs.open(i) {
                                if !controllers.iter().any(|x| x.instance_id() == c.instance_id()) {
                                    raw_joys.retain(|j| j.instance_id() != c.instance_id());
                                    scene.set_pad(Some(&c.name()));
                                    controllers.push(c);
                                }
                            }
                        }
                    }
                    Err(e) => eprintln!("pad mapping refused: {e}"),
                }
            }
            inputs.push(inp);
        }
        if scene.take_rumble() {
            if let Some(c) = controllers.last_mut() {
                let _ = c.set_rumble(0, 0x9000, 60);
            }
        }
        if scene.take_remap_request() {
            // Map the last pad again: its raw joystick answers the wizard.
            if let Some(c) = controllers.last() {
                let id = c.instance_id();
                let name = c.name();
                for i in 0..js.num_joysticks().unwrap_or(0) {
                    if let Ok(j) = js.open(i) {
                        if j.instance_id() == id {
                            scene.pad_wizard_start(&name, &j.guid().string(), id);
                            raw_joys.push(j);
                            break;
                        }
                    }
                }
            }
        }
        if let Some(rx) = &control {
            for line in rx.try_iter() {
                match control_input(&line) {
                    Some(inp) => inputs.push(inp),
                    None => eprintln!("control: unknown input {line}"),
                }
            }
        }
        for inp in inputs {
            if inp.quit {
                break 'main;
            }
            let is_input = inp.any();
            let Input {
                start,
                nav,
                fire,
                fav,
                alt,
                home,
                ..
            } = inp.clone();
            if home {
                scene.home();
                continue;
            }

            if scene.is_running() {
                if inp.menu {
                    after_pause(scene.toggle_pause());
                    continue;
                }
                if scene.is_paused() {
                    if is_input {
                        after_pause(scene.pause_input(nav, fire));
                    }
                    continue;
                }
                if scene.player_active() && is_input {
                    scene.player_input(nav, fire);
                }
                continue;
            }
            if inp.menu {
                continue;
            }
            if is_input && scene.touch(now()) {
                continue;
            }
            // The search bar and the list jumps.
            if inp.search || inp.osk {
                if scene.boot_started() && !scene.booting() {
                    scene.search_open(inp.osk);
                }
                continue;
            }
            if let Some(t) = &inp.text {
                scene.search_type(t);
                continue;
            }
            if inp.backspace {
                if scene.search_active() {
                    scene.search_backspace();
                } else {
                    scene.navigate(Nav::Back);
                }
                continue;
            }
            if inp.jump != 0 {
                scene.jump_letter(inp.jump);
                continue;
            }
            if let Some(last) = inp.edge {
                scene.jump_end(last);
                continue;
            }
            if let Some(target) = &inp.watch {
                scene.watch(target);
                continue;
            }
            if scene.osk_active() && (nav.is_some() || fire || fav || alt) {
                scene.osk_input(nav, fire, fav, alt);
                continue;
            }
            if start && !scene.boot_started() {
                scene.start_boot(now());
                continue;
            }
            if start && scene.booting() {
                scene.skip_boot(now());
                continue;
            }
            if let Some(n) = nav {
                scene.navigate(n);
            }
            if fav {
                scene.toggle_favorite();
            }
            if alt {
                scene.convert_selected();
            }
            if fire {
                match scene.activate() {
                    Action::Quit => break 'main,
                    Action::Restart => {
                        use std::os::unix::process::CommandExt;
                        let exe = std::env::current_exe()
                            .unwrap_or_else(|_| PathBuf::from("omarchy-crt-shell"));
                        let err = std::process::Command::new(exe)
                            .args(std::env::args().skip(1))
                            .exec();
                        eprintln!("restart failed: {err}");
                        break 'main;
                    }
                    Action::Launch(cmd) => {
                        if let Err(e) = menu::launch(&cmd) {
                            eprintln!("launch failed: {e}");
                        }
                    }
                    Action::None => {}
                }
            }
        }

        if let Some((mut cmd, title, lines)) = scene.take_launch() {
            if let Some(g) = lines {
                if crt_mode(Some(g)) {
                    lines_changed = true;
                    if args.fullscreen {
                        fit_output(canvas.window_mut());
                    }
                }
            }
            // The game must land on the tube whatever has focus: flag the
            // compositor handler, and take focus ourselves as well.
            omarchy_crt_shell::crt::output::expect_game();
            if args.fullscreen {
                crt_focus();
            }
            match cmd.spawn() {
                Ok(c) => {
                    eprintln!("running {title}: {cmd:?}");
                    child = Some(c);
                }
                Err(e) => {
                    eprintln!("cannot start retroarch: {e}");
                    scene.game_finished(false);
                }
            }
        }
        if let Some(n) = stick.poll(now()) {
            if !scene.is_running() && !scene.touch(now()) {
                scene.navigate(n);
            }
        }

        let t = now();
        if let Ok((ow, oh)) = canvas.output_size() {
            scene.set_output_size(ow, oh);
        }
        if scene.take_profile_preview() && child.is_none() {
            scene.save_profile();
            crt_mode_async(None);
        }
        scene.draw(&mut fb, t);
        fb.roll(scene.roll(), t as f32);
        fb.apply_gain(scene.power());
        for s in scene.take_sounds() {
            audio.play(s);
        }
        for data in scene.take_samples() {
            audio.play_samples(data);
        }

        if scene.boot_started() && next_dump < dumps.len() {
            let bt = scene_time(&scene, t);
            if bt >= dumps[next_dump] {
                let path = args
                    .dump_dir
                    .join(format!("frame_{:.2}.ppm", dumps[next_dump]));
                let _ = fb.write_ppm(&path);
                next_dump += 1;
            }
        }

        fb.to_bgra(&mut bytes);
        tex.update(None, &bytes, args.w * 4)
            .map_err(|e| e.to_string())?;
        canvas.clear();
        canvas.copy(&tex, None, None)?;
        canvas.present();
    }
    Ok(())
}

/// One user input, from a key, a pad button or the control pipe.
#[derive(Default, Clone)]
struct Input {
    start: bool,
    nav: Option<Nav>,
    fire: bool,
    fav: bool,
    alt: bool,
    home: bool,
    /// In-game menu of the running emulator.
    menu: bool,
    quit: bool,
    /// Characters typed into the search bar.
    text: Option<String>,
    backspace: bool,
    /// Open the search bar (keyboard), or with the on screen keyboard (pad).
    search: bool,
    osk: bool,
    /// Jump to the next (+1) or previous (-1) initial letter of the list.
    jump: i32,
    /// First (false) or last (true) row.
    edge: Option<bool>,
    /// A video file or URL to play now.
    watch: Option<String>,
}

impl Input {
    fn any(&self) -> bool {
        self.start
            || self.nav.is_some()
            || self.fire
            || self.fav
            || self.alt
            || self.home
            || self.text.is_some()
            || self.backspace
            || self.search
            || self.osk
            || self.jump != 0
            || self.edge.is_some()
            || self.watch.is_some()
    }
}

fn control_input(line: &str) -> Option<Input> {
    let mut inp = Input::default();
    if let Some(text) = line.strip_prefix("type ") {
        inp.text = Some(text.to_string());
        return Some(inp);
    }
    if let Some(target) = line.strip_prefix("watch ") {
        inp.watch = Some(target.trim().to_string());
        return Some(inp);
    }
    match omarchy_crt_shell::crt::control::normalize(line)? {
        "search" => inp.search = true,
        "osk" => inp.osk = true,
        "del" => inp.backspace = true,
        "next" => inp.jump = 1,
        "prev" => inp.jump = -1,
        "first" => inp.edge = Some(false),
        "last" => inp.edge = Some(true),
        "up" => inp.nav = Some(Nav::Up),
        "down" => inp.nav = Some(Nav::Down),
        "left" => inp.nav = Some(Nav::Left),
        "right" => inp.nav = Some(Nav::Right),
        "back" => inp.nav = Some(Nav::Back),
        "fav" => inp.fav = true,
        "alt" => inp.alt = true,
        "start" => inp.start = true,
        "home" => inp.home = true,
        "menu" => inp.menu = true,
        _ => {
            inp.start = true;
            inp.fire = true;
        }
    }
    Some(inp)
}

fn scene_time(scene: &Scene, now: f64) -> f64 {
    // Scene keeps its own t0; expose elapsed boot time for dump timing.
    if scene.boot_started() {
        now - scene_t0(scene)
    } else {
        -1.0
    }
}

fn scene_t0(scene: &Scene) -> f64 {
    scene.t0()
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    if let Some(dir) = &args.dump_audio {
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("{e}");
            std::process::exit(1);
        }
        for (kind, data) in audio::render_bank() {
            let path = dir.join(format!("{kind:?}.wav").to_lowercase());
            if let Err(e) = audio::write_wav(&path, &data) {
                eprintln!("{e}");
                std::process::exit(1);
            }
            println!("{}", path.display());
        }
        return;
    }
    let result = if let Some(dir) = &args.record {
        run_record(&args, dir)
    } else if args.headless {
        run_headless(&args)
    } else {
        run(&args)
    };
    if let Err(e) = result {
        eprintln!("omarchy-crt-shell: {e}");
        std::process::exit(1);
    }
}

/// Ask the compositor for fullscreen again after another window took it.

/// Ask the CLI to switch the CRT to a program's geometry, or back to the
/// full frame. Returns true when the command ran and succeeded.
fn crt_mode(geometry: Option<Geometry>) -> bool {
    let name = "omarchy-crt";
    let bin = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(name));
    let mut cmd = std::process::Command::new(bin);
    cmd.arg("mode");
    if let Some(g) = geometry {
        if let Some(h) = g.lines {
            cmd.arg("--lines").arg(h.to_string());
        }
        if g.shift_x != 0 {
            cmd.arg("--shift-x").arg(g.shift_x.to_string());
        }
        if g.shift_y != 0 {
            cmd.arg("--shift-y").arg(g.shift_y.to_string());
        }
    }
    match cmd
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        Ok(s) => s.success(),
        Err(_) => false,
    }
}

/// Ask the CLI to put keyboard focus (and the CRT workspace) back on us.
/// Compositor side of a pause action: the launcher comes to the front over
/// the paused game, resuming puts the game back in front. Both windows stay
/// mapped and rendered throughout.
fn after_pause(outcome: PauseOutcome) {
    use omarchy_crt_shell::crt::display;
    use omarchy_crt_shell::crt::output::raise;
    if display::on_tube() {
        match outcome {
            PauseOutcome::Shown => {
                display::raise(omarchy_crt_shell::crt::SHELL_CLASS);
            }
            PauseOutcome::Resumed => {
                display::raise("com.libretro.RetroArch");
            }
            _ => {}
        }
        return;
    }
    match outcome {
        PauseOutcome::Shown => {
            raise(omarchy_crt_shell::crt::SHELL_CLASS);
        }
        PauseOutcome::Resumed => {
            raise("com.libretro.RetroArch");
        }
        PauseOutcome::Quit | PauseOutcome::None => {}
    }
}

/// Size our window to the tube's current mode: a pinned floating window
/// keeps its size when the modeline changes under it.
fn fit_output(window: &mut sdl2::video::Window) {
    // On the tube our own compositor sizes every client to the mode.
    if omarchy_crt_shell::crt::display::on_tube() {
        return;
    }
    let name = "omarchy-crt";
    let bin = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(name));
    let Ok(out) = std::process::Command::new(bin)
        .args(["status", "--json"])
        .stderr(std::process::Stdio::null())
        .output()
    else {
        return;
    };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
        return;
    };
    if let (Some(w), Some(h)) = (v["mode"]["width"].as_u64(), v["mode"]["height"].as_u64()) {
        if w > 0 && h > 0 {
            let _ = window.set_size(w as u32, h as u32);
        }
    }
}

fn crt_focus() {
    let name = "omarchy-crt";
    let bin = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(name));
    let _ = std::process::Command::new(bin)
        .arg("focus")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// Like `crt_mode`, without waiting: for live adjustments while drawing.
fn crt_mode_async(geometry: Option<Geometry>) {
    let name = "omarchy-crt";
    let bin = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(name));
    let mut cmd = std::process::Command::new(bin);
    cmd.arg("mode");
    if let Some(g) = geometry {
        if let Some(h) = g.lines {
            cmd.arg("--lines").arg(h.to_string());
        }
        cmd.arg("--shift-x").arg(g.shift_x.to_string());
        cmd.arg("--shift-y").arg(g.shift_y.to_string());
    }
    let _ = cmd
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}
