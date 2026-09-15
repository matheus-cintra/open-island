use std::sync::atomic::{AtomicU64, Ordering};
#[derive(Default)]
pub struct Metrics {
    items: AtomicU64,
    bytes: AtomicU64,
    overflow: AtomicU64,
}
impl Metrics {
    pub fn observe(&self, items: usize, bytes: usize) {
        self.items.fetch_max(items as u64, Ordering::Relaxed);
        self.bytes.fetch_max(bytes as u64, Ordering::Relaxed);
    }
    pub fn overflow(&self) {
        let _ = self
            .overflow
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                Some(n.saturating_add(1))
            });
    }
    pub fn snapshot(&self) -> open_island_core::diagnostics::OutboxCounters {
        open_island_core::diagnostics::OutboxCounters {
            max_connection_items: self.items.load(Ordering::Relaxed),
            max_connection_bytes: self.bytes.load(Ordering::Relaxed),
            overflow_disconnects: self.overflow.load(Ordering::Relaxed),
            item_limit: super::outbox::MAX_ITEMS as u64,
            byte_limit: super::outbox::MAX_BYTES as u64,
        }
    }
}
