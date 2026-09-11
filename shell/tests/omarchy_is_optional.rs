//! The claim, kept true: the television does not need Omarchy.
//!
//! The README says OmaCRT runs on plain Hyprland and that the distribution is
//! not a requirement. That is the kind of sentence that is true on the day it
//! is written and quietly stops being true afterwards, because the machine it
//! is developed on is an Omarchy machine on Arch, where everything works and
//! nothing complains.
//!
//! So the source is read here rather than trusted. Two things are checked:
//! nothing new asks Omarchy for anything, and no package manager is named
//! outside the one table that maps a distribution to its own.

use std::path::{Path, PathBuf};

/// Every Rust file under `shell/src`.
fn sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut out,
    );
    out.sort();
    out
}

fn is_comment(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("//") || t.starts_with("*") || t.starts_with("/*")
}

/// Lines of code, not comments, that name Omarchy.
fn mentions(path: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter(|l| !is_comment(l))
        .filter(|l| l.to_lowercase().contains("omarchy"))
        .map(|l| l.trim().to_string())
        .collect()
}

/// What each file is allowed to say about Omarchy, and why.
///
/// A number going up means something new depends on Omarchy: either it
/// belongs to the desktop half and the count moves with a reason written
/// here, or it belongs to the television and it is a bug.
const ALLOWED: &[(&str, usize, &str)] = &[
    (
        "bin/omacrt.rs",
        8,
        "the desktop half: is Omarchy there, the plugin folder, the two rows \
         of the report that say what it adds, and the os-release id in the \
         install hints",
    ),
    (
        "crt/mod.rs",
        2,
        "the folder this project used to be called, and asking the lock \
         screen whether it is up before touching a plugin",
    ),
    (
        "plugin.rs",
        1,
        "the folder the two plugins go in. This file is the Omarchy half, and \
         the ids it installs are named in the manifests it carries",
    ),
    (
        "theme.rs",
        3,
        "the desktop's theme, read if it is there: every one of these has a \
         built-in palette behind it",
    ),
    (
        "main.rs",
        1,
        "the launcher's own --help, naming the theme file it can be given",
    ),
    (
        "scene/mod.rs",
        1,
        "the credit on the About screen: what this is built on",
    ),
    (
        "bin/omacrt-display/comp.rs",
        1,
        "the make in the EDID the leased output advertises to its own clients",
    ),
];

#[test]
fn nothing_new_asks_omarchy_for_anything() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut wrong = Vec::new();
    for path in sources() {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let found = mentions(&path).len();
        let allowed = ALLOWED
            .iter()
            .find(|(f, _, _)| *f == rel)
            .map(|(_, n, _)| *n)
            .unwrap_or(0);
        if found != allowed {
            wrong.push(format!(
                "{rel}: {found} line(s) name Omarchy, {allowed} allowed\n    {}",
                mentions(&path).join("\n    ")
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "the Omarchy half moved.\n\n{}\n\nIf this is desktop integration, put the \
         new count in ALLOWED with the reason. If it is the television, the \
         launcher, the timings or the command line, it is a bug: those work \
         without Omarchy and that is the whole claim.",
        wrong.join("\n\n")
    );
}

#[test]
fn no_package_manager_is_named_outside_the_hint_table() {
    // The hints come from /etc/os-release, so `pacman` belongs in exactly one
    // function and its tests. Anywhere else is a machine assuming it is on
    // the distribution this was written on.
    let managers = [
        "pacman",
        "apt-get",
        "apt ",
        "dnf ",
        "zypper",
        "xbps-install",
    ];
    let allowed_files = ["bin/omacrt.rs"];
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut wrong = Vec::new();
    for path in sources() {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if allowed_files.contains(&rel.as_str()) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in text.lines().filter(|l| !is_comment(l)) {
            for m in managers {
                if line.contains(m) {
                    wrong.push(format!("{rel}: {}", line.trim()));
                }
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "a package manager is named outside the install hints:\n{}\n\nThe hint \
         for a missing program comes from /etc/os-release, so that Fedora is \
         told dnf and Debian apt.",
        wrong.join("\n")
    );
}

#[test]
fn the_hints_cover_the_distributions_the_readme_claims() {
    // The README says the distribution is not a requirement. The install
    // hints are where that is either true or a lie, so the table is checked
    // against the families a reader might arrive on.
    let hints = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/bin/omacrt.rs"),
    )
    .expect("the CLI source");
    for family in [
        "arch", "fedora", "debian", "suse", "alpine", "void", "nixos",
    ] {
        assert!(
            hints.contains(&format!("\"{family}\"")),
            "no install hint for {family}: the README says the distribution \
             does not matter"
        );
    }
}
