use gpui::{
    App, Bounds, Context, Div, Entity, MouseButton, Pixels, Point, Render, SharedString, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, div, point,
    prelude::*, px, rgb, rgba, size,
};
use raw_window_handle::HasWindowHandle;
use selo_i18n::{Key, t};
use std::cell::Cell;
use std::rc::Rc;

const WIDTH: Pixels = px(380.);
const FONT_SIZE: Pixels = px(14.);
const LINE_HEIGHT: Pixels = px(21.);
const CHROME: Pixels = px(50.);
const MIN_HEIGHT: Pixels = px(71.);
const MAX_HEIGHT: Pixels = px(420.);
const CURSOR_OFFSET: Point<Pixels> = point(px(14.), px(18.));

pub enum PopupPhase {
    Translating,
    Done(String),
    Failed(String),
}

impl PopupPhase {
    fn body(&self) -> SharedString {
        match self {
            PopupPhase::Translating => t(Key::Translating).into(),
            PopupPhase::Done(text) => text.clone().into(),
            PopupPhase::Failed(err) => err.clone().into(),
        }
    }
}

pub struct Popup {
    phase: PopupPhase,
    source: Option<SharedString>,
    compare: bool,
    pinned: bool,
    copied: bool,
    top_left: Point<Pixels>,
    applied_height: Rc<Cell<f32>>,
}

impl Popup {
    pub fn open(
        cx: &mut App,
        at: Point<Pixels>,
        source: Option<String>,
    ) -> gpui::Result<(WindowHandle<Self>, Entity<Self>)> {
        let mut view = None;
        // Wayland: the dismiss surface probes the cursor and the app passes it in `at`.
        let top_left = {
            let wanted = at + CURSOR_OFFSET;
            #[cfg(target_os = "linux")]
            {
                let screen = cx
                    .primary_display()
                    .or_else(|| cx.displays().into_iter().next())
                    .map(|display| display.bounds().size)
                    .unwrap_or_else(|| size(px(1440.), px(900.)));
                let max_x = (screen.width - WIDTH - px(8.)).max(px(8.));
                let max_y = (screen.height - MIN_HEIGHT - px(8.)).max(px(8.));
                point(wanted.x.clamp(px(8.), max_x), wanted.y.clamp(px(8.), max_y))
            }
            #[cfg(not(target_os = "linux"))]
            {
                wanted
            }
        };
        #[cfg(target_os = "linux")]
        let kind = WindowKind::LayerShell(gpui::layer_shell::LayerShellOptions {
            namespace: "selo-popup".into(),
            layer: gpui::layer_shell::Layer::Overlay,
            anchor: gpui::layer_shell::Anchor::TOP | gpui::layer_shell::Anchor::LEFT,
            exclusive_zone: Some(px(-1.)),
            margin: Some((top_left.y, px(0.), px(0.), top_left.x)),
            ..Default::default()
        });
        #[cfg(not(target_os = "linux"))]
        let kind = WindowKind::PopUp;
        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: top_left,
                    size: size(WIDTH, MIN_HEIGHT),
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
                let entity = cx.new(|_| Self {
                    phase: PopupPhase::Translating,
                    source: source.map(SharedString::from),
                    compare: false,
                    pinned: false,
                    copied: false,
                    top_left,
                    applied_height: Rc::new(Cell::new(MIN_HEIGHT.as_f32())),
                });
                view = Some(entity.clone());
                entity
            },
        )?;
        window.update(cx, |_, window, _| {
            if let Ok(handle) = HasWindowHandle::window_handle(window) {
                let raw = handle.as_raw();
                selo_platform::set_glass(raw);
                let radius = window.rem_size().as_f32() * 0.75;
                selo_platform::round_corners(raw, radius as f64);
            }
        })?;
        Ok((window, view.expect("open_window ran the root builder")))
    }

    pub fn set_phase(&mut self, phase: PopupPhase, cx: &mut Context<Self>) {
        self.phase = phase;
        cx.notify();
    }

    pub fn is_pinned(&self) -> bool {
        self.pinned
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        if let PopupPhase::Done(text) = &self.phase
            && selo_platform::set_clipboard_text(text)
        {
            self.copied = true;
            cx.notify();
        }
    }
}

fn action(label: impl Into<SharedString>, active: bool) -> Div {
    div()
        .px_2()
        .py_1()
        .rounded_lg()
        .text_size(px(12.))
        .text_color(rgb(if active { 0xf2f2f4 } else { 0xd8dce5 }))
        .when(active, |button| button.bg(rgba(super::theme::ACTIVE)))
        .hover(|button| button.bg(rgba(super::theme::HOVER)))
        .child(label.into())
}

fn fit_window(
    text: Bounds<Pixels>,
    applied: &Cell<f32>,
    top_left: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let wanted = (text.size.height + CHROME).clamp(MIN_HEIGHT, MAX_HEIGHT);
    if (wanted.as_f32() - applied.get()).abs() < 0.5 {
        return;
    }
    applied.set(wanted.as_f32());

    // Deferred: a direct AppKit resize mid-prepaint leaves the card drawing at its old size.
    window.defer(cx, move |window, _| {
        window.resize(size(WIDTH, wanted));
        if let Ok(handle) = HasWindowHandle::window_handle(window) {
            selo_platform::pin_top_left(
                handle.as_raw(),
                top_left.x.as_f32() as f64,
                top_left.y.as_f32() as f64,
            );
        }
    });
}

impl Render for Popup {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tone = match &self.phase {
            PopupPhase::Translating => rgb(0xb8becb),
            PopupPhase::Done(_) => rgb(0xf2f2f4),
            PopupPhase::Failed(_) => rgb(0xff6b8b),
        };
        let applied = self.applied_height.clone();
        let top_left = self.top_left;
        let source = self.compare.then(|| self.source.clone()).flatten();
        let copyable = matches!(self.phase, PopupPhase::Done(_));
        let (compare, pinned, copied) = (self.compare, self.pinned, self.copied);

        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded_xl()
            .bg(rgba(super::theme::WINDOW))
            .border_1()
            .border_color(rgba(super::theme::BORDER))
            .child(
                div()
                    .on_children_prepainted(move |bounds, window, cx| {
                        if let Some(text) = bounds.first() {
                            fit_window(*text, &applied, top_left, window, cx);
                        }
                    })
                    .id("translation")
                    .flex_1()
                    .overflow_y_scroll()
                    .px_3()
                    .pt_3()
                    .pb_1()
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .text_size(FONT_SIZE)
                            .line_height(LINE_HEIGHT)
                            .when_some(source, |body, source| {
                                body.child(div().text_color(rgb(0xb8becb)).child(source))
                            })
                            .child(div().text_color(tone).child(self.phase.body())),
                    ),
            )
            .child(
                div()
                    .h(px(32.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .px_3()
                    .when(copyable, |row| {
                        row.child(
                            action(if copied { t(Key::Copied) } else { t(Key::Copy) }, copied)
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, _, cx| this.copy(cx)),
                                ),
                        )
                    })
                    .child(action(t(Key::Compare), compare).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.compare = !this.compare;
                            cx.notify();
                        }),
                    ))
                    .child(
                        action(if pinned { t(Key::Pinned) } else { t(Key::Pin) }, pinned)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.pinned = !this.pinned;
                                    cx.notify();
                                }),
                            ),
                    ),
            )
    }
}
