//! Talking to the running emulator.
//!
//! RetroArch listens for plain text commands on UDP (`network_cmd_enable`),
//! one command per datagram, on the loopback. The launcher uses a handful of
//! them for its pause menu; the list is in RetroArch's `command.c`.

use std::net::UdpSocket;
use std::time::Duration;

/// Port from `launch.cfg`; RetroArch's default.
pub const PORT: u16 = 55355;

/// Send one command. RetroArch's own sender (`retroarch --command`) is what
/// gets through: datagrams from a plain socket were ignored on this setup
/// while the identical bytes from `--command` acted, so the CLI does the
/// sending. Fire and forget.
pub fn send(cmd: &str) -> std::io::Result<()> {
    let status = std::process::Command::new("retroarch")
        .arg("--command")
        .arg(cmd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "retroarch --command {cmd}: {status}"
        )))
    }
}

/// Raw datagram to the command port, for experiments.
pub fn send_udp(cmd: &str) -> std::io::Result<()> {
    let sock = UdpSocket::bind("127.0.0.1:0")?;
    sock.set_write_timeout(Some(Duration::from_millis(200)))?;
    sock.send_to(cmd.as_bytes(), ("127.0.0.1", PORT))?;
    Ok(())
}

/// Ask RetroArch something and read the reply, for example `GET_STATUS`
/// returns `GET_STATUS PLAYING snes9x,Chrono Trigger,crc32=...` or
/// `GET_STATUS PAUSED ...`.
pub fn query(cmd: &str) -> std::io::Result<String> {
    let sock = UdpSocket::bind("127.0.0.1:0")?;
    sock.set_read_timeout(Some(Duration::from_millis(300)))?;
    sock.send_to(cmd.as_bytes(), ("127.0.0.1", PORT))?;
    let mut buf = [0u8; 1024];
    let (n, _) = sock.recv_from(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf[..n]).trim().to_string())
}

pub fn pause_toggle() -> std::io::Result<()> {
    send("PAUSE_TOGGLE")
}

pub fn save_state() -> std::io::Result<()> {
    send("SAVE_STATE")
}

pub fn load_state() -> std::io::Result<()> {
    send("LOAD_STATE")
}

pub fn reset() -> std::io::Result<()> {
    send("RESET")
}

pub fn quit() -> std::io::Result<()> {
    send("QUIT")
}

/// True when RetroArch reports a paused game.
pub fn paused() -> Option<bool> {
    let s = query("GET_STATUS").ok()?;
    Some(s.contains("PAUSED"))
}
