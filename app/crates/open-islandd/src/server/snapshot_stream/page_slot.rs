use open_island_core::snapshot_page::{SnapshotPage, PAGE_BYTES};
use std::{
    io::{self, Write},
    sync::{Arc, Condvar, Mutex, MutexGuard},
    time::{Duration, Instant},
};

pub(super) const IDLE: Duration = Duration::from_secs(30);
pub(super) struct PageSlot {
    pub id: u64,
    state: Mutex<State>,
    changed: Condvar,
}
struct State {
    page: Option<SnapshotPage>,
    expected: u64,
    progress: Instant,
    cancelled: bool,
}
impl PageSlot {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            state: Mutex::new(State {
                page: None,
                expected: 0,
                progress: Instant::now(),
                cancelled: false,
            }),
            changed: Condvar::new(),
        }
    }
    pub fn cancel(&self) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.cancelled = true;
        state.page = None;
        self.changed.notify_all();
    }
    pub fn expired(&self, now: Instant) -> bool {
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.cancelled || now.saturating_duration_since(state.progress) >= IDLE
    }
    fn wait<'a>(&self, state: MutexGuard<'a, State>) -> Result<MutexGuard<'a, State>, String> {
        let remaining = IDLE.saturating_sub(state.progress.elapsed());
        if state.cancelled || remaining.is_zero() {
            return Err("snapshot_expired".into());
        }
        self.changed
            .wait_timeout(state, remaining)
            .map(|(state, _)| state)
            .map_err(|_| "snapshot_unavailable".into())
    }
    fn put(&self, page: SnapshotPage) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|_| "snapshot_unavailable")?;
        while state.page.is_some() && !state.cancelled {
            state = self.wait(state)?;
        }
        if state.cancelled || state.progress.elapsed() >= IDLE {
            return Err("snapshot_expired".into());
        }
        state.page = Some(page);
        self.changed.notify_all();
        Ok(())
    }
    pub fn take(&self, expected: u64) -> Result<SnapshotPage, String> {
        let mut state = self.state.lock().map_err(|_| "snapshot_unavailable")?;
        loop {
            if state.cancelled || state.progress.elapsed() >= IDLE {
                return Err("snapshot_expired".into());
            }
            if state.expected != expected {
                return Err("snapshot_out_of_sequence".into());
            }
            if let Some(page) = state.page.take() {
                state.expected += 1;
                state.progress = Instant::now();
                self.changed.notify_all();
                return Ok(page);
            }
            state = self.wait(state)?;
        }
    }
    #[cfg(test)]
    pub fn age(&self, age: Duration) {
        self.state.lock().unwrap().progress = Instant::now() - age;
    }
}
pub(super) struct PageWriter {
    slot: Arc<PageSlot>,
    bytes: Vec<u8>,
    total: u64,
    index: u64,
}
impl PageWriter {
    pub fn new(slot: Arc<PageSlot>) -> Self {
        Self {
            slot,
            bytes: Vec::with_capacity(PAGE_BYTES),
            total: 0,
            index: 0,
        }
    }
    fn publish(&mut self, end: bool) -> io::Result<()> {
        let bytes = std::mem::replace(&mut self.bytes, Vec::with_capacity(PAGE_BYTES));
        self.slot
            .put(SnapshotPage {
                snapshot_id: self.slot.id,
                page_index: self.index,
                bytes,
                end,
                total_bytes: end.then_some(self.total),
            })
            .map_err(io::Error::other)?;
        self.index += 1;
        Ok(())
    }
    pub fn finish(mut self) -> io::Result<()> {
        self.publish(true)
    }
}
impl Write for PageWriter {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        if self.slot.expired(Instant::now()) {
            return Err(io::Error::other("snapshot_expired"));
        }
        let count = input.len().min(PAGE_BYTES - self.bytes.len());
        self.bytes.extend_from_slice(&input[..count]);
        self.total += count as u64;
        if self.bytes.len() == PAGE_BYTES {
            self.publish(false)?;
        }
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
