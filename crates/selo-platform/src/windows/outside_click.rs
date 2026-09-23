use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Foundation::POINT;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetWindowThreadProcessId, WindowFromPoint,
};

// ponytail: 40ms polling can miss a click shorter than the poll interval; switch to a hook if it
// ever matters.
#[derive(Default)]
pub struct OutsideClick {
    was_down: AtomicBool,
}

impl OutsideClick {
    pub fn new() -> Option<Self> {
        Some(Self::default())
    }

    pub fn take(&self) -> bool {
        // SAFETY: all calls are side-effect free queries of the mouse/cursor state.
        unsafe {
            if !fresh_press(&self.was_down, GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0) {
                return false;
            }
            let mut point = POINT::default();
            if GetCursorPos(&mut point).is_err() {
                return false;
            }
            let hwnd = WindowFromPoint(point);
            if hwnd.0.is_null() {
                return false;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            pid != GetCurrentProcessId()
        }
    }
}

// The release must clear the flag, or only the first press ever fires.
fn fresh_press(was_down: &AtomicBool, down: bool) -> bool {
    if !down {
        was_down.store(false, Ordering::Relaxed);
        return false;
    }
    !was_down.swap(true, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_press_after_a_release_is_fresh() {
        let was_down = AtomicBool::new(false);
        assert!(!fresh_press(&was_down, false));
        assert!(fresh_press(&was_down, true));
        assert!(!fresh_press(&was_down, true));
        assert!(!fresh_press(&was_down, false));
        assert!(fresh_press(&was_down, true));
        assert!(!fresh_press(&was_down, false));
        assert!(fresh_press(&was_down, true));
    }
}
