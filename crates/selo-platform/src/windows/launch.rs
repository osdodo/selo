use selo_core::{Error, Result};
use windows_registry::CURRENT_USER;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "Selo";

pub fn set_launch_at_login(enabled: bool) -> Result<()> {
    let key = CURRENT_USER
        .create(RUN_KEY)
        .map_err(|err| Error::Platform(err.to_string()))?;
    if enabled {
        let exe = std::env::current_exe().map_err(|err| Error::Platform(err.to_string()))?;
        key.set_string(VALUE, &format!("\"{}\"", exe.display()))
            .map_err(|err| Error::Platform(err.to_string()))?;
    } else {
        let _ = key.remove_value(VALUE);
    }
    Ok(())
}
