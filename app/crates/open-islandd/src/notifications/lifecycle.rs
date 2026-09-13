//! Daemon approval lifecycle: admission, generation-checked resolution, and hook
//! waiting. State mutation stays under the daemon mutex; wake and broadcast happen
//! only after the lock is released.

use open_island_core::{
    adapters::{claude_plan_return_mode, QuestionInput, CLAUDE_PLAN_TOOL},
    config::SoundEvent,
    protocol::{
        ApprovalDecision, ApprovalRequest, ApprovalResolved, ConfigChanged, EventData, HookEvent,
        HookEventKind, IslandToggle, QuestionFocus, QuestionRequest, QuestionResolved,
    },
    session::HookId,
    store::MAX_PENDING_APPROVALS,
};
use serde_json::{json, Value};
use std::{
    sync::{mpsc, Arc, OnceLock},
    time::{Duration, Instant},
};

use super::approval::{self, AnswerCell, DecisionCell, QuestionSettlement};
use crate::config_handle::ConfigHandle;
use crate::sound::SoundPlayer;

pub use super::approval::{DaemonState, PendingApproval, PendingQuestion, SharedState, Subscriber};

#[derive(Clone)]
pub struct DaemonContext {
    pub state: SharedState,
    pub config: ConfigHandle,
    pub sound: SoundPlayer,
}

pub fn announce_hook_event(ctx: &DaemonContext, kind: HookEventKind) {
    match kind {
        HookEventKind::SessionStart => ctx.sound.play(SoundEvent::SessionStart),
        HookEventKind::UserPromptSubmit => ctx.sound.play(SoundEvent::TaskAcknowledge),
        HookEventKind::Stop => ctx.sound.play(SoundEvent::TaskComplete),
        _ => {}
    }
}

struct AdmissionRequest {
    connection_id: u64,
    approval_id: String,
    event: HookEvent,
    cell: DecisionCell,
    wake_sender: mpsc::Sender<()>,
}

enum Admission {
    Duplicate,
    Capacity(ApprovalResolved),
    Accepted(u64, ApprovalRequest),
}

pub fn resolve_if_current(
    ctx: &DaemonContext,
    approval_id: &str,
    expected_generation: Option<u64>,
    decision: ApprovalDecision,
    broadcast: impl Fn(String) -> bool,
) -> Result<approval::Resolution, String> {
    let (outcome, wake) = {
        let mut state = ctx
            .state
            .lock()
            .map_err(|_| "daemon state unavailable".to_owned())?;
        let Some(pending) = state.pending.get(approval_id) else {
            return Err(format!("approval '{approval_id}' not found"));
        };
        let generation = pending.approval_generation;
        if expected_generation.is_some_and(|expected| expected != generation) {
            return Ok(approval::Resolution::Stale);
        }
        pending
            .cell
            .set(decision.clone())
            .map_err(|_| "approval decision already published".to_owned())?;
        let Some(pending) = state.pending.remove(approval_id) else {
            return Err(format!("approval '{approval_id}' not found"));
        };
        state.store.resolve_approval(approval_id, decision.clone());
        (
            approval::Resolution::Resolved(decision.clone()),
            (pending.session_id, pending.wake_sender, decision),
        )
    };
    let (session_id, wake_sender, winner) = wake;
    let _ = wake_sender.send(());
    broadcast(event_message(
        "approval-resolved",
        EventData::ApprovalResolved(ApprovalResolved {
            approval_id: approval_id.to_owned(),
            session_id,
            decision: winner,
        }),
    ));
    Ok(outcome)
}

/// Takes back an approval nobody can answer, so a retry of the same request is admitted
/// again instead of being deduplicated into a denial.
fn withdraw(ctx: &DaemonContext, approval_id: &str) {
    if let Ok(mut state) = ctx.state.lock() {
        state.pending.remove(approval_id);
        state.store.withdraw_approval(approval_id);
    }
}

fn admit_locked(state: &mut DaemonState, request: AdmissionRequest) -> Admission {
    if state.pending.contains_key(&request.approval_id) {
        return Admission::Duplicate;
    }
    if state.pending.len() >= MAX_PENDING_APPROVALS {
        let resolved = ApprovalResolved {
            approval_id: request.approval_id.clone(),
            session_id: request.event.session_id.clone(),
            decision: ApprovalDecision::Deny,
        };
        state.store.apply_hook_event(request.event);
        state
            .store
            .resolve_approval(&request.approval_id, ApprovalDecision::Deny);
        return Admission::Capacity(resolved);
    }
    let generation = state.approval_generation;
    state.approval_generation += 1;
    let approval = ApprovalRequest {
        approval_id: request.approval_id.clone(),
        session_id: request.event.session_id.clone(),
        tool_name: request.event.tool_name.clone(),
        tool_input: request.event.tool_input.clone(),
        reason: request.event.summary.clone(),
    };
    state.store.apply_hook_event(request.event);
    state.pending.insert(
        request.approval_id.clone(),
        PendingApproval {
            connection_id: request.connection_id,
            session_id: approval.session_id.clone(),
            approval_generation: generation,
            cell: request.cell,
            wake_sender: request.wake_sender,
        },
    );
    Admission::Accepted(generation, approval)
}

struct QuestionAdmissionRequest {
    connection_id: u64,
    event: HookEvent,
    question: QuestionInput,
    cell: AnswerCell,
    wake_sender: mpsc::Sender<()>,
}

enum QuestionAdmission {
    Duplicate,
    Capacity(QuestionResolved),
    Accepted(u64, QuestionRequest),
}

fn admit_question_locked(
    state: &mut DaemonState,
    request: QuestionAdmissionRequest,
) -> QuestionAdmission {
    let question_id = request.question.question_id.clone();
    if state.pending_questions.contains_key(&question_id) {
        return QuestionAdmission::Duplicate;
    }
    if state.pending_questions.len() >= MAX_PENDING_APPROVALS {
        let resolved = QuestionResolved {
            question_id: question_id.clone(),
            session_id: request.question.session_id.clone(),
            outcome: QuestionSettlement::Cancelled.outcome(),
        };
        state.store.apply_hook_event(request.event);
        state
            .store
            .resolve_question(&question_id, resolved.outcome.clone());
        return QuestionAdmission::Capacity(resolved);
    }
    let generation = state.approval_generation;
    state.approval_generation += 1;
    let announced = QuestionRequest {
        question_id: question_id.clone(),
        session_id: request.question.session_id.clone(),
        agent: request.question.agent.clone(),
        questions: request.question.questions.clone(),
        answerable: request.question.answerable,
        expires_in_ms: request.question.expires_in_ms,
    };
    state.store.apply_hook_event(request.event);
    state.pending_questions.insert(
        question_id.clone(),
        PendingQuestion {
            connection_id: request.question.answerable.then_some(request.connection_id),
            session_id: request.question.session_id,
            question_generation: generation,
            cell: request.cell,
            wake_sender: request.wake_sender,
            answerable: request.question.answerable,
            expires_at: request
                .question
                .expires_in_ms
                .map(|ms| Instant::now() + Duration::from_millis(ms)),
        },
    );
    QuestionAdmission::Accepted(generation, announced)
}

fn question_event(
    ctx: DaemonContext,
    connection_id: u64,
    event: HookEvent,
    question: QuestionInput,
    question_timeout: Duration,
    broadcast: impl Fn(String) -> bool + Clone,
) -> Result<Value, String> {
    let question_id = question.question_id.clone();
    let answerable = question.answerable;
    let (wake_sender, wake_receiver) = mpsc::channel();
    let cell: AnswerCell = Arc::new(OnceLock::new());
    let admission = {
        let mut state = ctx
            .state
            .lock()
            .map_err(|_| "daemon state unavailable".to_owned())?;
        admit_question_locked(
            &mut state,
            QuestionAdmissionRequest {
                connection_id,
                event,
                question,
                cell: cell.clone(),
                wake_sender,
            },
        )
    };
    let generation = match admission {
        QuestionAdmission::Duplicate => {
            return Ok(settled_response(&QuestionSettlement::Cancelled))
        }
        QuestionAdmission::Capacity(resolved) => {
            broadcast(event_message(
                "question-resolved",
                EventData::QuestionResolved(resolved),
            ));
            return Ok(settled_response(&QuestionSettlement::Cancelled));
        }
        QuestionAdmission::Accepted(generation, announced) => {
            broadcast(event_message(
                "question-asked",
                EventData::QuestionRequested(announced),
            ));
            ctx.sound.play(SoundEvent::ApprovalNeeded);
            generation
        }
    };
    if !answerable {
        return Ok(json!({"accepted": true, "settled": "pending"}));
    }
    approval::wait_for_answer(cell.clone(), wake_receiver, question_timeout);
    if cell.get().is_none() {
        let _ = settle_question(
            &ctx,
            &question_id,
            Some(generation),
            QuestionSettlement::Cancelled,
            broadcast,
        );
    }
    let settlement = cell.get().cloned().unwrap_or(QuestionSettlement::Cancelled);
    Ok(settled_response(&settlement))
}

fn settled_response(settlement: &QuestionSettlement) -> Value {
    let settled = match settlement {
        QuestionSettlement::Answered(_) | QuestionSettlement::AnsweredElsewhere => "answered",
        QuestionSettlement::Cancelled => "cancelled",
        QuestionSettlement::Expired => "expired",
    };
    json!({"accepted": true, "settled": settled, "answers": settlement.answers()})
}

pub fn hook_event(
    ctx: DaemonContext,
    connection_id: u64,
    event: HookEvent,
    approval_timeout: Duration,
    question_timeout: Duration,
    broadcast: impl Fn(String) -> bool + Clone,
) -> Result<Value, String> {
    if let Some(question) = open_island_core::adapters::question_from_event(&event) {
        return question_event(
            ctx,
            connection_id,
            event,
            question,
            question_timeout,
            broadcast,
        );
    }
    // The agent answered in its own terminal: record the event and close the card the
    // island is still showing. Nothing is pending when the island answered first.
    if let Some(question_id) = open_island_core::adapters::question_closed_by_event(&event) {
        if let Ok(mut state) = ctx.state.lock() {
            state.store.apply_hook_event(event);
        }
        let _ = settle_question(
            &ctx,
            &question_id,
            None,
            QuestionSettlement::AnsweredElsewhere,
            broadcast,
        );
        return Ok(json!({"accepted": true, "settled": "answered"}));
    }
    let mut waiter = None;
    if let Some(approval_id) = event.approval_id.clone() {
        let (wake_sender, wake_receiver) = mpsc::channel();
        let cell: DecisionCell = Arc::new(OnceLock::new());
        let leaving_plan = event.tool_name.as_deref() == Some(CLAUDE_PLAN_TOOL);
        let mut return_mode = None;
        let admission = {
            let mut state = ctx
                .state
                .lock()
                .map_err(|_| "daemon state unavailable".to_owned())?;
            let admission = admit_locked(
                &mut state,
                AdmissionRequest {
                    connection_id,
                    approval_id: approval_id.clone(),
                    event,
                    cell: cell.clone(),
                    wake_sender,
                },
            );
            if let Admission::Accepted(_, request) = &admission {
                if leaving_plan {
                    let prior = state.store.prior_mode(&request.session_id);
                    return_mode = Some(claude_plan_return_mode(prior.as_deref()).to_owned());
                }
            }
            admission
        };
        match admission {
            Admission::Duplicate => {
                return Ok(json!({"accepted": false, "decision": "deny"}));
            }
            Admission::Capacity(resolved) => {
                broadcast(event_message(
                    "approval-resolved",
                    EventData::ApprovalResolved(resolved),
                ));
                return Ok(json!({"accepted": false, "decision": "deny"}));
            }
            Admission::Accepted(generation, request) => {
                let reached_an_island = broadcast(event_message(
                    "approval-requested",
                    EventData::ApprovalRequested(request),
                ));
                if !reached_an_island {
                    withdraw(&ctx, &approval_id);
                    return Ok(json!({"accepted": false}));
                }
                ctx.sound.play(SoundEvent::ApprovalNeeded);
                waiter = Some((approval_id, generation, wake_receiver, cell, return_mode));
            }
        }
    } else {
        let kind = event.event.clone();
        let mut spamming = false;
        let mut announce = true;
        if let Ok(mut state) = ctx.state.lock() {
            let id = event.session_id.clone();
            let previous_completion = state.store.completion_id(&id).map(str::to_owned);
            state.store.apply_hook_event(event);
            let child = state.store.presentation_id(&id) != id;
            if child {
                use open_island_core::config::SubagentTiming;
                announce = kind == HookEventKind::Stop
                    && state.store.completion_id(&id) != previous_completion.as_deref()
                    && match ctx.config.get().notifications.subagent_timing {
                        SubagentTiming::RootResponses => false,
                        SubagentTiming::EveryCompletion => true,
                        SubagentTiming::AllFinished => state.store.family_children_finished(&id),
                    };
            }
            if kind == HookEventKind::UserPromptSubmit && !child {
                let sound = &ctx.config.get().sound;
                spamming = state.store.note_prompt(
                    Instant::now(),
                    sound.spam_window,
                    sound.spam_threshold,
                );
            }
        }
        if spamming {
            ctx.sound.play(SoundEvent::UserSpam);
        }
        if announce {
            announce_hook_event(&ctx, kind);
        }
    }
    let Some((approval_id, generation, receiver, cell, return_mode)) = waiter else {
        return Ok(json!({"accepted": true, "decision": "deny"}));
    };
    let decision = match approval::wait_for_decision(cell.clone(), receiver, approval_timeout) {
        approval::WaitOutcome::Woken(decision) => decision,
        approval::WaitOutcome::Timeout(waited) => approval::timeout_decision(&cell, waited, || {
            resolve_if_current(
                &ctx,
                &approval_id,
                Some(generation),
                ApprovalDecision::Deny,
                broadcast.clone(),
            )
        }),
        approval::WaitOutcome::Disconnected(waited) => {
            disconnect_pending(&ctx, connection_id, broadcast.clone());
            cell.get().cloned().unwrap_or(waited)
        }
    };
    Ok(json!({"accepted": true, "decision": decision, "mode": return_mode}))
}

pub fn disconnect_pending(
    ctx: &DaemonContext,
    connection_id: u64,
    broadcast: impl Fn(String) -> bool + Clone,
) {
    let (pending, questions) = ctx
        .state
        .lock()
        .map(|state| {
            (
                state
                    .pending
                    .iter()
                    .filter(|(_, pending)| pending.connection_id == connection_id)
                    .map(|(id, pending)| (id.clone(), pending.approval_generation))
                    .collect::<Vec<_>>(),
                state
                    .pending_questions
                    .iter()
                    .filter(|(_, pending)| pending.connection_id == Some(connection_id))
                    .map(|(id, pending)| (id.clone(), pending.question_generation))
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_default();
    for (approval_id, generation) in pending {
        let broadcast_for = broadcast.clone();
        let _ = resolve_if_current(
            ctx,
            &approval_id,
            Some(generation),
            ApprovalDecision::Deny,
            broadcast_for,
        );
    }
    for (question_id, generation) in questions {
        let _ = settle_question(
            ctx,
            &question_id,
            Some(generation),
            QuestionSettlement::Cancelled,
            broadcast.clone(),
        );
    }
}

/// The single generation-checked transition every question resolution funnels through,
/// mirroring `resolve_if_current`. State mutation happens under the lock; the wake, the
/// broadcast and the notification close happen after it is released.
pub fn settle_question(
    ctx: &DaemonContext,
    question_id: &str,
    expected_generation: Option<u64>,
    settlement: QuestionSettlement,
    broadcast: impl Fn(String) -> bool,
) -> Result<QuestionSettlement, String> {
    let (session_id, wake_sender, winner) = {
        let mut state = ctx
            .state
            .lock()
            .map_err(|_| "daemon state unavailable".to_owned())?;
        let Some(pending) = state.pending_questions.get(question_id) else {
            return Err(format!("question '{question_id}' not found"));
        };
        let generation = pending.question_generation;
        if expected_generation.is_some_and(|expected| expected != generation) {
            return Err(format!("question '{question_id}' is stale"));
        }
        pending
            .cell
            .set(settlement.clone())
            .map_err(|_| "question answer already published".to_owned())?;
        let Some(pending) = state.pending_questions.remove(question_id) else {
            return Err(format!("question '{question_id}' not found"));
        };
        state
            .store
            .resolve_question(question_id, settlement.outcome());
        (pending.session_id, pending.wake_sender, settlement)
    };
    let _ = wake_sender.send(());
    broadcast(event_message(
        "question-resolved",
        EventData::QuestionResolved(QuestionResolved {
            question_id: question_id.to_owned(),
            session_id,
            outcome: winner.outcome(),
        }),
    ));
    Ok(winner)
}

/// Resolves every question whose Codex deadline has passed. Called from the poller, the
/// only thread that ticks while a non-answerable question sits on the island.
pub fn expire_questions(
    ctx: &DaemonContext,
    now: Instant,
    broadcast: impl Fn(String) -> bool + Clone,
) {
    let expired = ctx
        .state
        .lock()
        .map(|state| {
            state
                .pending_questions
                .iter()
                .filter(|(_, pending)| pending.expires_at.is_some_and(|deadline| deadline <= now))
                .map(|(id, pending)| (id.clone(), pending.question_generation))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for (question_id, generation) in expired {
        let _ = settle_question(
            ctx,
            &question_id,
            Some(generation),
            QuestionSettlement::Expired,
            broadcast.clone(),
        );
    }
}

/// The session and answerability behind a pending question, for the notification action.
pub fn pending_question(ctx: &DaemonContext, question_id: &str) -> Option<(HookId, bool)> {
    let state = ctx.state.lock().ok()?;
    let pending = state.pending_questions.get(question_id)?;
    Some((pending.session_id.clone(), pending.answerable))
}

pub fn island_toggle_message(source: &str) -> String {
    event_message(
        "island-toggle",
        EventData::IslandToggle(IslandToggle {
            source: source.to_owned(),
        }),
    )
}

pub fn config_changed_message(config: Value) -> String {
    event_message(
        "config-changed",
        EventData::ConfigChanged(ConfigChanged { config }),
    )
}

pub fn open_settings_message(source: &str) -> String {
    event_message(
        "open-settings",
        EventData::IslandToggle(IslandToggle {
            source: source.to_owned(),
        }),
    )
}

pub fn question_focus_message(question_id: &str, session_id: HookId) -> String {
    event_message(
        "question-focus",
        EventData::QuestionFocus(QuestionFocus {
            question_id: question_id.to_owned(),
            session_id,
        }),
    )
}

fn event_message(event: &str, data: EventData) -> String {
    serde_json::to_string(&open_island_core::protocol::Event {
        v: 1,
        event: event.to_owned(),
        data,
    })
    .unwrap_or_else(|_| "{\"v\":1,\"event\":\"sessions-updated\",\"data\":[]}".to_owned())
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
