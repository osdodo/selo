use std::collections::HashMap;
use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

#[derive(Debug, Clone, Copy)]
pub struct Output {
    pub id: u64,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: i32,
}

impl Output {
    /// Wayland has no global desktop origin; `x`/`y` are whatever the compositor reports.
    pub fn logical(&self) -> (f64, f64, f64, f64) {
        let scale = self.scale.max(1) as f64;
        (
            self.x as f64,
            self.y as f64,
            self.width as f64 / scale,
            self.height as f64 / scale,
        )
    }
}

pub fn outputs() -> Vec<Output> {
    let Ok(connection) = Connection::connect_to_env() else {
        return Vec::new();
    };
    let mut queue = connection.new_event_queue();
    let qh = queue.handle();
    let _registry = connection.display().get_registry(&qh, ());
    let mut state = State::default();
    // One roundtrip binds the globals, a second delivers the output events.
    let _ = queue.roundtrip(&mut state);
    let _ = queue.roundtrip(&mut state);
    let mut outputs: Vec<Output> = state.outputs.into_values().collect();
    outputs.sort_by_key(|output| (output.x, output.y));
    outputs
}

#[derive(Default)]
struct State {
    outputs: HashMap<u64, Output>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _connection: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
            && interface == "wl_output"
        {
            // Cap at v4: enough for `scale` and the final `Done`, without the v4 name events.
            let output = registry.bind::<wl_output::WlOutput, _, _>(name, version.min(4), qh, ());
            state.outputs.insert(
                output.id().protocol_id() as u64,
                Output {
                    id: output.id().protocol_id() as u64,
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0,
                    scale: 1,
                },
            );
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for State {
    fn event(
        state: &mut Self,
        output: &wl_output::WlOutput,
        event: wl_output::Event,
        _data: &(),
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let Some(entry) = state.outputs.get_mut(&(output.id().protocol_id() as u64)) else {
            return;
        };
        match event {
            wl_output::Event::Geometry { x, y, .. } => {
                entry.x = x;
                entry.y = y;
            }
            #[allow(deprecated)]
            wl_output::Event::Mode { width, height, .. } => {
                entry.width = width.max(0) as u32;
                entry.height = height.max(0) as u32;
            }
            wl_output::Event::Scale { factor } => entry.scale = factor.max(1),
            _ => {}
        }
    }
}
