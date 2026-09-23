mod clipboard;
mod cursor;
mod dpi;
mod launch;
mod outside_click;
mod prompt;
mod selection;
mod window;

pub use clipboard::{set_text as set_clipboard_text, text as clipboard_text};
pub use cursor::{
    display as cursor_display, frame as display_frame, mouse_button_down,
    position as cursor_position, primary_height as primary_display_height,
};
pub use launch::set_launch_at_login;
pub use outside_click::OutsideClick;
pub use prompt::pick_file as prompt_pick_file;
pub use selection::PlatformSelection;
pub use window::{frame as window_frame, frontmost_name, pin_top_left, round_corners, set_glass};

/// The user's preferred language as a BCP-47 tag (e.g. `en-US`).
pub fn system_language() -> Option<String> {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buffer = [0u16; 85];
    let length = unsafe { GetUserDefaultLocaleName(&mut buffer) };
    if length <= 1 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..length as usize - 1]))
}
