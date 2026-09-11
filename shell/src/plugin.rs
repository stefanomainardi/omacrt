//! The desktop plugins, carried inside the binary.
//!
//! Omarchy loads a plugin from a folder in the user's own configuration, so
//! the two plugins are copies: the bar widget with its panel, and the library
//! overlay beside it. Copies go out of date. The binary is upgraded by a
//! package manager that cannot write into somebody's home directory, and the
//! folder it cannot write is the one holding a copy of the program that was
//! shipped with the last version. This machine ran a release-old copy for a
//! week, and the only reason anybody found out was a sound coming from the
//! wrong speakers.
//!
//! A symlink would be the obvious answer and is refused: Omarchy's plugin
//! validator does not allow one inside a plugin folder. So the files are
//! compiled in instead, and the folder is treated as a cache of them: what
//! differs is written, what matches is left alone, and the same bytes the
//! installer would copy are the bytes this writes.

use std::path::{Path, PathBuf};

/// The bar widget, its panel, and the manifest Omarchy reads.
pub const ID: &str = "io.github.stefanomainardi.omacrt";
/// The library overlay: a plugin of its own, summoned by the panel.
pub const LIBRARY_ID: &str = "io.github.stefanomainardi.omacrt.library";

const MAIN_FILES: &[(&str, &str)] = &[
    ("manifest.json", include_str!("../../plugin/manifest.json")),
    ("BarWidget.qml", include_str!("../../plugin/BarWidget.qml")),
    ("Panel.qml", include_str!("../../plugin/Panel.qml")),
    ("README.md", include_str!("../../plugin/README.md")),
];

const LIBRARY_FILES: &[(&str, &str)] = &[
    (
        "manifest.json",
        include_str!("../../plugin/library/manifest.json"),
    ),
    (
        "Library.qml",
        include_str!("../../plugin/library/Library.qml"),
    ),
];

/// Where the two folders live.
pub fn dirs() -> Option<(PathBuf, PathBuf)> {
    let home = std::env::var_os("HOME")?;
    let plugins = PathBuf::from(home).join(".config/omarchy/plugins");
    Some((plugins.join(ID), plugins.join(LIBRARY_ID)))
}

/// Whether the plugins are installed at all. On a machine with no Omarchy
/// there is nothing to keep up to date and nothing to complain about.
pub fn installed() -> bool {
    dirs()
        .map(|(main, _)| main.join("manifest.json").is_file())
        .unwrap_or(false)
}

/// Every file in the two folders that is not what this version carries.
///
/// The helper binary counts: the widget runs the copy in its own folder, so
/// an old one there is an old program answering the bar.
pub fn drift() -> Vec<PathBuf> {
    let Some((main, library)) = dirs() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (dir, files) in [(&main, MAIN_FILES), (&library, LIBRARY_FILES)] {
        for (name, body) in files {
            let path = dir.join(name);
            if std::fs::read_to_string(&path).ok().as_deref() != Some(*body) {
                out.push(path);
            }
        }
    }
    let helper = main.join("bin/omacrt");
    if let Ok(mine) = std::env::current_exe()
        && std::fs::read(&helper).ok() != std::fs::read(&mine).ok()
    {
        out.push(helper);
    }
    out
}

/// Write what differs, and say what was written.
///
/// `create` makes the folders when they are not there, which is what a first
/// install wants; without it an absent plugin is left absent, because a
/// machine without Omarchy has nowhere to put one.
///
/// Never while the lock screen is up: a changed file in a plugin folder makes
/// the shell reload the plugin, and a reload under the lock screen takes
/// Quickshell down with it.
pub fn sync(create: bool) -> Result<Vec<String>, String> {
    let Some((main, library)) = dirs() else {
        return Err("no HOME to install into".into());
    };
    if !create && !installed() {
        return Ok(Vec::new());
    }
    if crate::crt::locked() {
        return Err("the session is locked: a plugin reload would take the shell down".into());
    }
    let mut wrote = Vec::new();
    for (dir, files) in [(&main, MAIN_FILES), (&library, LIBRARY_FILES)] {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for (name, body) in files {
            let path = dir.join(name);
            if std::fs::read_to_string(&path).ok().as_deref() == Some(*body) {
                continue;
            }
            write_file(&path, body.as_bytes(), 0o644)?;
            wrote.push(format!("{name} in {}", dir.display()));
        }
    }
    // The helper the widget runs.
    let helper = main.join("bin/omacrt");
    let mine = std::env::current_exe().map_err(|e| e.to_string())?;
    if std::fs::read(&helper).ok() != std::fs::read(&mine).ok() {
        std::fs::create_dir_all(helper.parent().unwrap_or(&main))
            .map_err(|e| format!("{}: {e}", main.display()))?;
        let body = std::fs::read(&mine).map_err(|e| format!("{}: {e}", mine.display()))?;
        write_file(&helper, &body, 0o755)?;
        wrote.push(format!("bin/omacrt in {}", main.display()));
    }
    Ok(wrote)
}

/// Beside it and then renamed, so the shell never reads half a file and the
/// widget is never in the middle of running the binary being replaced.
fn write_file(path: &Path, body: &[u8], mode: u32) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    let tmp = path.with_extension("omacrt-new");
    let mut f = std::fs::File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
    f.write_all(body)
        .map_err(|e| format!("{}: {e}", tmp.display()))?;
    f.sync_all()
        .map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode))
        .map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", path.display())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_files_it_carries_are_the_files_it_installs() {
        // Every name in the manifest's own folder is one this can write, so
        // an installed plugin is never a mixture of two versions.
        assert!(MAIN_FILES.iter().any(|(n, _)| *n == "manifest.json"));
        assert!(MAIN_FILES.iter().any(|(n, _)| *n == "Panel.qml"));
        assert!(LIBRARY_FILES.iter().any(|(n, _)| *n == "Library.qml"));
        // And none of them is empty, which is what a missing include would
        // look like from here.
        for (name, body) in MAIN_FILES.iter().chain(LIBRARY_FILES) {
            assert!(body.len() > 100, "{name} is {} bytes", body.len());
        }
    }

    #[test]
    fn the_manifest_names_the_plugin_this_module_installs() {
        let (_, manifest) = MAIN_FILES[0];
        assert!(manifest.contains(ID), "the manifest does not name {ID}");
        let (_, library) = LIBRARY_FILES[0];
        assert!(
            library.contains(LIBRARY_ID),
            "the overlay manifest does not name {LIBRARY_ID}"
        );
    }
}
