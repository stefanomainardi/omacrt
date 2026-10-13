//! YouTube from the tube: a search through yt-dlp, run on a thread so the
//! launcher keeps drawing while it takes its few seconds. Playback goes
//! through mpv like any other video (mpv resolves the link with yt-dlp too).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

/// The yt-dlp to run, for the search and for mpv alike.
///
/// YouTube now hands out its streams only to a client that answers a
/// JavaScript challenge, and yt-dlp answers it with the solver scripts of
/// `yt-dlp-ejs` and a JavaScript runtime. A machine can have more than one
/// yt-dlp: on this one a pipx copy came first on the PATH, in a virtual
/// environment that could not see the system's solver scripts, and every
/// video closed the moment it opened. So the first yt-dlp on the PATH that
/// can load the solver is the one used, and the first of any otherwise.
pub fn binary() -> PathBuf {
    static FOUND: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    FOUND
        .get_or_init(|| {
            let all = candidates();
            all.iter()
                .find(|p| solves(p))
                .or_else(|| all.first())
                .cloned()
                .unwrap_or_else(|| PathBuf::from("yt-dlp"))
        })
        .clone()
}

/// Every `yt-dlp` on the PATH, in order, then the system's own, each once.
pub fn candidates() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();
    let path = std::env::var_os("PATH").unwrap_or_default();
    let dirs = std::env::split_paths(&path).chain([PathBuf::from("/usr/bin")]);
    for dir in dirs {
        let p = dir.join("yt-dlp");
        if !p.is_file() {
            continue;
        }
        let real = std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
        if !seen.contains(&real) {
            seen.push(real);
            out.push(p);
        }
    }
    out
}

/// Whether this yt-dlp can load the challenge solver. A Python script asks
/// its own interpreter, which is the one that matters inside a virtual
/// environment; anything else is a standalone build, which carries the
/// solver inside it.
pub fn solves(yt_dlp: &Path) -> bool {
    match interpreter(yt_dlp) {
        Some(python) => std::process::Command::new(python)
            .args(["-c", "import yt_dlp_ejs"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success()),
        None => yt_dlp.is_file(),
    }
}

/// The Python a script runs under, from its first line.
fn interpreter(script: &Path) -> Option<PathBuf> {
    use std::io::{BufRead, BufReader};
    let mut first = String::new();
    BufReader::new(std::fs::File::open(script).ok()?)
        .read_line(&mut first)
        .ok()?;
    shebang_python(&first)
}

/// `#!/venv/bin/python -E` gives `/venv/bin/python`; `#!/usr/bin/env
/// python3` gives `python3`; a line that is not a Python shebang gives
/// nothing.
fn shebang_python(line: &str) -> Option<PathBuf> {
    let rest = line.trim().strip_prefix("#!")?;
    let mut words = rest.split_whitespace();
    let first = words.next()?;
    let prog = if first.ends_with("/env") {
        words.next()?
    } else {
        first
    };
    let name = Path::new(prog).file_name()?.to_string_lossy().to_string();
    name.starts_with("python").then(|| PathBuf::from(prog))
}

/// The JavaScript runtime yt-dlp will find for the solver. Deno is the one
/// it uses without being told; the others need its `--js-runtimes`.
pub fn js_runtime() -> Option<&'static str> {
    ["deno"].into_iter().find(|r| {
        std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).any(|d| d.join(r).is_file()))
            .unwrap_or(false)
    })
}

#[derive(Clone, Debug)]
pub struct Hit {
    pub title: String,
    pub url: String,
    pub duration: Option<u64>,
    pub channel: String,
}

/// Start a search; poll the receiver for the answer.
pub fn search(query: &str, limit: usize) -> Receiver<Result<Vec<Hit>, String>> {
    let (tx, rx) = channel();
    let q = query.trim().to_string();
    std::thread::spawn(move || {
        let _ = tx.send(run(&q, limit));
    });
    rx
}

fn run(query: &str, limit: usize) -> Result<Vec<Hit>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let out = std::process::Command::new(binary())
        .args(["--flat-playlist", "--dump-single-json", "--no-warnings"])
        .arg(format!("ytsearch{limit}:{query}"))
        .output()
        .map_err(|e| format!("yt-dlp: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(err.lines().last().unwrap_or("yt-dlp failed").to_string());
    }
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("yt-dlp: {e}"))?;
    let mut hits = Vec::new();
    for e in v
        .get("entries")
        .and_then(|e| e.as_array())
        .unwrap_or(&Vec::new())
    {
        let url = e
            .get("url")
            .or_else(|| e.get("webpage_url"))
            .and_then(|u| u.as_str())
            .unwrap_or("");
        // Channels and playlists come back among videos: keep the videos.
        if !url.contains("watch?v=") && !url.contains("youtu.be/") {
            continue;
        }
        hits.push(Hit {
            title: e
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string(),
            url: url.to_string(),
            duration: e.get("duration").and_then(|d| d.as_f64()).map(|d| d as u64),
            channel: e
                .get("channel")
                .or_else(|| e.get("uploader"))
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string(),
        });
    }
    Ok(hits)
}

/// The desktop clipboard, when it holds a link.
pub fn clipboard_link() -> Option<String> {
    let out = std::process::Command::new("wl-paste")
        .args(["--no-newline"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text.starts_with("http://") || text.starts_with("https://") {
        Some(text.split_whitespace().next().unwrap_or(&text).to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod yt_dlp_tests {
    use super::shebang_python;
    use std::path::PathBuf;

    #[test]
    fn a_python_shebang_names_its_interpreter() {
        assert_eq!(
            shebang_python("#!/home/u/.local/share/pipx/venvs/yt-dlp/bin/python -E\n"),
            Some(PathBuf::from(
                "/home/u/.local/share/pipx/venvs/yt-dlp/bin/python"
            ))
        );
        assert_eq!(
            shebang_python("#!/usr/bin/env python3\n"),
            Some(PathBuf::from("python3"))
        );
        assert_eq!(shebang_python("#!/bin/sh\n"), None);
        assert_eq!(shebang_python("\u{7f}ELF"), None);
    }
}
