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
use std::collections::HashMap;
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
    fill: u32,
) -> impl IntoElement {
    div()
        .absolute()
        .left(px(rect.x * scale))
        .top(px(rect.y * scale))
        .min_w(px(rect.width * scale))
        .max_w(px(max_width))
        .min_h(px(rect.height * scale))
        .rounded_sm()
        .bg(rgb(fill))
        .text_size(font)
        .text_color(rgb(text_on(fill)))
        .child(div().px_1().child(phase.body()))
}

fn text_on(fill: u32) -> u32 {
    if luma(fill) < 0.5 {
        LIGHT_TEXT
    } else {
        DARK_TEXT
    }
}

fn luma(color: u32) -> f32 {
    let r = (color >> 16) & 0xff;
    let g = (color >> 8) & 0xff;
    let b = color & 0xff;
    (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.
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
    fill: u32,
}

type Fit = (Rc<Cell<f32>>, f32, f32, f32, f32);

// Move/resize read the cursor globally, and resize is ticked from the app's poll loop: AppKit
// window-move/resize are broken on this non-activating panel and a GPUI edge drag dies on pointer-out.
pub struct Overlay {
    image: Arc<RenderImage>,
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
        fills: &[u32],
        paragraphs: &[Paragraph],
    ) -> gpui::Result<(WindowHandle<Self>, Entity<Self>)> {
        let blocks = paragraphs
            .iter()
            .enumerate()
            .map(|(index, paragraph)| Block {
                rect: paragraph.rect,
                line_height: paragraph.line_height,
                phase: OverlayPhase::Translating,
                ratio: Rc::new(Cell::new(FONT_RATIO)),
                fill: fills.get(index).copied().unwrap_or(0),
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

pub fn prepare_overlay(
    image: &Image,
    paragraphs: &[Paragraph],
) -> gpui::Result<(Arc<RenderImage>, Vec<u32>)> {
    let pixels = decode(image)?;
    let fills = paragraphs
        .iter()
        .map(|paragraph| background_of(&pixels, paragraph.rect))
        .collect();
    Ok((bgra(pixels), fills))
}

// The dominant colour inside a line's box is its background; glyphs are the minority of pixels.
fn background_of(pixels: &image::RgbaImage, rect: Rect) -> u32 {
    let (w, h) = (pixels.width() as i64, pixels.height() as i64);
    let x0 = rect.x.floor().max(0.) as i64;
    let y0 = rect.y.floor().max(0.) as i64;
    let x1 = ((rect.x + rect.width).ceil() as i64).min(w);
    let y1 = ((rect.y + rect.height).ceil() as i64).min(h);
    let mut bins: HashMap<u32, (u32, u64, u64, u64)> = HashMap::new();
    for y in y0..y1 {
        for x in x0..x1 {
            let pixel = pixels.get_pixel(x as u32, y as u32).0;
            let bin = bins.entry(quantize(pixel)).or_insert((0, 0, 0, 0));
            bin.0 += 1;
            bin.1 += pixel[0] as u64;
            bin.2 += pixel[1] as u64;
            bin.3 += pixel[2] as u64;
        }
    }
    match bins.into_iter().max_by_key(|(_, bin)| bin.0) {
        Some((_, (count, r, g, b))) if count > 0 => {
            let avg = |sum: u64| (sum / count as u64) as u32;
            (avg(r) << 16) | (avg(g) << 8) | avg(b)
        }
        _ => 0,
    }
}

fn quantize(pixel: [u8; 4]) -> u32 {
    ((pixel[0] as u32 >> 3) << 10) | ((pixel[1] as u32 >> 3) << 5) | (pixel[2] as u32 >> 3)
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
                            block.fill,
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
    fn background_of_reads_the_dominant_colour_not_the_glyphs() {
        let mut pixels = image::RgbaImage::from_pixel(20, 10, image::Rgba([250, 250, 250, 255]));
        for x in 0..4 {
            pixels.put_pixel(x, 5, image::Rgba([10, 10, 10, 255]));
        }
        let rect = Rect {
            x: 0.,
            y: 0.,
            width: 20.,
            height: 10.,
        };
        assert_eq!(background_of(&pixels, rect), 0xfafafa);
    }

    #[test]
    fn light_fill_gets_dark_text_and_vice_versa() {
        assert!(luma(0xffffff) >= 0.5);
        assert!(luma(0x000000) < 0.5);
    }

    #[test]
    fn shrink_ratio_stops_at_the_readability_floor() {
        assert_eq!(shrink_ratio(0.72, 12.5, 999., 10.), None);
        let next = shrink_ratio(0.73, 20., 999., 10.).expect("still above the floor");
        assert!(next * 20. >= MIN_FONT);
    }
}
