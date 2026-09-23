use crate::linux::{block_on, file_uri_to_path, output};
use image::ImageFormat;
use selo_core::{Error, Image, Region, Result};
use std::io::Cursor;

/// Grabs `region` off the display it names via the XDG Screenshot portal. KWin does not
/// implement `wlr-screencopy`, so `xcap` cannot capture a native Wayland screen.
///
/// ponytail: v1 takes the whole desktop and crops, which only lines up on a single display.
/// Multi-head needs the output's position inside the portal image.
pub fn capture(region: Region) -> Result<Image> {
    let path = block_on(screenshot()).map_err(Error::Platform)?;
    let bytes = std::fs::read(&path).map_err(|err| Error::Platform(err.to_string()))?;
    let _ = std::fs::remove_file(&path);
    let full = image::load_from_memory(&bytes).map_err(|err| Error::Platform(err.to_string()))?;

    let outputs = output::outputs();
    let target = outputs
        .iter()
        .find(|output| output.id == region.display)
        .or_else(|| outputs.first());
    let scale = target.map_or(1.0, |output| {
        let logical_width = output.width as f64 / output.scale.max(1) as f64;
        if logical_width > 0.0 {
            full.width() as f64 / logical_width
        } else {
            1.0
        }
    });

    let x = (region.x as f64 * scale).round().max(0.0) as u32;
    let y = (region.y as f64 * scale).round().max(0.0) as u32;
    let width = ((region.width as f64 * scale).round() as u32).min(full.width().saturating_sub(x));
    let height =
        ((region.height as f64 * scale).round() as u32).min(full.height().saturating_sub(y));
    let cropped = full.crop_imm(x, y, width.max(1), height.max(1));

    let mut png = Vec::new();
    cropped
        .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .map_err(|err| Error::Platform(err.to_string()))?;
    Ok(Image {
        width: cropped.width(),
        height: cropped.height(),
        png,
    })
}

async fn screenshot() -> std::result::Result<std::path::PathBuf, String> {
    use ashpd::desktop::screenshot::Screenshot;

    let connection = crate::linux::portal_connection().await?;
    let request = Screenshot::request()
        .connection(Some(connection))
        .interactive(false)
        .modal(true)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    let response = request.response().map_err(|err| err.to_string())?;
    file_uri_to_path(response.uri().as_str()).ok_or_else(|| {
        format!(
            "screenshot portal returned a non-file URI: {}",
            response.uri()
        )
    })
}
