#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(target_os = "linux"))]
mod xcap;

#[cfg(target_os = "linux")]
pub use linux::capture;
#[cfg(not(target_os = "linux"))]
pub use xcap::capture;
