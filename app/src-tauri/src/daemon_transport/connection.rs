use super::{Events, Shared};
use open_island_core::protocol::Response;
use serde_json::{json, Value};
use std::{
    io::{ErrorKind, Read},
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{atomic::Ordering, Arc, Mutex},
    time::{Duration, Instant},
};

pub(super) fn run(
    shared: Arc<Shared>,
    path: PathBuf,
    events: Events,
    spawn_once: Box<dyn FnOnce() + Send>,
) {
    let mut spawn_once = Some(spawn_once);
    let mut delay = Duration::from_millis(100);
    while !shared.stopped.load(Ordering::Relaxed) {
        let Ok(mut reader) = super::socket::connect(&path) else {
            if let Some(spawn) = spawn_once.take() {
                spawn();
            }
            wait(&shared, delay);
            delay = (delay * 2).min(Duration::from_secs(2));
            continue;
        };
        spawn_once = None;
        let Ok(writer) = reader.try_clone() else {
            continue;
        };
        let Ok(shutdown) = reader.try_clone() else {
            continue;
        };
        if reader
            .set_read_timeout(Some(Duration::from_millis(200)))
            .is_err()
            || writer
                .set_write_timeout(Some(Duration::from_secs(2)))
                .is_err()
        {
            continue;
        }
        let generation = {
            let Ok(mut state) = shared.state.lock() else {
                return;
            };
            let Some(generation) = state.generation.checked_add(1) else {
                return;
            };
            state.generation = generation;
            state.writer = Some(Arc::new(Mutex::new(writer)));
            state.shutdown = Some(shutdown);
            generation
        };
        delay = Duration::from_millis(100);
        events(
            "daemon-connection",
            json!({"state": "connected", "generation": generation}),
        );
        read(&shared, &mut reader, generation, &events);
        shared.disconnect(generation);
        if !shared.stopped.load(Ordering::Relaxed) {
            events(
                "daemon-connection",
                json!({"state": "reconnecting", "generation": generation}),
            );
            wait(&shared, delay);
        }
    }
}
fn wait(shared: &Shared, delay: Duration) {
    let deadline = Instant::now() + delay;
    while !shared.stopped.load(Ordering::Relaxed) && Instant::now() < deadline {
        std::thread::sleep(
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(50)),
        );
    }
}
fn read(shared: &Shared, reader: &mut UnixStream, generation: u64, events: &Events) {
    let mut buffer = Vec::new();
    let mut chunk = [0; 8192];
    let mut started = None;
    while !shared.stopped.load(Ordering::Relaxed) {
        if started.is_some_and(|at: Instant| at.elapsed() >= Duration::from_secs(5)) {
            return;
        }
        let count = match reader.read(&mut chunk) {
            Ok(0) => return,
            Ok(count) => count,
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::TimedOut | ErrorKind::WouldBlock | ErrorKind::Interrupted
                ) =>
            {
                continue
            }
            Err(_) => return,
        };
        for byte in &chunk[..count] {
            if *byte == b'\n' {
                let Ok(value) = serde_json::from_slice::<Value>(&buffer) else {
                    return;
                };
                buffer.clear();
                started = None;
                if let Some(event) = value.get("event").and_then(Value::as_str) {
                    if let Some(data) = value.get("data") {
                        events(event, data.clone());
                    }
                } else if let Ok(response) = serde_json::from_value::<Response>(value) {
                    if response.v != 1 {
                        return;
                    }
                    if let Some(id) = response.id.as_u64() {
                        let reply = if response.ok {
                            Ok(response.data.unwrap_or(Value::Null))
                        } else {
                            Err(response
                                .error
                                .unwrap_or_else(|| "daemon_request_failed".to_owned()))
                        };
                        if let Ok(mut state) = shared.state.lock() {
                            state.pending.settle(id, generation, reply);
                        }
                    }
                }
            } else {
                started.get_or_insert_with(Instant::now);
                if buffer.len() >= 1024 * 1024 {
                    return;
                }
                buffer.push(*byte);
            }
        }
    }
}
