use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::StandardItem;
use selo_core::{Error, Result};
use selo_i18n::{Key, t};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
pub struct TrayEvent {
    pub quit: bool,
    pub settings: bool,
}

/// The KDE/freedesktop StatusNotifierItem tray. `tray-icon` needs a GTK main loop, which
/// cannot coexist with GPUI's Wayland loop, so this is pure D-Bus via `ksni`.
pub struct Tray {
    quit: Arc<AtomicBool>,
    settings: Arc<AtomicBool>,
    handle: Handle<SeloTray>,
}

impl Tray {
    pub fn new(tooltip: &str) -> Result<Self> {
        let quit = Arc::new(AtomicBool::new(false));
        let settings = Arc::new(AtomicBool::new(false));
        let handle = SeloTray {
            tooltip: tooltip.to_string(),
            quit: quit.clone(),
            settings: settings.clone(),
        }
        .spawn()
        .map_err(|err| Error::Platform(err.to_string()))?;
        Ok(Self {
            quit,
            settings,
            handle,
        })
    }

    pub fn poll(&self) -> TrayEvent {
        TrayEvent {
            quit: self.quit.swap(false, Ordering::Relaxed),
            settings: self.settings.swap(false, Ordering::Relaxed),
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        self.handle.shutdown().wait();
    }
}

struct SeloTray {
    tooltip: String,
    quit: Arc<AtomicBool>,
    settings: Arc<AtomicBool>,
}

impl ksni::Tray for SeloTray {
    /// KDE/GNOME only pop the menu on right click by default; make both buttons open it.
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        "selo".into()
    }

    fn title(&self) -> String {
        "Selo".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![tray_icon()]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: self.tooltip.clone(),
            ..Default::default()
        }
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        vec![
            StandardItem {
                label: t(Key::TraySettings).into(),
                activate: Box::new(|tray: &mut Self| {
                    tray.settings.store(true, Ordering::Relaxed);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: t(Key::TrayQuit).into(),
                activate: Box::new(|tray: &mut Self| {
                    tray.quit.store(true, Ordering::Relaxed);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

fn tray_icon() -> ksni::Icon {
    let (rgba, width, height) = super::icon::tray_rgba();
    // SNI wants ARGB32 in network byte order.
    let data = rgba
        .chunks_exact(4)
        .flat_map(|pixel| [pixel[3], pixel[0], pixel[1], pixel[2]])
        .collect();
    ksni::Icon {
        width: width as i32,
        height: height as i32,
        data,
    }
}
