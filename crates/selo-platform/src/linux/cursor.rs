use super::output;

/// Wayland exposes no global pointer position; always reported as `(0, 0)`.
pub fn position() -> (f64, f64) {
    (0.0, 0.0)
}

pub fn display() -> Option<u64> {
    output::outputs().first().map(|output| output.id)
}

pub fn primary_height() -> f64 {
    output::outputs()
        .first()
        .map_or(0.0, |output| output.logical().3)
}

pub fn frame(id: u64) -> Option<(f64, f64, f64, f64)> {
    let outputs = output::outputs();
    outputs
        .iter()
        .find(|output| output.id == id)
        .or_else(|| outputs.first())
        .map(output::Output::logical)
}

/// Wayland has no global mouse-button state.
pub fn mouse_button_down() -> bool {
    false
}
