/// Wayland has no global pointer monitoring, so outside-click close is never detected.
pub struct OutsideClick;

impl OutsideClick {
    pub fn new() -> Option<Self> {
        None
    }

    pub fn take(&self) -> bool {
        false
    }
}
