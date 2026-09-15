mod page_slot;
#[cfg(test)]
mod tests;

use open_island_core::snapshot_page::SnapshotPage;
use page_slot::{PageSlot, PageWriter};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{mpsc, Arc, Mutex},
    thread::JoinHandle,
    time::Instant,
};

const MAX_TOKENS: usize = 4;
struct Job<T> {
    document: Arc<T>,
    slot: Arc<PageSlot>,
}
struct Tokens {
    next_id: u64,
    connections: HashMap<u64, Arc<PageSlot>>,
}
pub struct SnapshotPool<T> {
    tokens: Mutex<Tokens>,
    sender: Option<mpsc::SyncSender<Job<T>>>,
    workers: Vec<JoinHandle<()>>,
}
impl<T: Serialize + Send + Sync + 'static> SnapshotPool<T> {
    pub fn new() -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Job<T>>(MAX_TOKENS);
        let receiver = Arc::new(Mutex::new(receiver));
        let mut pool = Self {
            tokens: Mutex::new(Tokens {
                next_id: 1,
                connections: HashMap::new(),
            }),
            sender: Some(sender),
            workers: Vec::new(),
        };
        for index in 0..MAX_TOKENS {
            let receiver = receiver.clone();
            pool.workers.push(
                std::thread::Builder::new()
                    .name(format!("island-snapshot-{index}"))
                    .spawn(move || loop {
                        let Ok(job) = receiver.lock().expect("snapshot receiver").recv() else {
                            break;
                        };
                        let mut writer = PageWriter::new(job.slot.clone());
                        match serde_json::to_writer(&mut writer, job.document.as_ref()) {
                            Ok(()) => {
                                let _ = writer.finish();
                            }
                            Err(_) => job.slot.cancel(),
                        }
                    })?,
            );
        }
        Ok(pool)
    }
    pub fn begin(&self, connection: u64, document: Arc<T>) -> Result<u64, String> {
        self.begin_with(connection, || Ok(document))
    }
    pub fn begin_with(
        &self,
        connection: u64,
        freeze: impl FnOnce() -> Result<Arc<T>, String>,
    ) -> Result<u64, String> {
        let mut tokens = self.tokens.lock().map_err(|_| "snapshot_unavailable")?;
        tokens.connections.retain(|id, slot| {
            if *id == connection || slot.expired(Instant::now()) {
                slot.cancel();
                false
            } else {
                true
            }
        });
        if tokens.connections.len() >= MAX_TOKENS {
            return Err("snapshot_busy".into());
        }
        let document = freeze()?;
        let id = tokens.next_id;
        tokens.next_id = id.checked_add(1).ok_or("snapshot_id_exhausted")?;
        let slot = Arc::new(PageSlot::new(id));
        self.sender
            .as_ref()
            .ok_or("snapshot_unavailable")?
            .try_send(Job {
                document,
                slot: slot.clone(),
            })
            .map_err(|_| "snapshot_busy")?;
        tokens.connections.insert(connection, slot);
        Ok(id)
    }
    pub fn page(
        &self,
        connection: u64,
        snapshot: u64,
        expected: u64,
    ) -> Result<SnapshotPage, String> {
        let slot = self
            .tokens
            .lock()
            .map_err(|_| "snapshot_unavailable")?
            .connections
            .get(&connection)
            .cloned()
            .ok_or("snapshot_not_found")?;
        if slot.id != snapshot {
            return Err("snapshot_not_found".into());
        }
        let result = slot.take(expected);
        if result.as_ref().is_ok_and(|p| p.end) || slot.expired(Instant::now()) {
            let mut tokens = self.tokens.lock().map_err(|_| "snapshot_unavailable")?;
            if tokens
                .connections
                .get(&connection)
                .is_some_and(|current| current.id == snapshot)
            {
                tokens.connections.remove(&connection);
            }
            slot.cancel();
        }
        result
    }
    pub fn cancel(&self, connection: u64) {
        if let Ok(mut tokens) = self.tokens.lock() {
            if let Some(slot) = tokens.connections.remove(&connection) {
                slot.cancel();
            }
        }
    }
}
impl<T> Drop for SnapshotPool<T> {
    fn drop(&mut self) {
        if let Ok(tokens) = self.tokens.lock() {
            for slot in tokens.connections.values() {
                slot.cancel();
            }
        }
        self.sender.take();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}
