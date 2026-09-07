//! YouTube from the tube: a search through yt-dlp, run on a thread so the
//! launcher keeps drawing while it takes its few seconds. Playback goes
//! through mpv like any other video (mpv resolves the link with yt-dlp too).

use std::sync::mpsc::{Receiver, channel};

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
    let out = std::process::Command::new("yt-dlp")
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
