//! Cooperative shutdown: one cloneable shared flag backed by an `Arc<AtomicBool>`
//! plus RAII signal-hook registrations for SIGTERM/SIGINT that request the same
//! flag and unregister on Drop. `signal_hook::flag::register` installs no helper
//! thread; the flag is polled by the worker loop and the accept loop.

use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::flag;
use signal_hook::low_level;
use signal_hook::SigId;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Default nonblocking accept/poll tick: the maximum slice of any shutdown
/// interruptible wait, so a requested flag is observed within one tick.
pub const TICK: Duration = Duration::from_millis(250);

/// One shared, cloneable shutdown flag. Every clone observes the same requested
/// state, so a signal registration, the daemon and the worker all read one bit.
#[derive(Clone, Default)]
pub struct ShutdownFlag {
    requested: Arc<AtomicBool>,
}

impl ShutdownFlag {
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests shutdown: stores `true` on the shared flag.
    pub fn request(&self) {
        self.requested.store(true, Ordering::SeqCst);
    }

    /// Returns whether shutdown has been requested.
    pub fn is_requested(&self) -> bool {
        self.requested.load(Ordering::SeqCst)
    }

    /// Returns a clone of the backing `Arc<AtomicBool>` for signal-hook
    /// registration.
    pub(crate) fn share(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.requested)
    }
}

/// RAII guard over the SIGTERM and SIGINT registrations installed for one
/// shared shutdown flag. Dropping the guard unregisters both IDs; no helper
/// thread is created.
pub struct SignalRegistrations {
    ids: [SigId; 2],
}

impl SignalRegistrations {
    /// Registers SIGTERM and SIGINT to request the given flag.
    ///
    /// # Errors
    /// Returns the first `io::Error` if a registration fails; a partially
    /// installed registration is unregistered before returning.
    pub fn register(flag: &ShutdownFlag) -> io::Result<Self> {
        let shared = flag.share();
        let term = flag::register(SIGTERM, Arc::clone(&shared))?;
        match flag::register(SIGINT, Arc::clone(&shared)) {
            Ok(int) => Ok(Self { ids: [term, int] }),
            Err(error) => {
                low_level::unregister(term);
                Err(error)
            }
        }
    }
}

impl Drop for SignalRegistrations {
    fn drop(&mut self) {
        for id in self.ids {
            low_level::unregister(id);
        }
    }
}

#[cfg(test)]
#[path = "shutdown_tests.rs"]
mod tests;

/// Sleeps in `min(TICK, remaining)` slices until `duration` fully elapses or
/// the flag is requested, returning false on shutdown and true only after the
/// whole interval.
pub fn wait_for_interval(flag: &ShutdownFlag, duration: Duration) -> bool {
    if flag.is_requested() {
        return false;
    }
    let deadline = Instant::now() + duration;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return true;
        }
        std::thread::sleep(std::cmp::min(TICK, remaining));
        if flag.is_requested() {
            return false;
        }
    }
}

/// Owns one named thread with a completion signal: a bounded join that detaches
/// the thread on deadline instead of blocking forever.
pub struct BoundedThread {
    done_rx: std::sync::mpsc::Receiver<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl BoundedThread {
    pub fn spawn<F>(name: &str, body: F) -> io::Result<Self>
    where
        F: FnOnce() + Send + 'static,
    {
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                body();
                let _ = done_tx.send(());
            })?;
        Ok(Self {
            done_rx,
            thread: Some(thread),
        })
    }

    /// Waits up to `deadline` for the thread to finish, joining it on
    /// completion (receive or disconnected sender) and detaching it on
    /// timeout.
    pub fn join_with_deadline(&mut self, deadline: Duration) -> bool {
        let completed = match self.done_rx.recv_timeout(deadline) {
            Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => true,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => false,
        };
        if let Some(thread) = self.thread.take() {
            if completed {
                let _ = thread.join();
            }
        }
        completed
    }
}
