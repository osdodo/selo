mod icon;

#[cfg(not(target_os = "linux"))]
mod tray_icon;
#[cfg(not(target_os = "linux"))]
pub use tray_icon::{Tray, TrayEvent};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{Tray, TrayEvent};
