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
///
/// This one follows redirects, which is right for a picture or a playlist and
/// wrong for anything carrying a credential: curl sends a header given with
/// `-H` to whatever host the redirect names, so a server that redirects
/// elsewhere is handed the header. Use [`curl_no_redirect`] when the request
/// is authenticated.
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

/// The same, for a request that carries a credential: no redirect is
/// followed at all.
///
/// curl resends a header given with `-H`, or with `header =` in a config, to
/// the host a redirect names, whatever host that is, and its redirect
/// protocol list allows HTTPS to fall back to HTTP on the way. A photograph
/// server that answers a request with a redirect would therefore be handed
/// the API key in clear text. There is nothing an authenticated call here
/// needs a redirect for: every one of them addresses an API directly.
pub fn curl_no_redirect(seconds: u32, max_bytes: u64) -> Command {
    let mut cmd = Command::new("curl");
    cmd.args([
        "-fsS",
        "-A",
        USER_AGENT,
        "--proto",
        "=http,https",
        "--max-redirs",
        "0",
        "--max-time",
        &seconds.to_string(),
        "--max-filesize",
        &max_bytes.to_string(),
        "--retry",
        "1",
    ]);
    cmd
}

/// The same as [`curl`], but a redirect may only go to HTTPS.
///
/// For a download whose contents are trusted enough to be unpacked into
/// another program's directory: following a redirect is fine, following one
/// down to plain HTTP is not.
pub fn curl_https_redirect(seconds: u32, max_bytes: u64) -> Command {
    let mut cmd = curl(seconds, max_bytes);
    cmd.args(["--proto-redir", "=https"]);
    cmd
}
