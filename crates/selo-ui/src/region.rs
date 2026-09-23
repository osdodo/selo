use gpui::{
    App, Bounds, Context, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, Render, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle,
    WindowKind, WindowOptions, div, point, prelude::*, px, rgb, rgba, size,
};
use selo_core::Region;
use std::sync::mpsc::Sender;

use crate::theme;

const MIN_DRAG: f32 = 8.;

pub struct RegionSelect {
    anchor: Option<Point<Pixels>>,
    cursor: Point<Pixels>,
    origin: (f64, f64),
    display: u64,
    // AppKit pushes a full-display window below the menu bar; window-local coords need the correction.
    offset: Point<Pixels>,
    done: Sender<Option<Region>>,
}

impl RegionSelect {
    pub fn open(
        cx: &mut App,
        display: u64,
        done: Sender<Option<Region>>,
    ) -> gpui::Result<WindowHandle<Self>> {
        let origin = selo_platform::display_frame(display)
            .map(|(x, y, _, _)| (x, y))
            .unwrap_or_default();
        // Wayland reports no primary display; fall back to the first output.
        let screen = cx
            .primary_display()
            .or_else(|| cx.displays().into_iter().next())
            .map(|display| display.bounds().size)
            .unwrap_or_else(|| size(px(1440.), px(900.)));

        // Layer-shell full-screen surface: Wayland places it; a toplevel's requested origin is ignored.
        #[cfg(target_os = "linux")]
        let kind = WindowKind::LayerShell(gpui::layer_shell::LayerShellOptions {
            namespace: "selo-region".into(),
            layer: gpui::layer_shell::Layer::Overlay,
            anchor: gpui::layer_shell::Anchor::TOP | gpui::layer_shell::Anchor::LEFT,
            exclusive_zone: Some(px(-1.)),
            keyboard_interactivity: gpui::layer_shell::KeyboardInteractivity::OnDemand,
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
            |_, cx| {
                cx.new(|_| Self {
                    anchor: None,
                    cursor: point(px(0.), px(0.)),
                    origin,
                    display,
                    offset: point(px(0.), px(0.)),
                    done,
                })
            },
        )
    }

    fn drag(&self) -> Option<(f32, f32, f32, f32)> {
        let anchor = self.anchor?;
        let (ax, ay) = (anchor.x.as_f32(), anchor.y.as_f32());
        let (cx, cy) = (self.cursor.x.as_f32(), self.cursor.y.as_f32());
        Some((ax.min(cx), ay.min(cy), (ax - cx).abs(), (ay - cy).abs()))
    }

    fn finish(&mut self) {
        let region = self
            .drag()
            .filter(|(_, _, w, h)| *w >= MIN_DRAG && *h >= MIN_DRAG)
            .map(|(x, y, w, h)| Region {
                x: (x + self.offset.x.as_f32()).max(0.) as u32,
                y: (y + self.offset.y.as_f32()).max(0.) as u32,
                width: w as u32,
                height: h as u32,
                display: self.display,
            });
        let _ = self.done.send(region);
    }
}

impl Render for RegionSelect {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgba(0x0000004d))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.anchor = Some(event.position);
                    this.cursor = event.position;
                    #[cfg(target_os = "linux")]
                    {
                        this.offset = point(px(this.origin.0 as f32), px(this.origin.1 as f32));
                    }
                    #[cfg(not(target_os = "linux"))]
                    {
                        let (gx, gy) = selo_platform::cursor_position();
                        this.offset = point(
                            px(gx as f32 - this.origin.0 as f32 - event.position.x.as_f32()),
                            px(gy as f32 - this.origin.1 as f32 - event.position.y.as_f32()),
                        );
                    }
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if this.anchor.is_some() {
                    this.cursor = event.position;
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, _, _| {
                    this.cursor = event.position;
                    this.finish();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _: &MouseDownEvent, _, _| {
                    this.anchor = None;
                    this.finish();
                }),
            )
            .when_some(self.drag(), |overlay, (x, y, w, h)| {
                overlay.child(
                    div()
                        .absolute()
                        .left(px(x))
                        .top(px(y))
                        .w(px(w))
                        .h(px(h))
                        .border_1()
                        .border_color(rgb(theme::ACCENT))
                        .bg(rgba(0xffffff1a)),
                )
            })
            // Only Wayland's layer surface takes keyboard focus; macOS's panel cannot, so gate Esc.
            .when(cfg!(target_os = "linux"), |overlay| {
                overlay.on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                    if event.keystroke.key == "escape" {
                        this.anchor = None;
                        this.finish();
                        cx.notify();
                    }
                }))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn dragged(from: (f32, f32), to: (f32, f32)) -> Option<Region> {
        dragged_with_offset(from, to, (0., 0.))
    }

    fn dragged_with_offset(from: (f32, f32), to: (f32, f32), offset: (f32, f32)) -> Option<Region> {
        let (done, results) = channel();
        let mut select = RegionSelect {
            anchor: Some(point(px(from.0), px(from.1))),
            cursor: point(px(to.0), px(to.1)),
            origin: (0., 0.),
            display: 7,
            offset: point(px(offset.0), px(offset.1)),
            done,
        };
        select.finish();
        results.recv().expect("finish always reports")
    }

    #[test]
    fn drags_up_and_left_the_same_as_down_and_right() {
        let forward = dragged((100., 80.), (160., 200.)).expect("big enough");
        let backward = dragged((160., 200.), (100., 80.)).expect("big enough");
        for region in [forward, backward] {
            assert_eq!(
                (region.x, region.y, region.width, region.height),
                (100, 80, 60, 120)
            );
        }
    }

    #[test]
    fn a_click_is_not_a_selection() {
        assert!(dragged((100., 80.), (103., 300.)).is_none());
        assert!(dragged((100., 80.), (100., 80.)).is_none());
    }

    #[test]
    fn window_local_drag_is_reported_in_display_coordinates() {
        let region = dragged_with_offset((100., 80.), (160., 200.), (0., 39.)).expect("big enough");
        assert_eq!(
            (region.x, region.y, region.width, region.height),
            (100, 119, 60, 120)
        );
        assert_eq!(region.display, 7);
    }
}
