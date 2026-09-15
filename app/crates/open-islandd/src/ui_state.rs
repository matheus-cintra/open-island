use crate::{notifications::lifecycle::DaemonContext, server::snapshot_stream::SnapshotPool};
use open_island_core::{
    message_delivery::PendingGeneration,
    ui_state::{UiApproval, UiQuestion, UiSession, UiSnapshot},
};
use std::{sync::Arc, time::Instant};

pub fn freeze(ctx: &DaemonContext) -> Result<Arc<UiSnapshot>, String> {
    let config = ctx.config.get().to_json_value();
    let cached = ctx.discovery.read();
    let now = Instant::now();
    let (mut store, mut snapshot) = {
        let state = ctx.state.lock().map_err(|_| "daemon_unavailable")?;
        let approvals = state
            .pending
            .values()
            .map(|pending| UiApproval {
                session_instance_id: None,
                session_generation: pending.session_generation,
                request: pending.request.clone(),
                pending_generation: PendingGeneration(pending.approval_generation),
            })
            .collect();
        let questions = state
            .pending_questions
            .values()
            .map(|pending| {
                let mut request = pending.request.clone();
                request.expires_in_ms = pending.expires_at.map(|deadline| {
                    deadline
                        .saturating_duration_since(now)
                        .as_millis()
                        .min(u64::MAX as u128) as u64
                });
                UiQuestion {
                    session_instance_id: None,
                    session_generation: pending.session_generation,
                    request,
                    pending_generation: PendingGeneration(pending.question_generation),
                }
            })
            .collect();
        (
            state.store.clone(),
            UiSnapshot {
                schema_version: 1,
                discovering: !cached.ready,
                daemon_epoch: ctx.messages.epoch.clone(),
                publication_revision: state.publication_revision,
                sessions: Vec::new(),
                child_sessions: Vec::new(),
                approvals,
                questions,
                message_deliveries: state.store.deliveries.snapshot(),
                config,
                usage: state.usage.clone(),
                update: state.update.clone(),
                quiet_scenes: state.scenes,
            },
        )
    };
    let (roots, children) = store.snapshot_cached(&cached.observation, false);
    let project = |sessions: Vec<open_island_core::session::Session>| {
        sessions
            .into_iter()
            .map(|mut session| {
                session.queued_messages = None;
                let birth = cached.observation.births.get(&session.pid).copied();
                let session_instance_id = birth
                    .map(|birth| {
                        crate::message_executor::identity(&session, birth, &ctx.messages.epoch)
                            .session_instance_id
                    })
                    .or_else(|| {
                        // A hook can own a pending response without a verified terminal process.
                        // Its instance belongs to this SessionStart, never to a cwd-matched process.
                        session
                            .hook_id
                            .as_ref()
                            .filter(|_| session.hook_generation != 0)
                            .map(|id| {
                                open_island_core::message_delivery::SessionInstanceId(format!(
                                    "hook:{}:{}",
                                    id, session.hook_generation
                                ))
                            })
                    });
                if birth.is_none() {
                    session.send_channel = None;
                    session.send_blocked = Some("unverified_target".into());
                }
                UiSession {
                    session,
                    session_instance_id,
                }
            })
            .collect()
    };
    snapshot.sessions = project(roots);
    snapshot.child_sessions = project(children);
    for child in &mut snapshot.child_sessions {
        // These OpenCode children share a terminal with the root. Terminal input
        // cannot address a child conversation; pending replies have their own RPC.
        child.session.send_channel = None;
        child.session.send_blocked = Some("unaddressable_child".into());
    }
    for approval in &mut snapshot.approvals {
        approval.session_instance_id = snapshot
            .sessions
            .iter()
            .chain(&snapshot.child_sessions)
            .find(|s| {
                s.session.hook_id.as_ref() == Some(&approval.request.session_id)
                    && s.session.hook_generation == approval.session_generation
            })
            .and_then(|s| s.session_instance_id.clone());
    }
    for question in &mut snapshot.questions {
        question.session_instance_id = snapshot
            .sessions
            .iter()
            .chain(&snapshot.child_sessions)
            .find(|s| {
                s.session.hook_id.as_ref() == Some(&question.request.session_id)
                    && s.session.hook_generation == question.session_generation
            })
            .and_then(|s| s.session_instance_id.clone());
    }
    snapshot
        .approvals
        .sort_by(|a, b| a.request.approval_id.cmp(&b.request.approval_id));
    snapshot
        .questions
        .sort_by(|a, b| a.request.question_id.cmp(&b.request.question_id));
    Ok(Arc::new(snapshot))
}
pub type UiSnapshots = SnapshotPool<UiSnapshot>;
