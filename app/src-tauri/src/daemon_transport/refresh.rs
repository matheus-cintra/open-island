use super::{sync::SyncedSnapshot, Transport};
use serde::Serialize;
use serde_json::Value;
use std::{
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Connecting,
    Connected,
    Reconnecting,
    Incompatible,
}
#[derive(Clone, Debug, Serialize)]
pub struct UiCache {
    pub phase: Phase,
    pub generation: u64,
    pub snapshot: Option<SyncedSnapshot>,
}
struct State {
    cache: UiCache,
    dirty: bool,
    online: bool,
    stopped: bool,
}
pub struct Refresh {
    state: Mutex<State>,
    changed: Condvar,
}
pub struct Worker {
    control: Arc<Refresh>,
    thread: Option<JoinHandle<()>>,
}
impl Default for Refresh {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                cache: UiCache {
                    phase: Phase::Connecting,
                    generation: 0,
                    snapshot: None,
                },
                dirty: false,
                online: false,
                stopped: false,
            }),
            changed: Condvar::new(),
        }
    }
}
impl Refresh {
    pub fn event(&self, event: &str, data: &Value) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if event == "daemon-connection" {
            let Some(generation) = data.get("generation").and_then(Value::as_u64) else {
                return;
            };
            if generation < state.cache.generation {
                return;
            }
            state.cache.generation = generation;
            state.online = data.get("state").and_then(Value::as_str) == Some("connected");
            state.cache.phase = if state.online {
                Phase::Connecting
            } else {
                Phase::Reconnecting
            };
            state.dirty = state.online;
        } else if event == "ui-state-invalidated" {
            if state.online && state.cache.phase != Phase::Incompatible {
                state.dirty = true;
            }
        } else {
            return;
        }
        self.changed.notify_all();
    }
    pub fn cache(&self) -> UiCache {
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .cache
            .clone()
    }
    pub fn start(
        self: &Arc<Self>,
        transport: Arc<Transport>,
        emit: Arc<dyn Fn(UiCache) + Send + Sync>,
    ) -> std::io::Result<Worker> {
        let control = self.clone();
        let worker_control = control.clone();
        let thread = std::thread::Builder::new()
            .name("island-state-refresh".into())
            .spawn(move || {
                worker_control.run(&transport, emit);
            })?;
        Ok(Worker {
            control,
            thread: Some(thread),
        })
    }
    fn cancelled(&self, generation: u64) -> bool {
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.stopped || !state.online || state.cache.generation != generation
    }
    fn run(&self, transport: &Transport, emit: Arc<dyn Fn(UiCache) + Send + Sync>) {
        let mut subscribed = None;
        let mut delay = Duration::from_millis(100);
        loop {
            let generation = {
                let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
                while !state.stopped && (!state.online || !state.dirty) {
                    state = self.changed.wait(state).unwrap_or_else(|p| p.into_inner());
                }
                if state.stopped {
                    return;
                }
                state.dirty = false;
                state.cache.generation
            };
            let result = if subscribed == Some(generation) {
                transport
                    .ui_snapshot_until(|| self.cancelled(generation))
                    .map(Some)
            } else {
                handshake(transport).and_then(|compatible| {
                    if !compatible {
                        return Ok(None);
                    }
                    subscribed = Some(generation);
                    transport
                        .ui_snapshot_until(|| self.cancelled(generation))
                        .map(Some)
                })
            };
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            if state.stopped {
                return;
            }
            if state.cache.generation != generation || !state.online {
                continue;
            }
            match result {
                Ok(Some(snapshot)) if snapshot.generation == generation => {
                    state.cache.snapshot = Some(snapshot);
                    state.cache.phase = Phase::Connected;
                    delay = Duration::from_millis(100);
                }
                Ok(None) => {
                    state.cache.phase = Phase::Incompatible;
                    state.dirty = false;
                }
                _ => {
                    state.cache.phase = Phase::Reconnecting;
                    let cache = state.cache.clone();
                    drop(state);
                    emit(cache);
                    let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
                    let (mut state, _) = self
                        .changed
                        .wait_timeout(state, delay)
                        .unwrap_or_else(|p| p.into_inner());
                    if state.cache.generation == generation && state.online {
                        state.dirty = true;
                    }
                    delay = (delay * 2).min(Duration::from_secs(2));
                    continue;
                }
            }
            let cache = state.cache.clone();
            drop(state);
            emit(cache);
        }
    }
}
fn handshake(transport: &Transport) -> Result<bool, String> {
    let ping = transport.request("ping", serde_json::json!({}))?;
    let required = [
        "ui_state_v1",
        "message_delivery_v1",
        "guarded_actions_v1",
        "diagnostics_v1",
    ];
    let compatible = ping
        .get("capabilities")
        .and_then(Value::as_array)
        .is_some_and(|caps| {
            required
                .iter()
                .all(|required| caps.iter().any(|cap| cap.as_str() == Some(required)))
        });
    if !compatible {
        return Ok(false);
    }
    let subscription = transport.request("subscribe_ui", serde_json::json!({}))?;
    if subscription.get("daemon_epoch") != ping.get("daemon_epoch")
        || ping.get("daemon_epoch").and_then(Value::as_str).is_none()
    {
        return Err("stale_epoch".into());
    }
    Ok(true)
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.control
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .stopped = true;
        self.control.changed.notify_all();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
