use selo_core::{Acquisition, Error, Result, Selection, SelectionProvider};
use std::process::Command;

pub struct PlatformSelection;

impl SelectionProvider for PlatformSelection {
    /// Wayland has no cross-app selection API; read the shared primary selection.
    fn selection(&self) -> Result<Selection> {
        let output = Command::new("wl-paste")
            .args(["--primary", "--no-newline"])
            .output()
            .map_err(|err| Error::Platform(format!("wl-paste: {err}")))?;
        if !output.status.success() {
            return Err(Error::NoSelection);
        }
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        if text.trim().is_empty() {
            return Err(Error::NoSelection);
        }
        Ok(Selection {
            text,
            via: Acquisition::Accessibility,
        })
    }
}
