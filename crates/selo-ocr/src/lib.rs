#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::LinuxOcr as Ocr;
#[cfg(target_os = "macos")]
pub use macos::MacOcr as Ocr;
#[cfg(target_os = "windows")]
pub use windows::WindowsOcr as Ocr;
