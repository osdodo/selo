use objc2_app_kit::{NSEvent, NSScreen};
use objc2_core_foundation::CGPoint;
use objc2_core_graphics::{CGDirectDisplayID, CGDisplayBounds, CGGetDisplaysWithPoint};
use objc2_foundation::MainThreadMarker;

/// Points, origin at the top-left of the primary display, y downward — AppKit is bottom-left.
pub fn position(mtm: MainThreadMarker) -> (f64, f64) {
    let ns = NSEvent::mouseLocation();
    (ns.x, primary_height(mtm) - ns.y)
}

pub fn primary_height(mtm: MainThreadMarker) -> f64 {
    NSScreen::screens(mtm)
        .iter()
        .next()
        .map(|screen| screen.frame().size.height)
        .unwrap_or_default()
}

pub fn display(mtm: MainThreadMarker) -> Option<u64> {
    let ns = NSEvent::mouseLocation();
    let point = CGPoint {
        x: ns.x,
        y: primary_height(mtm) - ns.y,
    };
    let mut display: CGDirectDisplayID = 0;
    let mut count: u32 = 0;
    // SAFETY: both pointers are valid for the call; the API only writes through them.
    unsafe { CGGetDisplaysWithPoint(point, 1, &mut display, &mut count) };
    (count > 0).then_some(display as u64)
}

/// A display's `(x, y, width, height)` in points, origin at the top-left of the **primary**
/// display. `CGDisplayBounds` already uses that origin, unlike `NSScreen`'s bottom-left.
pub fn frame(id: u64) -> Option<(f64, f64, f64, f64)> {
    let bounds = CGDisplayBounds(id as CGDirectDisplayID);
    if bounds.size.width <= 0. || bounds.size.height <= 0. {
        return None;
    }
    Some((
        bounds.origin.x,
        bounds.origin.y,
        bounds.size.width,
        bounds.size.height,
    ))
}
