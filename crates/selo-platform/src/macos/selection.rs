use super::clipboard;
use objc2_app_kit::NSWorkspace;
use objc2_application_services::{
    AXError, AXIsProcessTrusted, AXIsProcessTrustedWithOptions, AXUIElement,
};
use objc2_core_foundation::{CFBoolean, CFDictionary, CFRetained, CFString, CFType};
use selo_core::{Acquisition, Error, Selection, SelectionProvider};
use std::ptr::NonNull;

// AX attribute names are `#define`d C strings; objc2 emits no constants for them.
const FOCUSED_UI_ELEMENT: &str = "AXFocusedUIElement";
const SELECTED_TEXT: &str = "AXSelectedText";
const MANUAL_ACCESSIBILITY: &str = "AXManualAccessibility";
const TRUSTED_CHECK_PROMPT: &str = "AXTrustedCheckOptionPrompt";

/// The default is short enough that a busy app answers `kAXErrorCannotComplete`.
const MESSAGING_TIMEOUT_SECS: f32 = 2.0;

pub fn is_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

pub fn request_trust() {
    let key = CFString::from_str(TRUSTED_CHECK_PROMPT);
    let value: &CFType = CFBoolean::new(true);
    let options = CFDictionary::<CFString, CFType>::from_slices(&[&key], &[value]);
    unsafe { AXIsProcessTrustedWithOptions(Some(options.as_opaque())) };
}

/// Selected text of the frontmost application, or `None` when it exposes none.
/// The documented `AXUIElementCreateSystemWide()` route returns `kAXErrorCannotComplete` on macOS 27.
pub fn selected_text() -> Option<String> {
    let app = frontmost_element()?;
    unsafe { app.set_messaging_timeout(MESSAGING_TIMEOUT_SECS) };

    // Chromium/Electron only build their accessibility tree once a client asks for it.
    let name = CFString::from_str(MANUAL_ACCESSIBILITY);
    let enabled: &CFType = CFBoolean::new(true);
    unsafe { app.set_attribute_value(&name, enabled) };

    let focused = element_attribute(&app, FOCUSED_UI_ELEMENT).ok()?;
    let text = attribute(&focused, SELECTED_TEXT)
        .ok()?
        .downcast_ref::<CFString>()?
        .to_string();
    (!text.is_empty()).then_some(text)
}

fn frontmost_element() -> Option<CFRetained<AXUIElement>> {
    let pid = NSWorkspace::sharedWorkspace()
        .frontmostApplication()?
        .processIdentifier();
    Some(unsafe { AXUIElement::new_application(pid) })
}

fn attribute(element: &AXUIElement, name: &str) -> Result<CFRetained<CFType>, AXError> {
    let cf_name = CFString::from_str(name);
    let mut out: *const CFType = std::ptr::null();
    let err = unsafe { element.copy_attribute_value(&cf_name, NonNull::from(&mut out)) };
    if err != AXError::Success {
        return Err(err);
    }
    match NonNull::new(out.cast_mut()) {
        Some(ptr) => Ok(unsafe { CFRetained::from_raw(ptr) }),
        None => Err(AXError::NoValue),
    }
}

fn element_attribute(
    element: &AXUIElement,
    name: &str,
) -> Result<CFRetained<AXUIElement>, AXError> {
    let value = attribute(element, name)?;
    let child = value
        .downcast_ref::<AXUIElement>()
        .ok_or(AXError::NoValue)?;
    Ok(unsafe { CFRetained::retain(NonNull::from(child)) })
}

pub struct PlatformSelection;

impl SelectionProvider for PlatformSelection {
    fn selection(&self) -> selo_core::Result<Selection> {
        if !is_trusted() {
            request_trust();
            return Err(Error::Platform(
                "Accessibility permission is not granted".into(),
            ));
        }

        if let Some(text) = selected_text() {
            return Ok(Selection {
                text,
                via: Acquisition::Accessibility,
            });
        }
        match clipboard::copy_selection() {
            Some(text) => Ok(Selection {
                text,
                via: Acquisition::Clipboard,
            }),
            None => Err(Error::NoSelection),
        }
    }
}
