use gpui::{
    App, Bounds, Context, Div, Entity, MouseButton, ObjectFit, Pixels, Render, RenderImage,
    SharedString, StyledImage, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle,
    WindowKind, WindowOptions, div, img, point, prelude::*, px, rgb, size,
};
use raw_window_handle::HasWindowHandle;
use selo_core::{Image, Rect, Region};
use selo_i18n::{Key, t};
use selo_layout::Paragraph;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

const FONT_RATIO: f32 = 0.72;
const MIN_FONT: f32 = 9.;
const MAX_FONT: f32 = 30.;
const SHRINK_STEP: f32 = 0.9;
const EDGE: f32 = 6.;
const CORNER: f32 = 12.;
const MIN_WIDTH: f64 = 120.;
const MIN_HEIGHT: f64 = 60.;
const BLUR: f32 = 20.;
const LIGHT_TEXT: u32 = 0xf2f2f4;
const DARK_TEXT: u32 = 0x1b1d24;

pub enum OverlayPhase {
    Translating,
    Done(String),
    Failed(String),
}

impl OverlayPhase {
    fn body(&self) -> SharedString {
        match self {
            Self::Translating => t(Key::Translating).into(),
            Self::Done(text) => text.clone().into(),
            Self::Failed(err) => err.clone().into(),
        }
    }
}

pub(crate) fn translated_block(
    rect: Rect,
    font: Pixels,
    phase: &OverlayPhase,
    scale: f32,
    max_width: f32,
    backdrop: Option<&Backdrop>,
    dark: bool,
) -> impl IntoElement {
    div()
        .absolute()
        .left(px(rect.x * scale))
        .top(px(rect.y * scale))
        .min_w(px(rect.width * scale))
        .max_w(px(max_width))
        .min_h(px(rect.height * scale))
        .rounded_sm()
        .text_size(font)
        .text_color(rgb(if dark { LIGHT_TEXT } else { DARK_TEXT }))
        .when_some(backdrop, |block, backdrop| {
            block.overflow_hidden().child(
                img(backdrop.image.clone())
                    .absolute()
                    .left(px(-rect.x * scale))
                    .top(px(-rect.y * scale))
                    .w(px(backdrop.width))
                    .h(px(backdrop.height))
                    .object_fit(ObjectFit::Fill),
            )
        })
        .child(div().px_1().child(phase.body()))
}

pub(crate) struct Backdrop {
    pub image: Arc<RenderImage>,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug)]
enum Edge {
    Top,
    Right,
    Bottom,
    Left,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

const EDGES: [Edge; 8] = [
    Edge::Top,
    Edge::Bottom,
    Edge::Left,
    Edge::Right,
    Edge::TopLeft,
    Edge::TopRight,
    Edge::BottomLeft,
    Edge::BottomRight,
];

#[derive(Clone, Copy)]
struct Resize {
    edge: Edge,
    start_cursor: (f64, f64),
    start_frame: (f64, f64, f64, f64),
}

struct Block {
    rect: Rect,
    line_height: f32,
    phase: OverlayPhase,
    ratio: Rc<Cell<f32>>,
}

type Fit = (Rc<Cell<f32>>, f32, f32, f32, f32);

// Move/resize read the cursor globally, and resize is ticked from the app's poll loop: AppKit
// window-move/resize are broken on this non-activating panel and a GPUI edge drag dies on pointer-out.
pub struct Overlay {
    image: Arc<RenderImage>,
    backdrop: Arc<RenderImage>,
    dark: bool,
    region: Region,
    device_scale: f32,
    blocks: Vec<Block>,
    grabbing: bool,
    grab_cursor: (f64, f64),
    grab_origin: (f64, f64),
    resizing: Option<Resize>,
    pending_fit: Rc<Cell<bool>>,
    fit_width: f32,
}

impl Overlay {
    pub fn open(
        cx: &mut App,
        region: Region,
        device_scale: f32,
        rendered: Arc<RenderImage>,
        backdrop: Arc<RenderImage>,
        dark: bool,
        paragraphs: &[Paragraph],
    ) -> gpui::Result<(WindowHandle<Self>, Entity<Self>)> {
        let blocks = paragraphs
            .iter()
            .map(|paragraph| Block {
                rect: paragraph.rect,
                line_height: paragraph.line_height,
                phase: OverlayPhase::Translating,
                ratio: Rc::new(Cell::new(FONT_RATIO)),
            })
            .collect();

        let mut view = None;
        // Wayland ignores `window_bounds` origin; anchor + margin place it instead.
        #[cfg(target_os = "linux")]
        let kind = WindowKind::LayerShell(gpui::layer_shell::LayerShellOptions {
            namespace: "selo-overlay".into(),
            layer: gpui::layer_shell::Layer::Overlay,
            anchor: gpui::layer_shell::Anchor::TOP | gpui::layer_shell::Anchor::LEFT,
            exclusive_zone: Some(px(-1.)),
            margin: Some((px(region.y as f32), px(0.), px(0.), px(region.x as f32))),
            ..Default::default()
        });
        #[cfg(not(target_os = "linux"))]
        let kind = WindowKind::PopUp;
        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(region.x as f32), px(region.y as f32)),
                    size: size(px(region.width as f32), px(region.height as f32)),
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
                    image: rendered,
                    backdrop,
                    dark,
                    region,
                    device_scale,
                    blocks,
                    grabbing: false,
                    grab_cursor: (0., 0.),
                    grab_origin: (0., 0.),
                    resizing: None,
                    pending_fit: Rc::new(Cell::new(true)),
                    fit_width: 0.,
                });
                view = Some(entity.clone());
                entity
            },
        )?;
        let view =
            view.ok_or_else(|| gpui::private::anyhow::anyhow!("overlay view was not created"))?;
        Ok((window, view))
    }

    pub fn set_phase(&mut self, index: usize, phase: OverlayPhase, cx: &mut Context<Self>) {
        if let Some(block) = self.blocks.get_mut(index) {
            block.phase = phase;
            block.ratio.set(FONT_RATIO);
            self.pending_fit.set(true);
            cx.notify();
        }
    }

    fn begin_move(&mut self, window: &mut Window) {
        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return;
        };
        let Some(frame) = selo_platform::window_frame(handle.as_raw()) else {
            return;
        };
        self.grabbing = true;
        self.grab_cursor = selo_platform::cursor_position();
        self.grab_origin = (frame.0, frame.1);
    }

    fn drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.grabbing || self.resizing.is_some() {
            return;
        }
        let (now_x, now_y) = selo_platform::cursor_position();
        let (x, y) = (
            self.grab_origin.0 + (now_x - self.grab_cursor.0),
            self.grab_origin.1 + (now_y - self.grab_cursor.1),
        );
        window.defer(cx, move |window, _| {
            if let Ok(handle) = HasWindowHandle::window_handle(window) {
                selo_platform::pin_top_left(handle.as_raw(), x, y);
            }
        });
    }

    fn begin_resize(&mut self, edge: Edge, window: &mut Window) {
        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return;
        };
        let Some(frame) = selo_platform::window_frame(handle.as_raw()) else {
            return;
        };
        self.resizing = Some(Resize {
            edge,
            start_cursor: selo_platform::cursor_position(),
            start_frame: frame,
        });
    }

    pub fn tick(&mut self, window: &mut Window, cx: &mut App) {
        let Some(resize) = self.resizing else {
            return;
        };
        if !selo_platform::mouse_button_down() {
            self.resizing = None;
            if self.pending_fit.get() {
                window.refresh();
            }
            return;
        }
        let (now_x, now_y) = selo_platform::cursor_position();
        let (dx, dy) = (now_x - resize.start_cursor.0, now_y - resize.start_cursor.1);
        let (x, y, w, h) = resize_frame(resize.edge, resize.start_frame, dx, dy);

        window.defer(cx, move |window, _| {
            window.resize(size(px(w as f32), px(h as f32)));
            if let Ok(handle) = HasWindowHandle::window_handle(window) {
                selo_platform::pin_top_left(handle.as_raw(), x, y);
            }
        });
    }
}

fn resize_frame(edge: Edge, frame: (f64, f64, f64, f64), dx: f64, dy: f64) -> (f64, f64, f64, f64) {
    let (mut x, mut y, mut w, mut h) = frame;
    if matches!(edge, Edge::Left | Edge::TopLeft | Edge::BottomLeft) {
        x += dx;
        w -= dx;
    }
    if matches!(edge, Edge::Right | Edge::TopRight | Edge::BottomRight) {
        w += dx;
    }
    if matches!(edge, Edge::Top | Edge::TopLeft | Edge::TopRight) {
        y += dy;
        h -= dy;
    }
    if matches!(edge, Edge::Bottom | Edge::BottomLeft | Edge::BottomRight) {
        h += dy;
    }
    if w < MIN_WIDTH {
        if matches!(edge, Edge::Left | Edge::TopLeft | Edge::BottomLeft) {
            x -= MIN_WIDTH - w;
        }
        w = MIN_WIDTH;
    }
    if h < MIN_HEIGHT {
        if matches!(edge, Edge::Top | Edge::TopLeft | Edge::TopRight) {
            y -= MIN_HEIGHT - h;
        }
        h = MIN_HEIGHT;
    }
    (x, y, w, h)
}

const BACKDROP_MAX: u32 = 512;

fn backdrop(pixels: &image::RgbaImage, device_scale: f32) -> Arc<RenderImage> {
    let longest = pixels.width().max(pixels.height());
    let div = longest.div_ceil(BACKDROP_MAX).max(1);
    if div <= 1 {
        return bgra(image::imageops::fast_blur(pixels, BLUR * device_scale));
    }
    let small = image::imageops::resize(
        pixels,
        (pixels.width() / div).max(1),
        (pixels.height() / div).max(1),
        image::imageops::FilterType::Nearest,
    );
    bgra(image::imageops::fast_blur(
        &small,
        BLUR * device_scale / div as f32,
    ))
}

pub fn prepare_overlay(
    image: &Image,
    device_scale: f32,
) -> gpui::Result<(Arc<RenderImage>, Arc<RenderImage>, bool)> {
    let pixels = decode(image)?;
    let dark = average_luma(&pixels) < 0.5;
    let backdrop = backdrop(&pixels, device_scale);
    Ok((bgra(pixels), backdrop, dark))
}

fn average_luma(pixels: &image::RgbaImage) -> f32 {
    let mut sum = 0.;
    let mut count = 0u64;
    for (index, pixel) in pixels.pixels().enumerate() {
        if index % 16 == 0 {
            sum += 0.2126 * pixel[0] as f32 + 0.7152 * pixel[1] as f32 + 0.0722 * pixel[2] as f32;
            count += 1;
        }
    }
    if count == 0 {
        1.
    } else {
        sum / count as f32 / 255.
    }
}

fn decode(image: &Image) -> gpui::Result<image::RgbaImage> {
    Ok(image::load_from_memory_with_format(&image.png, image::ImageFormat::Png)?.into_rgba8())
}

// GPUI expects BGRA; swap channels or every capture is colour-shifted.
fn bgra(mut pixels: image::RgbaImage) -> Arc<RenderImage> {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Arc::new(RenderImage::new(vec![image::Frame::new(pixels)]))
}

fn grow_to_fit(bottom: f32, window: &mut Window, cx: &mut App) {
    let current = window.bounds().size.height.as_f32();
    if bottom <= current + 0.5 {
        return;
    }
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let Some((x, y, width, _)) = selo_platform::window_frame(handle.as_raw()) else {
        return;
    };
    let cap = (selo_platform::primary_display_height() - y).max(MIN_HEIGHT) as f32;
    let wanted = bottom.min(cap);
    if wanted <= current + 0.5 {
        return;
    }

    window.defer(cx, move |window, _| {
        window.resize(size(px(width as f32), px(wanted)));
        if let Ok(handle) = HasWindowHandle::window_handle(window) {
            selo_platform::pin_top_left(handle.as_raw(), x, y);
        }
    });
}

fn shrink_ratio(ratio: f32, unit: f32, measured: f32, allowed: f32) -> Option<f32> {
    let font = (unit * ratio).clamp(MIN_FONT, MAX_FONT);
    if measured <= allowed + 1.0 || font <= MIN_FONT + 0.5 {
        return None;
    }
    Some((ratio * SHRINK_STEP).max(MIN_FONT / unit.max(1.)))
}

fn zone(edge: Edge) -> Div {
    let base = div().absolute();
    match edge {
        Edge::Top => base.top_0().left_0().right_0().h(px(EDGE)),
        Edge::Bottom => base.bottom_0().left_0().right_0().h(px(EDGE)),
        Edge::Left => base.left_0().top_0().bottom_0().w(px(EDGE)),
        Edge::Right => base.right_0().top_0().bottom_0().w(px(EDGE)),
        Edge::TopLeft => base.top_0().left_0().size(px(CORNER)),
        Edge::TopRight => base.top_0().right_0().size(px(CORNER)),
        Edge::BottomLeft => base.bottom_0().left_0().size(px(CORNER)),
        Edge::BottomRight => base.bottom_0().right_0().size(px(CORNER)),
    }
}

impl Render for Overlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let width = window.bounds().size.width.as_f32();
        let region_width = self.region.width.max(1) as f32;
        let scale = width / region_width / self.device_scale;
        let image_height = self.region.height as f32 * width / region_width;
        let busy = self.grabbing || self.resizing.is_some();
        let pending = self.pending_fit.clone();

        if (width - self.fit_width).abs() > 0.5 {
            self.fit_width = width;
            for block in &self.blocks {
                block.ratio.set(FONT_RATIO);
            }
        }

        let backdrop = Backdrop {
            image: self.backdrop.clone(),
            width,
            height: image_height,
        };

        let fit: Vec<Fit> = self
            .blocks
            .iter()
            .map(|block| {
                (
                    block.ratio.clone(),
                    block.line_height * scale,
                    block.rect.height * scale,
                    block.rect.x * scale,
                    block.rect.y * scale,
                )
            })
            .collect();

        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, _| this.begin_move(window)),
            )
            .on_mouse_move(cx.listener(|this, _, window, cx| this.drag(window, cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.grabbing = false;
                    if this.pending_fit.get() {
                        cx.notify();
                    }
                }),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .on_children_prepainted(move |bounds, window, cx| {
                        if busy {
                            return;
                        }
                        let mut shrank = false;
                        for (ratio, unit, allowed, bx, by) in &fit {
                            let measured = bounds
                                .iter()
                                .filter(|b| {
                                    (b.origin.x.as_f32() - bx).abs() < 1.5
                                        && (b.origin.y.as_f32() - by).abs() < 1.5
                                })
                                .map(|b| b.size.height.as_f32())
                                .fold(0., f32::max);
                            if let Some(next) = shrink_ratio(ratio.get(), *unit, measured, *allowed)
                            {
                                ratio.set(next);
                                shrank = true;
                            }
                        }
                        if shrank {
                            window.defer(cx, |window, _| window.refresh());
                            return;
                        }
                        if !pending.get() {
                            return;
                        }
                        pending.set(false);
                        let bottom = bounds
                            .iter()
                            .map(|b| b.origin.y.as_f32() + b.size.height.as_f32())
                            .fold(image_height, f32::max);
                        grow_to_fit(bottom, window, cx);
                    })
                    .child(
                        img(self.image.clone())
                            .absolute()
                            .top_0()
                            .left_0()
                            .w_full()
                            .h(px(image_height))
                            .object_fit(ObjectFit::Fill),
                    )
                    .children(self.blocks.iter().map(|block| {
                        let max_width = (width - block.rect.x * scale).max(40.);
                        let unit = (block.line_height * scale).max(1.);
                        let ratio = block.ratio.get().max(MIN_FONT / unit);
                        let font = px((unit * ratio).clamp(MIN_FONT, MAX_FONT));
                        translated_block(
                            block.rect,
                            font,
                            &block.phase,
                            scale,
                            max_width,
                            Some(&backdrop),
                            self.dark,
                        )
                    })),
            )
            .children(EDGES.map(|edge| {
                zone(edge).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.begin_resize(edge, window);
                    }),
                )
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shrink_ratio_only_shrinks_an_overflowing_block() {
        assert_eq!(shrink_ratio(0.72, 20., 20., 20.), None);
        assert_eq!(shrink_ratio(0.72, 20., 40., 20.), Some(0.72 * SHRINK_STEP));
    }

    #[test]
    fn average_luma_picks_the_text_colour_side() {
        let white = image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 255, 255, 255]));
        let black = image::RgbaImage::from_pixel(8, 8, image::Rgba([0, 0, 0, 255]));
        assert!(average_luma(&white) >= 0.5);
        assert!(average_luma(&black) < 0.5);
    }

    #[test]
    fn shrink_ratio_stops_at_the_readability_floor() {
        assert_eq!(shrink_ratio(0.72, 12.5, 999., 10.), None);
        let next = shrink_ratio(0.73, 20., 999., 10.).expect("still above the floor");
        assert!(next * 20. >= MIN_FONT);
    }
}
