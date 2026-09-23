use gpui::{
    App, Bounds, Context, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point, Render,
    Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, div,
    point, prelude::*, px, size,
};
use std::sync::mpsc::Sender;

// Wayland has no global pointer monitor or cursor position; this surface provides both.
pub enum DismissEvent {
    Click,
    Pointer(Point<Pixels>),
}

pub struct Dismiss {
    events: Sender<DismissEvent>,
}

impl Dismiss {
    pub fn open(cx: &mut App, events: Sender<DismissEvent>) -> gpui::Result<WindowHandle<Self>> {
        // Wayland reports no primary display; fall back to the first output.
        let screen = cx
            .primary_display()
            .or_else(|| cx.displays().into_iter().next())
            .map(|display| display.bounds().size)
            .unwrap_or_else(|| size(px(1440.), px(900.)));

        #[cfg(target_os = "linux")]
        let kind = WindowKind::LayerShell(gpui::layer_shell::LayerShellOptions {
            namespace: "selo-dismiss".into(),
            layer: gpui::layer_shell::Layer::Top,
            anchor: gpui::layer_shell::Anchor::TOP
                | gpui::layer_shell::Anchor::LEFT
                | gpui::layer_shell::Anchor::RIGHT
                | gpui::layer_shell::Anchor::BOTTOM,
            exclusive_zone: Some(px(-1.)),
            keyboard_interactivity: gpui::layer_shell::KeyboardInteractivity::None,
            ..Default::default()
        });
        #[cfg(not(target_os = "linux"))]
        let kind = WindowKind::PopUp;

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: screen,
                })),
                titlebar: None,
                kind,
                focus: false,
                show: true,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            },
            move |_, cx| cx.new(|_| Self { events }),
        )
    }
}

impl Render for Dismiss {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let moves = self.events.clone();
        let left = self.events.clone();
        let right = self.events.clone();
        div()
            .size_full()
            .on_mouse_move(move |event: &MouseMoveEvent, _, _| {
                let _ = moves.send(DismissEvent::Pointer(event.position));
            })
            .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, _, _| {
                let _ = left.send(DismissEvent::Click);
            })
            .on_mouse_down(MouseButton::Right, move |_: &MouseDownEvent, _, _| {
                let _ = right.send(DismissEvent::Click);
            })
    }
}
