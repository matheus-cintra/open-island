use crate::broadcast::{broadcast, broadcast_sessions, make_broadcast};
use crate::daemon_config::{config_stamp, reload_config, VERSION};
use crate::notifications::{
    lifecycle::{self, DaemonContext, SharedState},
    shutdown::{self, ShutdownFlag},
};
use crate::scenes::{self, SceneSource, SystemScenes};
use crate::server::wire::event_message;
use crate::sound::DndProbe;
use crate::update::{self, cache::CachedCheck};
use crate::usage;
use open_island_core::{
    config::{self, SoundEvent},
    protocol::{EventData, QuietScenes, UpdateAvailable},
    send,
    store::ReminderScopes,
};
use serde_json::{json, Value};
use std::{
    env,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub fn poller(ctx: DaemonContext, flag: ShutdownFlag) {
    let mut dnd = DndProbe::new();
    let mut source = SystemScenes::new(Box::new(move || dnd.do_not_disturb()));
    let interval = env::var("OPEN_ISLAND_POLL_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(2000);
    let state = Arc::clone(&ctx.state);
    let mut previous = None;
    let mut first = true;
    let config_file = config::path();
    let mut stamp = config_file.as_deref().and_then(config_stamp);
    loop {
        if !first && !wait_for_tick(&flag, &ctx.scan_wakeup, Duration::from_millis(interval)) {
            return;
        }
        first = false;
        if let Some(path) = config_file.as_deref() {
            let current = config_stamp(path);
            if current != stamp {
                stamp = current;
                reload_config(&ctx, make_broadcast(Arc::clone(&state)));
            }
        }
        lifecycle::expire_questions(
            &ctx,
            std::time::Instant::now(),
            make_broadcast(Arc::clone(&state)),
        );
        announce_quiet_scenes(&ctx, &mut source);
        announce_idle_reminders(&ctx);
        if !island_connected(&state) && !deliveries_waiting(&state) {
            previous = None;
            continue;
        }
        let hooks = state
            .lock()
            .map(|state| state.store.discovery_targets())
            .unwrap_or_default();
        let before = ctx.discovery.read();
        let cached = ctx.discovery.refresh(&hooks).unwrap_or(before.clone());
        let reconciled = state
            .lock()
            .map(|mut state| {
                let reconciled = state.store.snapshot_cached(&cached.observation, true).0;
                state.store.observe_deliveries(&reconciled, usage::now_ms());
                reconciled
            })
            .unwrap_or_default();
        for session in &reconciled {
            let _ = ctx.messages.schedule(&state, session, false);
        }
        if (!before.ready && cached.ready)
            || cached.identities_changed(&before, &hooks)
            || previous.as_ref() != Some(&reconciled)
        {
            let delivered = broadcast(
                &state,
                event_message("sessions-updated", EventData::Sessions(reconciled.clone())),
            );
            if delivered {
                previous = Some(reconciled);
            }
        }
    }
}

fn wait_for_tick(flag: &ShutdownFlag, wakeup: &AtomicBool, duration: Duration) -> bool {
    if flag.is_requested() {
        return false;
    }
    let deadline = Instant::now() + duration;
    loop {
        if wakeup.swap(false, Ordering::AcqRel) {
            return true;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return true;
        }
        std::thread::sleep(std::cmp::min(shutdown::TICK, remaining));
        if flag.is_requested() {
            return false;
        }
    }
}

fn island_connected(state: &SharedState) -> bool {
    state
        .lock()
        .map(|state| {
            state
                .subscribers
                .iter()
                .any(|subscriber| subscriber.receives_actions && !subscriber.diagnostic_only)
        })
        .unwrap_or(true)
}

fn deliveries_waiting(state: &SharedState) -> bool {
    state
        .lock()
        .map(|state| state.store.deliveries.any_queued())
        .unwrap_or(true)
}

pub(crate) fn background_scanning(state: &SharedState) -> bool {
    island_connected(state) || deliveries_waiting(state)
}

pub(crate) fn refresh_discovery_now(ctx: &DaemonContext) {
    let hooks = ctx
        .state
        .lock()
        .map(|state| state.store.discovery_targets())
        .unwrap_or_default();
    let _ = ctx.discovery.refresh(&hooks);
}

pub fn usage_poller(ctx: DaemonContext, flag: ShutdownFlag) {
    let Some(home) = usage::home() else {
        return;
    };
    loop {
        let config = ctx.config.get();
        if !config.usage.show_limits {
            if !shutdown::wait_for_interval(&flag, config.usage.refresh_interval) {
                return;
            }
            continue;
        }
        let previous = ctx
            .state
            .lock()
            .map(|state| state.usage.clone())
            .unwrap_or_default();
        let report = usage::collect(&home, &config.usage, &previous, usage::now_ms());
        let mut crossed = Vec::new();
        if let Ok(mut state) = ctx.state.lock() {
            for entry in &report.providers {
                match entry.snapshot.as_ref() {
                    Some(snapshot) => {
                        if state.usage_watch.observe(
                            &entry.provider,
                            snapshot.peak_percent(),
                            config.usage.warn_threshold,
                        ) {
                            crossed.push(entry.provider.clone());
                        }
                    }
                    None => state.usage_watch.forget(&entry.provider),
                }
            }
            state.usage = report.clone();
        }
        for provider in &crossed {
            eprintln!("open-islandd: {provider} crossed the usage threshold");
            ctx.sound.play(SoundEvent::ContextLimit);
        }
        if previous != report {
            usage::save_cached(&report);
            broadcast(
                &ctx.state,
                event_message("usage-updated", EventData::UsageUpdated(report)),
            );
        }
        if !shutdown::wait_for_interval(&flag, config.usage.refresh_interval) {
            return;
        }
    }
}

pub fn update_poller(ctx: DaemonContext, flag: ShutdownFlag) {
    let cache_path = update::cache::path();
    loop {
        if ctx.config.get().updates.check_enabled {
            let _ = refresh_update(&ctx, cache_path.as_deref(), false);
        }
        if !shutdown::wait_for_interval(&flag, update::cache::TTL) {
            return;
        }
    }
}

pub fn refresh_update(
    ctx: &DaemonContext,
    cache_path: Option<&Path>,
    force: bool,
) -> Result<Option<UpdateAvailable>, String> {
    let now_ms = usage::now_ms();
    let cached = if force {
        None
    } else {
        cache_path.and_then(|path| update::cache::load(path, now_ms))
    };
    let tag = match cached {
        Some(cached) => cached.tag,
        None => {
            let tag = update::release::fetch()
                .and_then(|body| update::release::parse(&body))
                .map_err(|error| error.message().to_owned())?;
            if let Some(path) = cache_path {
                let check = CachedCheck {
                    tag: tag.clone(),
                    checked_at_ms: now_ms,
                };
                update::cache::save(path, &check);
            }
            tag
        }
    };
    if !update::is_newer(&tag, VERSION) {
        return Ok(None);
    }
    let notice = UpdateAvailable { version: tag };
    if let Ok(mut state) = ctx.state.lock() {
        if state.update.as_ref() == Some(&notice) {
            return Ok(Some(notice));
        }
        state.update = Some(notice.clone());
    }
    broadcast(
        &ctx.state,
        event_message(
            "update-available",
            EventData::UpdateAvailable(notice.clone()),
        ),
    );
    Ok(Some(notice))
}

pub fn send_message(ctx: &DaemonContext, session_id: &str, text: &str) -> Result<Value, String> {
    admit_message(ctx, session_id, text, None)
}
pub fn send_message_guarded(
    ctx: &DaemonContext,
    request: crate::server::guarded_actions::Send,
) -> Result<Value, String> {
    if request.identity.daemon_epoch != ctx.messages.epoch {
        return Err("stale_epoch".into());
    }
    if request.client_submission_id.0.is_empty() || request.client_submission_id.0.len() > 128 {
        return Err("invalid_submission_id".into());
    }
    admit_message(
        ctx,
        &request.id,
        &request.text,
        Some((&request.identity, request.client_submission_id)),
    )
}
fn admit_message(
    ctx: &DaemonContext,
    session_id: &str,
    text: &str,
    guard: Option<(
        &open_island_core::message_delivery::DeliveryIdentity,
        open_island_core::message_delivery::ClientSubmissionId,
    )>,
) -> Result<Value, String> {
    let text = send::normalize(text);
    if text.trim().is_empty() {
        return Err("empty message".to_owned());
    }
    open_island_core::input_bridge::validate_text(&text)?;
    let session = if guard.is_some() {
        crate::ui_state::freeze(ctx)?
            .targets()
            .find(|target| target.session.id == session_id)
            .map(|target| target.session.clone())
            .ok_or("stale_session".to_owned())?
    } else {
        crate::discovery_cache::sessions(ctx)?
            .into_iter()
            .find(|session| session.id == session_id)
            .ok_or_else(|| format!("session '{session_id}' not found"))?
    };
    let birth =
        open_island_core::process::birth_identity(session.pid).ok_or(if guard.is_some() {
            "stale_session"
        } else {
            "target_changed"
        })?;
    let identity = crate::message_executor::identity(&session, birth, &ctx.messages.epoch);
    if guard
        .as_ref()
        .is_some_and(|(expected, _)| **expected != identity)
    {
        return Err("stale_session".into());
    }
    if let Some(code) = &session.send_blocked {
        return Err(code.clone());
    }
    let message = {
        let mut state = ctx.state.lock().map_err(|_| "daemon_unavailable")?;
        if let Some((expected, _)) = &guard {
            if expected.daemon_epoch != ctx.messages.epoch {
                return Err("stale_epoch".into());
            }
            if **expected != identity || !state.store.delivery_target_current(&session) {
                return Err("stale_session".into());
            }
        }
        if !state.store.delivery_target_current(&session) {
            return Err("target_changed".to_owned());
        }
        let stopped = !matches!(
            session.attention,
            Some(
                open_island_core::session::Attention::Working
                    | open_island_core::session::Attention::NeedsAttention
            )
        );
        state
            .store
            .deliveries
            .admit(
                &session.id,
                text,
                usage::now_ms(),
                stopped,
                Some(identity),
                guard.as_ref().map(|(_, id)| id.clone()),
            )
            .map_err(str::to_owned)?
    };
    let _ = ctx.messages.schedule(&ctx.state, &session, false);
    crate::message_executor::publish(&ctx.state);
    broadcast_sessions(ctx);
    Ok(json!({"message_id": message.message_id, "delivered": false}))
}

pub fn announce_idle_reminders(ctx: &DaemonContext) {
    let config = ctx.config.get();
    let after = config.notifications.idle_reminder_after;
    if after.is_zero() {
        return;
    }
    let scopes = ReminderScopes {
        needs_response: config.notifications.reminder_needs_response,
        completed_tasks: config.notifications.reminder_completed_tasks,
    };
    let reminders = ctx
        .state
        .lock()
        .map(|mut state| {
            state
                .store
                .take_idle_reminders(Instant::now(), after, scopes)
        })
        .unwrap_or_default();
    if reminders.is_empty() {
        return;
    }
    for _ in &reminders {
        ctx.sound.play(SoundEvent::IdleReminder);
    }
}

pub fn announce_quiet_scenes(ctx: &DaemonContext, source: &mut dyn SceneSource) {
    let config = ctx.config.get();
    let evaluated = scenes::evaluate(&config.filters, source);
    let next = QuietScenes {
        active: evaluated.active(),
        focus_mode: evaluated.focus_mode,
        screen_off: evaluated.screen_off,
    };
    ctx.sound.set_quiet_scene(next.active);
    let changed = ctx
        .state
        .lock()
        .map(|mut state| {
            let changed = state.scenes != next;
            state.scenes = next;
            changed
        })
        .unwrap_or(false);
    if !changed {
        return;
    }
    broadcast(
        &ctx.state,
        event_message("quiet-scenes", EventData::QuietScenes(next)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notifications::lifecycle::{DaemonState, Subscriber};
    use std::{os::unix::net::UnixStream, thread};

    fn shared_state(subscribers: Vec<Subscriber>) -> SharedState {
        Arc::new(std::sync::Mutex::new(DaemonState {
            publication_revision: 0,
            no_island: 0,
            store: open_island_core::store::SessionStore::new(),
            pending: Default::default(),
            pending_questions: Default::default(),
            subscribers,
            approval_generation: 0,
            usage: Default::default(),
            usage_watch: Default::default(),
            scenes: Default::default(),
            update: None,
        }))
    }

    fn subscriber(receives_actions: bool, diagnostic_only: bool) -> Subscriber {
        let (socket, _peer) = UnixStream::pair().unwrap();
        Subscriber {
            receives_actions,
            diagnostic_only,
            ui_epoch: None,
            connection_id: 1,
            sender: crate::server::outbox::Outbox::new(socket),
        }
    }

    #[test]
    fn wait_for_tick_returns_early_when_the_scan_is_woken() {
        let flag = ShutdownFlag::new();
        let wakeup = Arc::new(AtomicBool::new(false));
        let waker = Arc::clone(&wakeup);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            waker.store(true, Ordering::Release);
        });
        let started = Instant::now();
        assert!(wait_for_tick(&flag, &wakeup, Duration::from_secs(30)));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn wait_for_tick_reports_shutdown_instead_of_sleeping_the_whole_interval() {
        let flag = ShutdownFlag::new();
        flag.request();
        let wakeup = Arc::new(AtomicBool::new(false));
        assert!(!wait_for_tick(&flag, &wakeup, Duration::from_secs(30)));
    }

    #[test]
    fn wait_for_tick_consumes_the_wakeup_and_returns_immediately() {
        let flag = ShutdownFlag::new();
        let wakeup = Arc::new(AtomicBool::new(true));
        let started = Instant::now();
        assert!(wait_for_tick(&flag, &wakeup, Duration::from_secs(30)));
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(!wakeup.load(Ordering::Acquire));
    }

    #[test]
    fn hook_and_diagnostic_connections_do_not_keep_the_scan_running() {
        let state = shared_state(vec![
            subscriber(false, false),
            subscriber(true, true),
        ]);
        assert!(!island_connected(&state));
        assert!(!background_scanning(&state));
    }

    #[test]
    fn a_subscribed_island_keeps_the_scan_running() {
        let state = shared_state(vec![subscriber(true, false)]);
        assert!(island_connected(&state));
        assert!(background_scanning(&state));
    }

    #[test]
    fn queued_deliveries_keep_the_scan_running_without_an_island() {
        let state = shared_state(Vec::new());
        assert!(!island_connected(&state));
        {
            let mut state = state.lock().unwrap();
            state
                .store
                .deliveries
                .admit("claude:1", "mensagem".into(), 0, true, None, None)
                .unwrap();
        }
        assert!(deliveries_waiting(&state));
        assert!(background_scanning(&state));
    }

    #[test]
    fn a_poisoned_state_fails_safe_and_keeps_the_scan_running() {
        let state = shared_state(Vec::new());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = state.lock().unwrap();
            panic!("poison the state");
        }));
        assert!(result.is_err());
        assert!(island_connected(&state));
        assert!(background_scanning(&state));
    }
}
