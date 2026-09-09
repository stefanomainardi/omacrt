//! The parts of the project that can be checked without a television.
//!
//! Everything here is a pure function over data: how a file name becomes a
//! title, how a title finds its box art, how a modeline behaves when a system
//! wants fewer lines, what the fit pipeline decides for a given source. These
//! are the places where a silent mistake shows up on the tube weeks later, so
//! they are the places worth pinning down.

use omacrt_shell::crt::output::Modeline;
use omacrt_shell::index::parse_name;
use omacrt_shell::library::clean_title;
use omacrt_shell::{covers, store, videofit};
use std::path::Path;

// ----------------------------------------------------------------- naming

#[test]
fn a_no_intro_name_splits_into_title_tags_region_and_disc() {
    let (title, tags, region, disc) = parse_name("Metal Gear Solid (USA) (Disc 2) (Rev 1)");
    assert_eq!(title, "Metal Gear Solid");
    assert_eq!(region, "USA");
    assert_eq!(disc, Some(2));
    assert!(tags.iter().any(|t| t.contains("Rev 1")), "tags: {tags:?}");
}

#[test]
fn a_title_keeps_its_brackets_out_of_the_way() {
    assert_eq!(
        clean_title(Path::new("Sonic The Hedgehog 2 (World) (Rev A).md")),
        "Sonic The Hedgehog 2"
    );
    assert_eq!(clean_title(Path::new("mslug.zip")), "mslug");
}

// ------------------------------------------------------------- box art

#[test]
fn a_thumbnail_name_escapes_what_the_repository_escapes() {
    // The libretro sets replace the characters a filesystem dislikes, and
    // the repository's own convention keeps the spaces around them.
    assert_eq!(covers::thumb_name("Sonic & Knuckles"), "Sonic _ Knuckles");
    assert!(!covers::thumb_name("Ratchet & Clank").contains('&'));
    assert_eq!(covers::thumb_name("Pac-Man"), "Pac-Man");
}

#[test]
fn normalising_a_title_drops_tags_case_and_punctuation() {
    let a = covers::normalize("Legend of Zelda, The - Ocarina of Time (USA) (Rev 2)");
    let b = covers::normalize("legend of zelda the   ocarina of time");
    assert_eq!(a, b, "{a} vs {b}");
}

#[test]
fn the_region_order_follows_the_configured_country() {
    let it = covers::regions_for("IT");
    assert!(it.iter().any(|r| r.eq_ignore_ascii_case("Europe")));
    let us = covers::regions_for("US");
    assert_eq!(us.first().copied(), Some("USA"), "{us:?}");
}

#[test]
fn every_system_label_is_a_thumbnail_set_name() {
    for system in [
        "nes",
        "snes",
        "megadrive",
        "psx",
        "n64",
        "saturn",
        "dreamcast",
    ] {
        let label = covers::label(system).unwrap_or_else(|| panic!("no label for {system}"));
        assert!(label.contains(" - "), "{system} -> {label}");
    }
}

// ------------------------------------------------------------- modelines

fn ntsc() -> Modeline {
    Modeline::parse("72 3520 3695 4033 4577 240 242 245 262 -hsync -vsync")
        .expect("the shipped NTSC modeline parses")
}

#[test]
fn the_shipped_modelines_are_15_khz() {
    let m = ntsc();
    assert!((m.hfreq_khz() - 15.73).abs() < 0.05, "{}", m.hfreq_khz());
    assert!((m.vfreq_hz() - 60.04).abs() < 0.05, "{}", m.vfreq_hz());
    assert_eq!(m.width(), 3520);
    assert_eq!(m.height(), 240);
}

#[test]
fn fewer_active_lines_keep_the_line_rate_and_the_refresh() {
    let full = ntsc();
    let cut = full.with_lines(224);
    assert_eq!(cut.height(), 224);
    assert!((cut.hfreq_khz() - full.hfreq_khz()).abs() < 0.01);
    assert!((cut.vfreq_hz() - full.vfreq_hz()).abs() < 0.01);
}

#[test]
fn an_interlaced_modeline_keeps_its_flag() {
    let m = Modeline::parse("72 3520 3695 4033 4577 480 484 490 525 -hsync -vsync interlace")
        .expect("480i parses");
    assert!(m.flags.contains("interlace"));
    assert_eq!(m.height(), 480);
}

#[test]
fn nonsense_is_not_a_modeline() {
    assert!(Modeline::parse("").is_none());
    assert!(Modeline::parse("72 3520 3695").is_none());
    assert!(Modeline::parse("seventy-two 3520 3695 4033 4577 240 242 245 262").is_none());
}

// ------------------------------------------------------------ video fit

fn probe(w: u32, h: u32, fps: f64) -> videofit::Probe {
    videofit::Probe {
        width: w,
        height: h,
        fps,
        duration: 90.0,
        ..Default::default()
    }
}

#[test]
fn a_film_at_24_fps_is_either_sped_up_or_pulled_down_but_never_both() {
    let fit = omacrt_shell::settings::VideoFit::default();
    let plan = videofit::plan(&probe(1920, 1080, 23.976), &fit);
    assert!(
        !(plan.speedup && plan.pulldown),
        "speedup {} pulldown {}",
        plan.speedup,
        plan.pulldown
    );
}

#[test]
fn the_plan_never_asks_the_tube_for_more_lines_than_it_has() {
    let fit = omacrt_shell::settings::VideoFit::default();
    for (w, h, fps) in [(1920, 1080, 25.0), (640, 480, 30.0), (720, 576, 50.0)] {
        let plan = videofit::plan(&probe(w, h, fps), &fit);
        assert!(plan.height <= 576, "{w}x{h}@{fps} -> {} lines", plan.height);
        assert!(plan.width >= 1, "{w}x{h}@{fps} -> width {}", plan.width);
    }
}

// ----------------------------------------------------------- durable files

#[test]
fn a_saved_file_survives_a_truncated_successor() {
    let dir = std::env::temp_dir().join(format!("omacrt-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("systems.toml");
    store::save(&path, "first = true\n").unwrap();
    store::save(&path, "second = true\n").unwrap();
    std::fs::write(&path, b"").unwrap();
    assert_eq!(
        store::load_string(&path).as_deref(),
        Some("first = true\n"),
        "the backup should stand in for a truncated file"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// -------------------------------------------------------- the tube follows

#[test]
fn the_core_geometry_is_read_from_the_emulator_log() {
    let log = "\
[INFO] [Core] Geometry: 640x480, Aspect: 1.333, FPS: 59.95, Sample rate: 44100.00 Hz.
[INFO] [GL] Detecting screen resolution: 3520x240.
[INFO] [Environ] SET_GEOMETRY: 320x240, Aspect: 1.333.
";
    // The last report wins: a game that changes its picture mid-play is what
    // this exists for.
    assert_eq!(
        omacrt_shell::library::core_geometry(log),
        Some((320, 240))
    );
    assert_eq!(
        omacrt_shell::library::core_geometry("nothing here"),
        None
    );
    assert_eq!(
        omacrt_shell::library::core_geometry("Geometry: 99999x99999, Aspect: 1"),
        None,
        "a nonsense size is not a geometry"
    );
}

#[test]
fn a_console_that_drew_480_lines_asks_for_480() {
    use omacrt_shell::library::default_lines;
    assert_eq!(default_lines("dreamcast"), Some(480));
    assert_eq!(default_lines("snes"), Some(224));
    assert_eq!(default_lines("nes"), Some(240));
    assert_eq!(default_lines("videos"), None);
}

// ------------------------------------------------- the picture and its shape

#[test]
fn the_picture_the_core_draws_carries_its_aspect_too() {
    use omacrt_shell::library::picture_in;
    let log = "\
[INFO] [Core] Geometry: 256x224, Aspect: 1.333, FPS: 60.10, Sample rate: 96000.00 Hz.
[INFO] [Environ] SET_GEOMETRY: 256x240, Aspect: 1.067.
";
    let p = picture_in(log).expect("the log holds a picture");
    assert_eq!((p.width, p.height), (256, 240));
    assert!((p.aspect - 1.067).abs() < 0.001);
    // What each choice asks for: the core's own aspect, or square pixels.
    assert!((p.wanted("core").unwrap() - 1.067).abs() < 0.001);
    assert!((p.wanted("pixel").unwrap() - 256.0 / 240.0).abs() < 0.001);
    assert_eq!(p.wanted("fill"), None);
    assert_eq!(p.wanted(""), None);
}

#[test]
fn square_pixels_in_a_wide_frame_land_in_the_middle_of_a_four_by_three_screen() {
    use omacrt_shell::library::viewport_keys;
    // A super resolution frame the set shows as 4:3. A 256x224 picture at
    // square pixels is 8:7, narrower than the screen, so it keeps every line
    // and loses width on both sides evenly.
    let keys = viewport_keys(3520, 240, 4.0 / 3.0, 256.0 / 224.0);
    let value = |k: &str| -> i64 {
        keys.lines()
            .find(|l| l.starts_with(k))
            .and_then(|l| l.split('"').nth(1).map(|v| v.parse().unwrap()))
            .unwrap_or_else(|| panic!("{k} is not in the keys"))
    };
    assert_eq!(value("aspect_ratio_index"), 23);
    assert_eq!(value("custom_viewport_height"), 240);
    let w = value("custom_viewport_width");
    assert_eq!(
        w,
        (3520.0f32 * (256.0 / 224.0) / (4.0 / 3.0)).round() as i64
    );
    assert_eq!(value("custom_viewport_x"), (3520 - w) / 2);
    assert_eq!(value("custom_viewport_y"), 0);

    // A picture wider than the screen keeps the whole width and loses lines.
    let wide = viewport_keys(3520, 240, 4.0 / 3.0, 16.0 / 9.0);
    assert!(wide.contains("custom_viewport_width = \"3520\""));
    assert!(wide.contains("custom_viewport_height = \"180\""));

    // The whole raster, when what is wanted is what the screen already is.
    assert!(
        viewport_keys(3520, 240, 4.0 / 3.0, 4.0 / 3.0).contains("custom_viewport_width = \"3520\"")
    );
    assert!(viewport_keys(0, 240, 4.0 / 3.0, 1.0).is_empty());
    assert!(viewport_keys(3520, 240, 4.0 / 3.0, 0.0).is_empty());
}

#[test]
fn every_shader_the_pause_menu_offers_is_installed_and_off_comes_first() {
    use omacrt_shell::library::{installed_shaders, shader_path};
    let shaders = installed_shaders();
    assert_eq!(shaders.first().map(|(name, _)| *name), Some(""));
    for (name, label) in &shaders {
        assert!(!label.is_empty());
        if !name.is_empty() {
            assert!(shader_path(name).is_some(), "{name} is not installed");
        }
    }
    assert_eq!(shader_path(""), None);
    assert_eq!(shader_path("nothing/at-all.slangp"), None);
}
