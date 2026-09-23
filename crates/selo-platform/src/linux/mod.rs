mod clipboard;
mod cursor;
mod desktop;
mod launch;
pub(crate) mod output;
mod outside_click;
mod prompt;
mod selection;
mod window;

pub use clipboard::{set_text as set_clipboard_text, text as clipboard_text};
pub use cursor::{
    display as cursor_display, frame as display_frame, mouse_button_down,
    position as cursor_position, primary_height as primary_display_height,
};
pub use desktop::install_app_entry;
pub use launch::set_launch_at_login;
pub use outside_click::OutsideClick;
pub use prompt::pick_file as prompt_pick_file;
pub use selection::PlatformSelection;
pub use window::{frame as window_frame, frontmost_name, pin_top_left, round_corners, set_glass};

/// The user's preferred language from the POSIX locale env (e.g. `en_US.UTF-8`).
pub fn system_language() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|key| std::env::var(key).ok())
        .filter(|value| !value.is_empty())
}

use std::future::Future;
use std::path::PathBuf;

/// Registers the app id on the connection; without it portals answer `NotAllowed`.
pub(crate) async fn portal_connection() -> std::result::Result<ashpd::zbus::Connection, String> {
    use ashpd::zbus;
    let connection = zbus::Connection::session()
        .await
        .map_err(|err| err.to_string())?;
    let proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.host.portal.Registry",
    )
    .await
    .map_err(|err| err.to_string())?;
    let options = std::collections::HashMap::<String, zbus::zvariant::Value<'static>>::new();
    let _ = proxy
        .call::<_, _, ()>("Register", &(crate::APP_ID, options))
        .await;
    Ok(connection)
}

/// Portals are async but the platform boundary is sync; drive them on the async-io executor.
pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
    async_io::block_on(future)
}

pub(crate) fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let path = uri.strip_prefix("file://")?;
    Some(PathBuf::from(percent_decode(path)))
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&input[index + 1..index + 3], 16)
        {
            decoded.push(byte);
            index += 3;
            continue;
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}
