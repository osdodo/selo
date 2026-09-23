use objc2::rc::Retained;
use objc2_app_kit::{NSPasteboard, NSPasteboardType, NSPasteboardTypeString};
use objc2_core_graphics::{
    CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
};
use objc2_foundation::{NSData, NSString};
use std::thread::sleep;
use std::time::{Duration, Instant};

const KEY_C: u16 = 8; // kVK_ANSI_C
const COPY_DEADLINE: Duration = Duration::from_millis(600);

fn string_type() -> &'static NSPasteboardType {
    unsafe { NSPasteboardTypeString }
}

pub fn text() -> Option<String> {
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard
        .stringForType(string_type())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

pub fn set_text(text: &str) -> bool {
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    pasteboard.setString_forType(&NSString::from_str(text), string_type())
}

/// Restores every pasteboard type; restoring only the string would destroy copied images and
/// rich text.
struct Snapshot {
    change_count: isize,
    items: Vec<(Retained<NSPasteboardType>, Retained<NSData>)>,
}

impl Snapshot {
    fn take(pasteboard: &NSPasteboard) -> Self {
        let mut items = Vec::new();
        if let Some(types) = pasteboard.types() {
            for ty in types.iter() {
                if let Some(data) = pasteboard.dataForType(&ty) {
                    items.push((ty, data));
                }
            }
        }
        Self {
            change_count: pasteboard.changeCount(),
            items,
        }
    }

    fn restore(&self, pasteboard: &NSPasteboard) {
        pasteboard.clearContents();
        for (ty, data) in &self.items {
            pasteboard.setData_forType(Some(data), ty);
        }
    }
}

fn press_copy() {
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState);
    let source = source.as_deref();
    for down in [true, false] {
        let Some(event) = CGEvent::new_keyboard_event(source, KEY_C, down) else {
            continue;
        };
        CGEvent::set_flags(Some(&event), CGEventFlags::MaskCommand);
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
    }
}

/// Only call on explicit user intent: it steals a keystroke and churns the clipboard.
pub fn copy_selection() -> Option<String> {
    let pasteboard = NSPasteboard::generalPasteboard();
    let before = Snapshot::take(&pasteboard);

    press_copy();

    // ⌘C reaches the frontmost app asynchronously; changeCount is the only signal that it
    // actually wrote something.
    let deadline = Instant::now() + COPY_DEADLINE;
    while Instant::now() < deadline && pasteboard.changeCount() == before.change_count {
        sleep(Duration::from_millis(20));
    }

    let copied = (pasteboard.changeCount() != before.change_count)
        .then(|| pasteboard.stringForType(string_type()))
        .flatten()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());

    before.restore(&pasteboard);
    copied
}
