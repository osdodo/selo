use selo_core::{Error, Image, OcrProvider, Rect, Result, TextBlock};
use std::thread::sleep;
use std::time::Duration;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Security::Cryptography::CryptographicBuffer;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows_future::AsyncStatus;

pub struct WindowsOcr;

impl OcrProvider for WindowsOcr {
    fn recognize(&self, image: &Image) -> Result<Vec<TextBlock>> {
        let rgba = image::load_from_memory_with_format(&image.png, image::ImageFormat::Png)
            .map_err(|err| Error::Ocr(err.to_string()))?
            .into_rgba8();
        let (width, height) = (rgba.width() as i32, rgba.height() as i32);
        if width == 0 || height == 0 {
            return Err(Error::Ocr("capture produced an empty image".into()));
        }
        // SoftwareBitmap BGRA8 wants blue first; the image crate hands back RGBA.
        let mut bgra = rgba.into_raw();
        for pixel in bgra.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }

        // SAFETY: COM/WinRT is initialised for this thread and every object is created and used
        // within it; failures fall through as `Error::Ocr`.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let buffer = CryptographicBuffer::CreateFromByteArray(&bgra)
                .map_err(|err| Error::Ocr(err.to_string()))?;
            let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
                &buffer,
                BitmapPixelFormat::Bgra8,
                width,
                height,
            )
            .map_err(|err| Error::Ocr(err.to_string()))?;
            let engine = OcrEngine::TryCreateFromUserProfileLanguages()
                .map_err(|err| Error::Ocr(format!("no Windows OCR language installed: {err}")))?;

            let operation = engine
                .RecognizeAsync(&bitmap)
                .map_err(|err| Error::Ocr(err.to_string()))?;
            // ponytail: blocking poll instead of an async runtime; runs on a background executor.
            while operation
                .Status()
                .map_err(|err| Error::Ocr(err.to_string()))?
                == AsyncStatus::Started
            {
                sleep(Duration::from_millis(10));
            }
            let result = operation
                .GetResults()
                .map_err(|err| Error::Ocr(err.to_string()))?;

            let lines = result.Lines().map_err(|err| Error::Ocr(err.to_string()))?;
            let count = lines.Size().map_err(|err| Error::Ocr(err.to_string()))?;
            let mut blocks = Vec::with_capacity(count as usize);
            for index in 0..count {
                let line = lines
                    .GetAt(index)
                    .map_err(|err| Error::Ocr(err.to_string()))?;
                let text = line
                    .Text()
                    .map_err(|err| Error::Ocr(err.to_string()))?
                    .to_string();
                if text.trim().is_empty() {
                    continue;
                }
                let Some(rect) = line_rect(&line) else {
                    continue;
                };
                blocks.push(TextBlock {
                    rect,
                    text,
                    confidence: 1.,
                });
            }
            Ok(blocks)
        }
    }
}

/// Union of the line's word boxes; the WinRT line itself carries no rectangle.
fn line_rect(line: &windows::Media::Ocr::OcrLine) -> Option<Rect> {
    let words = line.Words().ok()?;
    let count = words.Size().ok()?;
    let mut rect: Option<Rect> = None;
    for index in 0..count {
        let bounds = words.GetAt(index).ok()?.BoundingRect().ok()?;
        let word = Rect {
            x: bounds.X,
            y: bounds.Y,
            width: bounds.Width,
            height: bounds.Height,
        };
        rect = Some(match rect {
            None => word,
            Some(current) => {
                let x = current.x.min(word.x);
                let y = current.y.min(word.y);
                let right = (current.x + current.width).max(word.x + word.width);
                let bottom = (current.y + current.height).max(word.y + word.height);
                Rect {
                    x,
                    y,
                    width: right - x,
                    height: bottom - y,
                }
            }
        });
    }
    rect
}
