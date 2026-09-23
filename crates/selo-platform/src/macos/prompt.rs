use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSModalResponseOK, NSOpenPanel};
use objc2_foundation::NSString;
use selo_core::{Error, Result};

pub fn pick_file(title: &str) -> Result<Option<std::path::PathBuf>> {
    let mtm = main_thread()?;
    activate(mtm);

    let panel = NSOpenPanel::new(mtm);
    panel.setTitle(Some(&NSString::from_str(title)));
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(false);
    panel.setAllowsMultipleSelection(false);

    if panel.runModal() != NSModalResponseOK {
        return Ok(None);
    }
    Ok(panel
        .URL()
        .and_then(|url| url.path())
        .map(|path| std::path::PathBuf::from(path.to_string())))
}

fn main_thread() -> Result<MainThreadMarker> {
    MainThreadMarker::new()
        .ok_or_else(|| Error::Platform("prompt must run on the main thread".into()))
}

/// A modal panel that is not key takes no input. `activateIgnoringOtherApps` rather than
/// `activate()`: the latter only gained "bring me forward" in macOS 14, and this supports 13.
#[allow(deprecated)]
fn activate(mtm: MainThreadMarker) {
    NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
}
