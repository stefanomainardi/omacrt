//! Running a program from the menu.

/// Run a program and forget it: the arguments are an argv, never a command
/// line, so there is no shell in the process tree and nothing that could
/// read a filename or an answer from a server as a command.
pub fn launch(argv: &[String]) -> std::io::Result<()> {
    let Some((program, args)) = argv.split_first() else {
        return Ok(());
    };
    std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
}
