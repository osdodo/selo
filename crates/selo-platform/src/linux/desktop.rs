use selo_core::{Error, Result};
use std::path::{Path, PathBuf};

/// Rewritten only when content changes, so a fresh mtime does not rescan icon caches.
pub fn install_app_entry() -> Result<()> {
    let data = data_home();
    for size in [256u32, 512] {
        let path = data.join(format!(
            "icons/hicolor/{size}x{size}/apps/{}.png",
            crate::APP_ID
        ));
        write_if_changed(&path, &icon_png(size)?)?;
    }
    let exe = std::env::current_exe().map_err(|err| Error::Platform(err.to_string()))?;
    write_if_changed(
        &data.join(format!("applications/{}.desktop", crate::APP_ID)),
        desktop_entry(&exe).as_bytes(),
    )
}

fn desktop_entry(exe: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Selo\n\
         Exec=\"{}\"\n\
         Icon={}\n\
         Terminal=false\n\
         Categories=Utility;\n\
         StartupWMClass={}\n",
        exe.display(),
        crate::APP_ID,
        crate::APP_ID,
    )
}

fn icon_png(size: u32) -> Result<Vec<u8>> {
    const SOURCE: &[u8] = include_bytes!("../../../../assets/AppIcon.png");
    let image = image::load_from_memory_with_format(SOURCE, image::ImageFormat::Png)
        .map_err(|err| Error::Platform(err.to_string()))?
        .resize_exact(size, size, image::imageops::FilterType::Lanczos3);
    let mut png = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|err| Error::Platform(err.to_string()))?;
    Ok(png.into_inner())
}

fn write_if_changed(path: &Path, bytes: &[u8]) -> Result<()> {
    if std::fs::read(path).map(|old| old == bytes).unwrap_or(false) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|err| Error::Platform(err.to_string()))?;
    }
    std::fs::write(path, bytes).map_err(|err| Error::Platform(err.to_string()))
}

fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_points_at_the_binary_and_the_icon() {
        let entry = desktop_entry(Path::new("/opt/selo/selo"));
        assert!(entry.contains("Exec=\"/opt/selo/selo\"\n"));
        assert!(entry.contains("Icon=com.selo.app\n"));
        assert!(entry.contains("StartupWMClass=com.selo.app\n"));
    }

    #[test]
    fn icon_is_resized_from_the_shared_app_icon() {
        let png = icon_png(256).unwrap();
        let image = image::load_from_memory_with_format(&png, image::ImageFormat::Png).unwrap();
        assert_eq!((image.width(), image.height()), (256, 256));
    }
}
