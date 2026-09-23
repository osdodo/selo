use image::ImageFormat;
use selo_core::{Error, Image, Region, Result};
use std::io::Cursor;

pub fn capture(region: Region) -> Result<Image> {
    let monitors = xcap::Monitor::all().map_err(|err| Error::Platform(err.to_string()))?;
    let monitor = monitors
        .iter()
        .find(|monitor| monitor.id().ok() == Some(region.display as u32))
        .or_else(|| monitors.first())
        .ok_or_else(|| Error::Platform("no monitor".into()))?;

    let (x, y, width, height) = region_args(monitor, region)?;
    let shot = monitor
        .capture_region(x, y, width, height)
        .map_err(|err| Error::Platform(err.to_string()))?;

    // Denied Screen Recording hands back a correctly sized image filled with one colour, not
    // an error, which downstream is indistinguishable from "OCR found nothing".
    #[cfg(target_os = "macos")]
    {
        let first = shot.get_pixel(0, 0);
        if shot.pixels().all(|pixel| pixel == first) {
            return Err(Error::Platform(
                "capture came back as a flat colour — grant Screen Recording to Selo".into(),
            ));
        }
    }

    let mut png = Vec::new();
    shot.write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .map_err(|err| Error::Platform(err.to_string()))?;
    Ok(Image {
        width: shot.width(),
        height: shot.height(),
        png,
    })
}

#[cfg(target_os = "macos")]
fn region_args(_monitor: &xcap::Monitor, region: Region) -> Result<(u32, u32, u32, u32)> {
    // macOS points are xcap's unit; the returned image is physical pixels.
    Ok((region.x, region.y, region.width, region.height))
}

#[cfg(target_os = "windows")]
fn region_args(monitor: &xcap::Monitor, region: Region) -> Result<(u32, u32, u32, u32)> {
    // Windows xcap wants display-local physical pixels, so scale logical points by the DPI.
    let scale = monitor
        .scale_factor()
        .map_err(|err| Error::Platform(err.to_string()))?;
    Ok((
        (region.x as f32 * scale).round() as u32,
        (region.y as f32 * scale).round() as u32,
        (region.width as f32 * scale).round() as u32,
        (region.height as f32 * scale).round() as u32,
    ))
}
