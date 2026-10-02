//! 8x8 pixel icons for the menu, in the spirit of Omarchy's menu glyphs.
//!
//! `#` is the body, `+` the body in the light, `s` the body in shade and `o`
//! a second colour for the detail that tells an icon apart. An icon names
//! its colours as theme roles, so it follows the theme; a plain one takes
//! whatever colour it is drawn in. `bitmap` still draws only the `#` layer,
//! which is what a one-colour use of an icon wants.

use crate::fb::{Color, Framebuffer, lerp_color, scale};
use omacrt_shell::theme::Theme;

/// A theme colour, by role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hue {
    /// Whatever colour the caller draws it in.
    Plain,
    Accent,
    Magenta,
    Cyan,
    Yellow,
    Fg,
    Green,
    Red,
    Blue,
    Orange,
    Dim,
    Paper,
}

impl Hue {
    fn of(self, th: &Theme, plain: Color) -> Color {
        match self {
            Hue::Plain => plain,
            Hue::Accent => th.accent,
            Hue::Magenta => th.magenta,
            Hue::Cyan => th.cyan,
            Hue::Yellow => th.yellow,
            Hue::Fg => th.fg,
            Hue::Green => th.green,
            Hue::Red => th.red,
            Hue::Blue => th.blue,
            Hue::Orange => th.orange,
            Hue::Dim => th.dim,
            Hue::Paper => th.paper,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Icon {
    pub rows: [&'static str; 8],
    pub body: Hue,
    pub detail: Hue,
}

impl Icon {
    pub const fn plain(rows: [&'static str; 8]) -> Self {
        Icon {
            rows,
            body: Hue::Plain,
            detail: Hue::Plain,
        }
    }

    pub const fn art(rows: [&'static str; 8], body: Hue, detail: Hue) -> Self {
        Icon { rows, body, detail }
    }
}

impl std::ops::Deref for Icon {
    type Target = [&'static str];
    fn deref(&self) -> &[&'static str] {
        &self.rows
    }
}

/// Draw an icon in its own colours: lit when it is the one under the
/// cursor, sunk towards the ground when it is not. `plain` is the colour a
/// plain icon takes, and `gain` fades the whole thing in or out.
// A box, the icon, the theme and three ways it can look: the argument list
// is what drawing an icon is.
#[allow(clippy::too_many_arguments)]
pub fn paint(
    fb: &mut Framebuffer,
    x: i32,
    y: i32,
    icon: &Icon,
    th: &Theme,
    plain: Color,
    lit: bool,
    gain: f32,
) {
    let mut body = icon.body.of(th, plain);
    let mut detail = icon.detail.of(th, plain);
    if !lit && icon.body != Hue::Plain {
        body = lerp_color(th.bg, body, 0.72);
        detail = lerp_color(th.bg, detail, 0.72);
    }
    let light = lerp_color(body, th.paper, 0.5);
    let shade = scale(body, 0.55);
    for (j, row) in icon.rows.iter().enumerate() {
        for (i, ch) in row.chars().enumerate() {
            let c = match ch {
                '#' => body,
                '+' => light,
                's' => shade,
                'o' => detail,
                _ => continue,
            };
            fb.put(x + i as i32, y + j as i32, scale(c, gain));
        }
    }
}

pub const GAMEPAD: Icon = Icon::art(
    [
        "........", ".+#####.", "#+.###o#", "#...#o#s", "##.####s", "###..##s", ".#s..s#.",
        "........",
    ],
    Hue::Accent,
    Hue::Paper,
);
pub const NOTE: Icon = Icon::art(
    [
        "...##...", "...#+#..", "...#..#.", "...#....", ".###....", "#++#....", "####....",
        ".ss.....",
    ],
    Hue::Cyan,
    Hue::Paper,
);
pub const RESUME: Icon = Icon::art(
    [
        "##......", "#+##....", "#++###..", "#+######", "#####ss.", "###ss...", "#s......",
        "........",
    ],
    Hue::Green,
    Hue::Paper,
);
pub const SPOTIFY: Icon = Icon::plain([
    "..####..", ".#....#.", "#.####.#", "#......#", "#..###.#", "#......#", ".#.##.#.", "..####..",
]);
pub const FOLDER: Icon = Icon::art(
    [
        "........", "###.....", "#+######", "#++++++#", "#++++++#", "#++++++#", "#######s",
        "........",
    ],
    Hue::Yellow,
    Hue::Paper,
);
pub const STAR: Icon = Icon::art(
    [
        "...#....", "..+##...", "#+######", ".######.", "..####..", ".##.##s.", "#s....s#",
        "........",
    ],
    Hue::Yellow,
    Hue::Paper,
);
pub const CLOCK: Icon = Icon::art(
    [
        "..####..", ".#....#.", "#...o..#", "#...o..s", "#...oo.s", "#......s", ".#....s.",
        "..ssss..",
    ],
    Hue::Fg,
    Hue::Orange,
);
pub const TV: Icon = Icon::art(
    [
        "..o.o...", "...o....", "########", "#oooo#+#", "#oooo#s#", "#oooo#+#", "########",
        ".s....s.",
    ],
    Hue::Fg,
    Hue::Cyan,
);
pub const PAD: Icon = Icon::art(
    [
        "........", "...##...", "..####..", "........", ".######.", "#.####.#", "#..##..#",
        ".######.",
    ],
    Hue::Accent,
    Hue::Accent,
);
pub const GEAR: Icon = Icon::art(
    [
        "...##...", ".#.+#.#.", "..+###..", "#+#..##s", "###..##s", "..##ss..", ".#.#s.s.",
        "...ss...",
    ],
    Hue::Fg,
    Hue::Dim,
);
pub const SAVER: Icon = Icon::art(
    [
        "########", "#.#....#", "#...#..#", "#.#....#", "#....#.#", "########", "...##...",
        ".######.",
    ],
    Hue::Magenta,
    Hue::Magenta,
);
pub const INFO: Icon = Icon::art(
    [
        "..####..", ".#....#.", "#..##..#", "#......#", "#..##..#", "#..##..#", ".#....#.",
        "..####..",
    ],
    Hue::Blue,
    Hue::Blue,
);
pub const DESKTOP: Icon = Icon::art(
    [
        "########", "#.####.#", "#......#", "#......#", "#......#", "########", "...##...",
        "..####..",
    ],
    Hue::Fg,
    Hue::Fg,
);
pub const POWER: Icon = Icon::art(
    [
        "...##...", ".#.+#.#.", "#..##..#", "#......s", "#......s", ".#....s.", "..ssss..",
        "........",
    ],
    Hue::Red,
    Hue::Paper,
);
pub const CONSOLE: Icon = Icon::art(
    [
        "........", "########", "#......#", "#.####.#", "#......#", "########", ".#....#.",
        "........",
    ],
    Hue::Fg,
    Hue::Fg,
);
pub const PULSE: Icon = Icon::art(
    [
        "........", "....#...", "...##...", "...#.#..", "###..#.#", "......##", ".......#",
        "........",
    ],
    Hue::Green,
    Hue::Green,
);
/// A speaker with two waves coming off it: the sounds the launcher makes.
pub const SPEAKER: Icon = Icon::art(
    [
        "..##....", ".###.#..", "####..#.", "####.#.#", "####.#.#", "####..#.", ".###.#..",
        "..##....",
    ],
    Hue::Cyan,
    Hue::Cyan,
);
pub const BRUSH: Icon = Icon::art(
    [
        ".......##",
        "......##.",
        ".....##..",
        "....##...",
        "...##....",
        ".###.....",
        "###......",
        "##.......",
    ],
    Hue::Orange,
    Hue::Orange,
);

/// 10x10 system logos, one color each. `#` is lit.
pub type Logo = [&'static str; 10];

pub const LOGO_NES: Logo = [
    "..........",
    "##########",
    "#..#.....#",
    "####.#.#.#",
    "#..#..#..#",
    "#..#.#.#.#",
    "#........#",
    "##########",
    "..........",
    "..........",
];
pub const LOGO_SNES: Logo = [
    "..........",
    "..######..",
    ".#..#...#.",
    "#.###.#..#",
    "#..#.#.#.#",
    "#....#...#",
    ".#..#...#.",
    "..######..",
    "..........",
    "..........",
];
pub const LOGO_MD: Logo = [
    "..........",
    ".########.",
    "#...#....#",
    "#.#####..#",
    "#...#..#.#",
    "#.#####..#",
    "#...#.#..#",
    ".########.",
    "..........",
    "..........",
];
pub const LOGO_SMS: Logo = [
    "..........",
    "##########",
    "#........#",
    "#.##..##.#",
    "#.##..##.#",
    "#........#",
    "#.###..#.#",
    "##########",
    "..........",
    "..........",
];
pub const LOGO_PCE: Logo = [
    "..........",
    "..######..",
    ".#......#.",
    "#..####..#",
    "#..#..#..#",
    "#..####..#",
    ".#......#.",
    "..######..",
    "..........",
    "..........",
];
pub const LOGO_GB: Logo = [
    ".#######..",
    ".#.....#..",
    ".#.###.#..",
    ".#.###.#..",
    ".#.....#..",
    ".#.#...#..",
    ".#####.#..",
    ".#.....#..",
    ".#######..",
    "..........",
];
pub const LOGO_GBA: Logo = [
    "..........",
    ".########.",
    "##......##",
    "#.#.##.#.#",
    "###.##.###",
    "#.#.##.#.#",
    "##......##",
    ".########.",
    "..........",
    "..........",
];
pub const LOGO_NEOGEO: Logo = [
    "....##....",
    "...####...",
    "....##....",
    "....##....",
    "...####...",
    "..######..",
    ".########.",
    "#.#....#.#",
    "##########",
    "..........",
];
pub const LOGO_ARCADE: Logo = [
    "....##....",
    "...####...",
    "....##....",
    "....#.....",
    "....#.....",
    "..######..",
    ".#..#...#.",
    "##########",
    "#........#",
    "##########",
];
pub const LOGO_PSX: Logo = [
    "..........",
    "..#.......",
    ".###......",
    ".#.#####..",
    ".#....#.#.",
    ".#.#..#.#.",
    "###.#.###.",
    "..#....#..",
    "..######..",
    "..........",
];
pub const LOGO_N64: Logo = [
    "..........",
    ".#.....##.",
    ".##...#.#.",
    ".#.#.#..#.",
    ".#..#...#.",
    ".#......#.",
    ".#......#.",
    ".#..###.#.",
    ".#.....##.",
    "..........",
];
pub const LOGO_DC: Logo = [
    "..........",
    "...####...",
    "..#....#..",
    ".#..##..#.",
    ".#.#..#.#.",
    ".#.#..#.#.",
    ".#..###.#.",
    "..#....#..",
    "...####...",
    "..........",
];

/// The GameCube's own mark is a cube with a G cut into it; at ten pixels a
/// square inside a square is what is left of it.
pub const LOGO_GC: Logo = [
    "..........",
    ".########.",
    ".#......#.",
    ".#.####.#.",
    ".#.#..#.#.",
    ".#.#..#.#.",
    ".#.####.#.",
    ".#......#.",
    ".########.",
    "..........",
];

/// Logo and signature color for a system name; None for unknown systems.
pub fn system_logo(name: &str) -> Option<(&'static Logo, u32)> {
    Some(match name {
        "nes" | "famicom" => (&LOGO_NES, 0xe4002b),
        "snes" | "sfc" => (&LOGO_SNES, 0x8d6bd9),
        "megadrive" | "genesis" | "md" => (&LOGO_MD, 0x3a7bd5),
        "mastersystem" | "sms" => (&LOGO_SMS, 0xd94a4a),
        "pcengine" | "pce" | "tg16" => (&LOGO_PCE, 0xf58a2e),
        "gb" | "gbc" | "gameboy" => (&LOGO_GB, 0x8bc34a),
        "gba" => (&LOGO_GBA, 0x6c5ce7),
        "neogeo" => (&LOGO_NEOGEO, 0xf1c40f),
        "arcade" | "mame" | "fbneo" | "model3" => (&LOGO_ARCADE, 0xff4fa3),
        "psx" | "playstation" => (&LOGO_PSX, 0xc8c8c8),
        "n64" => (&LOGO_N64, 0x2ecc71),
        "dreamcast" | "dc" => (&LOGO_DC, 0xff8f3f),
        "gamecube" | "ngc" | "gcn" => (&LOGO_GC, 0x6f5faa),
        "videos" | "video" | "movies" => (&LOGO_FILM, 0xf5d76e),
        _ => return None,
    })
}
pub const FILM: Icon = Icon::art(
    [
        "o##o##o#", "#o##o##o", "........", "########", "#+++++##", "#######s", "#######s",
        ".sssssss",
    ],
    Hue::Magenta,
    Hue::Paper,
);
pub const LOGO_FILM: Logo = [
    "..........",
    "##########",
    "#.##..##.#",
    "##########",
    "#........#",
    "#........#",
    "##########",
    "#.##..##.#",
    "##########",
    "..........",
];
pub const FIT: Icon = Icon::art(
    [
        "########", "#......#", "#.####.#", "#.#..#.#", "#.#..#.#", "#.####.#", "#......#",
        "########",
    ],
    Hue::Cyan,
    Hue::Cyan,
);

/// Bar chart, for the system monitor.
pub const CHART: Icon = Icon::art(
    [
        "........", "..#...#.", "..#..##.", "..#..##.", ".##.###.", ".##.###.", "########",
        "........",
    ],
    Hue::Green,
    Hue::Green,
);
/// A framed picture, for the photo frame.
pub const PHOTO: Icon = Icon::art(
    [
        "########", "#.....o#", "#......#", "#..+...#", "#.+++.+#", "#++++++#", "#sssssss",
        "........",
    ],
    Hue::Green,
    Hue::Yellow,
);

// -- the pause menu, one icon for each thing it does ---------------------------

/// A diskette: saving, the way every game of the period drew it.
pub const SAVE: Icon = Icon::art(
    [
        "#######.", "##ss.s##", "##ss.s##", "########", "#oooooo#", "#oooooo#", "#oooooos",
        "ssssssss",
    ],
    Hue::Blue,
    Hue::Paper,
);
/// A folder with something coming out of it: loading.
pub const LOAD: Icon = Icon::art(
    [
        "....o...", "...ooo..", "..o.o.o.", "##..o...", "#+######", "#++++++#", "#######s",
        ".sssssss",
    ],
    Hue::Yellow,
    Hue::Cyan,
);
pub const REWIND: Icon = Icon::art(
    [
        "...#...#", "..##..##", ".#+#.#+#", "#++##++#", ".#s#.#s#", "..##..##", "...#...#",
        "........",
    ],
    Hue::Cyan,
    Hue::Paper,
);
pub const FORWARD: Icon = Icon::art(
    [
        "#...#...", "##..##..", "#+#.#+#.", "#++##++#", "#s#.#s#.", "##..##..", "#...#...",
        "........",
    ],
    Hue::Cyan,
    Hue::Paper,
);
/// An hourglass, for time slowed down.
pub const SLOW: Icon = Icon::art(
    [
        "########", ".#++++#.", "..#++#..", "...##...", "..#.o#..", ".#.ooo#.", "########",
        "........",
    ],
    Hue::Magenta,
    Hue::Yellow,
);
/// A circular arrow: start again.
pub const RESET: Icon = Icon::art(
    [
        "..###...", ".#...#.#", "#.....##", "#....###", "#.......", "#......s", ".#....s.",
        "..ssss..",
    ],
    Hue::Orange,
    Hue::Paper,
);
/// A house: back to where the launcher lives.
pub const LAUNCHER: Icon = Icon::art(
    [
        "...##...", "..#+##..", ".#++###.", "########", ".#+oo##.", ".#+oo##.", ".#+oo#s.",
        "........",
    ],
    Hue::Yellow,
    Hue::Accent,
);

// -- the pads screen ---------------------------------------------------------

/// A pad drawn big enough to be recognised from a sofa: 22 by 11, `#` for the
/// body and `o` for the buttons, so it takes two colours.
pub type PadArt = [&'static str; 11];

/// A modern pad: two grips, a d-pad cut out of the body on the left, four
/// buttons on the right.
pub const PAD_TWIN: PadArt = [
    "..####..........####..",
    ".####################.",
    "######################",
    "###..#..######..oo..##",
    "##..###..####..o..o..#",
    "###..#..######..oo..##",
    "######################",
    ".####..########..####.",
    "..###..########..###..",
    "..##....######....##..",
    "..##..............##..",
];

/// A round six button pad, the shape of a Mega Drive controller.
pub const PAD_ROUND: PadArt = [
    "......##########......",
    "....##############....",
    "..##################..",
    ".####..######..oo..oo.",
    "####..######..oo..oo.#",
    "####...####...oo..oo.#",
    ".####..######..oo..oo.",
    "..##################..",
    "....##############....",
    "......##########......",
    "......................",
];

/// A pad with its sticks below the buttons.
pub const PAD_STICKS: PadArt = [
    "...####........####...",
    ".####################.",
    "######################",
    "###..#..######..oo..##",
    "##..###..####..o..o..#",
    "###..#..######..oo..##",
    "######################",
    ".####..#.####.#..####.",
    "..###.ooo####ooo.###..",
    "..###..o.####.o..###..",
    "...##..............##.",
];

/// The mark of a cable.
pub const USB: Icon = Icon::plain([
    "...##...", "..####..", "...##...", ".#.##.#.", ".#.##.##", ".###..#.", "...##...", "...##...",
]);

/// The rune, as it is printed on the pads themselves.
pub const BLUETOOTH: Icon = Icon::plain([
    "...##...", "...###..", "#..#.#..", ".#.##...", "..###...", ".#.##...", "#..#.#..", "...###..",
]);

/// Shaking, for the pad being identified.
pub const SHAKE: Icon = Icon::plain([
    "......#.", "..##..#.", ".#..#.#.", "#.##.##.", "#.##.##.", ".#..#.#.", "..##..#.", "......#.",
]);

/// An empty socket.
pub const SOCKET: Icon = Icon::plain([
    "........", ".#....#.", "..#..#..", "...##...", "...##...", "..#..#..", ".#....#.", "........",
]);

/// Something went wrong, drawn as a spark rather than a word.
pub const SPARK: Icon = Icon::plain([
    "...#....", "#..#..#.", ".#.#.#..", "..###...", "#####.#.", "..###...", ".#.#.#..", "#..#..#.",
]);

/// A battery shell, 13 by 6. The bars inside are drawn separately so they can
/// take their own colour.
pub const BATTERY: [&str; 6] = [
    "###########..",
    "#.........#..",
    "#.........###",
    "#.........###",
    "#.........#..",
    "###########..",
];

/// The bars of a battery at `level` quarters, in the same 13 by 6 cell.
pub fn battery_bars(level: u8) -> [String; 6] {
    let mut rows: [String; 6] = std::array::from_fn(|_| ".".repeat(13));
    for i in 0..level.min(4) as usize {
        let x = 2 + i * 2;
        for row in rows.iter_mut().take(5).skip(1) {
            row.replace_range(x..x + 1, "#");
        }
    }
    rows
}
