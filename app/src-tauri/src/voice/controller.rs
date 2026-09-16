use super::capture::Controls;
use open_island_core::{
    message_delivery::{DaemonEpoch, SessionInstanceId},
    ui_state::UiSnapshot,
};
use serde::{Deserialize, Serialize};
use std::{
    sync::{atomic::Ordering, Arc, Mutex},
    thread::{self, JoinHandle},
    time::Instant,
};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub session_id: String,
    pub session_instance_id: SessionInstanceId,
    pub daemon_epoch: DaemonEpoch,
}

/// Commands must additionally require a connected, compatible daemon cache.
pub struct VerifiedTarget(Target);
impl VerifiedTarget {
    pub fn from_snapshot(target: Target, snapshot: &UiSnapshot) -> Result<Self, &'static str> {
        if target.daemon_epoch != snapshot.daemon_epoch
            || !snapshot.targets().any(|session| {
                session.session.id == target.session_id
                    && session.session_instance_id.as_ref() == Some(&target.session_instance_id)
            })
        {
            return Err("voice_target_stale");
        }
        Ok(Self(target))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Idle,
    RequestingPermission,
    Recording,
    Transcribing,
    Ready,
    Cancelled,
    Error,
}

// Deliberately no Debug: snapshots contain private draft text and target identity.
#[derive(Clone, Serialize)]
pub struct View {
    pub revision: u64,
    pub job_id: Option<String>,
    pub target: Option<Target>,
    pub phase: Phase,
    pub recorded_ms: u64,
    pub worker_active: bool,
    pub transcript: Option<String>,
    pub target_unavailable: bool,
    pub error: Option<&'static str>,
}
struct State {
    next_job: u64,
    view: View,
    controls: Controls,
    recording_started: Option<Instant>,
    closed: bool,
    model_selection: bool,
}
impl State {
    fn snapshot(&self) -> View {
        let mut view = self.view.clone();
        if view.phase == Phase::Recording {
            view.recorded_ms = self
                .recording_started
                .map_or(0, |time| time.elapsed().as_millis().min(60000) as u64);
        }
        view
    }
    fn revise(&mut self) {
        self.view.revision += 1;
    }
}
type Events = Arc<dyn Fn(View) + Send + Sync>;
pub struct Controller {
    events: Events,
    state: Arc<Mutex<State>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl Default for Controller {
    fn default() -> Self {
        Self::with_events(|_| {})
    }
}
impl Controller {
    pub fn with_events(events: impl Fn(View) + Send + Sync + 'static) -> Self {
        Self {
            events: Arc::new(events),
            state: Arc::new(Mutex::new(State {
                next_job: 0,
                view: View {
                    revision: 0,
                    job_id: None,
                    target: None,
                    phase: Phase::Idle,
                    recorded_ms: 0,
                    worker_active: false,
                    transcript: None,
                    target_unavailable: false,
                    error: None,
                },
                controls: Controls::default(),
                recording_started: None,
                closed: false,
                model_selection: false,
            })),
            worker: Mutex::new(None),
        }
    }
}
impl Controller {
    pub fn snapshot(&self) -> View {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .snapshot()
    }
    #[cfg(test)]
    pub fn start(
        &self,
        target: VerifiedTarget,
        work: impl FnOnce(Job) -> Result<String, &'static str> + Send + 'static,
    ) -> Result<View, &'static str> {
        self.start_checked(target, work, || true)
    }
    pub fn start_checked(
        &self,
        target: VerifiedTarget,
        work: impl FnOnce(Job) -> Result<String, &'static str> + Send + 'static,
        target_available: impl FnOnce() -> bool + Send + 'static,
    ) -> Result<View, &'static str> {
        let mut handle = self.worker.lock().map_err(|_| "voice_unavailable")?;
        {
            let state = self.state.lock().map_err(|_| "voice_unavailable")?;
            if state.closed {
                return Err("voice_unavailable");
            }
            if state.view.worker_active {
                return Err("voice_busy");
            }
        }
        // A settled worker has released all capture/inference resources. Join
        // outside the state lock before assigning the next job.
        if let Some(previous) = handle.take() {
            let _ = previous.join();
        }
        let controls = Controls::default();
        let (initial, id) = {
            let mut state = self.state.lock().map_err(|_| "voice_unavailable")?;
            if state.model_selection {
                return Err("voice_model_busy");
            }
            let next = state.next_job.checked_add(1).ok_or("voice_job_exhausted")?;
            state.next_job = next;
            let id = next.to_string();
            state.view = View {
                revision: state.view.revision + 1,
                job_id: Some(id.clone()),
                target: Some(target.0),
                phase: Phase::RequestingPermission,
                recorded_ms: 0,
                worker_active: true,
                transcript: None,
                target_unavailable: false,
                error: None,
            };
            state.controls = controls.clone();
            state.recording_started = None;
            (state.snapshot(), id)
        };
        let job = Job {
            events: self.events.clone(),
            state: self.state.clone(),
            id,
            controls,
        };
        let completion = job.clone();
        match thread::Builder::new()
            .name("island-voice".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(job)))
                    .unwrap_or(Err("voice_worker_failed"));
                let available = result.is_ok()
                    && std::panic::catch_unwind(std::panic::AssertUnwindSafe(target_available))
                        .unwrap_or(false);
                completion.finish(result, available);
            }) {
            Ok(thread) => *handle = Some(thread),
            Err(_) => {
                let mut state = self.state.lock().map_err(|_| "voice_unavailable")?;
                state.view.worker_active = false;
                state.view.phase = Phase::Error;
                state.view.error = Some("voice_unavailable");
                state.revise();
                let view = state.snapshot();
                drop(state);
                drop(handle);
                (self.events)(view);
                return Err("voice_unavailable");
            }
        }
        drop(handle);
        (self.events)(initial.clone());
        Ok(initial)
    }
    pub fn stop(&self, id: &str) -> Result<View, &'static str> {
        let mut state = self.state.lock().map_err(|_| "voice_unavailable")?;
        if state.view.job_id.as_deref() != Some(id) {
            return Err("voice_job_stale");
        }
        state.controls.stop.store(true, Ordering::Release);
        if state.view.phase == Phase::RequestingPermission {
            state.controls.cancel.store(true, Ordering::Release);
            state.view.phase = Phase::Cancelled;
            state.revise();
        }
        let view = state.snapshot();
        drop(state);
        (self.events)(view.clone());
        Ok(view)
    }
    pub fn cancel(&self, id: &str) -> Result<View, &'static str> {
        let mut state = self.state.lock().map_err(|_| "voice_unavailable")?;
        if state.view.job_id.as_deref() != Some(id) {
            return Err("voice_job_stale");
        }
        state.controls.cancel.store(true, Ordering::Release);
        state.controls.stop.store(true, Ordering::Release);
        state.view.recorded_ms = state.snapshot().recorded_ms;
        state.view.phase = Phase::Cancelled;
        state.view.transcript = None;
        state.view.error = None;
        state.revise();
        let view = state.snapshot();
        drop(state);
        (self.events)(view.clone());
        Ok(view)
    }
    pub fn shutdown(&self) {
        let mut handle = self
            .worker
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.closed = true;
            state.controls.cancel.store(true, Ordering::Release);
            state.controls.stop.store(true, Ordering::Release);
        }
        if let Some(worker) = handle.take() {
            let _ = worker.join();
        }
    }
    pub fn begin_model_selection(&self) -> Result<ModelSelection, &'static str> {
        let mut state = self.state.lock().map_err(|_| "voice_unavailable")?;
        if state.closed {
            return Err("voice_unavailable");
        }
        if state.view.worker_active || state.model_selection {
            return Err("voice_busy");
        }
        state.model_selection = true;
        Ok(ModelSelection(self.state.clone()))
    }
    pub fn begin_model_removal(&self) -> Result<ModelSelection, &'static str> {
        let mut handle = self.worker.lock().map_err(|_| "voice_unavailable")?;
        let selection = {
            let mut state = self.state.lock().map_err(|_| "voice_unavailable")?;
            if state.closed {
                return Err("voice_unavailable");
            }
            if state.model_selection {
                return Err("voice_model_busy");
            }
            state.model_selection = true;
            if state.view.worker_active {
                state.controls.cancel.store(true, Ordering::Release);
                state.controls.stop.store(true, Ordering::Release);
                state.view.recorded_ms = state.snapshot().recorded_ms;
                state.view.phase = Phase::Cancelled;
                state.view.transcript = None;
                state.view.error = None;
                state.revise();
            }
            ModelSelection(self.state.clone())
        };
        if let Some(worker) = handle.take() {
            let _ = worker.join();
        }
        Ok(selection)
    }
}

pub struct ModelSelection(Arc<Mutex<State>>);
impl Drop for ModelSelection {
    fn drop(&mut self) {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .model_selection = false;
    }
}

/// The native backend is constructed and used entirely on the owned worker.
pub fn native_work(
    state_directory: std::path::PathBuf,
    before_permission: impl FnOnce() -> Result<(), &'static str> + Send,
    level: impl Fn(&str, f32) + Send + 'static,
) -> impl FnOnce(Job) -> Result<String, &'static str> + Send {
    move |job| {
        let path = super::model::read(&state_directory)?;
        before_permission()?;
        let controls = job.controls();
        let source = super::native_capture::Native::prepare(&super::permission::NativePermission {
            cancelled: &controls.cancel,
        })
        .map_err(|error| error.code())?;
        job.recording()?;
        let id = job.id.clone();
        let audio = super::capture::run_with_levels(&source, controls.clone(), |value| {
            level(&id, value);
        })
        .map_err(|error| error.code())?;
        drop(source);
        let recorded_ms = audio.samples.len() as u64 * 1000 / audio.sample_rate as u64;
        job.transcribing(recorded_ms)?;
        super::transcribe::run(&path, audio, &controls.cancel)
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[derive(Clone)]
pub struct Job {
    events: Events,
    state: Arc<Mutex<State>>,
    id: String,
    controls: Controls,
}
impl Job {
    pub fn controls(&self) -> Controls {
        self.controls.clone()
    }
    pub fn recording(&self) -> Result<(), &'static str> {
        let mut state = self.state.lock().map_err(|_| "voice_unavailable")?;
        self.check(&state)?;
        if self.controls.stop.load(Ordering::Acquire) {
            return Err("voice_cancelled");
        }
        if state.view.phase != Phase::RequestingPermission {
            return Err("voice_invalid_transition");
        }
        state.recording_started = Some(Instant::now());
        state.view.phase = Phase::Recording;
        state.revise();
        let view = state.snapshot();
        drop(state);
        (self.events)(view);
        Ok(())
    }
    pub fn transcribing(&self, recorded_ms: u64) -> Result<(), &'static str> {
        let mut state = self.state.lock().map_err(|_| "voice_unavailable")?;
        self.check(&state)?;
        if state.view.phase != Phase::Recording {
            return Err("voice_invalid_transition");
        }
        state.view.recorded_ms = recorded_ms.min(60000);
        state.recording_started = None;
        state.view.phase = Phase::Transcribing;
        state.revise();
        let view = state.snapshot();
        drop(state);
        (self.events)(view);
        Ok(())
    }
    fn check(&self, state: &State) -> Result<(), &'static str> {
        if state.closed
            || state.view.job_id.as_deref() != Some(&self.id)
            || self.controls.cancel.load(Ordering::Acquire)
        {
            Err("voice_cancelled")
        } else {
            Ok(())
        }
    }
    fn finish(&self, result: Result<String, &'static str>, target_available: bool) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.view.job_id.as_deref() != Some(&self.id) {
            return;
        }
        state.view.worker_active = false;
        state.view.recorded_ms = state.snapshot().recorded_ms;
        state.view.transcript = None;
        state.view.error = None;
        if self.check(&state).is_err() || result == Err("voice_cancelled") {
            state.view.phase = Phase::Cancelled;
        } else {
            match result {
                Ok(_) if state.view.phase != Phase::Transcribing => {
                    state.view.phase = Phase::Error;
                    state.view.error = Some("voice_invalid_transition");
                }
                Ok(text) if !text.trim().is_empty() => {
                    state.view.phase = Phase::Ready;
                    state.view.transcript = Some(text);
                    state.view.target_unavailable = !target_available;
                }
                Ok(_) => {
                    state.view.phase = Phase::Error;
                    state.view.error = Some("no_speech");
                }
                Err(error) => {
                    state.view.phase = Phase::Error;
                    state.view.error = Some(error);
                }
            }
        }
        state.revise();
        let view = state.snapshot();
        drop(state);
        (self.events)(view);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{atomic::AtomicBool, mpsc},
        time::Duration,
    };
    fn target(name: &str) -> VerifiedTarget {
        VerifiedTarget(Target {
            session_id: name.into(),
            session_instance_id: SessionInstanceId(format!("instance-{name}")),
            daemon_epoch: DaemonEpoch("epoch".into()),
        })
    }
    fn wait_for(controller: &Controller, phase: Phase, active: bool) -> View {
        let end = Instant::now() + Duration::from_secs(3);
        loop {
            let view = controller.snapshot();
            if view.phase == phase && view.worker_active == active {
                return view;
            }
            assert!(Instant::now() < end, "voice phase did not settle");
            thread::sleep(Duration::from_millis(2));
        }
    }
    #[test]
    fn one_worker_stop_is_idempotent_and_ready_preserves_original_target() {
        let controller = Controller::default();
        let (send, receive) = mpsc::channel();
        let initial = controller
            .start(target("child"), move |job| {
                job.recording()?;
                send.send(job.clone()).unwrap();
                let controls = job.controls();
                while !controls.stop.load(Ordering::Acquire)
                    && !controls.cancel.load(Ordering::Acquire)
                {
                    thread::sleep(Duration::from_millis(2));
                }
                job.transcribing(1234)?;
                Ok("texto revisável".into())
            })
            .unwrap();
        let old_job = receive.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(
            controller
                .start(target("root"), |_| Ok("other".into()))
                .err(),
            Some("voice_busy")
        );
        let id = initial.job_id.unwrap();
        controller.stop(&id).unwrap();
        controller.stop(&id).unwrap();
        let ready = wait_for(&controller, Phase::Ready, false);
        assert_eq!(ready.target.unwrap().session_id, "child");
        assert_eq!(ready.transcript.as_deref(), Some("texto revisável"));
        assert_eq!(ready.recorded_ms, 1234);
        assert!(ready.revision > initial.revision);
        let next = controller
            .start(target("root"), |job| {
                job.recording()?;
                job.transcribing(0)?;
                Ok("new".into())
            })
            .unwrap();
        assert_ne!(next.job_id.as_deref(), Some(id.as_str()));
        assert!(next.job_id.as_ref().unwrap().parse::<u64>().unwrap() > id.parse::<u64>().unwrap());
        assert_eq!(old_job.recording(), Err("voice_cancelled"));
        assert_eq!(controller.cancel(&id).err(), Some("voice_job_stale"));
        assert_eq!(
            wait_for(&controller, Phase::Ready, false)
                .transcript
                .as_deref(),
            Some("new")
        );
    }
    #[test]
    fn cancel_discards_late_result_and_shutdown_joins_worker() {
        let controller = Controller::default();
        let returned = Arc::new(AtomicBool::new(false));
        let marker = returned.clone();
        let initial = controller
            .start(target("one"), move |job| {
                job.recording()?;
                job.transcribing(80)?;
                while !job.controls.cancel.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(2));
                }
                marker.store(true, Ordering::Release);
                Ok("late private text".into())
            })
            .unwrap();
        wait_for(&controller, Phase::Transcribing, true);
        controller
            .cancel(initial.job_id.as_deref().unwrap())
            .unwrap();
        let cancelled = wait_for(&controller, Phase::Cancelled, false);
        assert!(returned.load(Ordering::Acquire));
        assert!(cancelled.transcript.is_none());
        assert!(cancelled.error.is_none());
        let dropped = Arc::new(AtomicBool::new(false));
        let marker = dropped.clone();
        let (send, receive) = mpsc::channel();
        controller
            .start(target("two"), move |job| {
                send.send(()).unwrap();
                while !job.controls.cancel.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(2));
                }
                marker.store(true, Ordering::Release);
                Err("voice_cancelled")
            })
            .unwrap();
        receive.recv_timeout(Duration::from_secs(3)).unwrap();
        drop(controller);
        assert!(dropped.load(Ordering::Acquire));
    }
    #[test]
    fn pending_permission_stop_and_worker_failure_release_the_global_slot() {
        let controller = Controller::default();
        let initial = controller
            .start(target("one"), |job| {
                while !job.controls.cancel.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(2));
                }
                job.recording()?;
                Ok("must not appear".into())
            })
            .unwrap();
        controller.stop(initial.job_id.as_deref().unwrap()).unwrap();
        wait_for(&controller, Phase::Cancelled, false);
        controller
            .start(target("one"), |_| panic!("fixture worker failure"))
            .unwrap();
        assert_eq!(
            wait_for(&controller, Phase::Error, false).error,
            Some("voice_worker_failed")
        );
        controller
            .start(target("one"), |job| {
                job.recording()?;
                job.transcribing(0)?;
                Ok("  ".into())
            })
            .unwrap();
        assert_eq!(
            wait_for(&controller, Phase::Error, false).error,
            Some("no_speech")
        );
    }
    #[test]
    fn target_validation_uses_child_identity_and_epoch_not_group_or_cwd() {
        use open_island_core::{session::Session, ui_state::UiSession};
        let child = target("child").0;
        let mut session = Session::new("opencode", "/same", 1, "terminal");
        session.id = child.session_id.clone();
        let mut snapshot = UiSnapshot {
            schema_version: 1,
            discovering: false,
            daemon_epoch: child.daemon_epoch.clone(),
            publication_revision: 1,
            sessions: vec![],
            child_sessions: vec![UiSession {
                session,
                session_instance_id: Some(child.session_instance_id.clone()),
            }],
            approvals: vec![],
            questions: vec![],
            message_deliveries: vec![],
            config: serde_json::json!({}),
            usage: Default::default(),
            update: None,
            quiet_scenes: Default::default(),
        };
        assert!(VerifiedTarget::from_snapshot(child.clone(), &snapshot).is_ok());
        let mut wrong = child.clone();
        wrong.session_instance_id = SessionInstanceId("parent-instance".into());
        assert_eq!(
            VerifiedTarget::from_snapshot(wrong, &snapshot).err(),
            Some("voice_target_stale")
        );
        snapshot.daemon_epoch = DaemonEpoch("restarted".into());
        assert_eq!(
            VerifiedTarget::from_snapshot(child, &snapshot).err(),
            Some("voice_target_stale")
        );
    }
    #[test]
    fn model_dialog_lease_and_voice_job_exclude_each_other_and_release_on_drop() {
        let controller = Controller::default();
        let selection = controller.begin_model_selection().unwrap();
        assert_eq!(
            controller.begin_model_selection().err().map(|_| "busy"),
            Some("busy")
        );
        assert_eq!(
            controller
                .start(target("one"), |_| Ok("unused".into()))
                .err(),
            Some("voice_model_busy")
        );
        drop(selection);
        let initial = controller
            .start(target("one"), |job| {
                while !job.controls.cancel.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(2));
                }
                Err("voice_cancelled")
            })
            .unwrap();
        assert_eq!(
            controller.begin_model_selection().err().map(|_| "busy"),
            Some("busy")
        );
        controller
            .cancel(initial.job_id.as_deref().unwrap())
            .unwrap();
        wait_for(&controller, Phase::Cancelled, false);
        assert!(controller.begin_model_selection().is_ok());
    }
    #[test]
    fn model_removal_cancels_and_joins_before_configuration_can_change() {
        let controller = Controller::default();
        let released = Arc::new(AtomicBool::new(false));
        let marker = released.clone();
        controller
            .start(target("one"), move |job| {
                job.recording()?;
                job.transcribing(42)?;
                while !job.controls.cancel.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(2));
                }
                marker.store(true, Ordering::Release);
                Ok("late result".into())
            })
            .unwrap();
        wait_for(&controller, Phase::Transcribing, true);
        let removal = controller.begin_model_removal().unwrap();
        assert!(released.load(Ordering::Acquire));
        let view = controller.snapshot();
        assert_eq!(view.phase, Phase::Cancelled);
        assert!(!view.worker_active);
        assert!(view.transcript.is_none());
        assert!(controller.begin_model_selection().is_err());
        assert!(controller.begin_model_removal().is_err());
        assert_eq!(
            controller.start(target("two"), |_| unreachable!()).err(),
            Some("voice_model_busy")
        );
        drop(removal);
        assert!(controller.begin_model_selection().is_ok());
    }
    #[test]
    fn successful_backend_cannot_skip_recording_and_transcription_states() {
        let controller = Controller::default();
        controller
            .start(target("one"), |_| Ok("invalid shortcut".into()))
            .unwrap();
        let view = wait_for(&controller, Phase::Error, false);
        assert_eq!(view.error, Some("voice_invalid_transition"));
        assert!(view.transcript.is_none());
    }
    #[test]
    fn unavailable_return_target_preserves_text_and_cancellation_wins_during_revalidation() {
        for cancel in [false, true] {
            let controller = Controller::default();
            let (entered, receiving) = mpsc::channel();
            let (release, released) = mpsc::channel();
            let initial = controller
                .start_checked(
                    target("original"),
                    |job| {
                        job.recording()?;
                        job.transcribing(500)?;
                        Ok("preservar resultado".into())
                    },
                    move || {
                        entered.send(()).unwrap();
                        released.recv_timeout(Duration::from_secs(3)).unwrap();
                        false
                    },
                )
                .unwrap();
            receiving.recv_timeout(Duration::from_secs(3)).unwrap();
            assert_eq!(controller.snapshot().phase, Phase::Transcribing);
            if cancel {
                controller
                    .cancel(initial.job_id.as_deref().unwrap())
                    .unwrap();
            }
            release.send(()).unwrap();
            let view = wait_for(
                &controller,
                if cancel {
                    Phase::Cancelled
                } else {
                    Phase::Ready
                },
                false,
            );
            assert_eq!(view.target.unwrap().session_id, "original");
            if cancel {
                assert!(view.transcript.is_none());
            } else {
                assert!(view.target_unavailable);
                assert_eq!(view.transcript.as_deref(), Some("preservar resultado"));
            }
        }
    }
    #[test]
    fn exhausted_job_counter_refuses_without_starting_or_changing_view() {
        let controller = Controller::default();
        controller.state.lock().unwrap().next_job = u64::MAX;
        let before = controller.snapshot();
        assert_eq!(
            controller.start(target("one"), |_| unreachable!()).err(),
            Some("voice_job_exhausted")
        );
        let after = controller.snapshot();
        assert_eq!(before.revision, after.revision);
        assert_eq!(before.job_id, after.job_id);
        assert!(!after.worker_active);
    }
    #[test]
    fn events_publish_transitions_without_holding_state_and_match_late_snapshot() {
        let state_slot = Arc::new(Mutex::new(None::<Arc<Mutex<State>>>));
        let slot = state_slot.clone();
        let (events, receiver) = mpsc::channel();
        let controller = Controller::with_events(move |view| {
            assert!(slot.lock().unwrap().as_ref().unwrap().try_lock().is_ok());
            events.send(view).unwrap();
        });
        *state_slot.lock().unwrap() = Some(controller.state.clone());
        let (release, released) = mpsc::channel();
        let initial = controller
            .start(target("one"), move |job| {
                released.recv_timeout(Duration::from_secs(3)).unwrap();
                job.recording()?;
                job.transcribing(456)?;
                Ok("texto local".into())
            })
            .unwrap();
        let first = receiver.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(first.phase, Phase::RequestingPermission);
        assert_eq!(first.revision, initial.revision);
        release.send(()).unwrap();
        for phase in [Phase::Recording, Phase::Transcribing, Phase::Ready] {
            let event = receiver.recv_timeout(Duration::from_secs(3)).unwrap();
            assert_eq!(event.phase, phase);
            assert_eq!(event.job_id, initial.job_id);
            if phase == Phase::Ready {
                assert!(!event.worker_active);
                let snapshot = controller.snapshot();
                assert_eq!(snapshot.revision, event.revision);
                assert_eq!(snapshot.transcript, event.transcript);
            }
        }
        controller
            .cancel(initial.job_id.as_deref().unwrap())
            .unwrap();
        let cancelled = receiver.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(cancelled.phase, Phase::Cancelled);
        assert!(cancelled.transcript.is_none());
    }
}
