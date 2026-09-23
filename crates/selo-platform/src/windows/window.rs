use super::dpi;
use raw_window_handle::RawWindowHandle;
use std::mem::size_of;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{
    DWM_SYSTEMBACKDROP_TYPE, DWM_WINDOW_CORNER_PREFERENCE, DWMSBT_TRANSIENTWINDOW,
    DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_ROUND, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowRect, GetWindowTextLengthW, GetWindowTextW, SWP_NOACTIVATE,
    SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
};
use windows::core::BOOL;

fn hwnd(handle: RawWindowHandle) -> Option<HWND> {
    let RawWindowHandle::Win32(handle) = handle else {
        return None;
    };
    Some(HWND(handle.hwnd.get() as *mut core::ffi::c_void))
}

/// `(x, y)` is logical points from the primary display's top-left; Win32 wants physical pixels.
pub fn pin_top_left(handle: RawWindowHandle, x: f64, y: f64) {
    let Some(hwnd) = hwnd(handle) else {
        return;
    };
    let scale = dpi::window_scale(hwnd) as f64;
    // SAFETY: `hwnd` came from a live window handle; size and z-order are left alone.
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            None,
            (x * scale).round() as i32,
            (y * scale).round() as i32,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
    };
}

/// The window frame in logical points; GPUI does not track moves it did not perform.
pub fn frame(handle: RawWindowHandle) -> Option<(f64, f64, f64, f64)> {
    let hwnd = hwnd(handle)?;
    let mut rect = RECT::default();
    // SAFETY: `hwnd` is live and `rect` is a valid out pointer.
    unsafe { GetWindowRect(hwnd, &mut rect) }.ok()?;
    let scale = dpi::window_scale(hwnd) as f64;
    Some((
        rect.left as f64 / scale,
        rect.top as f64 / scale,
        (rect.right - rect.left) as f64 / scale,
        (rect.bottom - rect.top) as f64 / scale,
    ))
}

/// DWM system acrylic backdrop; GPUI's `Transparent` alone installs no blur. No-op on Windows 10.
pub fn set_glass(handle: RawWindowHandle) {
    let Some(hwnd) = hwnd(handle) else {
        return;
    };
    let backdrop: DWM_SYSTEMBACKDROP_TYPE = DWMSBT_TRANSIENTWINDOW;
    // App is always dark; DWM tints the backdrop and title bar from this flag, not the theme.
    let dark: BOOL = true.into();
    // Negative margins are DWM's idiom for covering the client area, not just the frame.
    let margins = MARGINS {
        cxLeftWidth: -1,
        cxRightWidth: -1,
        cyTopHeight: -1,
        cyBottomHeight: -1,
    };
    // SAFETY: `hwnd` is live and DWM reads the attributes synchronously.
    unsafe {
        let extend = DwmExtendFrameIntoClientArea(hwnd, &margins);
        let dark_result = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const _ as *const core::ffi::c_void,
            size_of::<BOOL>() as u32,
        );
        let backdrop_result = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const _ as *const core::ffi::c_void,
            size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
        );
        if std::env::var_os("SELO_GLASS_DEBUG").is_some() {
            eprintln!("glass: extend={extend:?} dark={dark_result:?} backdrop={backdrop_result:?}");
        }
    }
}

/// Rounds corners via DWM; fails silently before Windows 11.
pub fn round_corners(handle: RawWindowHandle, _radius: f64) {
    let Some(hwnd) = hwnd(handle) else {
        return;
    };
    let preference: DWM_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND;
    // SAFETY: `hwnd` is live and DWM reads the attribute synchronously.
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &preference as *const _ as *const core::ffi::c_void,
            size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        )
    };
}

pub fn frontmost_name() -> String {
    let hwnd = unsafe { GetForegroundWindow() };
    let length = unsafe { GetWindowTextLengthW(hwnd) };
    if length <= 0 {
        return String::new();
    }
    let mut buffer = vec![0u16; length as usize + 1];
    let written = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    String::from_utf16_lossy(&buffer[..written.max(0) as usize])
}
