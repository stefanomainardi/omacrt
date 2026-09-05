//! Running external commands from the menu.

/// Launch a command detached through `sh -c`.
pub fn launch(command: &str) -> std::io::Result<()> {
    if command.trim().is_empty() {
        return Ok(());
    }
    std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
}
