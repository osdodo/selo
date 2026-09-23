use raw_window_handle::RawWindowHandle;

/// Wayland clients cannot move their own toplevels; placement is set at open time.
pub fn pin_top_left(_handle: RawWindowHandle, _x: f64, _y: f64) {}

pub fn frame(_handle: RawWindowHandle) -> Option<(f64, f64, f64, f64)> {
    None
}

pub fn round_corners(_handle: RawWindowHandle, _radius: f64) {}

pub fn set_glass(_handle: RawWindowHandle) {}

pub fn frontmost_name() -> String {
    String::new()
}
