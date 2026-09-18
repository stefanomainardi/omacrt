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
/// The pads overlay, the same way.
pub const PADS_ID: &str = "io.github.stefanomainardi.omacrt.pads";

const MAIN_FILES: &[(&str, &str)] = &[
    ("manifest.json", include_str!("../../plugin/manifest.json")),
    ("BarWidget.qml", include_str!("../../plugin/BarWidget.qml")),
    ("Panel.qml", include_str!("../../plugin/Panel.qml")),
    ("README.md", include_str!("../../plugin/README.md")),
];

const PADS_FILES: &[(&str, &str)] = &[
    (
        "manifest.json",
        include_str!("../../plugin/pads/manifest.json"),
    ),
    ("Pads.qml", include_str!("../../plugin/pads/Pads.qml")),
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

/// The files that belong in one plugin folder: name and body.
type Files = &'static [(&'static str, &'static str)];

/// Every folder this version installs, with the files that belong in it. The
/// first is the bar widget, which is also where the helper binary lives.
pub fn folders() -> Option<Vec<(PathBuf, Files)>> {
    let home = std::env::var_os("HOME")?;
    let plugins = PathBuf::from(home).join(".config/omarchy/plugins");
    Some(vec![
        (plugins.join(ID), MAIN_FILES),
        (plugins.join(LIBRARY_ID), LIBRARY_FILES),
        (plugins.join(PADS_ID), PADS_FILES),
    ])
}

/// The bar widget's own folder, which carries the helper the widget runs.
pub fn main_dir() -> Option<PathBuf> {
    folders().and_then(|f| f.first().map(|(d, _)| d.clone()))
}

/// Whether the plugins are installed at all. On a machine with no Omarchy
/// there is nothing to keep up to date and nothing to complain about.
pub fn installed() -> bool {
    main_dir()
        .map(|main| main.join("manifest.json").is_file())
        .unwrap_or(false)
}

/// Every file in the two folders that is not what this version carries.
///
/// The helper binary counts: the widget runs the copy in its own folder, so
/// an old one there is an old program answering the bar.
pub fn drift() -> Vec<PathBuf> {
    let Some(folders) = folders() else {
        return Vec::new();
    };
    let main = folders[0].0.clone();
    let mut out = Vec::new();
    for (dir, files) in &folders {
        for (name, body) in *files {
            let path = dir.join(name);
            if std::fs::read_to_string(&path).ok().as_deref() != Some(*body) {
                out.push(path);
            }
        }
    }
    let helper = main.join("bin/omacrt");
    if let Ok(mine) = std::env::current_exe()
        && !helper_launches(&helper, &mine)
    {
        out.push(helper);
    }
    out
}

/// True when the widget's helper already launches `mine`.
fn helper_launches(helper: &Path, mine: &Path) -> bool {
    std::fs::read_to_string(helper).ok().as_deref() == Some(&launcher_text(mine))
}

/// The helper the bar runs: a launcher for this program, not a copy of it.
///
/// The helper used to be a copy, and a copy is a snapshot. No package upgrade
/// writes into anybody's home, so the bar went on running the build that was
/// current when the plugin was last installed: two versions of one program
/// sharing one state directory, and every rename on this side became a
/// failure that appeared only through the bar. A launcher has no version of
/// its own.
///
/// A symlink would say the same thing in one inode, and Omarchy's own
/// validator refuses every symlink inside a plugin folder: a copied plugin
/// could otherwise point at arbitrary files once it lands in the trusted
/// plugins directory. This is a regular file, which that rule allows.
///
/// `exec -a "$0"` keeps this path as `argv[0]`, because the panel finds the
/// DAC watcher with `pgrep -f` on exactly this path; `/proc/self/exe` still
/// resolves to the program, which is how it finds the compositor beside
/// itself rather than in the plugin folder.
fn launcher_text(mine: &Path) -> String {
    format!(
        "#!/bin/bash\n\
         # Written by `omacrt plugin sync`. Not a copy of the program: a copy\n\
         # is a snapshot, and the bar would go on running it after an upgrade.\n\
         exec -a \"$0\" {} \"$@\"\n",
        shell_quote(mine)
    )
}

/// A path as one shell word, for a file name that may hold anything a file
/// name may hold.
fn shell_quote(p: &Path) -> String {
    format!("'{}'", p.to_string_lossy().replace('\'', "'\\''"))
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
    let Some(folders) = folders() else {
        return Err("no HOME to install into".into());
    };
    let main = folders[0].0.clone();
    if !create && !installed() {
        return Ok(Vec::new());
    }
    if crate::crt::locked() {
        return Err("the session is locked: a plugin reload would take the shell down".into());
    }
    let mut wrote = Vec::new();
    for (dir, files) in &folders {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for (name, body) in *files {
            let path = dir.join(name);
            if std::fs::read_to_string(&path).ok().as_deref() == Some(*body) {
                continue;
            }
            write_file(&path, body.as_bytes(), 0o644)?;
            wrote.push(format!("{name} in {}", dir.display()));
        }
    }
    // The helper the widget runs: a launcher for this program, never a copy.
    let helper = main.join("bin/omacrt");
    let mine = std::env::current_exe().map_err(|e| e.to_string())?;
    if !helper_launches(&helper, &mine) {
        std::fs::create_dir_all(helper.parent().unwrap_or(&main))
            .map_err(|e| format!("{}: {e}", main.display()))?;
        write_file(&helper, launcher_text(&mine).as_bytes(), 0o755)?;
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
    // Closed before the rename, not at the end of this function: a file still
    // open for writing cannot be executed, and the widget runs the helper
    // this writes. The window was small and the failure is ETXTBSY, which
    // says nothing about where it came from.
    drop(f);
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

    /// Runs the helper and gives back what it printed.
    ///
    /// Retried while the kernel calls the file busy, which here has nothing
    /// to do with the launcher: these tests share a process with a hundred
    /// others, one of them forks while this file is being written, and the
    /// child holds the inherited write descriptor until its own exec. A file
    /// anybody has open for writing cannot be executed. It clears in
    /// microseconds, and without the retry this fails about once in eight
    /// runs of the suite and never on its own.
    fn run(helper: &Path, args: &[&str]) -> std::process::Output {
        for _ in 0..200 {
            match std::process::Command::new(helper).args(args).output() {
                Err(e) if e.raw_os_error() == Some(libc::ETXTBSY) => {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                other => return other.expect("the launcher runs"),
            }
        }
        panic!("{} stayed busy for a second", helper.display());
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("omacrt-helper-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_launcher_for_this_program_is_not_drift_and_anything_else_is() {
        let root = tmp("launcher");
        let mine = root.join("omacrt");
        std::fs::write(&mine, b"program").unwrap();
        let helper = root.join("bin/omacrt");
        std::fs::create_dir_all(helper.parent().unwrap()).unwrap();

        // Nothing there at all.
        assert!(!helper_launches(&helper, &mine));

        // A copy of the program is drift even when its bytes are current:
        // it is a snapshot, and the next build leaves it behind.
        std::fs::write(&helper, b"program").unwrap();
        assert!(!helper_launches(&helper, &mine));

        // What this version writes.
        write_file(&helper, launcher_text(&mine).as_bytes(), 0o755).expect("the launcher");
        assert!(helper_launches(&helper, &mine));

        // A launcher for some other install.
        let other = root.join("other/omacrt");
        std::fs::create_dir_all(other.parent().unwrap()).unwrap();
        assert!(!helper_launches(&helper, &other));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// It must survive the characters a home directory may hold, because the
    /// path is written into a shell script.
    #[test]
    fn the_path_goes_in_as_one_shell_word() {
        let text = launcher_text(Path::new("/home/o'brien/a dir/omacrt"));
        assert!(
            text.contains(r#"exec -a "$0" '/home/o'\''brien/a dir/omacrt' "$@""#),
            "{text}"
        );
    }

    /// The arguments the bar passes arrive unchanged, and nothing of the
    /// program is copied into the plugin folder.
    #[test]
    fn the_launcher_passes_its_arguments_on_and_stays_small() {
        let root = tmp("runs");
        let target = root.join("omacrt");
        write_file(&target, b"#!/bin/sh\nprintf '%s|' \"$@\"\n", 0o755).unwrap();
        let helper = root.join("bin/omacrt");
        std::fs::create_dir_all(helper.parent().unwrap()).unwrap();
        write_file(&helper, launcher_text(&target).as_bytes(), 0o755).unwrap();

        let out = run(&helper, &["dac", "watch"]);
        assert_eq!(String::from_utf8_lossy(&out.stdout), "dac|watch|");
        assert!(std::fs::metadata(&helper).unwrap().len() < 512);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// `argv[0]` stays the helper's own path, which is what the panel's
    /// `pgrep -f` matches on to find the DAC watcher it started.
    ///
    /// The stand-in is a real program because that is what the helper points
    /// at: the kernel runs a script through its interpreter and uses the
    /// file's own path, so a script would not show the property at all.
    #[test]
    fn the_bar_still_recognises_what_it_started() {
        let Some(shell) = ["/usr/bin/bash", "/bin/bash"]
            .into_iter()
            .map(PathBuf::from)
            .find(|p| p.is_file())
        else {
            return;
        };
        let root = tmp("argv0");
        let helper = root.join("bin/omacrt");
        std::fs::create_dir_all(helper.parent().unwrap()).unwrap();
        write_file(&helper, launcher_text(&shell).as_bytes(), 0o755).unwrap();

        let out = run(&helper, &["-c", r#"printf '%s' "$0""#]);
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            helper.display().to_string()
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_files_it_carries_are_the_files_it_installs() {
        // Every name in the manifest's own folder is one this can write, so
        // an installed plugin is never a mixture of two versions.
        assert!(MAIN_FILES.iter().any(|(n, _)| *n == "manifest.json"));
        assert!(MAIN_FILES.iter().any(|(n, _)| *n == "Panel.qml"));
        assert!(LIBRARY_FILES.iter().any(|(n, _)| *n == "Library.qml"));
        assert!(PADS_FILES.iter().any(|(n, _)| *n == "Pads.qml"));
        // And none of them is empty, which is what a missing include would
        // look like from here.
        for (name, body) in MAIN_FILES.iter().chain(LIBRARY_FILES).chain(PADS_FILES) {
            assert!(body.len() > 100, "{name} is {} bytes", body.len());
        }
    }

    #[test]
    fn the_manifest_names_the_plugin_this_module_installs() {
        let (_, manifest) = MAIN_FILES[0];
        assert!(manifest.contains(ID), "the manifest does not name {ID}");
        let (_, library) = LIBRARY_FILES[0];
        let (_, pads) = PADS_FILES[0];
        assert!(
            pads.contains(PADS_ID),
            "the pads manifest does not name {PADS_ID}"
        );
        assert!(
            library.contains(LIBRARY_ID),
            "the overlay manifest does not name {LIBRARY_ID}"
        );
    }
}
