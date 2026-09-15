mod connection;
mod pending;
pub mod refresh;
mod socket;
pub mod sync;
#[cfg(test)]
mod tests;

use open_island_core::protocol::Request;
use pending::{Pending, Reply};
use serde_json::{json, Value};
use std::{
    io::Write,
    net::Shutdown,
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::JoinHandle,
    time::Duration,
};

pub type Events = Arc<dyn Fn(&str, Value) + Send + Sync>;
pub struct Transport {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}
struct Shared {
    state: Mutex<State>,
    next_id: AtomicU64,
    stopped: AtomicBool,
}
#[derive(Default)]
struct State {
    generation: u64,
    writer: Option<Arc<Mutex<UnixStream>>>,
    shutdown: Option<UnixStream>,
    pending: Pending,
}
struct RequestGuard<'a> {
    shared: &'a Shared,
    id: u64,
}
impl Drop for RequestGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.pending.remove(self.id);
        }
    }
}
impl Transport {
    pub fn start(
        path: PathBuf,
        events: Events,
        spawn_once: Box<dyn FnOnce() + Send>,
    ) -> Result<Self, String> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            next_id: AtomicU64::new(1),
            stopped: AtomicBool::new(false),
        });
        let owner = shared.clone();
        let worker = std::thread::Builder::new()
            .name("island-daemon-connection".to_owned())
            .spawn(move || connection::run(owner, path, events, spawn_once))
            .map_err(|_| "transport_start_failed")?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }

    pub fn request(&self, method: &str, params: Value) -> Reply {
        self.request_timeout(method, params, Duration::from_secs(5))
    }

    fn request_timeout(&self, method: &str, params: Value, timeout: Duration) -> Reply {
        let id = self
            .shared
            .next_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| "request_id_exhausted")?;
        let (sender, receiver) = mpsc::channel();
        let (writer, generation) = {
            let mut state = self
                .shared
                .state
                .lock()
                .map_err(|_| "transport_unavailable")?;
            let writer = state.writer.clone().ok_or("daemon_unavailable")?;
            let generation = state.generation;
            state.pending.insert(id, generation, sender)?;
            (writer, generation)
        };
        let _guard = RequestGuard {
            shared: &self.shared,
            id,
        };
        let mut bytes = serde_json::to_vec(&Request {
            v: 1,
            id: json!(id),
            method: method.to_owned(),
            params: Some(params),
        })
        .map_err(|_| "invalid_request")?;
        bytes.push(b'\n');
        if bytes.len() > 1024 * 1024 {
            return Err("request_too_large".to_owned());
        }
        let written = writer
            .lock()
            .map_err(|_| "transport_unavailable")?
            .write_all(&bytes);
        if written.is_err() {
            self.shared.disconnect(generation);
            return Err("daemon_unavailable".to_owned());
        }
        receiver
            .recv_timeout(timeout)
            .map_err(|_| "daemon_response_timeout".to_owned())?
    }
}
impl Shared {
    fn disconnect(&self, generation: u64) {
        if let Ok(mut state) = self.state.lock() {
            if state.generation != generation {
                return;
            }
            if let Some(stream) = state.shutdown.take() {
                let _ = stream.shutdown(Shutdown::Both);
            }
            state.writer = None;
            state.pending.disconnect();
        }
    }
}
impl Drop for Transport {
    fn drop(&mut self) {
        self.shared.stopped.store(true, Ordering::Relaxed);
        let generation = self
            .shared
            .state
            .lock()
            .map(|state| state.generation)
            .unwrap_or(0);
        self.shared.disconnect(generation);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod refresh_tests;
