use crate::broadcast::{broadcast, broadcast_sessions, make_broadcast};
use crate::daemon_config::{config_stamp, reload_config, VERSION};
use crate::notifications::{
    lifecycle::{self, DaemonContext},
    shutdown::{self, ShutdownFlag},
};
use crate::scenes::{self, SceneSource, SystemScenes};
use crate::server::wire::event_message;
use crate::sound::DndProbe;
use crate::update::{self, cache::CachedCheck};
use crate::usage;
use open_island_core::{
    config::{self, SoundEvent},
    discovery,
    jump::JumpPlanner,
    protocol::{EventData, QuietScenes, UpdateAvailable},
    runner::SystemRunner,
    send,
    session::{QueuedMessage, Session},
    store::ReminderScopes,
};
use serde_json::{json, Value};
use std::{
    env,
    path::{Path, PathBuf},
    sync::Arc,
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
    let config_file = config::path();
    let mut stamp = config_file.as_deref().and_then(config_stamp);
    loop {
        if !shutdown::wait_for_interval(&flag, Duration::from_millis(interval)) {
            return;
        }
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
        let sessions = discovery::scan();
        let (mut reconciled, due) = state
            .lock()
            .map(|mut state| {
                let reconciled = state.store.snapshot(&sessions);
                let due = state.store.take_due_messages(&reconciled);
                (reconciled, due)
            })
            .unwrap_or_default();
        if !due.is_empty() {
            for (session, message) in &due {
                if let Err(error) = deliver_message(session, message) {
                    eprintln!(
                        "open-islandd: message {} to {}: {error}",
                        message.id, session.id
                    );
                }
            }
            reconciled = state
                .lock()
                .map(|mut state| state.store.snapshot(&sessions))
                .unwrap_or_default();
        }
        announce_idle_reminders(&ctx);
        if previous.as_ref() != Some(&reconciled) {
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
    let text = send::normalize(text);
    if text.trim().is_empty() {
        return Err("empty message".to_owned());
    }
    open_island_core::input_bridge::validate_text(&text)?;
    let processes = discovery::scan();
    let (message, due) = {
        let mut state = ctx
            .state
            .lock()
            .map_err(|_| "daemon state unavailable".to_owned())?;
        let sessions = state.store.snapshot(&processes);
        let session = sessions
            .iter()
            .find(|session| session.id == session_id)
            .cloned()
            .ok_or_else(|| format!("session '{session_id}' not found"))?;
        if let Some(code) = session.send_blocked.as_deref() {
            return Err(code.to_owned());
        }
        let message =
            state
                .store
                .enqueue_message(&session.id, text, usage::now_ms(), session.attention);
        let due = state
            .store
            .take_due_messages(std::slice::from_ref(&session))
            .into_iter()
            .find(|(_, candidate)| candidate.id == message.id)
            .map(|(_, candidate)| candidate);
        (message, due)
    };
    let delivered = match due {
        Some(message) => {
            let target = {
                let processes = discovery::scan();
                ctx.state
                    .lock()
                    .map_err(|_| "daemon state unavailable".to_owned())?
                    .store
                    .snapshot(&processes)
                    .into_iter()
                    .find(|candidate| candidate.id == session_id)
                    .ok_or_else(|| format!("session '{session_id}' not found"))?
            };
            deliver_message(&target, &message)?;
            true
        }
        None => false,
    };
    broadcast_sessions(ctx);
    Ok(json!({"message_id": message.id, "delivered": delivered}))
}

pub fn deliver_message(session: &Session, message: &QueuedMessage) -> Result<(), String> {
    let host = discovery::terminal_for_session(session)
        .ok_or_else(|| format!("session '{}' is no longer running", session.id))?;
    if let Some(socket) = host.env.get(open_island_core::input_bridge::ENV) {
        return open_island_core::input_bridge::send(
            std::path::Path::new(socket),
            host.agent_pid,
            &message.text,
        );
    }
    let runner = SystemRunner;
    let plan =
        JumpPlanner::new(open_island_core::resolvers::default_resolvers()).plan(&host, &runner);
    let runtime_dir = env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    let channel = send::channel_for(&host, &plan.steps, runtime_dir.as_deref(), &|path| {
        path.exists()
    })
    .map_err(|blocked| blocked.code().to_owned())?;
    send::execute(&send::plan(&channel, &message.text), &runner)
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
