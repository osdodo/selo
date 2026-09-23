use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, Context, Entity, FocusHandle, Focusable, KeyDownEvent, Keystroke,
    MouseButton, Render, TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds,
    WindowHandle, WindowKind, WindowOptions, anchored, deferred, div, point, px, rgb, rgba, size,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc::Sender;

use crate::text_input::TextInput;
use crate::theme;
use selo_i18n::{Key, t};

mod surface {
    pub const BG: u32 = 0x1e2024ff;
    pub const NAV: u32 = 0x18191dff;
    pub const CARD: u32 = 0x2a2d33ff;
    pub const FIELD: u32 = 0x24262bff;
    pub const POPOVER: u32 = 0x2a2d33ff;
    pub const BORDER: u32 = 0x383b42ff;
}

#[derive(Clone)]
pub struct SettingsField {
    pub key: String,
    pub label: String,
    pub kind: SettingsFieldKind,
    pub value: String,
    pub show_if: Option<(String, String)>,
}

#[derive(Clone)]
pub enum SettingsFieldKind {
    Text,
    Secret,
    Toggle,
    Choice(Vec<SettingsChoice>),
    Hotkey,
}

#[derive(Clone)]
pub struct SettingsChoice {
    pub label: String,
    pub value: String,
}

pub struct SettingsGroup {
    pub label: String,
    pub fields: Vec<SettingsField>,
}

#[derive(Clone)]
pub struct SettingsService {
    pub id: String,
    pub name: String,
    pub builtin: bool,
    pub ocr: bool,
    pub fields: Vec<SettingsField>,
}

#[derive(Clone, PartialEq)]
pub struct SettingsSave {
    pub general: Vec<(String, String)>,
    pub values: Vec<(String, String, bool, String)>,
    pub removed: Vec<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Group(usize),
    Services,
}

pub struct SettingsView {
    page: Page,
    groups: Vec<SettingsGroup>,
    services: Vec<SettingsService>,
    available: Vec<SettingsService>,
    pending_delete: Vec<String>,
    editing: Option<usize>,
    active: Option<usize>,
    cursor: usize,
    input: Option<(usize, Entity<TextInput>)>,
    open: Option<usize>,
    field_bounds: Rc<RefCell<HashMap<usize, Bounds<gpui::Pixels>>>>,
    add_open: bool,
    add_external: Sender<()>,
    save: Sender<SettingsSave>,
    saved: SettingsSave,
    focus: FocusHandle,
}

impl SettingsView {
    pub fn open(
        cx: &mut App,
        groups: Vec<SettingsGroup>,
        services: Vec<SettingsService>,
        available: Vec<SettingsService>,
        add_external: Sender<()>,
        save: Sender<SettingsSave>,
    ) -> gpui::Result<WindowHandle<Self>> {
        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::centered(size(px(760.), px(520.)), cx)),
                titlebar: Some(TitlebarOptions {
                    title: Some(t(Key::SettingsTitle).into()),
                    ..Default::default()
                }),
                kind: WindowKind::Normal,
                focus: true,
                show: true,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Opaque,
                app_id: Some(selo_platform::APP_ID.into()),
                ..Default::default()
            },
            move |_window, cx| {
                let saved = snapshot(&groups, &services, &available, &[]);
                cx.new(|cx| Self {
                    page: Page::Group(0),
                    groups,
                    services,
                    available,
                    pending_delete: Vec::new(),
                    editing: None,
                    active: None,
                    cursor: 0,
                    input: None,
                    open: None,
                    field_bounds: Rc::new(RefCell::new(HashMap::new())),
                    add_open: false,
                    add_external,
                    save,
                    saved,
                    focus: cx.focus_handle(),
                })
            },
        )?;
        window.update(cx, |_, window, cx| {
            // Commit the in-progress edit on close, or it is lost.
            window.on_window_should_close(cx, |window, cx| {
                if let Some(view) = window.root::<Self>().flatten() {
                    view.update(cx, |view, cx| view.commit_input(cx));
                }
                true
            });
        })?;
        Ok(window)
    }

    fn current_fields(&self) -> Option<&Vec<SettingsField>> {
        match self.page {
            Page::Group(index) => self.groups.get(index).map(|group| &group.fields),
            Page::Services => self
                .editing
                .and_then(|i| self.services.get(i))
                .map(|s| &s.fields),
        }
    }

    fn current_fields_mut(&mut self) -> Option<&mut Vec<SettingsField>> {
        match self.page {
            Page::Group(index) => self.groups.get_mut(index).map(|group| &mut group.fields),
            Page::Services => {
                let index = self.editing?;
                self.services.get_mut(index).map(|s| &mut s.fields)
            }
        }
    }

    fn active_chars(&self) -> Option<Vec<char>> {
        let index = self.active?;
        Some(self.current_fields()?.get(index)?.value.chars().collect())
    }

    fn activate_field(&mut self, field_index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let kind = self
            .current_fields()
            .and_then(|fields| fields.get(field_index))
            .map(|field| field.kind.clone());
        match kind {
            Some(SettingsFieldKind::Toggle) => {
                self.commit_input(cx);
                self.with_field(field_index, |field| {
                    field.value = if field.value == "true" {
                        "false".into()
                    } else {
                        "true".into()
                    };
                });
                self.push_save();
            }
            Some(SettingsFieldKind::Choice(choices)) if !choices.is_empty() => {
                self.commit_input(cx);
                self.active = None;
                self.open = if self.open == Some(field_index) {
                    None
                } else {
                    Some(field_index)
                };
            }
            Some(SettingsFieldKind::Text) => {
                if self
                    .input
                    .as_ref()
                    .is_some_and(|(index, _)| *index == field_index)
                {
                    if let Some((_, input)) = &self.input {
                        window.focus(&input.read(cx).focus_handle(cx), cx);
                    }
                    cx.stop_propagation();
                    return;
                }
                self.commit_input(cx);
                self.active = None;
                self.open = None;
                let value = self
                    .current_fields()
                    .and_then(|fields| fields.get(field_index))
                    .map_or_else(String::new, |field| field.value.clone());
                let input = cx.new(|cx| TextInput::new(value, cx));
                window.focus(&input.read(cx).focus_handle(cx), cx);
                self.input = Some((field_index, input));
            }
            Some(_) => {
                self.commit_input(cx);
                self.active = Some(field_index);
                self.input = None;
                self.open = None;
                self.cursor = self
                    .current_fields()
                    .and_then(|fields| fields.get(field_index))
                    .map_or(0, |field| field.value.chars().count());
                window.focus(&self.focus, cx);
            }
            None => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn commit_input(&mut self, cx: &mut Context<Self>) {
        if let Some((index, input)) = self.input.take() {
            let text = input.read(cx).content().to_string();
            if let Some(field) = self
                .current_fields_mut()
                .and_then(|fields| fields.get_mut(index))
            {
                field.value = text;
            }
        }
        self.push_save();
    }

    fn sync_engine_choices(&mut self) {
        let translate: Vec<SettingsChoice> = self
            .services
            .iter()
            .filter(|service| !service.ocr)
            .map(service_choice)
            .collect();
        let mut ocr = vec![SettingsChoice {
            label: t(Key::SystemDefault).to_string(),
            value: "system".to_string(),
        }];
        ocr.extend(
            self.services
                .iter()
                .filter(|service| service.ocr)
                .map(service_choice),
        );
        for group in &mut self.groups {
            for field in &mut group.fields {
                match field.key.as_str() {
                    "engine" => field.kind = SettingsFieldKind::Choice(translate.clone()),
                    "ocr_engine" => field.kind = SettingsFieldKind::Choice(ocr.clone()),
                    _ => {}
                }
            }
        }
    }

    fn push_save(&mut self) {
        let save = snapshot(
            &self.groups,
            &self.services,
            &self.available,
            &self.pending_delete,
        );
        if save != self.saved {
            let _ = self.save.send(save.clone());
            self.saved = save;
        }
    }

    fn with_field(&mut self, field_index: usize, edit: impl FnOnce(&mut SettingsField)) {
        if let Some(field) = self
            .current_fields_mut()
            .and_then(|fields| fields.get_mut(field_index))
        {
            edit(field);
        }
    }

    fn on_key(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;

        if matches!(keystroke.key.as_str(), "enter" | "escape") {
            self.commit_input(cx);
            self.active = None;
            cx.stop_propagation();
            cx.notify();
            return;
        }

        let Some(field_index) = self.active else {
            return;
        };
        let Some(mut chars) = self.active_chars() else {
            return;
        };

        let hotkey = self
            .current_fields()
            .and_then(|fields| fields.get(field_index))
            .is_some_and(|field| matches!(field.kind, SettingsFieldKind::Hotkey));
        if hotkey {
            if let Some(chord) = chord(keystroke) {
                self.with_field(field_index, |field| field.value = chord);
                self.active = None;
                self.push_save();
                cx.notify();
            }
            return;
        }

        let command = keystroke.modifiers.platform;
        let control = keystroke.modifiers.control;
        self.cursor = self.cursor.min(chars.len());
        let len = chars.len();

        let insert = |chars: &mut Vec<char>, cursor: &mut usize, text: &str| {
            let at = *cursor;
            let inserted: Vec<char> = text.chars().collect();
            *cursor = at + inserted.len();
            chars.splice(at..at, inserted);
        };

        match keystroke.key.as_str() {
            "backspace" => {
                if self.cursor > 0 {
                    chars.remove(self.cursor - 1);
                    self.cursor -= 1;
                }
            }
            "delete" => {
                if self.cursor < len {
                    chars.remove(self.cursor);
                }
            }
            "left" => self.cursor = self.cursor.saturating_sub(1),
            "right" => self.cursor = (self.cursor + 1).min(len),
            "home" => self.cursor = 0,
            "end" => self.cursor = len,
            "v" if command => {
                if let Some(text) = selo_platform::clipboard_text() {
                    insert(&mut chars, &mut self.cursor, &text);
                }
            }
            _ => {
                if !command
                    && !control
                    && let Some(text) = &keystroke.key_char
                    && !text.is_empty()
                {
                    insert(&mut chars, &mut self.cursor, text);
                }
            }
        }

        self.with_field(field_index, |field| {
            field.value = chars.into_iter().collect();
        });
        cx.notify();
    }

    fn render_nav(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let pages = self
            .groups
            .iter()
            .enumerate()
            .map(|(index, group)| (Page::Group(index), group.label.clone()))
            .chain(std::iter::once((
                Page::Services,
                t(Key::Services).to_string(),
            )));
        div()
            .flex()
            .flex_col()
            .w(px(184.))
            .h_full()
            .p_2()
            .gap_1()
            .border_r_1()
            .border_color(rgba(surface::BORDER))
            .bg(rgba(surface::NAV))
            .children(pages.into_iter().enumerate().map(|(index, (page, label))| {
                let active = self.page == page;
                div()
                    .id(("nav", index))
                    .px_3()
                    .py_2()
                    .rounded_lg()
                    .text_size(px(13.))
                    .when(active, |row| row.bg(rgba(theme::ACTIVE)))
                    .text_color(if active { rgb(0xe8e8ea) } else { rgb(0xd0d4de) })
                    .hover(|row| row.bg(rgba(theme::HOVER)))
                    .child(label)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.commit_input(cx);
                            this.page = page;
                            this.editing = None;
                            this.active = None;
                            this.open = None;
                            this.add_open = false;
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
            }))
    }

    fn render_group(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let group = &self.groups[index];
        div()
            .flex()
            .flex_col()
            .flex_1()
            .child(
                div()
                    .id(("group-scroll", index))
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_y_scroll()
                    .px_4()
                    .pt_4()
                    .pb_4()
                    .gap_3()
                    .child(self.fields_column(&group.fields, cx)),
            )
            .into_any_element()
    }

    fn render_services(&self, cx: &mut Context<Self>) -> AnyElement {
        let cards = self
            .services
            .iter()
            .enumerate()
            .map(|(index, service)| service_card(index, service, cx))
            .collect::<Vec<_>>();

        let available = self
            .available
            .iter()
            .enumerate()
            .map(|(index, service)| {
                let id = service.id.clone();
                div()
                    .id(("add-avail", index))
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .text_size(px(13.))
                    .text_color(rgb(0xd8dce5))
                    .hover(|row| row.bg(rgba(theme::HOVER)))
                    .child(service.name.clone())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            if let Some(position) = this.available.iter().position(|s| s.id == id) {
                                this.services.push(this.available.remove(position));
                            }
                            this.sync_engine_choices();
                            this.add_open = false;
                            this.push_save();
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
            })
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .flex_1()
            .child(
                div()
                    .id("services-scroll")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_y_scroll()
                    .px_4()
                    .pt_4()
                    .pb_2()
                    .gap_2()
                    .children(cards),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(rgba(surface::BORDER))
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .flex()
                            .child(secondary_button(t(Key::AddBuiltinService)).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    if !this.available.is_empty() {
                                        this.add_open = !this.add_open;
                                    }
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                            .when(self.add_open && !self.available.is_empty(), |wrapper| {
                                wrapper.child(deferred(
                                    div()
                                        .absolute()
                                        .bottom_full()
                                        .mb_1()
                                        .left(px(0.))
                                        .w_full()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .p_1()
                                        .rounded_lg()
                                        .border_1()
                                        .border_color(rgba(surface::BORDER))
                                        .bg(rgba(surface::POPOVER))
                                        .children(available),
                                ))
                            }),
                    )
                    .child(secondary_button(t(Key::AddExternalPlugin)).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| {
                            let _ = this.add_external.send(());
                        }),
                    )),
            )
            .into_any_element()
    }

    fn render_fields_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let name = self
            .editing
            .and_then(|index| self.services.get(index))
            .map_or_else(String::new, |service| service.name.clone());
        let mut column = div().flex().flex_col().gap_4().max_w(px(560.));
        if let Some(fields) = self.current_fields() {
            column = column.children(
                fields
                    .iter()
                    .enumerate()
                    .filter(|(_, field)| field_visible(fields, field))
                    .map(|(index, field)| field_element(index, field, self, cx)),
            );
        }
        div()
            .flex()
            .flex_col()
            .flex_1()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px_5()
                    .pt_5()
                    .pb_4()
                    .child(
                        div()
                            .id("back")
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_size(px(13.))
                            .text_color(rgb(0xd0d4de))
                            .hover(|row| row.text_color(rgb(0xe8e8ea)))
                            .child(t(Key::Back))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.commit_input(cx);
                                    this.editing = None;
                                    this.active = None;
                                    this.open = None;
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(18.))
                            .text_color(rgb(0xe8e8ea))
                            .child(name),
                    ),
            )
            .child(div().h(px(1.)).bg(rgba(surface::BORDER)))
            .child(
                div()
                    .id("fields-scroll")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_y_scroll()
                    .px_5()
                    .py_5()
                    .child(column),
            )
            .into_any_element()
    }

    fn fields_column(&self, fields: &[SettingsField], cx: &mut Context<Self>) -> impl IntoElement {
        div().flex().flex_col().gap_3().children(
            fields
                .iter()
                .enumerate()
                .filter(|(_, field)| field_visible(fields, field))
                .map(|(field_index, field)| field_element(field_index, field, self, cx)),
        )
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = match self.page {
            Page::Group(index) => self.render_group(index, cx),
            Page::Services if self.editing.is_some() => self.render_fields_page(cx),
            Page::Services => self.render_services(cx),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgba(surface::BG))
            .text_color(rgb(0xe8e8ea))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.open.is_some() || this.active.is_some() || this.add_open {
                        this.open = None;
                        this.active = None;
                        this.add_open = false;
                        cx.notify();
                    }
                    this.commit_input(cx);
                }),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(self.render_nav(cx))
                    .child(page),
            )
    }
}

fn service_card(
    index: usize,
    service: &SettingsService,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let name = service.name.clone();
    let is_builtin = service.builtin;
    let has_fields = !service.fields.is_empty();

    div()
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py_2()
        .rounded_lg()
        .border_1()
        .border_color(rgba(surface::BORDER))
        .bg(rgba(surface::CARD))
        .child(div().flex_1().text_size(px(13.)).child(name))
        .when(has_fields, |row| {
            row.child(
                card_action(("edit", index), t(Key::Edit), false).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.commit_input(cx);
                        this.editing = Some(index);
                        this.active = None;
                        this.open = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
            )
        })
        .child(
            card_action(("delete", index), t(Key::Delete), true).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.commit_input(cx);
                    if let Some(service) = this.services.get(index) {
                        if is_builtin {
                            this.available.push(service.clone());
                        } else {
                            this.pending_delete.push(service.id.clone());
                        }
                    }
                    this.services.remove(index);
                    this.sync_engine_choices();
                    this.editing = None;
                    this.push_save();
                    cx.stop_propagation();
                    cx.notify();
                }),
            ),
        )
        .into_any_element()
}

fn snapshot(
    groups: &[SettingsGroup],
    services: &[SettingsService],
    available: &[SettingsService],
    pending_delete: &[String],
) -> SettingsSave {
    let general = groups
        .iter()
        .flat_map(|group| group.fields.iter())
        .map(|field| (field.key.clone(), field.value.clone()))
        .collect();
    let mut values = Vec::new();
    for service in services {
        for field in &service.fields {
            let secret = matches!(field.kind, SettingsFieldKind::Secret);
            values.push((
                service.id.clone(),
                field.key.clone(),
                secret,
                field.value.clone(),
            ));
        }
    }
    let mut removed: Vec<String> = available.iter().map(|service| service.id.clone()).collect();
    removed.extend(pending_delete.iter().cloned());
    SettingsSave {
        general,
        values,
        removed,
    }
}

fn service_choice(service: &SettingsService) -> SettingsChoice {
    SettingsChoice {
        label: service.name.clone(),
        value: service.id.clone(),
    }
}

// Text, not ✎/✕: those glyphs are absent from base Noto Sans on minimal Linux.
fn card_action(
    id: (&'static str, usize),
    label: &'static str,
    danger: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .px_2()
        .py_1()
        .rounded_lg()
        .text_size(px(12.))
        .text_color(if danger { rgb(0xf7768e) } else { rgb(0xd8dce5) })
        .hover(|row| row.bg(rgba(theme::HOVER)))
        .child(label)
}

fn secondary_button(label: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(label.to_string())
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .px_4()
        .py_2()
        .rounded_lg()
        .bg(rgba(0xffffff14))
        .text_color(rgb(0xe4e7ee))
        .text_size(px(13.))
        .hover(|button| button.opacity(0.9))
        .child(label)
}

fn field_element(
    field_index: usize,
    field: &SettingsField,
    view: &SettingsView,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let focused = view.active == Some(field_index);
    let editable = matches!(
        field.kind,
        SettingsFieldKind::Text | SettingsFieldKind::Secret | SettingsFieldKind::Hotkey
    );
    let ime = if matches!(field.kind, SettingsFieldKind::Text) {
        view.input
            .as_ref()
            .filter(|(index, _)| *index == field_index)
            .map(|(_, input)| input.clone())
    } else {
        None
    };
    let simple_focused = focused && editable && ime.is_none();
    let (before, after) = if simple_focused {
        let chars: Vec<char> = field.value.chars().collect();
        let at = view.cursor.min(chars.len());
        let render = |text: Vec<char>| {
            if matches!(field.kind, SettingsFieldKind::Secret) {
                "•".repeat(text.len())
            } else {
                text.into_iter().collect()
            }
        };
        (render(chars[..at].to_vec()), render(chars[at..].to_vec()))
    } else {
        (display_field(field), String::new())
    };
    let placeholder = editable && field.value.is_empty() && !focused && ime.is_none();

    let open_choices = if view.open == Some(field_index) {
        match &field.kind {
            SettingsFieldKind::Choice(choices) => choices.clone(),
            _ => Vec::new(),
        }
    } else {
        Vec::new()
    };
    let current_value = field.value.clone();
    let mut options: Vec<AnyElement> = Vec::new();
    for (option_index, choice) in open_choices.iter().enumerate() {
        let value = choice.value.clone();
        let selected = value == current_value;
        options.push(
            div()
                .id(("option", field_index * 100 + option_index))
                .px_3()
                .py_2()
                .rounded_md()
                .text_size(px(12.))
                .bg(if selected {
                    rgba(theme::ACTIVE)
                } else {
                    rgba(0x00000000)
                })
                .text_color(if selected {
                    rgb(0xe8e8ea)
                } else {
                    rgb(0xd8dce5)
                })
                .hover(|row| row.bg(rgba(theme::HOVER)))
                .child(choice.label.clone())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.with_field(field_index, |field| {
                            field.value = value.clone();
                        });
                        this.open = None;
                        this.active = None;
                        this.push_save();
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .into_any_element(),
        );
    }

    let anchor = view
        .field_bounds
        .borrow()
        .get(&field_index)
        .map(|bounds| (bounds.origin, bounds.size.width));

    let field_row = div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .text_size(px(12.))
                .text_color(rgb(0xa8aebc))
                .child(field.label.clone()),
        )
        .child(
            div()
                .id(("field", field_index))
                .flex()
                .items_center()
                .px_3()
                .py_2()
                .rounded_lg()
                .border_1()
                .border_color(if focused || ime.is_some() {
                    rgb(theme::ACCENT)
                } else {
                    rgba(surface::BORDER)
                })
                .bg(rgba(surface::FIELD))
                .line_height(px(18.))
                .text_size(px(13.))
                .text_color(if placeholder {
                    rgb(0xa8aebc)
                } else {
                    rgb(0xe8e8ea)
                })
                .child(match &ime {
                    Some(input) => input.clone().into_any_element(),
                    None if placeholder => div()
                        .child(if matches!(field.kind, SettingsFieldKind::Hotkey) {
                            t(Key::ClickToRecord)
                        } else {
                            t(Key::ClickToFill)
                        })
                        .into_any_element(),
                    None => div().child(before).into_any_element(),
                })
                .when(simple_focused, |row| {
                    row.child(div().w(px(1.)).h(px(16.)).bg(rgb(theme::ACCENT)))
                })
                .child(after)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        this.activate_field(field_index, window, cx);
                    }),
                ),
        )
        .when(anchor.is_some() && !options.is_empty(), |column| {
            let (origin, width) = anchor.expect("checked above");
            column.child(deferred(
                anchored()
                    .position(origin)
                    .offset(point(px(0.), px(52.)))
                    .child(
                        div()
                            .id(("options", field_index))
                            .w(width)
                            .max_h(px(220.))
                            .overflow_y_scroll()
                            .p_1()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgba(surface::BORDER))
                            .bg(rgba(surface::POPOVER))
                            .children(options),
                    ),
            ))
        });

    div()
        .on_children_prepainted({
            let field_bounds = view.field_bounds.clone();
            move |children, _, _| {
                if let Some(bounds) = children.first() {
                    field_bounds.borrow_mut().insert(field_index, *bounds);
                }
            }
        })
        .child(field_row)
        .into_any_element()
}

fn field_visible(fields: &[SettingsField], field: &SettingsField) -> bool {
    match &field.show_if {
        None => true,
        Some((key, value)) => fields
            .iter()
            .any(|other| &other.key == key && &other.value == value),
    }
}

fn display_field(field: &SettingsField) -> String {
    match &field.kind {
        SettingsFieldKind::Toggle => {
            if field.value == "true" {
                t(Key::On).into()
            } else {
                t(Key::Off).into()
            }
        }
        SettingsFieldKind::Choice(choices) => choices
            .iter()
            .find(|choice| choice.value == field.value)
            .map_or_else(|| field.value.clone(), |choice| choice.label.clone()),
        SettingsFieldKind::Secret => "•".repeat(field.value.chars().count()),
        _ => field.value.clone(),
    }
}

fn chord(keystroke: &Keystroke) -> Option<String> {
    let key = keystroke.key.as_str();
    if matches!(
        key,
        "alt" | "control" | "shift" | "cmd" | "platform" | "function" | ""
    ) {
        return None;
    }
    let modifiers = &keystroke.modifiers;
    let mut parts = Vec::new();
    if modifiers.control {
        parts.push("ctrl");
    }
    if modifiers.alt {
        parts.push("alt");
    }
    if modifiers.shift {
        parts.push("shift");
    }
    if modifiers.platform {
        parts.push("cmd");
    }
    if parts.is_empty() {
        return None;
    }
    parts.push(key);
    Some(parts.join("-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(key: &str, value: &str, show_if: Option<(&str, &str)>) -> SettingsField {
        SettingsField {
            key: key.to_string(),
            label: key.to_string(),
            kind: SettingsFieldKind::Text,
            value: value.to_string(),
            show_if: show_if.map(|(key, value)| (key.to_string(), value.to_string())),
        }
    }

    #[test]
    fn conditional_fields_follow_the_referenced_value() {
        let fields = vec![
            field("type", "free", None),
            field("api_key", "", Some(("type", "api"))),
        ];
        assert!(field_visible(&fields, &fields[0]));
        assert!(!field_visible(&fields, &fields[1]));

        let mut fields = fields;
        fields[0].value = "api".into();
        assert!(field_visible(&fields, &fields[1]));
    }
}
