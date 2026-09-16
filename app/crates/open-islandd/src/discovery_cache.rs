use open_island_core::{
    discovery::{self, Observation},
    session::Session,
};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Cached {
    pub observation: Arc<Observation>,
    pub ready: bool,
    pub generation: u64,
}
impl Cached {
    pub fn identities_changed(&self, previous: &Self, hooks: &[Session]) -> bool {
        hooks
            .iter()
            .chain(&self.observation.sessions)
            .any(|session| {
                self.observation.births.get(&session.pid)
                    != previous.observation.births.get(&session.pid)
            })
    }
}
pub struct DiscoveryCache {
    current: Mutex<Cached>,
    scanning: Mutex<()>,
}
impl Default for DiscoveryCache {
    fn default() -> Self {
        Self {
            current: Mutex::new(Cached {
                observation: Arc::new(Observation::empty()),
                ready: false,
                generation: 0,
            }),
            scanning: Mutex::new(()),
        }
    }
}
impl DiscoveryCache {
    pub fn read(&self) -> Cached {
        self.current
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub fn refresh(&self, hooks: &[Session]) -> Result<Cached, &'static str> {
        self.refresh_with(|| discovery::observe(hooks))
    }
    pub fn refresh_with(
        &self,
        provider: impl FnOnce() -> Result<Observation, &'static str>,
    ) -> Result<Cached, &'static str> {
        let _scan = self.scanning.try_lock().map_err(|_| "discovery_busy")?;
        let mut observation = provider()?;
        observation.complete = true;
        let mut current = self.current.lock().map_err(|_| "discovery_unavailable")?;
        current.generation = current.generation.saturating_add(1);
        current.observation = Arc::new(observation);
        current.ready = true;
        Ok(current.clone())
    }
}

pub fn sessions(
    ctx: &crate::notifications::lifecycle::DaemonContext,
) -> Result<Vec<Session>, String> {
    let cached = ctx.discovery.read();
    let mut state = ctx.state.lock().map_err(|_| "daemon_unavailable")?;
    Ok(state.store.snapshot_cached(&cached.observation, true).0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notifications::lifecycle::{DaemonContext, DaemonState};
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
        thread,
        time::Duration,
    };

    fn context() -> DaemonContext {
        DaemonContext {
            discovery: Arc::new(DiscoveryCache::default()),
            admission: Arc::new(crate::server::admission::Admission::default()),
            snapshots: Arc::new(crate::ui_state::UiSnapshots::new().unwrap()),
            state: Arc::new(Mutex::new(DaemonState {
                publication_revision: 0,
                no_island: 0,
                store: open_island_core::store::SessionStore::new(),
                pending: Default::default(),
                pending_questions: Default::default(),
                subscribers: Vec::new(),
                approval_generation: 0,
                usage: Default::default(),
                usage_watch: Default::default(),
                scenes: Default::default(),
                update: None,
            })),
            config: Default::default(),
            sound: crate::sound::SoundPlayer::silent(),
            messages: Arc::new(crate::message_executor::MessageExecutor::new().unwrap()),
            scan_wakeup: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
    #[test]
    fn detached_discard_only_removes_the_exact_old_record_and_never_cancels_sending() {
        use open_island_core::message_delivery::{DeliveryIdentity, SessionInstanceId};
        let ctx = context();
        let old = DeliveryIdentity {
            daemon_epoch: ctx.messages.epoch.clone(),
            session_instance_id: SessionInstanceId("old".into()),
        };
        let new = DeliveryIdentity {
            session_instance_id: SessionInstanceId("new".into()),
            ..old.clone()
        };
        let (first, second) = {
            let mut state = ctx.state.lock().unwrap();
            let first = state
                .store
                .deliveries
                .admit(
                    "reused",
                    "old text".into(),
                    0,
                    true,
                    Some(old.clone()),
                    None,
                )
                .unwrap();
            let second = state
                .store
                .deliveries
                .admit(
                    "reused",
                    "new text".into(),
                    0,
                    true,
                    Some(new.clone()),
                    None,
                )
                .unwrap();
            (first.message_id, second.message_id)
        };
        let request = |message_id, identity| crate::server::guarded_actions::Cancel {
            id: "reused".into(),
            message_id,
            identity,
        };
        assert_eq!(
            crate::server::guarded_actions::cancel(&ctx, request(second, old.clone())).unwrap_err(),
            "stale_session"
        );
        crate::server::guarded_actions::cancel(&ctx, request(first, old)).unwrap();
        {
            let mut state = ctx.state.lock().unwrap();
            assert_eq!(state.store.deliveries.snapshot().len(), 1);
            assert_eq!(state.store.deliveries.snapshot()[0].text, "new text");
            state.store.deliveries.reserve("reused").unwrap().unwrap();
        }
        assert_eq!(
            crate::server::guarded_actions::cancel(&ctx, request(second, new)).unwrap_err(),
            "delivery_in_progress"
        );
        assert_eq!(
            ctx.state.lock().unwrap().store.deliveries.snapshot().len(),
            1
        );
    }
    #[test]
    fn one_scan_in_flight_reads_do_not_wait_and_errors_preserve_last_observation() {
        let cache = Arc::new(DiscoveryCache::default());
        let first = cache.refresh_with(|| Ok(Observation::empty())).unwrap();
        let (entered, waiting) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let other = cache.clone();
        let worker = thread::spawn(move || {
            other
                .refresh_with(|| {
                    entered.send(()).unwrap();
                    resume.recv_timeout(Duration::from_secs(3)).unwrap();
                    Err("fixture_scan_failed")
                })
                .err()
        });
        waiting.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(
            cache
                .refresh_with(|| panic!("second scan must not start"))
                .err(),
            Some("discovery_busy")
        );
        for _ in 0..100 {
            assert!(Arc::ptr_eq(&first.observation, &cache.read().observation));
        }
        release.send(()).unwrap();
        assert_eq!(worker.join().unwrap(), Some("fixture_scan_failed"));
        assert_eq!(cache.read().generation, 1);
        assert!(Arc::ptr_eq(&first.observation, &cache.read().observation));
    }

    #[test]
    fn repeated_publications_use_cache_and_hooks_appear_before_the_next_scan() {
        use open_island_core::{
            process::ProcessBirthIdentity,
            protocol::{HookEvent, HookEventKind},
        };
        let ctx = context();
        assert!(crate::ui_state::freeze(&ctx).unwrap().discovering);
        let calls = AtomicUsize::new(0);
        let current = ctx
            .discovery
            .refresh_with(|| {
                calls.fetch_add(1, Ordering::SeqCst);
                let mut observation = Observation::empty();
                observation
                    .sessions
                    .push(Session::new("claude", "/fixture", 424242, "kitty"));
                observation.alive.insert(424242);
                observation.births.insert(424242, ProcessBirthIdentity(123));
                Ok(observation)
            })
            .unwrap();
        let mut hook = HookEvent::new("claude", "hook", HookEventKind::UserPromptSubmit);
        hook.pid = Some(424242);
        hook.cwd = Some("/fixture".into());
        hook.prompt = Some("cached hook update".into());
        ctx.state.lock().unwrap().store.apply_hook_event(hook);
        for _ in 0..100 {
            crate::broadcast::broadcast_sessions(&ctx);
            assert_eq!(
                sessions(&ctx).unwrap()[0].name.as_deref(),
                Some("cached hook update")
            );
            let snapshot = crate::ui_state::freeze(&ctx).unwrap();
            assert!(!snapshot.discovering);
            assert_eq!(
                snapshot.sessions[0].session.name.as_deref(),
                Some("cached hook update")
            );
            assert!(snapshot.sessions[0].session_instance_id.is_some());
            assert!(Arc::ptr_eq(
                &current.observation,
                &ctx.discovery.read().observation
            ));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(ctx.state.lock().unwrap().publication_revision, 100);
    }

    #[test]
    fn new_hook_during_a_scan_survives_the_older_table_until_the_next_observation() {
        use open_island_core::protocol::{HookEvent, HookEventKind};
        let ctx = context();
        let observation = Observation::empty();
        let mut hook = HookEvent::new("claude", "arrived", HookEventKind::SessionStart);
        hook.pid = Some(424243);
        ctx.state.lock().unwrap().store.apply_hook_event(hook);
        ctx.discovery.refresh_with(|| Ok(observation)).unwrap();
        assert_eq!(crate::ui_state::freeze(&ctx).unwrap().sessions.len(), 1);
        ctx.discovery
            .refresh_with(|| Ok(Observation::empty()))
            .unwrap();
        assert!(crate::ui_state::freeze(&ctx).unwrap().sessions.is_empty());
    }

    #[test]
    fn recycled_pid_changes_publication_identity_even_with_identical_session_fields() {
        use open_island_core::process::ProcessBirthIdentity;
        let cache = DiscoveryCache::default();
        let mut observation = Observation::empty();
        observation
            .sessions
            .push(Session::new("codex", "/fixture", 42, "kitty"));
        observation.births.insert(42, ProcessBirthIdentity(1));
        let before = cache.refresh_with(|| Ok(observation.clone())).unwrap();
        observation.births.insert(42, ProcessBirthIdentity(2));
        let after = cache.refresh_with(|| Ok(observation)).unwrap();
        assert_eq!(before.observation.sessions, after.observation.sessions);
        assert!(after.identities_changed(&before, &[]));
    }

    #[test]
    fn stale_cached_process_cannot_admit_a_guarded_send_or_approval() {
        use open_island_core::{
            message_delivery::{ClientSubmissionId, DeliveryIdentity},
            process,
            protocol::ApprovalDecision,
        };
        let ctx = context();
        let pid = std::process::id();
        let old_birth =
            process::ProcessBirthIdentity(process::birth_identity(pid).unwrap().0.wrapping_add(1));
        let mut observation = Observation::empty();
        let mut session = Session::new("codex", "/fixture", pid, "kitty");
        session.send_channel = Some("tmux".into());
        observation.sessions.push(session.clone());
        observation.births.insert(pid, old_birth);
        observation.alive.insert(pid);
        ctx.discovery.refresh_with(|| Ok(observation)).unwrap();
        let identity: DeliveryIdentity =
            crate::message_executor::identity(&session, old_birth, &ctx.messages.epoch);
        assert_eq!(
            crate::poll::send_message_guarded(
                &ctx,
                crate::server::guarded_actions::Send {
                    id: session.id,
                    text: "synthetic text".into(),
                    identity: identity.clone(),
                    client_submission_id: ClientSubmissionId("fixture".into()),
                }
            )
            .unwrap_err(),
            "stale_session"
        );
        assert_eq!(
            crate::server::guarded_actions::resolve(
                &ctx,
                crate::server::guarded_actions::Resolve {
                    approval_id: "fixture".into(),
                    pending_generation: 1,
                    decision: ApprovalDecision::Allow,
                    identity,
                }
            )
            .unwrap_err(),
            "stale_session"
        );
        assert!(ctx
            .state
            .lock()
            .unwrap()
            .store
            .deliveries
            .snapshot()
            .is_empty());
        assert_eq!(ctx.messages.active(), 0);
    }
}
