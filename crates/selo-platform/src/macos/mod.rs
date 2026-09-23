mod clipboard;
mod cursor;
mod outside_click;
mod prompt;
mod selection;
mod window;

pub use clipboard::{set_text as set_clipboard_text, text as clipboard_text};
pub use outside_click::OutsideClick;
pub use prompt::pick_file as prompt_pick_file;
pub use selection::PlatformSelection;

/// Global points, origin at the top-left of the primary display, y downward.
pub fn cursor_position() -> (f64, f64) {
    MainThreadMarker::new()
        .map(cursor::position)
        .unwrap_or_default()
}

pub fn cursor_display() -> Option<u64> {
    MainThreadMarker::new().and_then(cursor::display)
}

/// A display's `(x, y, width, height)` in global points. Needed because gpui reports
/// every display's origin as (0,0).
pub fn display_frame(id: u64) -> Option<(f64, f64, f64, f64)> {
    cursor::frame(id)
}
pub use window::{frame as window_frame, frontmost_name, pin_top_left, round_corners, set_glass};

pub fn mouse_button_down() -> bool {
    objc2_app_kit::NSEvent::pressedMouseButtons() & 1 != 0
}

pub fn primary_display_height() -> f64 {
    MainThreadMarker::new()
        .map(cursor::primary_height)
        .unwrap_or_default()
}

/// The user's preferred language as a BCP-47 tag (e.g. `zh-Hans-CN`).
pub fn system_language() -> Option<String> {
    objc2_foundation::NSLocale::preferredLanguages()
        .firstObject()
        .map(|language| language.to_string())
}

/// Runs `/usr/bin/open -a <app>` rather than the binary so LaunchServices (and TCC)
/// attributes the process to the bundle, not launchd.
pub fn set_launch_at_login(enabled: bool) -> Result<()> {
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
    let dir = home.join("Library/LaunchAgents");
    let plist = dir.join("com.selo.app.plist");
    let domain = format!("gui/{}", user_id());

    if !enabled {
        let _ = Command::new("launchctl")
            .args(["bootout", &domain, &plist.to_string_lossy()])
            .status();
        let _ = std::fs::remove_file(&plist);
        return Ok(());
    }

    std::fs::create_dir_all(&dir).map_err(|err| Error::Platform(err.to_string()))?;
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n\
         \t<key>Label</key><string>com.selo.app</string>\n\
         \t<key>ProgramArguments</key>\n\t<array><string>/usr/bin/open</string><string>{}</string></array>\n\
         \t<key>RunAtLoad</key><true/>\n\
         </dict>\n</plist>\n",
        app_bundle().display()
    );
    std::fs::write(&plist, content).map_err(|err| Error::Platform(err.to_string()))?;
    let _ = Command::new("launchctl")
        .args(["bootstrap", &domain, &plist.to_string_lossy()])
        .status();
    Ok(())
}

fn app_bundle() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    exe.ancestors()
        .find(|path| path.extension().is_some_and(|ext| ext == "app"))
        .map_or(exe.clone(), Path::to_path_buf)
}

fn user_id() -> String {
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map_or_else(String::new, |id| id.trim().to_string())
}

use objc2_foundation::MainThreadMarker;
use selo_core::{Error, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
