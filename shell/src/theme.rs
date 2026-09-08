//! Palette read from the current Omarchy theme (`~/.config/omarchy/current/colors.toml`).
//! Falls back to Tokyo Night when the file is missing or a key is absent.

use crate::fb::{Color, parse_hex, rgb};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Theme {
    pub name: String,
    pub bg: Color,
    pub fg: Color,
    pub dim: Color,
    pub paper: Color,
    pub accent: Color,
    pub selection: Color,
    pub green: Color,
    pub bright_green: Color,
    pub cyan: Color,
    pub blue: Color,
    pub magenta: Color,
    pub yellow: Color,
    pub orange: Color,
    pub red: Color,
}

impl Theme {
    /// A step above the background, for the dark squares of the Mode 7 floor.
    pub fn fg_dark_floor(&self) -> Color {
        crate::fb::lerp_color(self.bg, self.dim, 0.28)
    }

    pub fn tokyo_night() -> Self {
        Self {
            name: "tokyo-night".into(),
            bg: rgb(0x0b, 0x0d, 0x14),
            fg: rgb(0xa9, 0xb1, 0xd6),
            dim: rgb(0x56, 0x5f, 0x89),
            paper: rgb(0xc0, 0xca, 0xf5),
            accent: rgb(0x7a, 0xa2, 0xf7),
            selection: rgb(0x29, 0x2e, 0x42),
            green: rgb(0x9e, 0xce, 0x6a),
            bright_green: rgb(0xb9, 0xf2, 0x7c),
            cyan: rgb(0x7d, 0xcf, 0xff),
            blue: rgb(0x7d, 0xa6, 0xff),
            magenta: rgb(0xbb, 0x9a, 0xf7),
            yellow: rgb(0xe0, 0xaf, 0x68),
            orange: rgb(0xff, 0x9e, 0x64),
            red: rgb(0xf7, 0x76, 0x8e),
        }
    }

    pub fn default_path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(Path::new(&home).join(".config/omarchy/current/colors.toml"))
    }

    /// Every installed Omarchy theme: (name, colors.toml path), sorted.
    pub fn installed() -> Vec<(String, PathBuf)> {
        let home = match std::env::var_os("HOME") {
            Some(h) => PathBuf::from(h),
            None => return Vec::new(),
        };
        let dir = home.join(".local/share/omarchy/themes");
        let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.join("colors.toml").exists())
                    .map(|p| {
                        (
                            p.file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned(),
                            p.join("colors.toml"),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.sort();
        out
    }

    /// Load a specific theme directory's colors under its own name.
    pub fn load_named(path: &Path, name: &str) -> Option<Self> {
        let mut t = Self::load(path)?;
        t.name = name.to_string();
        Some(t)
    }

    /// Linear blend between two themes, for live switching.
    pub fn blend(a: &Self, b: &Self, t: f32) -> Self {
        let l = |x: Color, y: Color| crate::fb::lerp_color(x, y, t);
        Self {
            name: if t < 0.5 {
                a.name.clone()
            } else {
                b.name.clone()
            },
            bg: l(a.bg, b.bg),
            fg: l(a.fg, b.fg),
            dim: l(a.dim, b.dim),
            paper: l(a.paper, b.paper),
            accent: l(a.accent, b.accent),
            selection: l(a.selection, b.selection),
            green: l(a.green, b.green),
            bright_green: l(a.bright_green, b.bright_green),
            cyan: l(a.cyan, b.cyan),
            blue: l(a.blue, b.blue),
            magenta: l(a.magenta, b.magenta),
            yellow: l(a.yellow, b.yellow),
            orange: l(a.orange, b.orange),
            red: l(a.red, b.red),
        }
    }

    pub fn load(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        let table: toml::Table = text.parse().ok()?;
        let base = Self::tokyo_night();
        let get = |key: &str, fallback: Color| -> Color {
            table
                .get(key)
                .and_then(|v| v.as_str())
                .and_then(parse_hex)
                .unwrap_or(fallback)
        };
        // The current theme is a symlink target; use its directory name when we can.
        let name = std::fs::read_link(path.parent()?)
            .ok()
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "omarchy".to_string());
        Some(Self {
            name,
            bg: get("darker_background", base.bg),
            fg: get("foreground", base.fg),
            dim: get("dark_foreground", base.dim),
            paper: get("bright_foreground", base.paper),
            accent: get("accent", base.accent),
            selection: get("selection", base.selection),
            green: get("green", base.green),
            bright_green: get("bright_green", base.bright_green),
            cyan: get("bright_cyan", base.cyan),
            blue: get("bright_blue", base.blue),
            magenta: get("bright_magenta", base.magenta),
            yellow: get("bright_yellow", base.yellow),
            orange: get("orange", base.orange),
            red: get("red", base.red),
        })
    }
}
