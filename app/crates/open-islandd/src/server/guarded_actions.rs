use open_island_core::message_delivery::{ClientSubmissionId, DeliveryIdentity};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Send {
    pub id: String,
    pub text: String,
    #[serde(flatten)]
    pub identity: DeliveryIdentity,
    pub client_submission_id: ClientSubmissionId,
}

#[derive(Deserialize)]
pub struct Cancel {
    pub id: String,
    pub message_id: u64,
    #[serde(flatten)]
    pub identity: DeliveryIdentity,
}

pub fn cancel(
    ctx: &crate::notifications::lifecycle::DaemonContext,
    request: Cancel,
) -> Result<serde_json::Value, String> {
    if request.identity.daemon_epoch != ctx.messages.epoch {
        return Err("stale_epoch".into());
    }
    let snapshot = crate::ui_state::freeze(ctx)?;
    let session = snapshot.targets().find(|s| {
        s.session.id == request.id
            && s.session_instance_id.as_ref() == Some(&request.identity.session_instance_id)
    });
    if let Some(session) = session {
        validate_process(ctx, &session.session, &request.identity)?;
    }
    {
        let mut state = ctx.state.lock().map_err(|_| "daemon_unavailable")?;
        if session.is_some_and(|session| !state.store.delivery_target_current(&session.session)) {
            return Err("stale_session".into());
        }
        state
            .store
            .deliveries
            .cancel_guarded(&request.id, request.message_id, &request.identity)
            .map_err(str::to_owned)?;
    }
    crate::message_executor::publish(&ctx.state);
    crate::broadcast::broadcast_sessions(ctx);
    Ok(serde_json::Value::Null)
}

#[derive(Deserialize)]
pub struct Resolve {
    pub approval_id: String,
    pub pending_generation: u64,
    pub decision: open_island_core::protocol::ApprovalDecision,
    #[serde(flatten)]
    pub identity: DeliveryIdentity,
}
#[derive(Deserialize)]
pub struct Answer {
    pub question_id: String,
    pub pending_generation: u64,
    pub answers: Vec<Vec<String>>,
    #[serde(flatten)]
    pub identity: DeliveryIdentity,
}
fn target(
    ctx: &crate::notifications::lifecycle::DaemonContext,
    identity: &DeliveryIdentity,
) -> Result<open_island_core::session::Session, String> {
    if identity.daemon_epoch != ctx.messages.epoch {
        return Err("stale_epoch".into());
    }
    let session = crate::ui_state::freeze(ctx)?
        .targets()
        .find(|s| s.session_instance_id.as_ref() == Some(&identity.session_instance_id))
        .map(|s| s.session.clone())
        .ok_or("stale_session")?;
    validate_process(ctx, &session, identity)?;
    Ok(session)
}
fn validate_process(
    ctx: &crate::notifications::lifecycle::DaemonContext,
    session: &open_island_core::session::Session,
    expected: &DeliveryIdentity,
) -> Result<(), String> {
    // Hook-only pending replies do not address an OS process. Process-backed
    // identities must still match at the action boundary, independently of cache.
    if expected.session_instance_id.0.starts_with("hook:") {
        return Ok(());
    }
    let birth = open_island_core::process::birth_identity(session.pid).ok_or("stale_session")?;
    if crate::message_executor::identity(session, birth, &ctx.messages.epoch) != *expected {
        return Err("stale_session".into());
    }
    Ok(())
}
pub fn resolve(
    ctx: &crate::notifications::lifecycle::DaemonContext,
    request: Resolve,
) -> Result<serde_json::Value, String> {
    let target = target(ctx, &request.identity)?;
    crate::notifications::lifecycle::resolve_if_current_checked(
        ctx,
        &request.approval_id,
        Some(request.pending_generation),
        request.decision,
        crate::broadcast::make_broadcast(ctx.state.clone()),
        |state| {
            let pending = state
                .pending
                .get(&request.approval_id)
                .ok_or("stale_pending")?;
            if pending.approval_generation != request.pending_generation {
                return Err("stale_pending".into());
            }
            if target.hook_generation != pending.session_generation
                || target.hook_id.as_ref() != Some(&pending.session_id)
                || !state.store.delivery_target_current(&target)
            {
                return Err("stale_session".into());
            }
            Ok(())
        },
    )?;
    Ok(serde_json::Value::Null)
}
pub fn answer(
    ctx: &crate::notifications::lifecycle::DaemonContext,
    request: Answer,
) -> Result<serde_json::Value, String> {
    let target = target(ctx, &request.identity)?;
    crate::notifications::lifecycle::settle_question_checked(
        ctx,
        &request.question_id,
        Some(request.pending_generation),
        crate::notifications::approval::QuestionSettlement::Answered(request.answers),
        crate::broadcast::make_broadcast(ctx.state.clone()),
        |state| {
            let pending = state
                .pending_questions
                .get(&request.question_id)
                .ok_or("stale_pending")?;
            if pending.question_generation != request.pending_generation {
                return Err("stale_pending".into());
            }
            if target.hook_generation != pending.session_generation
                || target.hook_id.as_ref() != Some(&pending.session_id)
                || !state.store.delivery_target_current(&target)
            {
                return Err("stale_session".into());
            }
            Ok(())
        },
    )?;
    Ok(serde_json::Value::Null)
}
