use gpui::{App, AsyncApp, Entity, QuitMode, WindowHandle, point, px};
use selo_core::{
    Error, OcrProvider, Region, SelectionProvider, TranslateProvider, TranslateRequest,
};
use selo_engine::{JsOcr, JsPlugin, Loaded, load_all};
use selo_i18n::{Key, Locale, t};
use selo_ocr::Ocr;
// Only non-Wayland platforms can read the cursor directly; Linux probes it via the dismiss surface.
#[cfg(not(target_os = "linux"))]
use selo_platform::cursor_position;
use selo_platform::{
    Code, Hotkeys, Modifiers, OutsideClick, PlatformSelection, Tray, capture, cursor_display,
    frontmost_name, prompt_pick_file, set_launch_at_login,
};
use selo_plugin::{ConfigType, PluginKind, Settings};
use selo_ui::{
    Dismiss, DismissEvent, Overlay, OverlayPhase, Popup, PopupPhase, RegionSelect, SettingsChoice,
    SettingsField, SettingsFieldKind, SettingsGroup, SettingsSave, SettingsService, SettingsView,
    prepare_overlay,
};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

const POLL: Duration = Duration::from_millis(40);
/// ponytail: fixed delay for the compositor to drop the overlay before the shot; a frozen-image capture would remove the race.
const OVERLAY_SETTLE: Duration = Duration::from_millis(120);

const TRANSLATE: usize = 0;
const CAPTURE: usize = 1;
/// KDE won't take `Alt+letter` globally; `Ctrl+Alt+letter` is the free family there.
#[cfg(target_os = "linux")]
const HOTKEYS: [(Modifiers, Code); 2] = [
    (Modifiers::CONTROL.union(Modifiers::ALT), Code::KeyD),
    (Modifiers::CONTROL.union(Modifiers::ALT), Code::KeyS),
];
#[cfg(not(target_os = "linux"))]
const HOTKEYS: [(Modifiers, Code); 2] =
    [(Modifiers::ALT, Code::KeyD), (Modifiers::ALT, Code::KeyS)];

#[cfg(target_os = "linux")]
const DEFAULT_HOTKEY_TRANSLATE: &str = "ctrl-alt-d";
#[cfg(not(target_os = "linux"))]
const DEFAULT_HOTKEY_TRANSLATE: &str = "alt-d";
#[cfg(target_os = "linux")]
const DEFAULT_HOTKEY_CAPTURE: &str = "ctrl-alt-s";
#[cfg(not(target_os = "linux"))]
const DEFAULT_HOTKEY_CAPTURE: &str = "alt-s";

type Engine = Arc<dyn TranslateProvider + Send + Sync>;

type OcrEngine = Arc<dyn OcrProvider + Send + Sync>;

const BUILTIN_PLUGINS: [&str; 7] = [
    include_str!("../../../plugins/deepseek.js"),
    include_str!("../../../plugins/google.js"),
    include_str!("../../../plugins/deepl.js"),
    include_str!("../../../plugins/bing.js"),
    include_str!("../../../plugins/youdao.js"),
    include_str!("../../../plugins/ocrspace.js"),
    include_str!("../../../plugins/local.js"),
];
const DEFAULT_ENGINE: &str = "deepl";

const DEFAULT_PLUGINS: [&str; 4] = ["deepl", "google", "bing", "local"];

struct Selo {
    engine: Engine,
    ocr: OcrEngine,
    target: Option<String>,
    popup: Option<WindowHandle<Popup>>,
    region: Sender<Option<Region>>,
    select: Option<WindowHandle<RegionSelect>>,
    result: Option<WindowHandle<Overlay>>,
    /// Full-display click catcher under the popup/overlay; Wayland has no global pointer monitor.
    dismiss: Option<WindowHandle<Dismiss>>,
    dismiss_tx: Sender<DismissEvent>,
    dismiss_events: Receiver<DismissEvent>,
    pointer: Option<(f32, f32)>,
    pending_popup: Option<PendingPopup>,
    settings: Option<WindowHandle<SettingsView>>,
    settings_tx: Sender<SettingsSave>,
    settings_add_external: Sender<()>,
    settings_saved: Receiver<SettingsSave>,
    add_external_requests: Receiver<()>,
    reopen_requests: Receiver<()>,
}

enum PendingPopup {
    Translate(TranslateRequest),
    Error(String),
}

fn main() {
    let (reopen_tx, reopen_requests) = channel();
    let application = gpui_platform::application();
    // macOS calls this only when no window is open, so it means "open settings".
    application.on_reopen(move |_| {
        let _ = reopen_tx.send(());
    });
    // GPUI's default quit mode is `LastWindowClosed` off macOS, which would kill the tray app.
    application
        .with_quit_mode(QuitMode::Explicit)
        .run(|cx: &mut App| {
            let support = support_dir();
            let plugins_dir = support.join("plugins");
            let _ = std::fs::create_dir_all(&plugins_dir);
            let mut settings = match Settings::load(support.join("config.toml")) {
                Ok(settings) => settings,
                Err(err) => {
                    eprintln!("settings: {err}");
                    cx.quit();
                    return;
                }
            };
            selo_i18n::set(
                settings
                    .language()
                    .map(Locale::from_code)
                    .or_else(|| selo_platform::system_language().map(|tag| Locale::from_tag(&tag)))
                    .unwrap_or_default(),
            );
            if let Err(err) = selo_platform::install_app_entry() {
                eprintln!("launcher entry: {err}");
            }
            let mut tray = match Tray::new("Selo") {
                Ok(tray) => tray,
                Err(err) => {
                    eprintln!("tray: {err}");
                    cx.quit();
                    return;
                }
            };
            // A global hotkey is exclusive per process; a second instance must exit rather than linger.
            let hotkeys = match Hotkeys::new(&HOTKEYS) {
                Ok(hotkeys) => hotkeys,
                Err(err) => {
                    eprintln!("hotkey: {err} (is another Selo instance already running?)");
                    cx.quit();
                    return;
                }
            };
            let mut plugins = load_all(&plugins_dir, &BUILTIN_PLUGINS);
            let engine = match select_engine(&settings, &plugins) {
                Ok(engine) => engine,
                Err(err) => {
                    eprintln!("engine: {err}");
                    cx.quit();
                    return;
                }
            };
            let ocr = select_ocr(&settings, &plugins);
            let target = target_code(&settings);

            let outside = OutsideClick::new();
            if outside.is_none() && !cfg!(target_os = "linux") {
                eprintln!("no global mouse monitor — the popup will not close on an outside click");
            }

            let (region_tx, regions) = channel();
            let (settings_tx, settings_saved) = channel();
            let (add_external_tx, add_external_requests) = channel();
            let (dismiss_tx, dismiss_events) = channel();
            let mut app = Selo {
                engine,
                ocr,
                target,
                popup: None,
                region: region_tx,
                select: None,
                result: None,
                dismiss: None,
                dismiss_tx,
                dismiss_events,
                pointer: None,
                pending_popup: None,
                settings: None,
                settings_tx,
                settings_add_external: add_external_tx,
                settings_saved,
                add_external_requests,
                reopen_requests,
            };
            println!(
                "selo ready — {} translates the selection, {} captures a region",
                settings
                    .hotkey_translate()
                    .unwrap_or(DEFAULT_HOTKEY_TRANSLATE),
                settings.hotkey_capture().unwrap_or(DEFAULT_HOTKEY_CAPTURE)
            );

            cx.spawn(async move |cx| {
                let mut hotkeys = hotkeys;
                loop {
                    cx.background_executor().timer(POLL).await;

                    let tray_event = tray.poll();
                    if tray_event.quit {
                        cx.update(|cx| cx.quit());
                        return;
                    }
                    if tray_event.settings {
                        app.open_settings(cx, &settings, &plugins);
                    }
                    // A global NSEvent monitor only sees presses to *other* apps, so no hit test is needed.
                    let clicked_away = outside.as_ref().is_some_and(OutsideClick::take);
                    if clicked_away {
                        app.dismiss_result(cx);
                        app.dismiss_popup_on_outside(cx);
                    }
                    let mut at = None;
                    while let Ok(event) = app.dismiss_events.try_recv() {
                        match event {
                            DismissEvent::Click => {
                                app.pending_popup = None;
                                app.dismiss_result(cx);
                                app.dismiss_popup_on_outside(cx);
                            }
                            DismissEvent::Pointer(position) => {
                                at = Some((position.x.as_f32(), position.y.as_f32()));
                            }
                        }
                    }
                    if let Some(at) = at {
                        app.pointer = Some(at);
                    }
                    if let Some(pending) = app.pending_popup.take() {
                        match app.pointer {
                            Some(at) => app.open_popup(cx, at, pending),
                            None => app.pending_popup = Some(pending),
                        }
                    }
                    if let Ok(region) = regions.try_recv() {
                        app.dismiss_select(cx);
                        if let Some(region) = region
                            && let Err(err) = app.translate_region(cx, region).await
                        {
                            eprintln!("{err}");
                            app.show_error(cx, err);
                        }
                    }
                    if let Ok(save) = app.settings_saved.try_recv() {
                        app.apply_settings(
                            cx,
                            &mut settings,
                            &mut plugins,
                            &plugins_dir,
                            save,
                            &mut tray,
                            &mut hotkeys,
                        );
                    }
                    if drained(&app.add_external_requests) {
                        app.import_plugin(cx, &settings, &mut plugins, &plugins_dir);
                    }
                    if drained(&app.reopen_requests) {
                        app.open_settings(cx, &settings, &plugins);
                    }
                    for press in hotkeys.drain_presses() {
                        match press {
                            TRANSLATE => {
                                if let Err(err) = app.translate_selection(cx) {
                                    eprintln!("{err} (frontmost: {})", frontmost_name());
                                }
                            }
                            CAPTURE => app.select_region(cx),
                            _ => {}
                        }
                    }
                    app.tick_result(cx);
                    app.sync_dismiss(cx);
                }
            })
            .detach();
        });
}

fn drained(requests: &Receiver<()>) -> bool {
    let mut any = false;
    while requests.try_recv().is_ok() {
        any = true;
    }
    any
}

#[cfg(target_os = "macos")]
fn support_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
    home.join("Library/Application Support/Selo")
}

#[cfg(target_os = "windows")]
fn support_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
        .join("Selo")
}

#[cfg(target_os = "linux")]
fn support_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Selo")
}

fn select_engine(settings: &Settings, plugins: &[Loaded]) -> selo_core::Result<Engine> {
    let usable = |id: &str| {
        plugins
            .iter()
            .any(|plugin| plugin.meta.id == id && plugin.meta.kind == PluginKind::Translate)
            && !settings.removed().iter().any(|removed| removed == id)
    };
    let wanted = settings.engine().unwrap_or(DEFAULT_ENGINE);
    if usable(wanted) {
        match build_engine(wanted, settings, plugins) {
            Ok(engine) => return Ok(engine),
            Err(err) => eprintln!("plugin `{wanted}`: {err}"),
        }
    }
    if wanted != DEFAULT_ENGINE
        && usable(DEFAULT_ENGINE)
        && let Ok(engine) = build_engine(DEFAULT_ENGINE, settings, plugins)
    {
        return Ok(engine);
    }
    plugins
        .iter()
        .find(|plugin| usable(&plugin.meta.id))
        .map_or_else(
            || Err(Error::Translate("no usable translation plugin".into())),
            |plugin| build_engine(&plugin.meta.id, settings, plugins),
        )
}

fn build_engine(id: &str, settings: &Settings, plugins: &[Loaded]) -> selo_core::Result<Engine> {
    let loaded = plugins
        .iter()
        .find(|loaded| loaded.meta.id == id)
        .ok_or_else(|| Error::Translate(format!("no plugin `{id}`")))?;
    let plugin = JsPlugin::new(
        loaded.source.clone(),
        settings.config_for_meta(&loaded.meta),
    )?;
    println!("engine: plugin `{}`", loaded.meta.name);
    Ok(Arc::new(plugin))
}

fn select_ocr(settings: &Settings, plugins: &[Loaded]) -> OcrEngine {
    let wanted = settings.ocr_engine();
    if let Some(id) = wanted
        && !settings.removed().iter().any(|removed| removed == id)
        && let Some(loaded) = plugins
            .iter()
            .find(|plugin| plugin.meta.id == id && plugin.meta.kind == PluginKind::Ocr)
        && let Ok(provider) = build_ocr(loaded, settings)
    {
        return provider;
    }
    Arc::new(Ocr)
}

fn build_ocr(loaded: &Loaded, settings: &Settings) -> selo_core::Result<OcrEngine> {
    let provider = JsOcr::new(
        loaded.source.clone(),
        settings.config_for_meta(&loaded.meta),
    )?;
    println!("ocr: plugin `{}`", loaded.meta.name);
    Ok(Arc::new(provider))
}

fn is_hidden(settings: &Settings, loaded: &Loaded) -> bool {
    if settings.removed().iter().any(|id| id == &loaded.meta.id) {
        return true;
    }
    loaded.builtin
        && !DEFAULT_PLUGINS.contains(&loaded.meta.id.as_str())
        && !settings.added().iter().any(|id| id == &loaded.meta.id)
        && settings.engine() != Some(loaded.meta.id.as_str())
        && settings.ocr_engine() != Some(loaded.meta.id.as_str())
}

fn settings_view_data(
    settings: &Settings,
    plugins: &[Loaded],
) -> (
    Vec<SettingsGroup>,
    Vec<SettingsService>,
    Vec<SettingsService>,
) {
    let mut services = Vec::new();
    let mut available = Vec::new();
    let mut engine_options = Vec::new();
    let mut ocr_options = vec![("system".to_string(), t(Key::SystemDefault).to_string())];
    for loaded in plugins {
        let service = SettingsService {
            id: loaded.meta.id.clone(),
            name: loaded.meta.name.clone(),
            builtin: loaded.builtin,
            ocr: loaded.meta.kind == PluginKind::Ocr,
            fields: plugin_fields(settings, loaded),
        };
        if is_hidden(settings, loaded) {
            available.push(service);
            continue;
        }
        match loaded.meta.kind {
            PluginKind::Translate => {
                engine_options.push((loaded.meta.id.clone(), loaded.meta.name.clone()))
            }
            PluginKind::Ocr => ocr_options.push((loaded.meta.id.clone(), loaded.meta.name.clone())),
        }
        services.push(service);
    }

    let groups = vec![
        SettingsGroup {
            label: t(Key::General).to_string(),
            fields: vec![
                choice_field(
                    "ui_language",
                    t(Key::UiLanguage),
                    selo_i18n::current().code(),
                    Locale::ALL
                        .iter()
                        .map(|locale| (locale.code().to_string(), locale.label().to_string()))
                        .collect(),
                ),
                SettingsField {
                    key: "launch_at_login".to_string(),
                    label: t(Key::LaunchAtLogin).to_string(),
                    kind: SettingsFieldKind::Toggle,
                    value: settings.launch_at_login().to_string(),
                    show_if: None,
                },
            ],
        },
        SettingsGroup {
            label: t(Key::Translation).to_string(),
            fields: vec![
                choice_field(
                    "engine",
                    t(Key::TranslationEngine),
                    settings.engine().unwrap_or(DEFAULT_ENGINE),
                    engine_options,
                ),
                choice_field(
                    "target_language",
                    t(Key::TargetLanguage),
                    target_code(settings).as_deref().unwrap_or("auto"),
                    vec![
                        ("auto".to_string(), t(Key::Auto).to_string()),
                        ("zh".to_string(), "中文（简体）".to_string()),
                        ("zh-Hant".to_string(), "中文（繁體）".to_string()),
                        ("en".to_string(), "English".to_string()),
                        ("ja".to_string(), "日本語".to_string()),
                        ("ko".to_string(), "한국어".to_string()),
                        ("fr".to_string(), "Français".to_string()),
                        ("de".to_string(), "Deutsch".to_string()),
                        ("es".to_string(), "Español".to_string()),
                        ("ru".to_string(), "Русский".to_string()),
                        ("pt".to_string(), "Português".to_string()),
                        ("it".to_string(), "Italiano".to_string()),
                        ("ar".to_string(), "العربية".to_string()),
                        ("th".to_string(), "ไทย".to_string()),
                        ("vi".to_string(), "Tiếng Việt".to_string()),
                    ],
                ),
                SettingsField {
                    key: "hotkey_translate".to_string(),
                    label: t(Key::TranslateHotkey).to_string(),
                    kind: SettingsFieldKind::Hotkey,
                    value: settings
                        .hotkey_translate()
                        .unwrap_or(DEFAULT_HOTKEY_TRANSLATE)
                        .to_string(),
                    show_if: None,
                },
            ],
        },
        SettingsGroup {
            label: t(Key::Capture).to_string(),
            fields: vec![
                choice_field(
                    "ocr_engine",
                    t(Key::OcrEngine),
                    settings.ocr_engine().unwrap_or("system"),
                    ocr_options,
                ),
                SettingsField {
                    key: "hotkey_capture".to_string(),
                    label: t(Key::CaptureHotkey).to_string(),
                    kind: SettingsFieldKind::Hotkey,
                    value: settings
                        .hotkey_capture()
                        .unwrap_or(DEFAULT_HOTKEY_CAPTURE)
                        .to_string(),
                    show_if: None,
                },
            ],
        },
    ];
    (groups, services, available)
}

fn target_code(settings: &Settings) -> Option<String> {
    match settings.target_language()? {
        "auto" | "" => None,
        code => Some(code.to_string()),
    }
}

fn plugin_fields(settings: &Settings, loaded: &Loaded) -> Vec<SettingsField> {
    let meta = &loaded.meta;
    meta.config
        .iter()
        .map(|field| {
            let secret = field.kind == ConfigType::Secret;
            let value = match field.kind {
                ConfigType::Secret => settings.secret(&meta.id, &field.key).unwrap_or_default(),
                ConfigType::String => settings
                    .value(&meta.id, &field.key)
                    .map(str::to_string)
                    .or_else(|| field.default.clone())
                    .unwrap_or_default(),
            };
            let kind = if secret {
                SettingsFieldKind::Secret
            } else if field.options.is_empty() {
                SettingsFieldKind::Text
            } else {
                SettingsFieldKind::Choice(
                    field
                        .options
                        .iter()
                        .map(|option| SettingsChoice {
                            label: option.clone(),
                            value: option.clone(),
                        })
                        .collect(),
                )
            };
            SettingsField {
                key: field.key.clone(),
                label: field.label.clone(),
                kind,
                value,
                show_if: field
                    .show_if
                    .as_ref()
                    .map(|show| (show.key.clone(), show.value.clone())),
            }
        })
        .collect()
}

fn parse_hotkey(chord: &str) -> Option<(Modifiers, Code)> {
    let mut modifiers = Modifiers::empty();
    let mut parts = chord.split('-');
    let key = loop {
        let part = parts.next()?;
        match part {
            "ctrl" => modifiers |= Modifiers::CONTROL,
            "alt" => modifiers |= Modifiers::ALT,
            "shift" => modifiers |= Modifiers::SHIFT,
            "cmd" => modifiers |= Modifiers::META,
            _ => break part,
        }
    };
    if modifiers.is_empty() {
        return None;
    }
    let code = match key {
        "space" => Code::Space,
        "enter" => Code::Enter,
        "tab" => Code::Tab,
        "escape" => Code::Escape,
        _ => {
            let letter = key.chars().next()?.to_ascii_uppercase();
            Code::from_str(&format!("Key{letter}")).ok()?
        }
    };
    Some((modifiers, code))
}

fn choice_field(
    key: &str,
    label: &str,
    value: &str,
    options: Vec<(String, String)>,
) -> SettingsField {
    SettingsField {
        key: key.to_string(),
        label: label.to_string(),
        kind: SettingsFieldKind::Choice(
            options
                .into_iter()
                .map(|(value, label)| SettingsChoice { value, label })
                .collect(),
        ),
        value: value.to_string(),
        show_if: None,
    }
}

impl Selo {
    fn open_settings(&mut self, cx: &mut AsyncApp, settings: &Settings, plugins: &[Loaded]) {
        self.dismiss_settings(cx);
        let (groups, services, available) = settings_view_data(settings, plugins);
        let result = cx.update(|cx| {
            SettingsView::open(
                cx,
                groups,
                services,
                available,
                self.settings_add_external.clone(),
                self.settings_tx.clone(),
            )
        });
        match result {
            Ok(window) => {
                cx.update(|cx| cx.activate(true));
                self.settings = Some(window);
            }
            Err(err) => eprintln!("settings window: {err}"),
        }
    }

    fn import_plugin(
        &mut self,
        cx: &mut AsyncApp,
        settings: &Settings,
        plugins: &mut Vec<Loaded>,
        plugins_dir: &std::path::Path,
    ) {
        let picked = match prompt_pick_file(t(Key::PickPluginFile)) {
            Ok(Some(path)) => path,
            Ok(None) => return,
            Err(err) => return eprintln!("pick file: {err}"),
        };
        if picked.extension().is_none_or(|ext| ext != "js") {
            return eprintln!("not a .js plugin: {}", picked.display());
        }
        let Some(file_name) = picked.file_name() else {
            return;
        };
        if let Err(err) = std::fs::create_dir_all(plugins_dir) {
            return eprintln!("plugins dir: {err}");
        }
        if let Err(err) = std::fs::copy(&picked, plugins_dir.join(file_name)) {
            return eprintln!("copy plugin: {err}");
        }
        *plugins = load_all(plugins_dir, &BUILTIN_PLUGINS);
        self.open_settings(cx, settings, plugins);
    }

    fn dismiss_settings(&mut self, cx: &mut AsyncApp) {
        if let Some(window) = self.settings.take() {
            cx.update(|cx| {
                let _closed = window.update(cx, |_, window, _| window.remove_window());
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_settings(
        &mut self,
        cx: &mut AsyncApp,
        settings: &mut Settings,
        plugins: &mut Vec<Loaded>,
        plugins_dir: &std::path::Path,
        save: SettingsSave,
        tray: &mut Tray,
        hotkeys: &mut Hotkeys,
    ) {
        for id in &save.removed {
            if let Some(path) = plugins
                .iter()
                .find(|plugin| &plugin.meta.id == id)
                .and_then(|plugin| plugin.path.clone())
            {
                let _ = std::fs::remove_file(path);
            }
        }
        settings.set_removed(save.removed.clone());
        let added: Vec<String> = plugins
            .iter()
            .filter(|plugin| {
                plugin.builtin
                    && !DEFAULT_PLUGINS.contains(&plugin.meta.id.as_str())
                    && !save.removed.contains(&plugin.meta.id)
            })
            .map(|plugin| plugin.meta.id.clone())
            .collect();
        settings.set_added(added);

        let mut locale = selo_i18n::current();
        for (key, value) in save.general {
            match key.as_str() {
                "ui_language" => {
                    locale = Locale::from_code(&value);
                    settings.set_language(Some(value));
                }
                "engine" => settings.set_engine(Some(value)),
                "ocr_engine" => settings.set_ocr_engine((value != "system").then_some(value)),
                "target_language" => settings.set_target_language(Some(value)),
                "launch_at_login" => settings.set_launch_at_login(value == "true"),
                "hotkey_translate" => settings.set_hotkey_translate(Some(value)),
                "hotkey_capture" => settings.set_hotkey_capture(Some(value)),
                _ => {}
            }
        }
        let relocalise = locale != selo_i18n::current();
        for (engine, key, secret, value) in save.values {
            if value.is_empty() {
                continue;
            }
            if secret {
                if let Err(err) = settings.set_secret(&engine, &key, &value) {
                    eprintln!("keyring: {err}");
                }
            } else {
                settings.set_value(&engine, &key, value);
            }
        }

        for (index, chord) in [
            (TRANSLATE, settings.hotkey_translate()),
            (CAPTURE, settings.hotkey_capture()),
        ] {
            if let Some(binding) = chord.and_then(parse_hotkey)
                && let Err(err) = hotkeys.rebind(index, binding)
            {
                eprintln!("hotkey: {err}");
            }
        }
        if let Err(err) = set_launch_at_login(settings.launch_at_login()) {
            eprintln!("login item: {err}");
        }
        self.target = target_code(settings);

        if let Err(err) = settings.save() {
            eprintln!("settings: {err}");
        }
        *plugins = load_all(plugins_dir, &BUILTIN_PLUGINS);
        match select_engine(settings, plugins) {
            Ok(engine) => self.engine = engine,
            Err(err) => eprintln!("engine: {err}"),
        }
        self.ocr = select_ocr(settings, plugins);

        if relocalise {
            selo_i18n::set(locale);
            match Tray::new("Selo") {
                Ok(new) => *tray = new,
                Err(err) => eprintln!("tray: {err}"),
            }
            self.open_settings(cx, settings, plugins);
        }
    }

    fn dismiss_popup(&mut self, cx: &mut AsyncApp) {
        if let Some(window) = self.popup.take() {
            cx.update(|cx| {
                let _closed = window.update(cx, |_, window, _| window.remove_window());
            });
        }
        self.sync_dismiss(cx);
    }

    fn dismiss_popup_on_outside(&mut self, cx: &mut AsyncApp) {
        if !self.popup_pinned(cx) {
            self.dismiss_popup(cx);
        }
    }

    fn popup_pinned(&self, cx: &mut AsyncApp) -> bool {
        self.popup.as_ref().is_some_and(|window| {
            cx.update(|cx| {
                window
                    .update(cx, |popup, _, _| popup.is_pinned())
                    .unwrap_or(false)
            })
        })
    }

    fn dismiss_select(&mut self, cx: &mut AsyncApp) {
        if let Some(window) = self.select.take() {
            cx.update(|cx| {
                let _closed = window.update(cx, |_, window, _| window.remove_window());
            });
        }
    }

    fn dismiss_result(&mut self, cx: &mut AsyncApp) {
        if let Some(window) = self.result.take() {
            cx.update(|cx| {
                let _closed = window.update(cx, |_, window, _| window.remove_window());
            });
        }
        self.sync_dismiss(cx);
    }

    fn sync_dismiss(&mut self, cx: &mut AsyncApp) {
        if !cfg!(target_os = "linux") {
            return;
        }
        let wanted = self.result.is_some()
            || self.pending_popup.is_some()
            || (self.popup.is_some() && !self.popup_pinned(cx));
        match (wanted, self.dismiss) {
            (true, None) => {
                let events = self.dismiss_tx.clone();
                match cx.update(|cx| Dismiss::open(cx, events)) {
                    Ok(window) => self.dismiss = Some(window),
                    Err(err) => eprintln!("dismiss overlay: {err}"),
                }
            }
            (false, Some(window)) => {
                self.dismiss = None;
                cx.update(|cx| {
                    let _closed = window.update(cx, |_, window, _| window.remove_window());
                });
            }
            _ => {}
        }
    }

    #[cfg(target_os = "linux")]
    fn reset_dismiss(&mut self, cx: &mut AsyncApp) {
        self.pointer = None;
        if let Some(window) = self.dismiss.take() {
            cx.update(|cx| {
                let _closed = window.update(cx, |_, window, _| window.remove_window());
            });
        }
        self.sync_dismiss(cx);
    }

    fn prime_pointer(&mut self, cx: &mut AsyncApp) {
        #[cfg(target_os = "linux")]
        {
            self.reset_dismiss(cx);
        }
        #[cfg(not(target_os = "linux"))]
        {
            let (x, y) = cursor_position();
            self.pointer = Some((x as f32, y as f32));
            let _ = cx;
        }
    }

    fn open_popup(&mut self, cx: &mut AsyncApp, at: (f32, f32), pending: PendingPopup) {
        let source = match &pending {
            PendingPopup::Translate(request) => Some(request.text.clone()),
            PendingPopup::Error(_) => None,
        };
        match cx.update(|cx| Popup::open(cx, point(px(at.0), px(at.1)), source)) {
            Ok((window, view)) => {
                match pending {
                    PendingPopup::Translate(request) => {
                        spawn_translation(cx, self.engine.clone(), request, view)
                    }
                    PendingPopup::Error(err) => cx.update(|cx| {
                        view.update(cx, |popup, cx| popup.set_phase(PopupPhase::Failed(err), cx))
                    }),
                }
                self.popup = Some(window);
            }
            Err(err) => eprintln!("popup: {err}"),
        }
        self.sync_dismiss(cx);
    }

    // Edge resizing is polled: during a drag the pointer's events go elsewhere, so read the cursor.
    fn tick_result(&mut self, cx: &mut AsyncApp) {
        if let Some(window) = self.result {
            cx.update(|cx| {
                let _ = window.update(cx, |overlay, window, cx| overlay.tick(window, cx));
            });
        }
    }

    fn select_region(&mut self, cx: &mut AsyncApp) {
        if self.select.is_some() {
            return;
        }
        self.dismiss_popup(cx);
        self.dismiss_result(cx);
        self.pending_popup = None;
        let display = cursor_display().unwrap_or(0);
        match cx.update(|cx| RegionSelect::open(cx, display, self.region.clone())) {
            Ok(window) => self.select = Some(window),
            Err(err) => eprintln!("region overlay: {err}"),
        }
    }

    async fn translate_region(
        &mut self,
        cx: &mut AsyncApp,
        region: Region,
    ) -> selo_core::Result<()> {
        cx.background_executor().timer(OVERLAY_SETTLE).await;
        let ocr = self.ocr.clone();
        let (image, device_scale, rendered, backdrop, dark, blocks) = cx
            .background_executor()
            .spawn(async move {
                catch_unwind(AssertUnwindSafe(|| {
                    let image = capture(region)?;
                    let blocks = ocr.recognize(&image)?;
                    let device_scale = image.width as f32 / region.width.max(1) as f32;
                    let (rendered, backdrop, dark) = prepare_overlay(&image, device_scale)
                        .map_err(|err| Error::Platform(err.to_string()))?;
                    Ok::<_, Error>((image, device_scale, rendered, backdrop, dark, blocks))
                }))
                .map_err(|_| Error::Platform("region OCR worker panicked".into()))?
            })
            .await?;

        let paragraphs = selo_layout::cluster(&blocks);
        for b in &blocks {
            println!(
                "  line {:.0}x{:.0} {:?}",
                b.rect.width, b.rect.height, b.text
            );
        }
        for p in &paragraphs {
            println!(
                "  para {:.0}x{:.0} {:?}",
                p.rect.width, p.rect.height, p.text
            );
        }
        if paragraphs.is_empty() {
            return Err(Error::Ocr("no text recognised in that region".into()));
        }
        println!(
            "ocr: region {}x{} at ({},{}), image {}x{}, {} lines → {} paragraphs",
            region.width,
            region.height,
            region.x,
            region.y,
            image.width,
            image.height,
            blocks.len(),
            paragraphs.len()
        );

        self.dismiss_popup(cx);
        self.dismiss_result(cx);
        let (window, view) = cx
            .update(|cx| {
                Overlay::open(
                    cx,
                    region,
                    device_scale,
                    rendered,
                    backdrop,
                    dark,
                    &paragraphs,
                )
            })
            .map_err(|err| Error::Platform(err.to_string()))?;

        self.result = Some(window);
        spawn_paragraphs(
            cx,
            self.engine.clone(),
            view,
            paragraphs,
            self.target.clone(),
        );
        self.sync_dismiss(cx);
        Ok(())
    }

    fn show_error(&mut self, cx: &mut AsyncApp, err: Error) {
        self.dismiss_popup(cx);
        self.dismiss_result(cx);
        self.pending_popup = Some(PendingPopup::Error(err.to_string()));
        self.prime_pointer(cx);
    }

    fn translate_selection(&mut self, cx: &mut AsyncApp) -> selo_core::Result<()> {
        // The a11y read is immediate and the ⌘C fallback waits on the target app, so nothing of ours blocks long.
        let selection = PlatformSelection.selection()?;
        println!(
            "selection via {:?}: {} chars",
            selection.via,
            selection.text.chars().count()
        );

        self.dismiss_popup(cx);
        self.dismiss_result(cx);
        let request = TranslateRequest::with_target(selection.text, self.target.clone());
        self.pending_popup = Some(PendingPopup::Translate(request));
        self.prime_pointer(cx);
        Ok(())
    }
}

fn spawn_translation(
    cx: &mut AsyncApp,
    engine: Engine,
    request: TranslateRequest,
    view: Entity<Popup>,
) {
    cx.spawn(async move |cx| {
        let outcome = cx
            .background_executor()
            .spawn(async move { engine.translate(&request) })
            .await;
        let phase = match outcome {
            Ok(text) => PopupPhase::Done(text),
            Err(err) => PopupPhase::Failed(err.to_string()),
        };
        view.update(cx, |popup, cx| popup.set_phase(phase, cx));
    })
    .detach();
}

fn spawn_paragraphs(
    cx: &mut AsyncApp,
    engine: Engine,
    view: Entity<Overlay>,
    paragraphs: Vec<selo_layout::Paragraph>,
    target: Option<String>,
) {
    cx.spawn(async move |cx| {
        for (index, paragraph) in paragraphs.into_iter().enumerate() {
            let request = TranslateRequest::with_target(paragraph.text, target.clone());
            let engine = engine.clone();
            let outcome = cx
                .background_executor()
                .spawn(async move { engine.translate(&request) })
                .await;
            let phase = match outcome {
                Ok(text) => OverlayPhase::Done(text),
                Err(err) => OverlayPhase::Failed(err.to_string()),
            };
            view.update(cx, |overlay, cx| overlay.set_phase(index, phase, cx));
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_default_plugins_are_listed_by_default() {
        let plugins = load_all("/nonexistent/selo/plugins", &BUILTIN_PLUGINS);
        let settings = Settings::load("/nonexistent/selo/config.toml").unwrap();
        let mut visible: Vec<&str> = plugins
            .iter()
            .filter(|plugin| !is_hidden(&settings, plugin))
            .map(|plugin| plugin.meta.id.as_str())
            .collect();
        visible.sort();
        assert_eq!(visible, ["bing", "deepl", "google", "local"]);
    }

    #[test]
    fn a_restored_builtin_stays_visible() {
        let plugins = load_all("/nonexistent/selo/plugins", &BUILTIN_PLUGINS);
        let youdao = plugins.iter().find(|p| p.meta.id == "youdao").unwrap();
        let mut settings = Settings::load("/nonexistent/selo/config.toml").unwrap();
        assert!(is_hidden(&settings, youdao));
        settings.set_added(vec!["youdao".into()]);
        assert!(!is_hidden(&settings, youdao));
    }
}
