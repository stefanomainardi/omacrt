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
    pub fn tokyo_night() -> Self {
        Self {
            name: "tokyo-night".into(),
            bg: rgb(0x0b, 0x0d, 0x14),
            fg: rgb(0xa9, 0xb1, 0xd6),
            dim: rgb(0x56, 0x5f, 0x89),
            paper: rgb(0xc0, 0xca, 0xf5),
            accent: rgb(0x7a, 0xa2, 0xf7),
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
