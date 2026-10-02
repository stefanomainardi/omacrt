//! The consoles as voxel models, built from their real measurements.
//!
//! One voxel is 3.5 mm, so a console about 26 cm wide is about 75 voxels
//! across. Where a part sits was measured on Evan Amos's photographs and on
//! RetroArch's top views, as fractions of the body, and a detail smaller
//! than a voxel (a name, a logo) is stylised to a few cells of colour, the
//! way sprites of the period did it.

use crate::fb::rgb;
use crate::voxel::{Face, Model};

/// Millimetres to voxels.
fn mm(v: f32) -> i32 {
    (v / 3.5).round() as i32
}

/// The model for a system, if one has been built.
pub fn model(name: &str) -> Option<Model> {
    Some(match name {
        "snes" | "sfc" => snes(),
        "saturn" => saturn(),
        "n64" => n64(),
        "nes" | "famicom" => nes(),
        "megadrive" | "genesis" | "md" => megadrive(),
        "psx" | "playstation" => psx(),
        "dreamcast" | "dc" => dreamcast(),
        "neogeo" => neogeo(),
        "arcade" | "mame" | "mame2003" | "fbneo" | "naomi" | "model3" => cabinet(),
        "gb" | "gbc" | "gameboy" => gameboy(),
        "gamecube" | "ngc" | "gcn" => gamecube(),
        "scummvm" => pc(),
        "boombox" => boombox(),
        "television" => television(),
        // The same set switched off: its tube dark.
        "television-off" => {
            let mut m = television();
            m.lamps_on = false;
            m
        }
        _ => return None,
    })
}

/// The Super Nintendo as Europe and Japan had it (the Super Famicom's
/// shape): 200 by 242 by 72 mm, light grey, the darker deck with the slot
/// and the four coloured dots, POWER, eject and RESET in front of it, the
/// name at the front, two ports and the lamp.
fn snes() -> Model {
    let (w, d, h) = (mm(200.0), mm(242.0), mm(72.0));
    let mut m = Model::new(w, d, h);
    let body = m.mat(rgb(204, 204, 208));
    let deck = m.mat(rgb(150, 150, 158));
    let dark = m.mat(rgb(62, 62, 68));
    let button = m.mat(rgb(214, 214, 218));
    let eject = m.mat(rgb(132, 132, 140));
    let reset = m.mat(rgb(76, 76, 82));
    let port = m.mat(rgb(170, 170, 176));
    let rib = m.mat(rgb(180, 180, 186));
    let lamp = m.lamp(rgb(230, 40, 40));
    let (blue, red, green, yellow) = (
        m.mat(rgb(60, 100, 210)),
        m.mat(rgb(220, 50, 50)),
        m.mat(rgb(60, 170, 80)),
        m.mat(rgb(240, 200, 40)),
    );
    let top = h - 4;
    // The body, its vertical edges rounded.
    m.rounded(0, 0, 0, w, d, top, 5, body);
    // Ribs along the back of the top.
    m.cube(3, d - 7, top, w - 6, 5, 1, body);
    for x in (4..w - 4).step_by(2) {
        m.cube(x, d - 7, top, 1, 5, 1, rib);
    }
    // The deck: 8 to 92 per cent across, 12 to 72 per cent from the back.
    let (dx0, dx1) = (w * 8 / 100, w * 92 / 100);
    let (dy0, dy1) = (d * 28 / 100, d * 88 / 100);
    m.rounded(dx0, dy0, top, dx1 - dx0, dy1 - dy0, 2, 3, deck);
    // The cartridge slot near the back of the deck, a dark channel.
    let sy = d * 65 / 100;
    m.cube(w * 20 / 100, sy, top + 1, w * 60 / 100, 2, 1, 0);
    m.cube(w * 20 / 100, sy, top, w * 60 / 100, 2, 1, dark);
    // The four dots in their light circle at the right of the deck.
    let (lx, ly) = (w * 84 / 100, d * 78 / 100);
    m.cylinder(
        lx as f32 + 0.5,
        ly as f32 + 0.5,
        top + 1,
        2.6,
        2.6,
        1,
        button,
    );
    m.set(lx - 1, ly + 1, top + 1, blue);
    m.set(lx + 1, ly + 1, top + 1, red);
    m.set(lx - 1, ly - 1, top + 1, green);
    m.set(lx + 1, ly - 1, top + 1, yellow);
    // POWER at the left, eject in the middle, RESET at the right.
    let by = d * 32 / 100;
    m.cube(w * 14 / 100, by, top + 2, w * 11 / 100, 5, 1, button);
    m.paint_top(w * 16 / 100, by + 3, w * 7 / 100, 1, dark);
    m.paint_top(w * 36 / 100, by, w * 28 / 100, 5, eject);
    m.cube(w * 74 / 100, by, top + 2, w * 11 / 100, 5, 1, reset);
    // The name on the light strip in front of the deck.
    m.paint_top(w * 8 / 100, d * 18 / 100, w * 24 / 100, 1, dark);
    m.paint_top(w * 8 / 100, d * 14 / 100, w * 34 / 100, 1, dark);
    m.set(w * 88 / 100, d * 12 / 100, top - 1, lamp);
    // Two ports on the front: grey mouths with a dark row of pins.
    for (x0, x1) in [(8, 26), (36, 54)] {
        let (a, b) = (w * x0 / 100, w * x1 / 100);
        m.cube(a, 0, 4, b - a, 1, 6, 0);
        m.cube(a, 1, 4, b - a, 1, 6, port);
        m.cube(a + 1, 1, 6, b - a - 2, 1, 2, dark);
    }
    m
}

/// The first Saturn as Europe had it: 260 by 230 by 83 mm, near black, the
/// cartridge slot at the back, the lid for the disc in the middle with the
/// name, power, open and access along the front edge of the top, the vents
/// on the right and two ports low on the front.
fn saturn() -> Model {
    let (w, d, h) = (mm(260.0), mm(230.0), mm(83.0));
    let mut m = Model::new(w, d, h);
    let body = m.mat(rgb(46, 46, 56));
    let lid = m.mat(rgb(58, 58, 72));
    let black = m.mat(rgb(18, 18, 22));
    let button = m.mat(rgb(96, 96, 112));
    let white = m.mat(rgb(226, 226, 232));
    let port = m.mat(rgb(84, 84, 96));
    let logo = m.mat(rgb(150, 150, 164));
    let top = h - 6;
    m.rounded(0, 0, 0, w, d, top, 6, body);
    // A step along the front edge of the top, which the real one has.
    m.cube(0, 0, top - 1, w, 2, 1, 0);
    // The lid: 20 to 80 per cent across, 15 to 70 per cent from the back.
    let (lx0, lx1) = (w * 20 / 100, w * 80 / 100);
    let (ly0, ly1) = (d * 30 / 100, d * 85 / 100);
    m.rounded(lx0, ly0, top, lx1 - lx0, ly1 - ly0, 3, 9, lid);
    m.rounded(
        lx0 + 3,
        ly0 + 3,
        top + 3,
        lx1 - lx0 - 6,
        ly1 - ly0 - 8,
        1,
        7,
        lid,
    );
    // Its two ridges and the name across its front.
    m.paint_top(lx0 + 6, d * 62 / 100, (lx1 - lx0) * 45 / 100, 1, button);
    m.paint_top(
        w * 50 / 100,
        d * 55 / 100,
        (lx1 - lx0) * 40 / 100,
        1,
        button,
    );
    m.paint_top(w * 38 / 100, d * 44 / 100, w * 24 / 100, 1, white);
    m.paint_top(w * 48 / 100, d * 74 / 100, 3, 2, logo);
    // The cartridge slot at the back, in its frame.
    let (sx0, sx1) = (w * 33 / 100, w * 67 / 100);
    m.cube(sx0 - 2, d - 8, top, sx1 - sx0 + 4, 6, 1, black);
    m.cube(sx0, d - 6, top, sx1 - sx0, 2, 1, 0);
    // Power, open and access along the front of the top.
    m.cylinder(w as f32 * 0.12, 8.0, top, 3.5, 1.6, 1, button);
    m.cylinder(w as f32 * 0.50, 7.0, top, 6.0, 1.6, 1, button);
    m.cylinder(w as f32 * 0.88, 8.0, top, 3.5, 1.6, 1, button);
    // SEGA in white at the left of the top.
    m.paint_top(3, 12, 6, 1, white);
    // The vents down the right side.
    for y in (d * 25 / 100..d * 85 / 100).step_by(3) {
        m.paint_right(y, 6, 1, top - 10, black);
    }
    // Two ports low on the front.
    for (x0, x1) in [(28, 36), (42, 50)] {
        let (a, b) = (w * x0 / 100, w * x1 / 100);
        m.cube(a, 0, 3, b - a, 1, 4, 0);
        m.cube(a, 1, 3, b - a, 1, 4, port);
        m.cube(a + 1, 1, 4, b - a - 2, 1, 1, black);
    }
    m
}

/// Whether a system has a voxel model.
pub fn has(name: &str) -> bool {
    matches!(
        name,
        "snes"
            | "sfc"
            | "saturn"
            | "n64"
            | "nes"
            | "famicom"
            | "megadrive"
            | "genesis"
            | "md"
            | "psx"
            | "playstation"
            | "dreamcast"
            | "dc"
            | "neogeo"
            | "arcade"
            | "mame"
            | "mame2003"
            | "fbneo"
            | "naomi"
            | "gb"
            | "gbc"
            | "gameboy"
            | "gamecube"
            | "ngc"
            | "gcn"
            | "scummvm"
            | "boombox"
            | "television"
            | "television-off"
    )
}

/// The Nintendo 64: 260 by 190 by 73 mm, charcoal. Two round feet stick out
/// at the front corners, low down; between them, under the overhang of the
/// deck, the front carries the window with the N and the four light grey
/// ports. The broad raised back takes up the rear of the top, with the grey
/// slot cover across it and the vents in front of that; the power slider,
/// the reset button and the memory lid are on the deck.
fn n64() -> Model {
    let (w, d, h) = (mm(260.0), mm(190.0), mm(73.0));
    let mut m = Model::new(w, d, h);
    let body = m.mat(rgb(72, 72, 80));
    let feet = m.mat(rgb(62, 62, 70));
    let hump = m.mat(rgb(80, 80, 90));
    let dark = m.mat(rgb(24, 24, 28));
    let grey = m.mat(rgb(176, 176, 184));
    let white = m.mat(rgb(226, 226, 232));
    let lid = m.mat(rgb(90, 90, 100));
    let (red, green, blue, yellow) = (
        m.mat(rgb(220, 50, 50)),
        m.mat(rgb(50, 170, 80)),
        m.mat(rgb(50, 90, 220)),
        m.mat(rgb(240, 200, 40)),
    );
    let lamp = m.lamp(rgb(230, 40, 40));
    let foot = w * 24 / 100;
    let face = 5;
    let deck = 13;
    // The low body behind, and the two round feet at the front corners.
    m.rounded(1, face, 0, w - 2, d - face, 6, 6, feet);
    m.rounded(0, 0, 0, foot, 20, 6, 7, feet);
    m.rounded(w - foot, 0, 0, foot, 20, 6, 7, feet);
    // The upper body: its front is set back from the feet and carries the
    // window and the ports under the lip of the deck.
    m.rounded(3, face, 5, w - 6, d - face - 1, deck - 5, 8, body);
    m.rounded(2, face - 1, deck - 1, w - 4, d - face, 1, 8, body);
    // The broad raised back.
    let (hx0, hx1) = (w * 13 / 100, w * 87 / 100);
    let hy0 = d * 42 / 100;
    m.rounded(hx0, hy0, deck, hx1 - hx0, d - hy0 - 2, 3, 9, hump);
    m.rounded(
        hx0 + 3,
        hy0 + 3,
        deck + 3,
        hx1 - hx0 - 6,
        d - hy0 - 8,
        1,
        8,
        hump,
    );
    // The grey slot cover across it, its mouth, and the vents in front.
    let (sx0, sx1) = (w * 30 / 100, w * 70 / 100);
    m.cube(sx0, d * 66 / 100, deck + 4, sx1 - sx0, 7, 1, grey);
    m.cube(sx0 + 2, d * 68 / 100, deck + 4, sx1 - sx0 - 4, 2, 1, dark);
    for x in (w * 26 / 100..w * 74 / 100).step_by(2) {
        m.paint_top(x, hy0 + 4, 1, 3, dark);
    }
    // Power slider on the left, reset on the right, the memory lid between.
    m.cube(w * 13 / 100, d * 26 / 100, deck, 7, 4, 1, dark);
    m.cube(w * 14 / 100, d * 27 / 100, deck + 1, 3, 2, 1, grey);
    m.cylinder(w as f32 * 0.83, d as f32 * 0.30, deck, 3.0, 2.2, 1, dark);
    m.paint_top(w * 38 / 100, d * 14 / 100, w * 24 / 100, d * 20 / 100, lid);
    m.paint_top(w * 38 / 100, d * 14 / 100, w * 24 / 100, 1, dark);
    // The window on the front, with the name and the N.
    let (wx, wz) = (w * 43 / 100, 6);
    m.cube(wx, face, wz, w * 14 / 100, 1, 6, dark);
    m.cube(wx + 1, face, wz + 5, w * 14 / 100 - 2, 1, 1, white);
    for (i, c) in [(0, red), (1, green), (2, blue), (3, yellow)] {
        m.set(wx + 3 + i, face, wz + 2 + (i % 2), c);
    }
    m.set(w / 2, face, 5, lamp);
    // Four light grey ports with their dark mouths.
    for x0 in [w * 26 / 100, w * 34 / 100, w * 59 / 100, w * 67 / 100] {
        m.cube(x0, face, 6, 5, 1, 4, grey);
        m.cube(x0 + 1, face, 7, 3, 1, 2, dark);
    }
    m
}

/// The NES: 256 by 203 by 85 mm, a light grey box. The ribbed panel sits at
/// the front right of the top with a dark strip behind it; the front has the
/// lid with the name in red, the dark band with power, reset and the lamp,
/// and the black end with the two ports.
fn nes() -> Model {
    let (w, d, h) = (mm(256.0), mm(203.0), mm(80.0));
    let mut m = Model::new(w, d, h);
    let body = m.mat(rgb(200, 198, 194));
    let rib = m.mat(rgb(170, 168, 164));
    let black = m.mat(rgb(40, 40, 44));
    let band = m.mat(rgb(118, 118, 122));
    let lidc = m.mat(rgb(206, 204, 200));
    let seam = m.mat(rgb(160, 158, 154));
    let red = m.mat(rgb(206, 40, 40));
    let button = m.mat(rgb(72, 72, 76));
    let port = m.mat(rgb(126, 126, 130));
    let lamp = m.lamp(rgb(230, 40, 40));
    m.rounded(0, 0, 0, w, d, h, 2, body);
    // The ribbed panel and the dark strip behind it.
    let (px0, px1) = (w * 68 / 100, w * 97 / 100);
    for y in (2..d * 55 / 100).step_by(2) {
        m.paint_top(px0, y, px1 - px0, 1, rib);
    }
    m.paint_top(px0, d * 55 / 100, px1 - px0, 4, black);
    // The seam of the lid on the top.
    let (lx1, ly1) = (w * 60 / 100, d * 45 / 100);
    m.paint_top(3, ly1, lx1 - 3, 1, seam);
    m.paint_top(lx1, 1, 1, ly1, seam);
    // The front: the lid with the name, the band with the buttons, the
    // black end with the ports.
    let end = w * 68 / 100;
    m.paint_front(2, h * 45 / 100, end - 4, h * 50 / 100, lidc);
    m.paint_front(5, h * 80 / 100, 9, 1, red);
    m.paint_front(5, h * 70 / 100, 16, 1, red);
    m.paint_front(0, 0, end, h * 42 / 100, band);
    m.paint_front(4, 3, 6, 3, button);
    m.paint_front(12, 3, 6, 3, button);
    m.paint_front(end, 0, w - end, h, black);
    for x0 in [end + 4, end + 13] {
        m.paint_front(x0, 3, 6, 5, port);
        m.paint_front(x0 + 1, 5, 4, 1, black);
    }
    if let Some(y) = (0..d).find(|&y| m.get(1, y, 4) != 0) {
        m.set(1, y, 4, lamp);
    }
    m
}

/// The first Mega Drive: 280 by 212 by 70 mm, black. The grille at the back
/// left, the volume, power and reset at the front left, the raised disc with
/// the slot and the 16-BIT plate, the name and two ports on the front.
fn megadrive() -> Model {
    let (w, d, h) = (mm(280.0), mm(212.0), mm(70.0));
    let mut m = Model::new(w, d, h);
    let body = m.mat(rgb(44, 44, 52));
    let disc = m.mat(rgb(76, 76, 90));
    let inner = m.mat(rgb(54, 54, 64));
    let dark = m.mat(rgb(16, 16, 20));
    let gold = m.mat(rgb(206, 168, 72));
    let white = m.mat(rgb(220, 216, 206));
    let red = m.mat(rgb(210, 40, 40));
    let grey = m.mat(rgb(170, 170, 176));
    let top = 14;
    m.rounded(0, 0, 0, w, d, top, 4, body);
    // The grille.
    for y in (d * 58 / 100..d * 92 / 100).step_by(2) {
        m.paint_top(w * 8 / 100, y, w * 25 / 100, 1, dark);
    }
    // The disc, raised, with its inner step, the slot across it and the
    // 16-BIT plate on its front.
    let (cx, cy) = (w as f32 * 0.64, d as f32 * 0.50);
    m.cylinder(cx, cy, top, 21.0, 21.0, 3, disc);
    m.cylinder(cx, cy, top + 2, 15.0, 15.0, 1, inner);
    m.cube((cx - 13.0) as i32, (cy + 5.0) as i32, top + 1, 26, 2, 3, 0);
    m.cube((cx - 13.0) as i32, (cy + 5.0) as i32, top, 26, 2, 1, dark);
    let (px, py) = ((cx - 7.0) as i32, (cy - 16.0) as i32);
    m.cube(px, py + 3, top + 3, 14, 4, 1, dark);
    m.cube(px + 2, py + 5, top + 4, 10, 1, 1, gold);
    m.cube(px, py, top + 3, 14, 3, 1, white);
    m.set(px + 7, py + 1, top + 3, red);
    // Volume, power and reset at the front left.
    m.cube(w * 6 / 100, d * 12 / 100, top, 3, 9, 1, dark);
    m.cube(w * 6 / 100, d * 16 / 100, top + 1, 3, 2, 1, grey);
    m.cube(w * 12 / 100, d * 24 / 100, top, 11, 5, 1, dark);
    m.set(w * 14 / 100, d * 26 / 100, top + 1, red);
    m.set(w * 18 / 100, d * 26 / 100, top + 1, white);
    m.cube(w * 12 / 100, d * 12 / 100, top, 8, 3, 1, grey);
    // On the front: the name, and the two ports.
    m.paint_front(w * 66 / 100, 10, w * 20 / 100, 1, white);
    for x0 in [w * 56 / 100, w * 68 / 100] {
        m.paint_front(x0, 3, 7, 4, dark);
        m.paint_front(x0 + 1, 5, 5, 1, disc);
    }
    m
}

/// The first PlayStation: 270 by 188 by 60 mm, grey. The round lid in the
/// middle, reset and power on its left with the green lamp, open on its
/// right, the raised strip at the back, two ports under their memory card
/// slots on the front, and the grooves down the right.
fn psx() -> Model {
    let (w, d, h) = (mm(270.0), mm(188.0), mm(60.0));
    let mut m = Model::new(w, d, h);
    let body = m.mat(rgb(184, 184, 190));
    let lid = m.mat(rgb(194, 194, 200));
    let rim = m.mat(rgb(150, 150, 156));
    let button = m.mat(rgb(166, 166, 172));
    let dark = m.mat(rgb(64, 64, 70));
    let lamp = m.lamp(rgb(60, 220, 90));
    let (r, y, g, b) = (
        m.mat(rgb(224, 60, 60)),
        m.mat(rgb(240, 192, 40)),
        m.mat(rgb(60, 180, 90)),
        m.mat(rgb(60, 120, 220)),
    );
    let top = h - 3;
    m.rounded(0, 0, 0, w, d, top, 3, body);
    m.cube(
        w * 38 / 100,
        d * 82 / 100,
        top,
        w * 24 / 100,
        d * 18 / 100,
        2,
        lid,
    );
    // The lid: a disc with a dark rim.
    let (cx, cy) = (w as f32 * 0.52, d as f32 * 0.50);
    m.cylinder(cx, cy, top, 18.5, 18.5, 1, rim);
    m.cylinder(cx, cy, top, 17.5, 17.5, 1, lid);
    for (i, c) in [r, y, g, b].into_iter().enumerate() {
        m.set(
            cx as i32 - 1 + (i as i32 % 2),
            cy as i32 - 1 + (i as i32 / 2),
            top,
            c,
        );
    }
    // Reset and power on the left, open on the right.
    m.cylinder(w as f32 * 0.10, d as f32 * 0.72, top, 2.0, 1.6, 1, button);
    m.cylinder(w as f32 * 0.12, d as f32 * 0.48, top, 4.0, 3.2, 1, button);
    m.set(w * 5 / 100, d * 34 / 100, top - 1, lamp);
    m.cylinder(w as f32 * 0.88, d as f32 * 0.24, top, 4.0, 3.2, 1, button);
    // Two ports on the front, each under its memory card slot.
    for x0 in [w * 32 / 100, w * 52 / 100] {
        m.paint_front(x0, top - 3, 9, 1, dark);
        m.paint_front(x0, 3, 9, 4, button);
        m.paint_front(x0 + 1, 4, 7, 2, dark);
    }
    // The grooves down the right.
    for yy in (4..d - 4).step_by(2) {
        m.paint_right(yy, 2, 1, top - 4, rim);
    }
    m
}

/// The Dreamcast: 190 by 195 by 78 mm, pale. The round lid over most of the
/// top with the swirl and the triangle, power on the left and open on the
/// right, four ports on the front and the vents on the right.
fn dreamcast() -> Model {
    let (w, d, h) = (mm(190.0), mm(195.0), mm(72.0));
    let mut m = Model::new(w, d, h);
    let body = m.mat(rgb(222, 222, 220));
    let lid = m.mat(rgb(236, 236, 236));
    let rim = m.mat(rgb(190, 190, 192));
    let panel = m.mat(rgb(206, 206, 208));
    let dark = m.mat(rgb(36, 36, 40));
    let blue = m.mat(rgb(40, 90, 210));
    let button = m.mat(rgb(212, 212, 214));
    let top = h - 3;
    m.rounded(0, 0, 0, w, d, top, 5, body);
    let (cx, cy) = (w as f32 * 0.52, d as f32 * 0.56);
    m.cylinder(cx, cy, top, 20.0, 19.0, 1, rim);
    m.cylinder(cx, cy, top, 19.0, 18.0, 2, lid);
    // The triangle pointing at the catch, and the swirl.
    for (i, wdt) in [(0, 5), (1, 3), (2, 1)] {
        m.paint_top(w / 2 - wdt / 2, d * 18 / 100 + 2 - i, wdt, 1, rim);
    }
    for (dx, dy) in [(0, 0), (1, 0), (1, 1), (0, 2), (-1, 1)] {
        m.set(w * 62 / 100 + dx, d * 70 / 100 + dy, top + 1, blue);
    }
    // Power and open.
    m.cylinder(w as f32 * 0.12, d as f32 * 0.24, top, 3.0, 3.0, 1, button);
    m.cylinder(w as f32 * 0.88, d as f32 * 0.24, top, 3.0, 3.0, 1, button);
    // The port panel and its four ports.
    m.paint_front(w * 10 / 100, 2, w * 80 / 100, 11, panel);
    for i in 0..4 {
        let x0 = w * 15 / 100 + i * w * 19 / 100;
        m.paint_front(x0, 5, 5, 4, dark);
    }
    for yy in (d * 10 / 100..d * 40 / 100).step_by(2) {
        m.paint_right(yy, 3, 1, 8, rim);
    }
    m
}

/// The Neo Geo AES: 325 by 237 by 60 mm, black. The raised ring round the
/// wide slot, the vents behind it, the big round button on the left, the gold
/// name, two controller sockets and the memory card slot on the front.
fn neogeo() -> Model {
    let (w, d, h) = (mm(325.0), mm(237.0), mm(60.0));
    let mut m = Model::new(w, d, h);
    let body = m.mat(rgb(44, 44, 52));
    let ring = m.mat(rgb(60, 60, 70));
    let dark = m.mat(rgb(14, 14, 16));
    let gold = m.mat(rgb(206, 168, 72));
    let button = m.mat(rgb(76, 76, 88));
    let port = m.mat(rgb(70, 70, 80));
    let lamp = m.lamp(rgb(230, 40, 40));
    let top = h - 5;
    m.rounded(0, 0, 0, w, d, top, 3, body);
    // The ring and the slot inside it.
    let (x0, y0, rw, rd) = (w * 22 / 100, d * 30 / 100, w * 66 / 100, d * 52 / 100);
    m.rounded(x0, y0, top, rw, rd, 3, 9, ring);
    m.rounded(x0 + 6, y0 + 7, top + 1, rw - 12, rd - 14, 2, 4, 0);
    m.rounded(x0 + 6, y0 + 7, top, rw - 12, rd - 14, 1, 4, dark);
    for y in (d * 86 / 100..d * 96 / 100).step_by(2) {
        m.paint_top(w * 58 / 100, y, w * 30 / 100, 1, dark);
    }
    // The big round button and the gold name.
    m.cylinder(w as f32 * 0.12, d as f32 * 0.45, top, 5.0, 4.0, 1, button);
    m.cylinder(
        w as f32 * 0.12,
        d as f32 * 0.47,
        top + 1,
        3.0,
        2.0,
        1,
        button,
    );
    m.paint_top(w * 36 / 100, d * 16 / 100, w * 22 / 100, 1, gold);
    // Two sockets and the memory card slot.
    for xs in [w * 15 / 100, w * 42 / 100] {
        m.paint_front(xs, 3, 10, 5, port);
        m.paint_front(xs + 1, 5, 8, 1, dark);
    }
    m.paint_front(w * 70 / 100, 4, w * 20 / 100, 2, dark);
    if let Some(y) = (0..d).find(|&y| m.get(4, y, 6) != 0) {
        m.set(4, y, 6, lamp);
    }
    m
}

/// An upright arcade cabinet, in voxels rather than millimetres: an upright
/// is a type rather than one machine. Side panels with T-molding in pink, the
/// marquee lit from inside, the screen set back in its bezel with a tiny game
/// on it, the control panel with its stick and buttons, and the coin door.
fn cabinet() -> Model {
    let (w, d, h) = (24, 28, 62);
    let mut m = Model::new(w, d, h);
    let side = m.mat(rgb(40, 34, 50));
    let body = m.mat(rgb(58, 48, 70));
    let panel = m.mat(rgb(74, 64, 90));
    let tmold = m.mat(rgb(255, 79, 163));
    let marquee = m.lamp(rgb(255, 120, 190));
    let glass = m.glow(rgb(18, 36, 52));
    let pixel = m.lamp(rgb(120, 255, 150));
    let alien = m.lamp(rgb(255, 110, 110));
    let coin = m.glow(rgb(255, 158, 60));
    let stick = m.mat(rgb(220, 40, 40));
    let (b1, b2, b3) = (
        m.mat(rgb(220, 40, 40)),
        m.mat(rgb(240, 200, 40)),
        m.mat(rgb(60, 120, 220)),
    );
    let door = m.mat(rgb(46, 38, 56));
    m.cube(0, 0, 0, 2, d, h, side);
    m.cube(w - 2, 0, 0, 2, d, h, side);
    m.cube(2, 6, 0, w - 4, d - 6, h, body);
    // The lower front, the control panel sticking out, the coin door.
    m.cube(2, 4, 0, w - 4, 2, 26, body);
    m.cube(2, 0, 24, w - 4, 6, 3, panel);
    m.paint_front(8, 6, 8, 12, door);
    m.paint_front(9, 13, 2, 3, coin);
    m.paint_front(13, 13, 2, 3, coin);
    m.cube(7, 2, 27, 1, 1, 2, stick);
    for (x, c) in [(12, b1), (15, b2), (18, b3)] {
        m.set(x, 2, 26, c);
    }
    // The screen, set back, with scanlines and a tiny game.
    m.cube(2, 6, 30, w - 4, 3, 22, glass);
    for (x, z, c) in [
        (8, 44, alien),
        (12, 46, alien),
        (16, 44, alien),
        (11, 34, pixel),
        (12, 34, pixel),
        (12, 35, pixel),
    ] {
        m.set(x, 6, z, c);
    }
    // The marquee.
    m.cube(2, 4, 52, w - 4, 3, 8, marquee);
    // T-molding down the front edges of the side panels.
    for z in 0..h {
        m.set(1, 0, z, tmold);
        m.set(w - 2, 0, z, tmold);
    }
    m
}

/// The original Game Boy, standing up as it would on a shelf: 90 by 32 by
/// 148 mm, warm grey, the rounded bottom right corner, the dark bezel with
/// the green screen, the cross, the two magenta buttons, start and select,
/// and the speaker's slots.
fn gameboy() -> Model {
    let (w, d, h) = (mm(90.0), mm(32.0), mm(148.0));
    let mut m = Model::new(w, d, h);
    let body = m.mat(rgb(196, 196, 190));
    let bezel = m.mat(rgb(84, 84, 104));
    let screen = m.lamp(rgb(140, 170, 40));
    let pixel = m.lamp(rgb(48, 88, 32));
    let dark = m.mat(rgb(40, 40, 46));
    let magenta = m.mat(rgb(160, 30, 90));
    let pill = m.mat(rgb(120, 120, 130));
    let blue = m.mat(rgb(40, 50, 140));
    m.cube(0, 0, 0, w, d, h, body);
    // The bottom right corner rounded, through the whole depth.
    let r = 7;
    for z in 0..r {
        for x in w - r..w {
            let (dx, dz) = ((x - (w - r)) as f32, (r - 1 - z) as f32);
            if dx * dx + dz * dz > (r * r) as f32 {
                for y in 0..d {
                    m.set(x, y, z, 0);
                }
            }
        }
    }
    // The bezel and the screen, with a little of a game on it.
    let (bx, bz, bw, bh) = (3, h * 52 / 100, w - 6, h * 40 / 100);
    m.cube(bx, 0, bz, bw, 1, bh, bezel);
    m.cube(bx + 4, 0, bz + 3, bw - 8, 1, bh - 7, screen);
    for (x, z) in [
        (bx + 6, bz + 5),
        (bx + 7, bz + 5),
        (bx + 12, bz + 8),
        (bx + 13, bz + 8),
        (bx + 10, bz + 12),
    ] {
        m.set(x, 0, z, pixel);
    }
    m.cube(bx + 2, 0, bz + bh - 2, bw - 10, 1, 1, blue);
    // The cross, the two buttons, start and select, the speaker.
    let cz = h * 34 / 100;
    m.cube(4, 0, cz - 1, 7, 1, 3, dark);
    m.cube(6, 0, cz - 3, 3, 1, 7, dark);
    m.cube(w - 9, 0, cz, 3, 1, 3, magenta);
    m.cube(w - 5, 0, cz + 2, 3, 1, 3, magenta);
    m.cube(w / 2 - 5, 0, h * 20 / 100, 3, 1, 1, pill);
    m.cube(w / 2, 0, h * 20 / 100, 3, 1, 1, pill);
    for i in 0..4 {
        m.cube(w - 12 + i * 2, 0, 3 + i, 1, 1, 5, dark);
    }
    m
}

/// The GameCube: 150 by 161 by 110 mm, indigo. The round lid on the top with
/// the cube in its middle, the handle at the back, open, reset and power on
/// the front of the top, and four controller ports low on the front.
fn gamecube() -> Model {
    let (w, d, h) = (mm(150.0), mm(161.0), mm(110.0));
    let mut m = Model::new(w, d, h);
    let body = m.mat(rgb(84, 70, 140));
    let lid = m.mat(rgb(100, 86, 160));
    let rim = m.mat(rgb(60, 50, 104));
    let grey = m.mat(rgb(170, 170, 180));
    let dark = m.mat(rgb(30, 26, 44));
    let logo = m.mat(rgb(190, 180, 220));
    let top = h - 5;
    m.rounded(0, 0, 0, w, d, top, 4, body);
    // The lid.
    let (cx, cy) = (w as f32 / 2.0, d as f32 * 0.48);
    m.cylinder(cx, cy, top, 16.0, 16.0, 1, rim);
    m.cylinder(cx, cy, top, 15.0, 15.0, 2, lid);
    for (dx, dy) in [
        (0, 0),
        (1, 0),
        (0, 1),
        (1, 1),
        (-1, 0),
        (2, 1),
        (0, -1),
        (1, 2),
    ] {
        m.set(cx as i32 + dx - 1, cy as i32 + dy - 1, top + 2, logo);
    }
    // The handle across the back.
    m.cube(4, d - 5, top, 2, 4, 5, body);
    m.cube(w - 6, d - 5, top, 2, 4, 5, body);
    m.cube(4, d - 5, top + 4, w - 8, 4, 1, body);
    // Open, reset and power along the front of the top.
    m.cylinder(w as f32 * 0.18, 5.0, top, 2.5, 2.0, 1, grey);
    m.cylinder(w as f32 * 0.50, 5.0, top, 2.0, 1.5, 1, grey);
    m.cylinder(w as f32 * 0.82, 5.0, top, 2.5, 2.0, 1, grey);
    // Four ports low on the front.
    for i in 0..4 {
        let x0 = w * 12 / 100 + i * w * 20 / 100;
        m.paint_front(x0, 3, 6, 5, grey);
        m.paint_front(x0 + 1, 5, 4, 1, dark);
    }
    m
}

/// A PC of the years ScummVM's games come from, in voxels rather than
/// millimetres: a beige desktop case with its floppy drives and lamps, and
/// on it a monitor whose screen shows a little of an adventure.
fn pc() -> Model {
    let (w, d, h) = (40, 34, 42);
    let mut m = Model::new(w, d, h);
    let beige = m.mat(rgb(214, 206, 184));
    let shade = m.mat(rgb(186, 178, 156));
    let dark = m.mat(rgb(40, 38, 34));
    let sky = m.lamp(rgb(90, 140, 220));
    let grass = m.lamp(rgb(70, 160, 70));
    let hero = m.lamp(rgb(230, 200, 120));
    let led = m.lamp(rgb(60, 220, 90));
    let disk = m.lamp(rgb(240, 160, 40));
    // The case.
    m.cube(0, 0, 0, w, d, 12, beige);
    m.cube(0, 0, 11, w, d, 1, shade);
    // Two floppy drives, the lamps, the button.
    m.paint_front(w * 55 / 100, 7, 14, 1, dark);
    m.paint_front(w * 55 / 100, 4, 14, 1, dark);
    m.paint_front(4, 5, 2, 1, led);
    m.paint_front(8, 5, 2, 1, disk);
    m.paint_front(4, 8, 4, 2, shade);
    // The monitor on top: a deep tube behind a front with its screen.
    m.cube(6, 6, 12, 28, 24, 2, shade);
    m.rounded(9, 12, 14, 22, 18, 22, 4, beige);
    m.cube(5, 4, 14, 30, 8, 28, beige);
    let (sx, sz) = (8, 18);
    m.cube(sx, 4, sz, 24, 1, 20, dark);
    m.cube(sx + 2, 4, sz + 8, 20, 1, 10, sky);
    m.cube(sx + 2, 4, sz + 2, 20, 1, 6, grass);
    m.cube(sx + 9, 4, sz + 5, 2, 1, 4, hero);
    m.set(31, 4, 16, led);
    m
}

/// A disc of cells on the front of a model, facing the viewer: centred on
/// (cx, cz) across the front, from `y` back `deep` cells.
#[allow(clippy::too_many_arguments)]
fn front_disc(m: &mut Model, cx: f32, cz: f32, r: f32, y: i32, deep: i32, mat: u8) {
    for k in (cz - r) as i32 - 1..=(cz + r) as i32 + 1 {
        for i in (cx - r) as i32 - 1..=(cx + r) as i32 + 1 {
            let (u, v) = (i as f32 + 0.5 - cx, k as f32 + 0.5 - cz);
            if u * u + v * v <= r * r {
                for j in y..y + deep {
                    m.set(i, j, k, mat);
                }
            }
        }
    }
}

/// A radio cassette recorder of the eighties, at seven millimetres a cell
/// because it is twice a console's size: 600 by 170 by 300 mm, gunmetal,
/// a chrome handle, two speakers with their cones set back in chrome rings,
/// the cassette deck between them with its two reels behind the window,
/// the tuning scale above it, the piano keys under it, a VU lamp, and the
/// aerial up from the right. Its lamps are the window and the scale, lit
/// while something plays.
fn boombox() -> Model {
    let (w, d, h) = (86, 24, 62);
    let mut m = Model::new(w, d, h);
    let body = m.mat(rgb(70, 72, 80));
    let body_hi = m.mat(rgb(104, 108, 118));
    let chrome = m.mat(rgb(196, 200, 210));
    let grille = m.mat(rgb(30, 30, 36));
    let cone = m.mat(rgb(52, 52, 60));
    let cap = m.mat(rgb(150, 152, 160));
    let window = m.lamp(rgb(255, 170, 90));
    let reel = m.mat(rgb(40, 34, 30));
    let scale = m.lamp(rgb(255, 200, 110));
    let vu = m.lamp(rgb(110, 255, 120));
    let (red, key) = (m.mat(rgb(220, 50, 50)), m.mat(rgb(200, 200, 204)));
    m.rounded(0, 2, 0, w, d - 2, 36, 3, body);
    m.cube(0, 2, 34, w, d - 2, 2, body_hi);
    // The handle: two posts and a bar.
    for x in [10, 74] {
        m.cube(x, 10, 36, 2, 4, 7, chrome);
    }
    m.cube(10, 10, 42, 66, 4, 2, chrome);
    // The speakers, set back in their rings.
    for cx in [18.0f32, 68.0] {
        front_disc(&mut m, cx, 17.0, 14.0, 2, 1, chrome);
        front_disc(&mut m, cx, 17.0, 12.5, 2, 2, 0);
        front_disc(&mut m, cx, 17.0, 12.5, 4, 1, grille);
        front_disc(&mut m, cx, 17.0, 8.0, 3, 1, cone);
        front_disc(&mut m, cx, 17.0, 3.0, 3, 1, cap);
    }
    // The deck: a window with its two reels, the scale above, the keys.
    m.cube(34, 2, 11, 18, 1, 15, 0);
    m.cube(34, 3, 11, 18, 1, 15, window);
    for rx in [39.0f32, 47.0] {
        front_disc(&mut m, rx, 18.0, 2.5, 3, 1, reel);
    }
    m.cube(32, 2, 28, 22, 1, 4, scale);
    for i in 0..6 {
        m.cube(34 + i * 3, 2, 7, 2, 1, 3, if i == 0 { red } else { key });
    }
    m.cube(30, 2, 20, 2, 1, 5, vu);
    // The aerial, up from the right shoulder.
    for k in 0..19 {
        m.set(80 - k / 6, 12, 36 + k, chrome);
    }
    m
}

/// A television of the eighties, at seven millimetres a cell: 450 by 390 by
/// 380 mm, grey plastic, the tube's face bulging a cell out of the cabinet
/// and glowing with its scanlines, two knobs and the speaker's slots on the
/// right, feet, and the rabbit ears in a V on top.
fn television() -> Model {
    let (w, d, h) = (64, 56, 84);
    let mut m = Model::new(w, d, h);
    let body = m.mat(rgb(150, 146, 140));
    let body_lo = m.mat(rgb(100, 96, 92));
    let bezel = m.mat(rgb(40, 40, 44));
    let (glass, glass_lo) = (m.lamp(rgb(150, 200, 255)), m.lamp(rgb(100, 150, 220)));
    let knob = m.mat(rgb(50, 50, 56));
    let slot = m.mat(rgb(30, 30, 34));
    let rod = m.mat(rgb(200, 204, 212));
    m.rounded(0, 2, 3, w, d - 2, 51, 4, body);
    m.cube(0, 2, 3, w, d - 2, 2, body_lo);
    for x in [4, w - 8] {
        m.cube(x, 10, 0, 4, 30, 3, body_lo);
    }
    // The tube, a cell proud of the front, in its dark bezel.
    m.cube(4, 1, 8, 42, 1, 42, bezel);
    for z in 11..47 {
        for x in 7..43 {
            // The corners of a tube are round.
            let (u, v) = ((x as f32 - 24.5) / 18.0, (z as f32 - 28.5) / 18.0);
            if u.powi(4) + v.powi(4) <= 1.0 {
                m.set(x, 0, z, if z % 2 == 0 { glass } else { glass_lo });
                m.set(x, 1, z, bezel);
            }
        }
    }
    // The knobs, standing out, and the speaker's slots under them.
    for z in [40, 30] {
        front_disc(&mut m, 55.0, z as f32, 3.0, 0, 2, knob);
    }
    for z in (10..24).step_by(3) {
        m.cube(50, 1, z, 10, 1, 1, slot);
    }
    // The rabbit ears.
    for k in 0..30 {
        m.set(30 - k / 2, 30, 54 + k, rod);
        m.set(33 + k / 2, 30, 54 + k, rod);
    }
    m.cube(28, 26, 54, 8, 8, 2, body_lo);
    m
}

/// How the thing a console plays goes into it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Way {
    /// Dropped in from above: a cartridge into its slot, a disc into its lid.
    Down,
    /// Pushed in from the front: the NES's cartridge, a floppy, a coin.
    In,
}

/// What a console plays and where it ends up once it is in, in the
/// console's own voxels: a box `w` across, `t` from front to back and `h`
/// tall, or a disc of radius `disc`.
#[derive(Clone, Copy, Debug)]
pub struct Media {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub w: i32,
    pub t: i32,
    pub h: i32,
    pub color: crate::fb::Color,
    pub way: Way,
    /// The game's cover goes on its front as a label.
    pub label: bool,
    pub disc: Option<f32>,
    /// A coin under the lights of a cabinet: round, glowing, its rim a
    /// shade darker.
    pub glow: bool,
    /// The lid that opens for it, for a console that takes discs.
    pub lid: Option<Lid>,
    /// How far it is pressed down once it is in, as the NES's cartridge is.
    pub press: i32,
    /// How far above its way in a thing pushed in starts, so that seen from
    /// three quarters it comes down to the slot rather than seeming to slide
    /// along the floor.
    pub arc: i32,
}

/// A lid, as the box of cells it occupies when shut (x0..x1, y0..y1,
/// z0..z1), hinged along its back top edge.
#[derive(Clone, Copy, Debug)]
pub struct Lid {
    pub x0: i32,
    pub y0: i32,
    pub z0: i32,
    pub x1: i32,
    pub y1: i32,
    pub z1: i32,
}

fn cart(x: i32, y: i32, z: i32, w: i32, t: i32, h: i32, color: crate::fb::Color) -> Media {
    Media {
        x,
        y,
        z,
        w,
        t,
        h,
        color,
        way: Way::Down,
        label: true,
        disc: None,
        glow: false,
        lid: None,
        press: 0,
        arc: 0,
    }
}

fn disc(cx: f32, cy: f32, z: i32, r: f32, lid: Lid) -> Media {
    Media {
        x: cx.round() as i32,
        y: cy.round() as i32,
        z,
        w: 0,
        t: 0,
        h: 1,
        color: rgb(200, 204, 214),
        way: Way::Down,
        label: true,
        disc: Some(r),
        glow: false,
        lid: Some(lid),
        press: 0,
        arc: 0,
    }
}

/// Where each console takes its game, from the same measurements as its
/// model.
pub fn media(name: &str) -> Option<Media> {
    Some(match name {
        "snes" | "sfc" => {
            let (w, d, h) = (mm(200.0), mm(242.0), mm(72.0));
            cart(
                w * 22 / 100,
                d * 65 / 100 - 2,
                h - 8,
                w * 56 / 100,
                5,
                24,
                rgb(150, 150, 158),
            )
        }
        "megadrive" | "genesis" | "md" => {
            let (w, d) = (mm(280.0), mm(212.0));
            let (cx, cy) = (w as f32 * 0.64, d as f32 * 0.50);
            cart(
                (cx - 12.0) as i32,
                (cy + 4.0) as i32,
                11,
                24,
                4,
                28,
                rgb(30, 30, 34),
            )
        }
        "n64" => {
            let (w, d) = (mm(260.0), mm(190.0));
            cart(
                w * 33 / 100,
                d * 68 / 100 - 1,
                12,
                w * 34 / 100,
                4,
                22,
                rgb(150, 150, 160),
            )
        }
        "gb" | "gbc" | "gameboy" => {
            let (w, d, h) = (mm(90.0), mm(32.0), mm(148.0));
            cart(w / 2 - 8, d - 4, h - 12, 16, 2, 19, rgb(150, 150, 150))
        }
        "neogeo" => {
            let (w, d, h) = (mm(325.0), mm(237.0), mm(60.0));
            let (x0, y0, rw, rd) = (w * 22 / 100, d * 30 / 100, w * 66 / 100, d * 52 / 100);
            cart(
                x0 + 8,
                y0 + (rd - 14) / 2 + 4,
                h - 15,
                rw - 16,
                7,
                30,
                rgb(30, 30, 34),
            )
        }
        "nes" | "famicom" => {
            let (w, h) = (mm(256.0), mm(80.0));
            let mut m = cart(
                6,
                2,
                h * 55 / 100,
                w * 68 / 100 - 12,
                26,
                4,
                rgb(150, 150, 150),
            );
            m.way = Way::In;
            m.label = false;
            m
        }
        "psx" | "playstation" => {
            let (w, d, h) = (mm(270.0), mm(188.0), mm(60.0));
            let top = h - 3;
            let lid = Lid {
                x0: 21,
                y0: 8,
                z0: top,
                x1: 61,
                y1: 47,
                z1: top + 1,
            };
            disc(w as f32 * 0.52, d as f32 * 0.50, top - 1, 14.0, lid)
        }
        "saturn" => {
            let (w, d, h) = (mm(260.0), mm(230.0), mm(83.0));
            let top = h - 6;
            let lid = Lid {
                x0: w * 20 / 100,
                y0: d * 30 / 100,
                z0: top,
                x1: w * 80 / 100,
                y1: d * 85 / 100,
                z1: top + 4,
            };
            disc(w as f32 * 0.50, d as f32 * 0.57, top - 1, 15.0, lid)
        }
        "dreamcast" | "dc" => {
            let (w, d, h) = (mm(190.0), mm(195.0), mm(72.0));
            let top = h - 3;
            let lid = Lid {
                x0: 7,
                y0: 11,
                z0: top,
                x1: 50,
                y1: 51,
                z1: top + 2,
            };
            disc(w as f32 * 0.52, d as f32 * 0.56, top - 1, 15.0, lid)
        }
        "gamecube" | "ngc" | "gcn" => {
            let (w, d, h) = (mm(150.0), mm(161.0), mm(110.0));
            let top = h - 5;
            let lid = Lid {
                x0: 5,
                y0: 5,
                z0: top,
                x1: 39,
                y1: 40,
                z1: top + 2,
            };
            disc(w as f32 * 0.50, d as f32 * 0.48, top - 1, 12.0, lid)
        }
        "arcade" | "mame" | "mame2003" | "fbneo" | "naomi" | "model3" => Media {
            x: 7,
            y: 6,
            z: 11,
            w: 6,
            t: 1,
            h: 6,
            color: rgb(240, 196, 70),
            way: Way::In,
            label: false,
            disc: None,
            glow: true,
            lid: None,
            press: 0,
            arc: 18,
        },
        "scummvm" => Media {
            x: 23,
            y: 0,
            z: 4,
            w: 12,
            t: 10,
            h: 1,
            color: rgb(40, 60, 140),
            way: Way::In,
            label: true,
            disc: None,
            glow: false,
            lid: None,
            press: 0,
            arc: 10,
        },
        _ => return None,
    })
}

/// How far the thing travels before it is in.
const TRAVEL: i32 = 22;

/// Swing a lid open by `angle` radians about its back top edge. The cells
/// are lifted out, the well under them is darkened with its spindle, and the
/// lid is set back in turned, each target cell taking the source cell that
/// turns onto it so the lid keeps its shape at any angle.
fn swing(m: &mut Model, lid: Lid, angle: f32, well: u8, spindle: u8) {
    let (lw, ld, lh) = (lid.x1 - lid.x0, lid.y1 - lid.y0, lid.z1 - lid.z0);
    let mut shut = vec![0u8; (lw * ld * lh) as usize];
    for z in 0..lh {
        for y in 0..ld {
            for x in 0..lw {
                let c = m.get(lid.x0 + x, lid.y0 + y, lid.z0 + z);
                if c != 0 {
                    shut[((z * ld + y) * lw + x) as usize] = c;
                    m.set(lid.x0 + x, lid.y0 + y, lid.z0 + z, 0);
                }
            }
        }
    }
    // The well the lid covered.
    for y in lid.y0 + 2..lid.y1 - 2 {
        for x in lid.x0 + 2..lid.x1 - 2 {
            // Only where the lid's bottom layer covered the body.
            if shut[((y - lid.y0) * lw + (x - lid.x0)) as usize] != 0
                && m.get(x, y, lid.z0 - 1) != 0
            {
                m.set(x, y, lid.z0 - 1, well);
            }
        }
    }
    let (cx, cy) = ((lid.x0 + lid.x1) / 2, (lid.y0 + lid.y1) / 2);
    m.cylinder(cx as f32, cy as f32, lid.z0 - 1, 2.0, 2.0, 1, spindle);
    let (hy, hz) = (lid.y1 as f32, lid.z1 as f32);
    let (s, c) = angle.sin_cos();
    let reach = ld as f32 + 2.0;
    for tz in lid.z0..(hz + reach) as i32 {
        for ty in (hy - reach) as i32..(hy + lh as f32 + 2.0) as i32 {
            for tx in lid.x0..lid.x1 {
                let (dy, dz) = (ty as f32 + 0.5 - hy, tz as f32 + 0.5 - hz);
                let (sy, sz) = (dy * c - dz * s, dy * s + dz * c);
                let (y, z) = (
                    (sy + hy - 0.5).round() as i32 - lid.y0,
                    (sz + hz - 0.5).round() as i32 - lid.z0,
                );
                if y < 0 || z < 0 || y >= ld || z >= lh {
                    continue;
                }
                let cell = shut[((z * ld + y) * lw + (tx - lid.x0)) as usize];
                if cell != 0 {
                    m.set(tx, ty, tz, cell);
                }
            }
        }
    }
}

/// The console at one moment of taking its game: the lid `lid` of the way
/// open (0 shut, 1 fully), the game `p` of the way in (0 outside, 1 in),
/// and its lamps on or off. Returns the model, and where the game's label
/// goes and on which faces, for the renderer.
pub fn scene(name: &str, lid: f32, p: f32, lamps: bool) -> Option<(Model, [f32; 4], Face)> {
    let mut base = model(name)?;
    base.lamps_on = lamps;
    let md = media(name)?;
    // Below zero the game is not there yet: a disc waits for its lid.
    let shown = p >= 0.0;
    let p = p.clamp(0.0, 1.0);
    let off = ((1.0 - p) * TRAVEL as f32).round() as i32;
    // Room above for all of the game as it comes down, and for a lid
    // standing open; room in front for a game pushed in.
    let lid_room = md.lid.map_or(0, |l| l.y1 - l.y0 + 2);
    let (front, top) = match md.way {
        Way::Down => (0, (TRAVEL + md.h + 2).max(lid_room)),
        Way::In => (TRAVEL + 2, 0),
    };
    let mut m = base.padded(front, top);
    if let Some(l) = md.lid {
        let well = m.mat(rgb(30, 30, 36));
        let spindle = m.mat(rgb(120, 120, 130));
        let open = lid.clamp(0.0, 1.0) * 1.75;
        if open > 0.01 {
            swing(&mut m, l, open, well, spindle);
        }
    }
    let mat = if md.glow {
        m.glow(md.color)
    } else if md.label {
        m.decal(rgb(230, 230, 232))
    } else {
        m.mat(md.color)
    };
    let (x, mut y, mut z) = (md.x, md.y + front, md.z);
    match md.way {
        Way::Down => z += off,
        Way::In if md.press > 0 => {
            // Slid in over the first four fifths, pressed down in the last.
            let slide = (p / 0.8).min(1.0);
            let down = ((p - 0.8) / 0.2).clamp(0.0, 1.0);
            y -= ((1.0 - slide) * TRAVEL as f32).round() as i32;
            z -= (down * md.press as f32).round() as i32;
        }
        Way::In => {
            y -= off;
            z += ((1.0 - p) * md.arc as f32).round() as i32;
        }
    }
    if !shown {
        return Some((m, [0.0, 0.0, 1.0, 1.0], Face::Front));
    }
    if let Some(r) = md.disc {
        // A disc shows its print on its top; the hole in its middle.
        m.cylinder(x as f32, y as f32, z, r, r, 1, mat);
        let hole = m.mat(rgb(150, 154, 170));
        m.cylinder(x as f32, y as f32, z, 1.6, 1.6, 1, hole);
        let (xf, yf) = (x as f32, y as f32);
        Some((m, [xf - r, yf - r, xf + r, yf + r], Face::Top))
    } else if md.label && md.t > md.h {
        // A floppy, lying flat: the metal shutter on the edge that goes in
        // first, the label with the cover on its top at the other end.
        let body = m.mat(md.color);
        let metal = m.mat(rgb(176, 180, 190));
        m.cube(x, y, z, md.w, md.t, md.h, body);
        let back = y + md.t;
        m.cube(
            x + md.w * 3 / 10,
            back - 4,
            z,
            md.w * 4 / 10,
            4,
            md.h,
            metal,
        );
        let (lx0, lx1, ly0, ly1) = (x + 1, x + md.w - 1, y + 1, back - 5);
        m.cube(lx0, ly0, z + md.h - 1, lx1 - lx0, ly1 - ly0, 1, mat);
        let rect = [lx0 as f32, ly0 as f32, lx1 as f32, ly1 as f32];
        Some((m, rect, Face::Top))
    } else if md.label {
        // A cartridge: its shell, the grip ridges along its top, and the
        // label set into the upper part of its front with a border of shell
        // round it.
        let shell = m.mat(md.color);
        let ridge = m.mat(crate::fb::lerp_color(md.color, 0, 0.35));
        m.cube(x, y, z, md.w, md.t, md.h, shell);
        for i in (x + 1..x + md.w - 1).step_by(2) {
            m.cube(i, y, z + md.h - 2, 1, md.t, 2, ridge);
        }
        let (lx0, lx1) = (x + 2, x + md.w - 2);
        let (lz0, lz1) = (z + md.h * 35 / 100, z + md.h - 4);
        m.cube(lx0, y, lz0, lx1 - lx0, 1, lz1 - lz0, mat);
        let rect = [lx0 as f32, lz0 as f32, lx1 as f32, lz1 as f32];
        Some((m, rect, Face::Front))
    } else if md.glow {
        let rim = m.mat(crate::fb::lerp_color(md.color, 0, 0.4));
        let r = md.w as f32 / 2.0;
        for k in 0..md.h {
            for i in 0..md.w {
                let (dx, dz) = (i as f32 + 0.5 - r, k as f32 + 0.5 - r);
                let d2 = dx * dx + dz * dz;
                if d2 <= r * r {
                    let c = if d2 > (r - 1.0) * (r - 1.0) { rim } else { mat };
                    m.cube(x + i, y, z + k, 1, md.t, 1, c);
                }
            }
        }
        Some((m, [0.0, 0.0, 1.0, 1.0], Face::Front))
    } else {
        m.cube(x, y, z, md.w, md.t, md.h, mat);
        let rect = [x as f32, z as f32, (x + md.w) as f32, (z + md.h) as f32];
        Some((m, rect, Face::Front))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What one frame of the arrival costs, for each model, at the scale the
    /// stage draws it. Not run by default: it measures, it does not check.
    /// `cargo test --release consoles::tests::arrival_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn arrival_cost() {
        for name in [
            "snes",
            "saturn",
            "n64",
            "nes",
            "megadrive",
            "psx",
            "dreamcast",
            "neogeo",
            "arcade",
            "gb",
            "gamecube",
            "scummvm",
        ] {
            let m = model(name).unwrap();
            let t = std::time::Instant::now();
            let frames = 30;
            for i in 0..frames {
                let v = crate::voxel::View {
                    yaw: i as f32 * 0.21,
                    pitch: 0.55,
                    scale: 1.4,
                };
                std::hint::black_box(crate::voxel::render(&m, &v, 0xffffff, 0));
            }
            let ms = t.elapsed().as_secs_f64() * 1000.0 / frames as f64;
            println!("{name:10} {ms:.2} ms a frame");
        }
    }

    #[test]
    fn every_model_fits_its_box_and_is_not_empty() {
        for name in [
            "snes",
            "saturn",
            "n64",
            "nes",
            "megadrive",
            "psx",
            "dreamcast",
            "neogeo",
            "arcade",
            "gb",
            "gamecube",
            "scummvm",
        ] {
            let m = model(name).unwrap();
            // A Game Boy stands up and is thin; everything else lies down.
            assert!(m.w > 20 && m.d > 5 && m.h > 10, "{name} is the wrong size");
            assert_ne!(m.get(m.w / 2, m.d / 2, 1), 0, "{name} has no body");
        }
    }
}
