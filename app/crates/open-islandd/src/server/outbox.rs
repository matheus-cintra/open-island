use std::{
    collections::VecDeque,
    net::Shutdown,
    os::unix::net::UnixStream,
    sync::{Arc, Condvar, Mutex},
};

pub const MAX_ITEMS: usize = 128;
pub const MAX_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_FRAME: usize = 256 * 1024;
struct Item {
    message: String,
    replace: Option<String>,
}
#[derive(Default)]
struct Queue {
    items: VecDeque<Item>,
    bytes: usize,
    closed: bool,
}
struct Inner {
    queue: Mutex<Queue>,
    changed: Condvar,
    socket: UnixStream,
    metrics: Arc<super::outbox_metrics::Metrics>,
}
#[derive(Clone)]
pub struct Outbox(Arc<Inner>);
impl Outbox {
    pub fn new(socket: UnixStream) -> Self {
        Self::with_metrics(socket, Arc::default())
    }
    pub fn with_metrics(socket: UnixStream, metrics: Arc<super::outbox_metrics::Metrics>) -> Self {
        Self(Arc::new(Inner {
            queue: Mutex::new(Queue::default()),
            changed: Condvar::new(),
            socket,
            metrics,
        }))
    }
    pub fn send_with_key(
        &self,
        message: String,
        event: Option<&str>,
    ) -> Result<(), &'static str> {
        let replace = event
            .filter(|event| {
                matches!(
                    *event,
                    "ui-state-invalidated"
                        | "message-deliveries-invalidated"
                        | "sessions-updated"
                        | "usage-updated"
                        | "quiet-scenes"
                        | "update-available"
                        | "config-changed"
                )
            })
            .map(str::to_owned);
        let mut queue = self.0.queue.lock().map_err(|_| "outbox_unavailable")?;
        if queue.closed {
            return Err("outbox_closed");
        }
        if message.len() + 1 > MAX_FRAME {
            queue.closed = true;
            drop(queue);
            self.close();
            return Err("snapshot_requires_paging");
        }
        if let Some(key) = &replace {
            if let Some(index) = queue
                .items
                .iter()
                .position(|item| item.replace.as_ref() == Some(key))
            {
                let removed = queue.items.remove(index).expect("queued state");
                queue.bytes -= removed.message.len() + 1;
            }
        }
        if queue.items.len() == MAX_ITEMS || queue.bytes + message.len() + 1 > MAX_BYTES {
            self.0.metrics.overflow();
            queue.closed = true;
            drop(queue);
            self.close();
            return Err("outbox_full");
        }
        queue.bytes += message.len() + 1;
        queue.items.push_back(Item { message, replace });
        self.0.metrics.observe(queue.items.len(), queue.bytes);
        self.0.changed.notify_one();
        Ok(())
    }
    pub fn receive(&self) -> Option<String> {
        let mut queue = self.0.queue.lock().ok()?;
        loop {
            if queue.closed {
                return None;
            }
            if let Some(item) = queue.items.pop_front() {
                queue.bytes -= item.message.len() + 1;
                return Some(item.message);
            }
            queue = self.0.changed.wait(queue).ok()?;
        }
    }
    pub fn close(&self) {
        if let Ok(mut queue) = self.0.queue.lock() {
            queue.closed = true;
            queue.items.clear();
            queue.bytes = 0;
        }
        let _ = self.0.socket.shutdown(Shutdown::Both);
        self.0.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    #[test]
    fn coalesces_state_and_closes_both_directions_on_critical_overflow() {
        let (socket, mut peer) = UnixStream::pair().unwrap();
        let outbox = Outbox::new(socket);
        for n in 0..1000 {
            outbox
                .send_with_key(
                    format!("{{\"event\":\"sessions-updated\",\"data\":{n}}}"),
                    Some("sessions-updated"),
                )
                .unwrap();
        }
        assert_eq!(outbox.0.queue.lock().unwrap().items.len(), 1);
        assert!(outbox.receive().unwrap().contains("999"));
        for _ in 0..MAX_ITEMS {
            outbox.send_with_key("{}".into(), None).unwrap();
        }
        assert_eq!(
            outbox.send_with_key("{}".into(), None),
            Err("outbox_full")
        );
        assert_eq!(peer.read(&mut [0]).unwrap(), 0);
        assert!(outbox.receive().is_none());
        let counters = outbox.0.metrics.snapshot();
        assert_eq!(counters.max_connection_items, MAX_ITEMS as u64);
        assert_eq!(counters.overflow_disconnects, 1);
    }
    #[test]
    fn concurrent_overflow_counts_one_disconnection() {
        let (socket, _peer) = UnixStream::pair().unwrap();
        let outbox = Outbox::new(socket);
        for _ in 0..MAX_ITEMS {
            outbox.send_with_key("{}".into(), None).unwrap();
        }
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let outbox = outbox.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    outbox.send_with_key("{}".into(), None)
                })
            })
            .collect();
        for thread in threads {
            assert!(thread.join().unwrap().is_err());
        }
        assert_eq!(outbox.0.metrics.snapshot().overflow_disconnects, 1);
    }
    #[test]
    fn byte_limit_applies_before_item_limit() {
        let (socket, _peer) = UnixStream::pair().unwrap();
        let outbox = Outbox::new(socket);
        for _ in 0..16 {
            outbox
                .send_with_key("x".repeat(MAX_FRAME - 1), None)
                .unwrap();
        }
        assert_eq!(outbox.send_with_key("x".into(), None), Err("outbox_full"));
    }
}
