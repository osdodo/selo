use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::{HMONITOR, MONITOR_DEFAULTTONEAREST, MonitorFromPoint};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};

/// Dots per inch over 96; Win32 gives physical pixels while the app uses logical points.
pub fn monitor_scale(monitor: HMONITOR) -> f32 {
    let mut x = 96u32;
    let mut y = 96u32;
    // SAFETY: both out pointers are valid for the call; a failure leaves them at 96.
    unsafe {
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y);
    }
    x as f32 / 96.
}

pub fn point_scale(x: i32, y: i32) -> f32 {
    let monitor = unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) };
    monitor_scale(monitor)
}

pub fn window_scale(hwnd: HWND) -> f32 {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 { 1. } else { dpi as f32 / 96. }
}
