use selo_core::{Error, Result};
use selo_i18n::{Key, t};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// The status item disappears when this is dropped, so the app has to keep it alive.
pub struct Tray {
    _icon: TrayIcon,
    quit: MenuId,
    settings: MenuId,
}

#[derive(Default)]
pub struct TrayEvent {
    pub quit: bool,
    pub settings: bool,
}

impl Tray {
    /// Must be built on the main thread after the app finished launching.
    pub fn new(tooltip: &str) -> Result<Self> {
        let menu = Menu::new();
        let settings = MenuItem::new(t(Key::TraySettings), true, None);
        let quit = MenuItem::new(t(Key::TrayQuit), true, None);
        for item in [&settings, &quit] {
            menu.append(item)
                .map_err(|err| Error::Platform(err.to_string()))?;
        }

        let icon = TrayIconBuilder::new()
            .with_tooltip(tooltip)
            .with_icon(tray_icon())
            .with_menu(Box::new(menu))
            .build()
            .map_err(|err| Error::Platform(err.to_string()))?;

        Ok(Self {
            _icon: icon,
            quit: quit.id().clone(),
            settings: settings.id().clone(),
        })
    }

    pub fn poll(&self) -> TrayEvent {
        let mut event = TrayEvent::default();
        while let Ok(clicked) = MenuEvent::receiver().try_recv() {
            event.quit |= clicked.id == self.quit;
            event.settings |= clicked.id == self.settings;
        }
        event
    }
}

fn tray_icon() -> Icon {
    let (rgba, width, height) = super::icon::tray_rgba();
    Icon::from_rgba(rgba, width, height).expect("icon buffer matches its declared size")
}
