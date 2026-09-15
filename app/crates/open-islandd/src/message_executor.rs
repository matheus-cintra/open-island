use crate::{
    message_dispatch::{self, Target},
    message_resources::Resources,
    message_runner::MessageRunner,
    notifications::lifecycle::SharedState,
    usage,
};
use open_island_core::{
    message_delivery::{DaemonEpoch, DeliveryIdentity, DeliveryState, MessageDelivery},
    process,
    session::Session,
};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, SyncSender},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

const WORKERS: usize = 8;
pub struct MessageExecutor {
    pub resources: Arc<Resources>,
    pub epoch: DaemonEpoch,
    sender: Mutex<Option<SyncSender<Job>>>,
    active: Arc<AtomicUsize>,
    stopping: Arc<AtomicBool>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}
struct Permit(Arc<AtomicUsize>);
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
struct Job {
    state: SharedState,
    target: Target,
    message: MessageDelivery,
    started: Instant,
    _permit: Permit,
    effects: bool,
    settled: bool,
}
impl Job {
    fn settle(&mut self, outcome: DeliveryState, error: Option<String>) {
        if self.settled {
            return;
        }
        if let Ok(mut state) = self.state.lock() {
            state.store.deliveries.settle(
                self.message.message_id,
                self.message.attempt_id.unwrap_or(0),
                outcome,
                error,
                usage::now_ms(),
            );
        }
        self.settled = true;
        publish(&self.state);
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.settle(
            if self.effects {
                DeliveryState::Unconfirmed
            } else {
                DeliveryState::Failed
            },
            Some("delivery_cancelled".to_owned()),
        );
    }
}
pub fn identity(
    session: &Session,
    birth: process::ProcessBirthIdentity,
    epoch: &DaemonEpoch,
) -> DeliveryIdentity {
    DeliveryIdentity::for_process(session, birth, epoch)
}
pub fn publish(state: &SharedState) {
    crate::broadcast::broadcast(
        state,
        serde_json::json!({"v":1,"event":"message-deliveries-invalidated","data":{}}).to_string(),
    );
}
impl MessageExecutor {
    pub fn new() -> std::io::Result<Self> {
        let epoch = open_island_core::epoch::generate()?;
        let (sender, receiver) = mpsc::sync_channel::<Job>(WORKERS);
        let receiver = Arc::new(Mutex::new(receiver));
        let resources = Arc::new(Resources::default());
        let stopping = Arc::new(AtomicBool::new(false));
        let mut executor = Self {
            epoch: epoch.clone(),
            resources: resources.clone(),
            sender: Mutex::new(Some(sender)),
            active: Arc::new(AtomicUsize::new(0)),
            stopping: stopping.clone(),
            workers: Mutex::new(Vec::new()),
        };
        for index in 0..WORKERS {
            let receiver = receiver.clone();
            let resources = resources.clone();
            let cancelled = stopping.clone();
            let epoch = epoch.clone();
            let worker = std::thread::Builder::new()
                .name(format!("island-delivery-{index}"))
                .spawn(move || loop {
                    let received = receiver
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .recv_timeout(Duration::from_millis(100));
                    let mut job = match received {
                        Ok(job) => job,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if cancelled.load(Ordering::Acquire) {
                                break;
                            }
                            continue;
                        }
                    };
                    let runner = MessageRunner::new(job.started, cancelled.clone());
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        message_dispatch::deliver(
                            &job.state,
                            &job.target,
                            &job.message,
                            &epoch,
                            &runner,
                            &resources,
                        )
                    }));
                    job.effects = runner.effects.load(Ordering::Acquire);
                    match result {
                        Ok((outcome, error)) => job.settle(outcome, error),
                        Err(_) => job.settle(
                            if job.effects {
                                DeliveryState::Unconfirmed
                            } else {
                                DeliveryState::Failed
                            },
                            Some("delivery_worker_failed".to_owned()),
                        ),
                    }
                })?;
            executor
                .workers
                .get_mut()
                .unwrap_or_else(|error| error.into_inner())
                .push(worker);
        }
        Ok(executor)
    }
    pub fn schedule(
        &self,
        state: &SharedState,
        session: &Session,
        guarded: bool,
    ) -> Result<bool, String> {
        if self.stopping.load(Ordering::Acquire) {
            return Ok(false);
        }
        if self
            .active
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                (active < WORKERS).then_some(active + 1)
            })
            .is_err()
        {
            return Ok(false);
        }
        let permit = Permit(self.active.clone());
        let birth =
            process::birth_identity(session.pid).unwrap_or(process::ProcessBirthIdentity::new(0));
        let started = Instant::now();
        let message = state
            .lock()
            .map_err(|_| "daemon_unavailable")?
            .store
            .reserve_message(session)?;
        let Some(message) = message else {
            return Ok(false);
        };
        let mut job = Job {
            state: state.clone(),
            target: Target {
                session: session.clone(),
                birth,
                guarded: guarded || message.client_submission_id.is_some(),
            },
            message,
            started,
            _permit: permit,
            effects: false,
            settled: false,
        };
        let sender = self.sender.lock().map_err(|_| "executor_unavailable")?;
        let Some(sender) = sender.as_ref() else {
            if let Ok(mut state) = job.state.lock() {
                state
                    .store
                    .deliveries
                    .rollback(job.message.message_id, job.message.attempt_id.unwrap_or(0));
            }
            job.settled = true;
            return Ok(false);
        };
        match sender.try_send(job) {
            Ok(()) => {
                publish(state);
                Ok(true)
            }
            Err(error) => {
                job = match error {
                    mpsc::TrySendError::Full(job) | mpsc::TrySendError::Disconnected(job) => job,
                };
                if let Ok(mut state) = job.state.lock() {
                    state
                        .store
                        .deliveries
                        .rollback(job.message.message_id, job.message.attempt_id.unwrap_or(0));
                }
                job.settled = true;
                Ok(false)
            }
        }
    }
    pub fn active(&self) -> usize {
        self.active.load(Ordering::Acquire)
    }
    pub fn shutdown(&self) {
        self.stopping.store(true, Ordering::Release);
        self.sender
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        for worker in self
            .workers
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .drain(..)
        {
            let _ = worker.join();
        }
    }
}
impl Drop for MessageExecutor {
    fn drop(&mut self) {
        self.shutdown();
    }
}
