pub(crate) fn tray_rgba() -> (Vec<u8>, u32, u32) {
    #[cfg(target_os = "macos")]
    const PNG: &[u8] = include_bytes!("../../../../assets/tray.png");
    #[cfg(not(target_os = "macos"))]
    const PNG: &[u8] = include_bytes!("../../../../assets/tray-color.png");
    let image = image::load_from_memory_with_format(PNG, image::ImageFormat::Png)
        .expect("tray icon is a valid PNG")
        .into_rgba8();
    let (width, height) = image.dimensions();
    (image.into_raw(), width, height)
}

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    #[test]
    fn tray_icon_is_colored() {
        let (rgba, ..) = super::tray_rgba();
        assert!(
            rgba.chunks(4).any(|p| p[0] != p[1] || p[1] != p[2]),
            "expected the color tray icon on this platform"
        );
    }
}
