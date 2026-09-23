pub const APP_ID: &str = "com.selo.app";

mod capture;
mod hotkey;
mod tray;

pub use capture::capture;
pub use global_hotkey::hotkey::{Code, Modifiers};
pub use hotkey::Hotkeys;
pub use tray::{Tray, TrayEvent};

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use macos::{
    OutsideClick, PlatformSelection, clipboard_text, cursor_display, cursor_position,
    display_frame, frontmost_name, mouse_button_down, pin_top_left, primary_display_height,
    prompt_pick_file, round_corners, set_clipboard_text, set_glass, set_launch_at_login,
    system_language, window_frame,
};

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
pub use windows::{
    OutsideClick, PlatformSelection, clipboard_text, cursor_display, cursor_position,
    display_frame, frontmost_name, mouse_button_down, pin_top_left, primary_display_height,
    prompt_pick_file, round_corners, set_clipboard_text, set_glass, set_launch_at_login,
    system_language, window_frame,
};

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub use linux::{
    OutsideClick, PlatformSelection, clipboard_text, cursor_display, cursor_position,
    display_frame, frontmost_name, install_app_entry, mouse_button_down, pin_top_left,
    primary_display_height, prompt_pick_file, round_corners, set_clipboard_text, set_glass,
    set_launch_at_login, system_language, window_frame,
};

/// Only Linux has a launcher entry to install; macOS ships an app bundle and Windows a resource.
#[cfg(not(target_os = "linux"))]
pub fn install_app_entry() -> selo_core::Result<()> {
    Ok(())
}
