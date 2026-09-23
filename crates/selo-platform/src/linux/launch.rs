use selo_core::{Error, Result};
use std::path::PathBuf;

const ENTRY: &str = "selo.desktop";

pub fn set_launch_at_login(enabled: bool) -> Result<()> {
    let path = config_home().join("autostart").join(ENTRY);
    if !enabled {
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    let dir = path.parent().expect("autostart path always has a parent");
    std::fs::create_dir_all(dir).map_err(|err| Error::Platform(err.to_string()))?;
    let exe = std::env::current_exe().map_err(|err| Error::Platform(err.to_string()))?;
    let content = format!(
        "[Desktop Entry]\nType=Application\nName=Selo\nExec=\"{}\"\nIcon={}\nX-GNOME-Autostart-enabled=true\n",
        exe.display(),
        crate::APP_ID,
    );
    std::fs::write(&path, content).map_err(|err| Error::Platform(err.to_string()))
}

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
}
