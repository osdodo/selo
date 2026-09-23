use std::io::Write;
use std::process::{Command, Stdio};

pub fn text() -> Option<String> {
    let output = Command::new("wl-paste").arg("--no-newline").output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
        .filter(|value| !value.is_empty())
}

/// `wl-copy` forks to serve the selection.
pub fn set_text(text: &str) -> bool {
    let Ok(mut child) = Command::new("wl-copy").stdin(Stdio::piped()).spawn() else {
        return false;
    };
    let written = child
        .stdin
        .take()
        .is_some_and(|mut stdin| stdin.write_all(text.as_bytes()).is_ok());
    let _ = child.wait();
    written
}
