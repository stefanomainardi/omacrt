//! Menu items shown as an `ls` listing, loaded from
//! `~/.config/omarchy-crt/menu.toml` when present.

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Deserialize)]
pub struct Item {
    pub name: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub quit: bool,
}

#[derive(Debug, Deserialize)]
struct File {
    #[serde(default)]
    item: Vec<Item>,
}

pub fn default_items() -> Vec<Item> {
    let it = |name: &str, command: &str, quit: bool| Item {
        name: name.into(),
        command: command.into(),
        quit,
    };
    vec![
        it("games/", "", false),
        it("tv-profile", "", false),
        it("screensaver", "", false),
        it("about", "", false),
        it("desktop/", "", true),
        it("poweroff", "systemctl poweroff", false),
    ]
}

pub fn default_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(Path::new(&home).join(".config/omarchy-crt/menu.toml"))
}

pub fn load(path: &Path) -> Vec<Item> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return default_items();
    };
    match toml::from_str::<File>(&text) {
        Ok(f) if !f.item.is_empty() => f.item,
        Ok(_) => default_items(),
        Err(e) => {
            eprintln!("menu.toml: {e}; using defaults");
            default_items()
        }
    }
}

/// Launch a command detached through `sh -c`.
pub fn launch(command: &str) -> std::io::Result<()> {
    if command.trim().is_empty() {
        return Ok(());
    }
    std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
}
