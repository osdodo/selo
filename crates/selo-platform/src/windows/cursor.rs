use super::dpi;
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, MONITORINFO,
    MonitorFromPoint,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

/// Logical points from the primary display's top-left; Win32 reports physical pixels.
pub fn position() -> (f64, f64) {
    let mut point = POINT::default();
    // SAFETY: `point` is a valid out pointer.
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return (0., 0.);
    }
    let scale = dpi::point_scale(point.x, point.y) as f64;
    (point.x as f64 / scale, point.y as f64 / scale)
}

pub fn display() -> Option<u64> {
    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point) }.ok()?;
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    (!monitor.0.is_null()).then_some(monitor.0 as usize as u64)
}

pub fn primary_height() -> f64 {
    let monitor = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    monitor_frame(monitor).map_or(0., |(_, _, _, height)| height as f64)
}

/// `(x, y, width, height)` in logical points from the primary display's top-left.
pub fn frame(id: u64) -> Option<(f64, f64, f64, f64)> {
    monitor_frame(HMONITOR(id as *mut core::ffi::c_void))
}

fn monitor_frame(monitor: HMONITOR) -> Option<(f64, f64, f64, f64)> {
    if monitor.0.is_null() {
        return None;
    }
    let mut info = MONITORINFO {
        cbSize: core::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `monitor` is a live handle and `info` is a valid out pointer.
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return None;
    }
    let scale = dpi::monitor_scale(monitor) as f64;
    let rect = info.rcMonitor;
    Some((
        rect.left as f64 / scale,
        rect.top as f64 / scale,
        (rect.right - rect.left) as f64 / scale,
        (rect.bottom - rect.top) as f64 / scale,
    ))
}

pub fn mouse_button_down() -> bool {
    let state = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) };
    state < 0
}
