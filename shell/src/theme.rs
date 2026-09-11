//! Palette read from the current Omarchy theme (`~/.config/omarchy/current/colors.toml`).
//! Falls back to Tokyo Night when the file is missing or a key is absent.

use crate::colour::{Color, parse_hex, rgb};
use std::path::{Path, PathBuf};

/// A theme as the Style screen needs it: its name, where its colours live if
/// they live anywhere, and the swatch already read.
pub struct Installed {
    pub name: String,
    /// The file the colours came from. Empty for one of the built in
    /// palettes, which are compiled in and need nothing installed.
    pub path: PathBuf,
    /// Accent and green, or nothing when the file would not parse.
    pub swatch: Option<(Color, Color)>,
}

impl Installed {
    pub fn built_in(&self) -> bool {
        self.path.as_os_str().is_empty()
    }
}

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
        crate::colour::lerp_color(self.bg, self.dim, 0.28)
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

    /// Palettes that need nothing installed.
    ///
    /// Without Omarchy there is no `~/.local/share/omarchy/themes` to read,
    /// and the Style screen offered one row that followed nothing. These are
    /// the same shape as a theme file, chosen to be four different rooms
    /// rather than four shades of one.
    pub fn built_in() -> Vec<Theme> {
        vec![
            Self::tokyo_night(),
            Self::phosphor(),
            Self::amber(),
            Self::trinitron(),
        ]
    }

    /// A monochrome monitor that has been on since 1983.
    pub fn phosphor() -> Self {
        Self {
            name: "phosphor".into(),
            bg: rgb(0x03, 0x0a, 0x05),
            fg: rgb(0x6d, 0xd0, 0x82),
            dim: rgb(0x2c, 0x5c, 0x38),
            paper: rgb(0x9c, 0xf0, 0xad),
            accent: rgb(0x4c, 0xff, 0x7a),
            selection: rgb(0x0d, 0x2a, 0x14),
            green: rgb(0x6d, 0xd0, 0x82),
            bright_green: rgb(0xb6, 0xff, 0xc4),
            cyan: rgb(0x7a, 0xe8, 0xc0),
            blue: rgb(0x5a, 0xc8, 0x9a),
            magenta: rgb(0x9c, 0xf0, 0xad),
            yellow: rgb(0xcf, 0xf7, 0x8a),
            orange: rgb(0xa8, 0xe0, 0x60),
            red: rgb(0xff, 0x8a, 0x6a),
        }
    }

    /// The other monochrome, the one with the warm tube.
    pub fn amber() -> Self {
        Self {
            name: "amber".into(),
            bg: rgb(0x14, 0x0b, 0x02),
            fg: rgb(0xe8, 0xa5, 0x3d),
            dim: rgb(0x6b, 0x47, 0x14),
            paper: rgb(0xff, 0xcb, 0x78),
            accent: rgb(0xff, 0xb0, 0x33),
            selection: rgb(0x33, 0x1d, 0x06),
            green: rgb(0xd2, 0xc2, 0x4a),
            bright_green: rgb(0xf6, 0xe4, 0x7a),
            cyan: rgb(0xe0, 0xb6, 0x6a),
            blue: rgb(0xc8, 0x92, 0x40),
            magenta: rgb(0xff, 0xc0, 0x8a),
            yellow: rgb(0xff, 0xd4, 0x6a),
            orange: rgb(0xff, 0x9b, 0x2e),
            red: rgb(0xff, 0x6b, 0x35),
        }
    }

    /// A television, not a monitor: the colours a shadow mask gives you.
    pub fn trinitron() -> Self {
        Self {
            name: "trinitron".into(),
            bg: rgb(0x07, 0x08, 0x10),
            fg: rgb(0xd6, 0xdc, 0xe8),
            dim: rgb(0x4e, 0x56, 0x66),
            paper: rgb(0xf2, 0xf4, 0xf8),
            accent: rgb(0x3f, 0x8e, 0xff),
            selection: rgb(0x14, 0x1a, 0x2a),
            green: rgb(0x3f, 0xc5, 0x6b),
            bright_green: rgb(0x6d, 0xf0, 0x95),
            cyan: rgb(0x3f, 0xc7, 0xd8),
            blue: rgb(0x3f, 0x8e, 0xff),
            magenta: rgb(0xd0, 0x5f, 0xd0),
            yellow: rgb(0xf2, 0xc0, 0x3f),
            orange: rgb(0xf2, 0x8c, 0x3f),
            red: rgb(0xe8, 0x3f, 0x50),
        }
    }

    pub fn default_path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(Path::new(&home).join(".config/omarchy/current/colors.toml"))
    }

    /// Every installed Omarchy theme, sorted, with the two colours the Style
    /// screen shows beside each name. The swatch is read here, once, because
    /// the alternative is a file read and a TOML parse per visible row per
    /// frame on a screen that is drawn sixty times a second.
    pub fn installed_with_swatches() -> Vec<Installed> {
        let mut out: Vec<Installed> = Self::installed()
            .into_iter()
            .map(|(name, path)| {
                let swatch = Self::load_named(&path, &name).map(|t| (t.accent, t.green));
                Installed { name, path, swatch }
            })
            .collect();
        // The built in palettes come last, and they are always there: a
        // machine with no Omarchy themes installed still has something to
        // choose between. One whose name the desktop already has is left
        // out: two rows called tokyo-night is a puzzle, not a choice.
        let installed: Vec<String> = out.iter().map(|i| i.name.clone()).collect();
        out.extend(
            Self::built_in()
                .into_iter()
                .filter(|t| !installed.contains(&t.name))
                .map(|t| Installed {
                    name: t.name.clone(),
                    path: PathBuf::new(),
                    swatch: Some((t.accent, t.green)),
                }),
        );
        out
    }

    /// One of the compiled in palettes, by name.
    pub fn by_name(name: &str) -> Option<Theme> {
        Self::built_in().into_iter().find(|t| t.name == name)
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
        let l = |x: Color, y: Color| crate::colour::lerp_color(x, y, t);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A machine with no desktop themes still has something to choose
    /// between, and every one of them resolves without touching a disk.
    #[test]
    fn the_built_in_palettes_need_nothing_installed() {
        let built = Theme::built_in();
        assert!(built.len() >= 4, "one palette is not a choice");
        for t in &built {
            let found = Theme::by_name(&t.name).expect("resolves by name");
            assert_eq!(found.name, t.name);
            assert_ne!(found.bg, found.paper, "{}: unreadable", t.name);
        }
        assert!(Theme::by_name("no such palette").is_none());
    }

    /// The Style screen reads this list, and a row with no swatch draws
    /// nothing beside its name.
    #[test]
    fn every_built_in_palette_carries_its_swatch() {
        let listed = Theme::installed_with_swatches();
        let built: Vec<&Installed> = listed.iter().filter(|i| i.built_in()).collect();
        // A name the desktop already has is not offered twice.
        let names: Vec<&String> = listed.iter().map(|i| &i.name).collect();
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "a palette is listed twice");
        for i in built {
            assert!(i.swatch.is_some(), "{} has no swatch", i.name);
            assert!(i.path.as_os_str().is_empty());
        }
    }
}
