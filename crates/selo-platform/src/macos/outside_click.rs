use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSEvent, NSEventMask};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Fires on mouse presses in another application. A global `NSEvent` monitor sees only
/// events delivered elsewhere and needs no Accessibility permission, unlike a `CGEventTap`.
pub struct OutsideClick {
    monitor: Option<Retained<AnyObject>>,
    clicked: Arc<AtomicBool>,
}

impl OutsideClick {
    /// Must be called on the main thread.
    pub fn new() -> Option<Self> {
        let clicked = Arc::new(AtomicBool::new(false));
        let flag = clicked.clone();
        let handler = block2::RcBlock::new(move |_event| {
            flag.store(true, Ordering::Relaxed);
        });
        let monitor = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
            NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown,
            &handler,
        )?;
        Some(Self {
            monitor: Some(monitor),
            clicked,
        })
    }

    pub fn take(&self) -> bool {
        self.clicked.swap(false, Ordering::Relaxed)
    }
}

impl Drop for OutsideClick {
    fn drop(&mut self) {
        if let Some(monitor) = self.monitor.take() {
            unsafe { NSEvent::removeMonitor(&monitor) };
        }
    }
}
