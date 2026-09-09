//! Talking to the running emulator.
//!
//! Through the tube's own compositor, which presses RetroArch's hotkeys as
//! real key events on its keyboard. Not over RetroArch's UDP command
//! interface: processing a datagram crashes RetroArch 1.22 in its input poll,
//! so `library.rs` writes `network_cmd_enable = false` into every launch and
//! nothing here listens on or speaks to a socket.
//!
//! There used to be a fallback that sent the datagram when the launcher was
//! not on the tube. It could not work - the interface it needed was the one
//! disabled above - and a UDP send to a port nobody listens on returns
//! success, so the pause menu reported a save that had not happened. Off the
//! tube these now say plainly that there is nobody to press the key.

/// On the tube the display process presses the emulator's hotkey for us: real
/// key events on its own keyboard, which is what RetroArch reads. Anywhere
/// else there is no keyboard to press, and saying so beats a silent success.
fn act(key: &str) -> std::io::Result<()> {
    if crate::crt::display::on_tube() || crate::crt::display::running() {
        return crate::crt::display::send(&format!("key {key}"));
    }
    Err(std::io::Error::other(
        "no emulator on this display: the keys go through the tube's compositor",
    ))
}

pub fn menu() -> std::io::Result<()> {
    act("menu")
}

pub fn pause_toggle() -> std::io::Result<()> {
    act("pause")
}

pub fn save_state() -> std::io::Result<()> {
    act("save")
}

pub fn load_state() -> std::io::Result<()> {
    act("load")
}

/// Toggle RetroArch's fast forward (its `space` hotkey).
pub fn fast_forward() -> std::io::Result<()> {
    act("ff")
}

/// Hold RetroArch's rewind key for two seconds (needs `rewind = true` on the system).
pub fn rewind() -> std::io::Result<()> {
    act("r 2000")
}

/// Toggle slow motion (RetroArch's `e` hotkey).
pub fn slow_motion() -> std::io::Result<()> {
    act("slow")
}

pub fn reset() -> std::io::Result<()> {
    act("reset")
}

pub fn quit() -> std::io::Result<()> {
    act("quit")
}
