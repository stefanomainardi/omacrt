//! The leash every fetch goes out on.
//!
//! The launcher has no HTTP client of its own: it asks curl, which is on any
//! machine that can install this. What matters is that all eight callers ask
//! for the same guarantees, and the way to make sure of that is for there to
//! be one place that grants them rather than eight copies of the same flags.
//!
//! A fetch may only speak HTTP or HTTPS, before and after a redirect, so a
//! server that answers with `file:///etc/shadow` or `scp://` gets nowhere. It
//! has a deadline and a size limit, because the answer is written to this
//! machine's disk. It fails on an error status rather than saving the error
//! page. And the URL is always an argument to curl, never a word in a shell
//! command line.

use std::process::Command;

/// What this project calls itself to a server it does not own. The version
/// comes from `Cargo.toml`, so it cannot drift from the one that is running.
pub const USER_AGENT: &str = concat!(
    "omacrt/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/stefanomainardi/omacrt)"
);

/// A curl that will not run longer than `seconds`, will not write more than
/// `max_bytes`, and will not follow a redirect out of HTTP. Give it the URL,
/// and anything else it needs, with `arg`.
pub fn curl(seconds: u32, max_bytes: u64) -> Command {
    let mut cmd = Command::new("curl");
    cmd.args([
        "-fsSL",
        "-A",
        USER_AGENT,
        "--proto",
        "=http,https",
        "--proto-redir",
        "=http,https",
        "--max-time",
        &seconds.to_string(),
        "--max-filesize",
        &max_bytes.to_string(),
        "--retry",
        "1",
    ]);
    cmd
}
