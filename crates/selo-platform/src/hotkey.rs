#[cfg(all(not(target_os = "windows"), not(target_os = "linux")))]
mod platform {
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
    use selo_core::{Error, Result};

    pub struct Hotkeys {
        manager: GlobalHotKeyManager,
        bindings: Vec<(Modifiers, Code)>,
        ids: Vec<u32>,
    }

    impl Hotkeys {
        pub fn new(bindings: &[(Modifiers, Code)]) -> Result<Self> {
            let manager =
                GlobalHotKeyManager::new().map_err(|err| Error::Platform(err.to_string()))?;
            let mut ids = Vec::with_capacity(bindings.len());
            for &(modifiers, code) in bindings {
                let hotkey = HotKey::new(Some(modifiers), code);
                manager
                    .register(hotkey)
                    .map_err(|err| Error::Platform(err.to_string()))?;
                ids.push(hotkey.id());
            }
            Ok(Self {
                manager,
                bindings: bindings.to_vec(),
                ids,
            })
        }

        pub fn rebind(&mut self, index: usize, binding: (Modifiers, Code)) -> Result<()> {
            let Some(slot) = self.bindings.get_mut(index) else {
                return Err(Error::Platform(format!("no hotkey at index {index}")));
            };
            let _ = self.manager.unregister(HotKey::new(Some(slot.0), slot.1));
            let hotkey = HotKey::new(Some(binding.0), binding.1);
            self.manager
                .register(hotkey)
                .map_err(|err| Error::Platform(err.to_string()))?;
            self.ids[index] = hotkey.id();
            *slot = binding;
            Ok(())
        }

        pub fn drain_presses(&self) -> Vec<usize> {
            let mut pressed = Vec::new();
            while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                if event.state == HotKeyState::Pressed
                    && let Some(index) = self.ids.iter().position(|&id| id == event.id)
                {
                    pressed.push(index);
                }
            }
            pressed
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use ashpd::desktop::Session;
    use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
    use futures::StreamExt;
    use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
    use global_hotkey::hotkey::{Code, Modifiers};
    use selo_core::{Error, Result};
    use selo_i18n::{Key, t};
    use std::sync::mpsc::{Receiver, Sender, channel};
    use std::thread::{self, JoinHandle};

    const IDS: [&str; 2] = ["translate", "capture"];

    enum Command {
        Rebind {
            index: usize,
            binding: (Modifiers, Code),
            done: Sender<Result<()>>,
        },
        Stop,
    }

    /// Global hotkeys through the XDG GlobalShortcuts portal. Wayland gives no X11
    /// `XGrabKey`, so `global-hotkey` is unusable; the portal needs one user approval on first bind.
    pub struct Hotkeys {
        bindings: Vec<(Modifiers, Code)>,
        commands: UnboundedSender<Command>,
        presses: Receiver<usize>,
        thread: Option<JoinHandle<()>>,
    }

    impl Hotkeys {
        pub fn new(bindings: &[(Modifiers, Code)]) -> Result<Self> {
            let (commands, command_rx) = unbounded();
            let (presses, press_rx) = channel();
            let (ready_tx, ready_rx) = channel();
            let initial = bindings.to_vec();
            let thread = thread::Builder::new()
                .name("selo-hotkeys".into())
                .spawn(move || run(initial, command_rx, presses, ready_tx))
                .map_err(|err| Error::Platform(format!("start hotkey thread: {err}")))?;
            match ready_rx.recv() {
                Ok(Ok(())) => {}
                Ok(Err(err)) => {
                    let _ = thread.join();
                    return Err(err);
                }
                Err(_) => return Err(Error::Platform("hotkey thread exited".into())),
            }
            Ok(Self {
                bindings: bindings.to_vec(),
                commands,
                presses: press_rx,
                thread: Some(thread),
            })
        }

        pub fn rebind(&mut self, index: usize, binding: (Modifiers, Code)) -> Result<()> {
            if index >= self.bindings.len() {
                return Err(Error::Platform(format!("no hotkey at index {index}")));
            }
            let (done, result) = channel();
            self.commands
                .unbounded_send(Command::Rebind {
                    index,
                    binding,
                    done,
                })
                .map_err(|_| Error::Platform("hotkey thread exited".into()))?;
            result
                .recv()
                .map_err(|_| Error::Platform("hotkey thread exited".into()))??;
            self.bindings[index] = binding;
            Ok(())
        }

        pub fn drain_presses(&self) -> Vec<usize> {
            self.presses.try_iter().collect()
        }
    }

    impl Drop for Hotkeys {
        fn drop(&mut self) {
            let _ = self.commands.unbounded_send(Command::Stop);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn run(
        initial: Vec<(Modifiers, Code)>,
        commands: UnboundedReceiver<Command>,
        presses: Sender<usize>,
        ready: Sender<Result<()>>,
    ) {
        async_io::block_on(async move {
            let connection = match crate::linux::portal_connection().await {
                Ok(connection) => connection,
                Err(err) => {
                    let _ = ready.send(Err(Error::Platform(err)));
                    return;
                }
            };
            let proxy = match GlobalShortcuts::with_connection(connection).await {
                Ok(proxy) => proxy,
                Err(err) => {
                    let _ = ready.send(Err(Error::Platform(err.to_string())));
                    return;
                }
            };
            let session = match proxy.create_session(Default::default()).await {
                Ok(session) => session,
                Err(err) => {
                    let _ = ready.send(Err(Error::Platform(err.to_string())));
                    return;
                }
            };
            let mut current = initial;
            if let Err(err) = bind(&proxy, &session, &current).await {
                let _ = ready.send(Err(err));
                return;
            }
            let _ = ready.send(Ok(()));

            let Ok(activated) = proxy.receive_activated().await else {
                eprintln!("hotkey: failed to subscribe to portal Activated signals");
                return;
            };

            let rebind = async move {
                let mut commands = commands;
                while let Some(command) = commands.next().await {
                    match command {
                        Command::Stop => return,
                        Command::Rebind {
                            index,
                            binding,
                            done,
                        } => {
                            if let Some(slot) = current.get_mut(index) {
                                *slot = binding;
                            }
                            let result = bind(&proxy, &session, &current).await;
                            let _ = done.send(result);
                        }
                    }
                }
            };
            let listen = async move {
                let mut activated = activated;
                while let Some(event) = activated.next().await {
                    if let Some(index) = IDS.iter().position(|id| *id == event.shortcut_id()) {
                        let _ = presses.send(index);
                    }
                }
            };
            futures::pin_mut!(rebind, listen);
            let _ = futures::future::select(rebind, listen).await;
        });
    }

    async fn bind(
        proxy: &GlobalShortcuts,
        session: &Session<GlobalShortcuts>,
        bindings: &[(Modifiers, Code)],
    ) -> Result<()> {
        let shortcuts: Vec<NewShortcut> = bindings
            .iter()
            .enumerate()
            .map(|(index, (modifiers, code))| {
                let trigger = trigger(*modifiers, *code);
                let id = IDS.get(index).copied().unwrap_or("translate");
                NewShortcut::new(id, description(index)).preferred_trigger(trigger.as_str())
            })
            .collect();
        let request = proxy
            .bind_shortcuts(session, &shortcuts, None, Default::default())
            .await
            .map_err(|err| Error::Platform(err.to_string()))?;
        request
            .response()
            .map_err(|err| Error::Platform(err.to_string()))?;
        Ok(())
    }

    fn description(index: usize) -> &'static str {
        match index {
            0 => t(Key::HotkeyTranslateDesc),
            _ => t(Key::HotkeyCaptureDesc),
        }
    }

    fn trigger(modifiers: Modifiers, code: Code) -> String {
        let mut parts: Vec<String> = Vec::new();
        if modifiers.contains(Modifiers::CONTROL) {
            parts.push("CTRL".into());
        }
        if modifiers.contains(Modifiers::ALT) {
            parts.push("ALT".into());
        }
        if modifiers.contains(Modifiers::SHIFT) {
            parts.push("SHIFT".into());
        }
        if modifiers.contains(Modifiers::META) {
            parts.push("LOGO".into());
        }
        parts.push(key_name(code));
        parts.join("+")
    }

    fn key_name(code: Code) -> String {
        let debug = format!("{code:?}");
        if let Some(letter) = debug.strip_prefix("Key") {
            return letter.to_ascii_lowercase();
        }
        if let Some(digit) = debug.strip_prefix("Digit") {
            return digit.to_string();
        }
        match debug.as_str() {
            "Space" => "space".into(),
            "Enter" => "Return".into(),
            "Tab" => "Tab".into(),
            "Escape" => "Escape".into(),
            other => other.into(),
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
    use selo_core::{Error, Result};
    use std::sync::mpsc::{Receiver, Sender, channel};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage,
    };

    enum Command {
        Rebind {
            index: usize,
            binding: (Modifiers, Code),
            done: Sender<Result<()>>,
        },
        Stop,
    }

    pub struct Hotkeys {
        bindings: Vec<(Modifiers, Code)>,
        commands: Sender<Command>,
        presses: Receiver<usize>,
        thread: Option<JoinHandle<()>>,
    }

    impl Hotkeys {
        pub fn new(bindings: &[(Modifiers, Code)]) -> Result<Self> {
            let (commands, command_rx) = channel();
            let (presses, press_rx) = channel();
            let (ready_tx, ready_rx) = channel();
            let initial = bindings.to_vec();
            let thread = thread::Builder::new()
                .name("selo-hotkeys".into())
                .spawn(move || hotkey_thread(initial, command_rx, presses, ready_tx))
                .map_err(|err| Error::Platform(format!("start hotkey thread: {err}")))?;
            let ready = ready_rx
                .recv()
                .map_err(|_| Error::Platform("hotkey thread exited".into()))?;
            if let Err(err) = ready {
                let _ = thread.join();
                return Err(err);
            }
            Ok(Self {
                bindings: bindings.to_vec(),
                commands,
                presses: press_rx,
                thread: Some(thread),
            })
        }

        pub fn rebind(&mut self, index: usize, binding: (Modifiers, Code)) -> Result<()> {
            if index >= self.bindings.len() {
                return Err(Error::Platform(format!("no hotkey at index {index}")));
            }
            let (done, result) = channel();
            self.commands
                .send(Command::Rebind {
                    index,
                    binding,
                    done,
                })
                .map_err(|_| Error::Platform("hotkey thread exited".into()))?;
            result
                .recv()
                .map_err(|_| Error::Platform("hotkey thread exited".into()))??;
            self.bindings[index] = binding;
            Ok(())
        }

        pub fn drain_presses(&self) -> Vec<usize> {
            self.presses.try_iter().collect()
        }
    }

    impl Drop for Hotkeys {
        fn drop(&mut self) {
            let _ = self.commands.send(Command::Stop);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn hotkey_thread(
        bindings: Vec<(Modifiers, Code)>,
        commands: Receiver<Command>,
        presses: Sender<usize>,
        ready: Sender<Result<()>>,
    ) {
        let manager = match GlobalHotKeyManager::new() {
            Ok(manager) => manager,
            Err(err) => {
                let _ = ready.send(Err(Error::Platform(err.to_string())));
                return;
            }
        };
        let mut ids = Vec::with_capacity(bindings.len());
        for &(modifiers, code) in &bindings {
            let hotkey = HotKey::new(Some(modifiers), code);
            if let Err(err) = manager.register(hotkey) {
                let _ = ready.send(Err(Error::Platform(err.to_string())));
                return;
            }
            ids.push(hotkey.id());
        }
        let _ = ready.send(Ok(()));
        let mut current = bindings;
        loop {
            while let Ok(command) = commands.try_recv() {
                match command {
                    Command::Stop => return,
                    Command::Rebind {
                        index,
                        binding,
                        done,
                    } => {
                        let result = (|| {
                            let old = current.get(index).copied().ok_or_else(|| {
                                Error::Platform(format!("no hotkey at index {index}"))
                            })?;
                            let _ = manager.unregister(HotKey::new(Some(old.0), old.1));
                            let hotkey = HotKey::new(Some(binding.0), binding.1);
                            manager
                                .register(hotkey)
                                .map_err(|err| Error::Platform(err.to_string()))?;
                            ids[index] = hotkey.id();
                            current[index] = binding;
                            Ok(())
                        })();
                        let _ = done.send(result);
                    }
                }
            }
            let mut message = MSG::default();
            while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
                unsafe {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                if event.state == HotKeyState::Pressed
                    && let Some(index) = ids.iter().position(|&id| id == event.id)
                {
                    eprintln!("hotkey: id={} index={index}", event.id);
                    let _ = presses.send(index);
                }
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
}

pub use platform::Hotkeys;
