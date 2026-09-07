//! The parts of the project that can be checked without a television.
//!
//! Everything here is a pure function over data: how a file name becomes a
//! title, how a title finds its box art, how a modeline behaves when a system
//! wants fewer lines, what the fit pipeline decides for a given source. These
//! are the places where a silent mistake shows up on the tube weeks later, so
//! they are the places worth pinning down.

use omarchy_crt_shell::crt::output::Modeline;
use omarchy_crt_shell::index::parse_name;
use omarchy_crt_shell::library::clean_title;
use omarchy_crt_shell::{covers, store, videofit};
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
    let fit = omarchy_crt_shell::settings::VideoFit::default();
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
    let fit = omarchy_crt_shell::settings::VideoFit::default();
    for (w, h, fps) in [(1920, 1080, 25.0), (640, 480, 30.0), (720, 576, 50.0)] {
        let plan = videofit::plan(&probe(w, h, fps), &fit);
        assert!(plan.height <= 576, "{w}x{h}@{fps} -> {} lines", plan.height);
        assert!(plan.width >= 1, "{w}x{h}@{fps} -> width {}", plan.width);
    }
}

// ----------------------------------------------------------- durable files

#[test]
fn a_saved_file_survives_a_truncated_successor() {
    let dir = std::env::temp_dir().join(format!("omarchy-crt-tests-{}", std::process::id()));
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
