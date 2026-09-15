use crate::{
    message_resources::{normalized, tmux_key, ResourceKey},
    message_runner::MessageRunner,
    notifications::lifecycle::DaemonContext,
};
use open_island_core::{
    jump::{JumpExecutor, JumpPlanner, JumpStep},
    message_delivery::DeliveryIdentity,
    process,
    runner::CommandRunner,
    session::Session,
};
use serde::Deserialize;
use std::{
    collections::HashMap,
    path::Path,
    process::Output,
    sync::{atomic::AtomicBool, Arc},
    time::Instant,
};

#[derive(Deserialize)]
pub struct Jump {
    pub id: String,
    #[serde(flatten)]
    pub identity: DeliveryIdentity,
}
struct CheckedRunner<'a> {
    runner: &'a MessageRunner,
    validate: &'a dyn Fn() -> Result<(), String>,
}
impl CommandRunner for CheckedRunner<'_> {
    fn run(&self, program: &str, args: &[&str]) -> Result<Output, String> {
        (self.validate)()?;
        self.runner.run_effect(program, args)
    }
}
fn validate(
    ctx: &DaemonContext,
    session: &Session,
    identity: &DeliveryIdentity,
) -> Result<(), String> {
    if identity.daemon_epoch != ctx.messages.epoch {
        return Err("stale_epoch".into());
    }
    let birth = process::birth_identity(session.pid).ok_or("stale_session")?;
    let actual = crate::message_executor::identity(session, birth, &ctx.messages.epoch);
    let state = ctx.state.lock().map_err(|_| "daemon_unavailable")?;
    if &actual != identity || !state.store.delivery_target_current(session) {
        return Err("stale_session".into());
    }
    Ok(())
}
pub fn jump(ctx: &DaemonContext, request: Jump) -> Result<serde_json::Value, String> {
    let started = Instant::now();
    if request.identity.daemon_epoch != ctx.messages.epoch {
        return Err("stale_epoch".into());
    }
    let snapshot = crate::ui_state::freeze(ctx)?;
    let target = snapshot
        .targets()
        .find(|s| s.session.id == request.id)
        .ok_or("stale_session")?;
    if target.session_instance_id.as_ref() != Some(&request.identity.session_instance_id) {
        return Err("stale_session".into());
    }
    let runner = MessageRunner::new(started, Arc::new(AtomicBool::new(false)));
    let check = || {
        runner.remaining()?;
        validate(ctx, &target.session, &request.identity)
    };
    check()?;
    let host = open_island_core::discovery::terminal_for_delivery(&target.session, || {
        runner.remaining().is_ok()
    })
    .ok_or("stale_session")?;
    let checked = CheckedRunner {
        runner: &runner,
        validate: &check,
    };
    let planner = JumpPlanner::new(open_island_core::resolvers::default_resolvers());
    let plan = planner.plan(&host, &checked);
    if plan.steps.is_empty() {
        return Err("host_unsupported".into());
    }
    let mut births: HashMap<_, _> = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            JumpStep::RaiseWindow { pid } | JumpStep::ActivateApp { pid } => {
                Some((*pid, process::birth_identity(*pid)))
            }
            _ => None,
        })
        .collect();
    births.insert(host.raise_pid, process::birth_identity(host.raise_pid));
    let mut keys = Vec::new();
    if let Some(socket) = host.env.get(open_island_core::input_bridge::ENV) {
        keys.push(ResourceKey::Bridge(normalized(Path::new(socket))));
    }
    for step in &plan.steps {
        match step {
            JumpStep::TmuxSelectPane { socket, .. }
            | JumpStep::TmuxSelectWindow { socket, .. }
            | JumpStep::TmuxSwitchClient { socket, .. } => {
                keys.push(tmux_key(socket.as_deref(), &host.env))
            }
            JumpStep::ZellijFocusPane { session, .. } => {
                keys.push(ResourceKey::Zellij(session.clone()))
            }
            _ => {}
        }
    }
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from);
    if let Ok(channel) =
        open_island_core::send::channel_for(&host, &plan.steps, runtime.as_deref(), &Path::exists)
    {
        match channel {
            open_island_core::send::Channel::Wezterm { socket, pane_id } => keys.push(
                ResourceKey::Wezterm(normalized(Path::new(&socket)), pane_id),
            ),
            open_island_core::send::Channel::Kitty { socket, window_id } => {
                keys.push(ResourceKey::Kitty(socket, window_id))
            }
            _ => {}
        }
    }
    keys.sort();
    keys.dedup();
    let mut guards = Vec::new();
    for key in keys {
        guards.push(ctx.messages.resources.acquire(key, &runner)?);
    }
    check()?;
    let current_host = open_island_core::discovery::terminal_for_delivery(&target.session, || {
        runner.remaining().is_ok()
    })
    .ok_or("stale_session")?;
    if current_host != host {
        return Err("target_changed".into());
    }
    let current = planner.plan(&current_host, &checked);
    if current.steps != plan.steps {
        return Err("target_changed".into());
    }
    let check_window = || {
        check()?;
        for (pid, birth) in &births {
            if birth.is_none() || process::birth_identity(*pid) != *birth {
                return Err("target_changed".into());
            }
        }
        Ok(())
    };
    let guarded = CheckedRunner {
        runner: &runner,
        validate: &check_window,
    };
    JumpExecutor::new(&guarded).execute_guarded(&plan.steps, check_window)?;
    if let Some(hook) = &target.session.hook_id {
        let mut state = ctx.state.lock().map_err(|_| "daemon_unavailable")?;
        if state.store.delivery_target_current(&target.session) {
            state.store.mark_seen(hook, Instant::now());
        }
    }
    drop(guards);
    crate::broadcast::broadcast_sessions(ctx);
    Ok(serde_json::Value::Null)
}
