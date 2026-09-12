//! The three files the lease setup needs on the system side, and whether the
//! ones installed are the ones this version carries.
//!
//! They live outside a user's home, under `/usr/local/lib` and
//! `/etc/systemd/system`, so only root writes them and neither a package
//! upgrade of this program nor `omacrt-install` without `--system` can. An
//! install therefore falls behind silently, and the failure it produces
//! arrives much later and looks like something else: on this project's own
//! machine they sat four days behind, which is why the unit had no
//! `TimeoutStopSec` of its own and was killed part way through a hotplug on
//! every shutdown, and why the EDID went on being written without the
//! FreeSync range so the variable refresh rate disappeared at each reboot.
//!
//! Nothing here writes anything: this only answers whether they match, and
//! `omacrt doctor` says so.

use std::path::{Path, PathBuf};

/// What this version carries, against where the installer puts it.
const FILES: &[(&str, &str)] = &[
    (
        "/usr/local/lib/omacrt/crt-lease-setup.sh",
        include_str!("../../scripts/crt-lease-setup.sh"),
    ),
    (
        "/usr/local/lib/omacrt/edid-non-desktop.py",
        include_str!("../../scripts/edid-non-desktop.py"),
    ),
    (
        "/etc/systemd/system/omacrt-lease.service",
        include_str!("../../systemd/omacrt-lease.service"),
    ),
];

/// True when the lease setup is installed at all. A machine that has never
/// run `omacrt-install --system` drives the tube by hand and has nothing to
/// be behind.
pub fn installed() -> bool {
    Path::new(FILES[2].0).is_file()
}

/// The installed files that are not what this version carries.
pub fn drift() -> Vec<PathBuf> {
    if !installed() {
        return Vec::new();
    }
    FILES
        .iter()
        .filter(|(path, body)| {
            let have = std::fs::read_to_string(path).unwrap_or_default();
            settled(&have) != settled(body)
        })
        .map(|(path, _)| PathBuf::from(path))
        .collect()
}

/// The unit as installed differs from the unit in the repository by the two
/// values the installer substitutes: which connector belongs to the
/// television, and the FreeSync range. Those are this machine's answers and
/// not a version, so the comparison drops them and keeps the directive, which
/// means removing one still counts as drift.
fn settled(text: &str) -> String {
    text.lines()
        .map(|line| match line.strip_prefix("Environment=") {
            // The directive and the variable's name are kept, its value is
            // not, so removing an `Environment=` line still counts as drift.
            Some(rest) => match rest.split_once('=') {
                Some((var, _)) => format!("Environment={var}="),
                None => line.to_string(),
            },
            None => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_substituted_values_are_not_a_version() {
        let repo = "Environment=OMACRT_CONNECTOR=HDMI-A-1\nExecStart=/x\n";
        let installed = "Environment=OMACRT_CONNECTOR=DP-3\nExecStart=/x\n";
        assert_eq!(settled(repo), settled(installed));
    }

    #[test]
    fn a_missing_directive_is_drift() {
        let repo = "Environment=OMACRT_FREESYNC=48:62\nExecStart=/x\n";
        let installed = "ExecStart=/x\n";
        assert_ne!(settled(repo), settled(installed));
    }

    #[test]
    fn anything_else_changing_is_drift() {
        let repo = "TimeoutStopSec=60\nExecStart=/x\n";
        let installed = "ExecStart=/x\n";
        assert_ne!(settled(repo), settled(installed));
    }

    /// The files this compares against have to be the files the installer
    /// writes, or the check is a decoration.
    #[test]
    fn it_carries_the_three_files_the_installer_installs() {
        let installer = include_str!("../../bin/omacrt-install");
        for (path, body) in FILES {
            assert!(!body.is_empty(), "{path} is empty");
            let name = path.rsplit('/').next().unwrap();
            assert!(
                installer.contains(name),
                "{name} is not written by omacrt-install"
            );
        }
    }
}
