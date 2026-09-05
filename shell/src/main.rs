//! omarchy-crt-shell: native boot screen and launcher for a 15 kHz CRT.
//!
//! Renders a low-resolution framebuffer (320x240 by default) and shows it
//! through SDL2, either in a scaled window for development or fullscreen on
//! the CRT output. No fake scanlines: the tube provides them.

mod assets;
mod audio;
mod bt;
mod crt_tag;
mod effects;
mod etch;
mod fb;
mod font8x8;
mod icons;
mod library;
mod menu;
mod pad;
mod profile;
mod scene;
mod settings;
mod theme;

use audio::Audio;
use fb::Framebuffer;
use pad::Stick;
use scene::{Action, Nav, Scene, SysInfo};
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
    let library =
        library::Library::load(&args.systems.clone().unwrap_or_else(library::default_path));
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
                            "fire" => match scene.activate() {
                                Action::Quit => break,
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
        if let Some((_, title)) = scene.take_launch() {
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
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let gcs = sdl.game_controller()?;
    let audio = if args.no_audio {
        Audio::silent()
    } else {
        Audio::open(&sdl.audio()?)?
    };

    let mut builder = video.window(
        "omarchy-crt",
        args.w as u32 * args.scale,
        args.h as u32 * args.scale,
    );
    builder.position_centered();
    if args.fullscreen {
        builder.fullscreen_desktop();
    } else {
        builder.resizable();
    }
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
    let mut controllers = Vec::new();
    for i in 0..gcs.num_joysticks()? {
        if gcs.is_game_controller(i) {
            if let Ok(c) = gcs.open(i) {
                controllers.push(c);
            }
        }
    }
    let mut stick = Stick::new();

    let mut scene = build_scene(args);
    if let Some(c) = controllers.last() {
        scene.set_pad(Some(&c.name()));
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
    let mut child: Option<std::process::Child> = None;
    'main: loop {
        if let Some(c) = child.as_mut() {
            match c.try_wait() {
                Ok(Some(status)) => {
                    child = None;
                    scene.game_finished(status.success());
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("wait failed: {e}");
                    child = None;
                    scene.game_finished(false);
                }
            }
        }
        for ev in pump.poll_iter() {
            let mut start = false;
            let mut nav = None;
            let mut fire = false;
            let mut fav = false;
            match ev {
                Event::Quit { .. } => break 'main,
                Event::KeyDown {
                    keycode: Some(k),
                    repeat: false,
                    ..
                } => match k {
                    Keycode::Q => break 'main,
                    Keycode::Escape | Keycode::Backspace => nav = Some(Nav::Back),
                    Keycode::Space | Keycode::Return => {
                        start = true;
                        fire = true;
                    }
                    Keycode::Up | Keycode::K => nav = Some(Nav::Up),
                    Keycode::Down | Keycode::J => nav = Some(Nav::Down),
                    Keycode::Left | Keycode::H => nav = Some(Nav::Left),
                    Keycode::Right | Keycode::L => nav = Some(Nav::Right),
                    Keycode::F => fav = true,
                    _ => {}
                },
                Event::MouseButtonDown { .. } => start = true,
                Event::ControllerDeviceAdded { which, .. } => {
                    if let Ok(c) = gcs.open(which) {
                        scene.set_pad(Some(&c.name()));
                        controllers.push(c);
                    }
                }
                Event::ControllerDeviceRemoved { which, .. } => {
                    controllers.retain(|c| c.instance_id() != which);
                    match controllers.last() {
                        Some(c) => scene.set_pad(Some(&c.name())),
                        None => scene.set_pad(None),
                    }
                }
                Event::ControllerAxisMotion { axis, value, .. } => stick.set(axis, value),
                Event::ControllerButtonDown { button, .. } => match button {
                    Button::A | Button::Start => {
                        start = true;
                        fire = true;
                    }
                    Button::DPadUp => nav = Some(Nav::Up),
                    Button::DPadDown => nav = Some(Nav::Down),
                    Button::DPadLeft => nav = Some(Nav::Left),
                    Button::DPadRight => nav = Some(Nav::Right),
                    Button::B | Button::Back => nav = Some(Nav::Back),
                    Button::Y => fav = true,
                    _ => {}
                },
                _ => {}
            }
            let is_input = start || nav.is_some() || fire || fav;
            if scene.is_running() {
                continue;
            }
            if is_input && scene.touch(now()) {
                continue;
            }
            if start && !scene.boot_started() {
                scene.start_boot(now());
                continue;
            }
            if let Some(n) = nav {
                scene.navigate(n);
            }
            if fav {
                scene.toggle_favorite();
            }
            if fire {
                match scene.activate() {
                    Action::Quit => break 'main,
                    Action::Launch(cmd) => {
                        if let Err(e) = menu::launch(&cmd) {
                            eprintln!("launch failed: {e}");
                        }
                    }
                    Action::None => {}
                }
            }
        }

        if let Some((mut cmd, title)) = scene.take_launch() {
            match cmd.spawn() {
                Ok(c) => {
                    eprintln!("running {title}");
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
        scene.draw(&mut fb, t);
        fb.roll(scene.roll(), t as f32);
        fb.apply_gain(scene.power());
        for s in scene.take_sounds() {
            audio.play(s);
        }
        for data in scene.take_samples() {
            audio.play_samples(data);
        }
        audio.ambient(scene.wants_ambient());

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
