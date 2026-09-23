use super::cursor;
use objc2::MainThreadOnly;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSScreen, NSView, NSVisualEffectBlendingMode,
    NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindowOrderingMode,
    NSWorkspace,
};
use objc2_foundation::{MainThreadMarker, NSPoint};
use raw_window_handle::RawWindowHandle;

/// Pins the window's **top-left** to `(x, y)`. Resizing an `NSWindow` keeps its
/// bottom-left origin fixed, so a growing window creeps upward — callers re-pin after resize.
pub fn pin_top_left(handle: RawWindowHandle, x: f64, y: f64) {
    let RawWindowHandle::AppKit(handle) = handle else {
        return;
    };
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    // SAFETY: an AppKit raw window handle always carries a live `NSView` pointer, and the
    // main-thread requirement is checked above.
    let view: &NSView = unsafe { &*handle.ns_view.as_ptr().cast() };
    let Some(window) = view.window() else {
        return;
    };

    let top = cursor::primary_height(mtm) - y;
    window.setFrameTopLeftPoint(NSPoint::new(x, top));

    // Nudge back on screen if growing pushed the bottom past the display edge.
    if let Some(screen) = NSScreen::screens(mtm).iter().next() {
        let visible = screen.visibleFrame();
        let frame = window.frame();
        if frame.origin.y < visible.origin.y {
            window.setFrameTopLeftPoint(NSPoint::new(
                frame.origin.x,
                visible.origin.y + frame.size.height,
            ));
        }
    }
}

/// A window's frame as `(x, y, width, height)` in points, `(x, y)` being its **top-left**.
/// `NSWindow.frame` is bottom-left, hence the flip.
pub fn frame(handle: RawWindowHandle) -> Option<(f64, f64, f64, f64)> {
    let RawWindowHandle::AppKit(handle) = handle else {
        return None;
    };
    let mtm = MainThreadMarker::new()?;
    // SAFETY: an AppKit raw window handle always carries a live `NSView` pointer, and the
    // main-thread requirement is checked above.
    let view: &NSView = unsafe { &*handle.ns_view.as_ptr().cast() };
    let window = view.window()?;
    let frame = window.frame();
    let top = cursor::primary_height(mtm) - (frame.origin.y + frame.size.height);
    Some((frame.origin.x, top, frame.size.width, frame.size.height))
}

pub fn set_glass(handle: RawWindowHandle) {
    let RawWindowHandle::AppKit(handle) = handle else {
        return;
    };
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    // SAFETY: an AppKit raw window handle always carries a live `NSView` pointer, and the
    // main-thread requirement is checked above.
    let view: &NSView = unsafe { &*handle.ns_view.as_ptr().cast() };
    let Some(window) = view.window() else {
        return;
    };
    let Some(content) = window.contentView() else {
        return;
    };
    window.setOpaque(false);

    let effect =
        NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), NSView::bounds(&content));
    effect.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    effect.setMaterial(NSVisualEffectMaterial::HUDWindow);
    effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    effect.setState(NSVisualEffectState::Active);
    content.addSubview_positioned_relativeTo(&effect, NSWindowOrderingMode::Below, None);
}

pub fn round_corners(handle: RawWindowHandle, radius: f64) {
    let RawWindowHandle::AppKit(handle) = handle else {
        return;
    };
    let Some(_mtm) = MainThreadMarker::new() else {
        return;
    };
    // SAFETY: an AppKit raw window handle always carries a live `NSView` pointer, and the
    // main-thread requirement is checked above.
    let view: &NSView = unsafe { &*handle.ns_view.as_ptr().cast() };
    let Some(content) = view.window().and_then(|window| window.contentView()) else {
        return;
    };
    content.setWantsLayer(true);
    // SAFETY: `layer` is the live `CALayer` AppKit created for the now-layer-backed view. The
    // two setters go through `msg_send!` because `CALayer` lives in `objc2-quartz-core`, which
    // is not a dependency just for this.
    unsafe {
        let layer: *mut AnyObject = objc2::msg_send![&*content, layer];
        if !layer.is_null() {
            let _: () = objc2::msg_send![layer, setCornerRadius: radius];
            let _: () = objc2::msg_send![layer, setMasksToBounds: true];
        }
    }
}

pub fn frontmost_name() -> String {
    NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .and_then(|app| app.localizedName())
        .map(|name| name.to_string())
        .unwrap_or_else(|| "<none>".into())
}
