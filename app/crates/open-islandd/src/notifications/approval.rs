//! Daemon-owned approval authority: the set-once `DecisionCell` and the
//! generation-checked publication seam every resolution path funnels through.

use open_island_core::{
    protocol::{ApprovalDecision, QuestionOutcome},
    session::HookId,
    store::SessionStore,
};
use std::{
    collections::HashMap,
    sync::{mpsc, Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

/// A set-once decision shared between a pending waiter and the daemon transition.
/// The winner publishes exactly once; every later claimant reads the winner back.
pub type DecisionCell = Arc<OnceLock<ApprovalDecision>>;

pub struct PendingApproval {
    pub connection_id: u64,
    pub session_id: HookId,
    pub approval_generation: u64,
    pub cell: DecisionCell,
    pub wake_sender: mpsc::Sender<()>,
}

pub struct Subscriber {
    pub connection_id: u64,
    pub sender: mpsc::Sender<String>,
}

/// How a pending question left the island. Everything that is not `Answered` sends no
/// answer to the agent, so its own terminal keeps the question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuestionSettlement {
    Answered(Vec<Vec<String>>),
    /// The agent reported the question resolved in its own terminal, so the card closes
    /// with nothing to send back.
    AnsweredElsewhere,
    Cancelled,
    Expired,
}

impl QuestionSettlement {
    pub fn outcome(&self) -> QuestionOutcome {
        match self {
            Self::Answered(_) | Self::AnsweredElsewhere => QuestionOutcome::Answered,
            Self::Cancelled => QuestionOutcome::Cancelled,
            Self::Expired => QuestionOutcome::Expired,
        }
    }

    pub fn answers(&self) -> Option<&[Vec<String>]> {
        match self {
            Self::Answered(answers) => Some(answers),
            Self::AnsweredElsewhere | Self::Cancelled | Self::Expired => None,
        }
    }
}

pub type AnswerCell = Arc<OnceLock<QuestionSettlement>>;

pub struct PendingQuestion {
    /// `None` for a question the hook cannot answer: Codex sends its `PreToolUse` and exits,
    /// so tying the card to that connection would cancel it the moment the hook returns.
    pub connection_id: Option<u64>,
    pub session_id: HookId,
    pub question_generation: u64,
    pub cell: AnswerCell,
    pub wake_sender: mpsc::Sender<()>,
    pub answerable: bool,
    pub expires_at: Option<Instant>,
}

pub struct DaemonState {
    pub store: SessionStore,
    pub pending: HashMap<String, PendingApproval>,
    pub pending_questions: HashMap<String, PendingQuestion>,
    pub subscribers: Vec<Subscriber>,
    pub approval_generation: u64,
    pub usage: open_island_core::usage::UsageReport,
    pub usage_watch: open_island_core::usage::ThresholdWatch,
    pub scenes: open_island_core::protocol::QuietScenes,
}

pub type SharedState = Arc<Mutex<DaemonState>>;

/// The outcome of the single generation-checked daemon state transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    /// This caller resolved the current approval and published its decision.
    Resolved(ApprovalDecision),
    /// Another claimant had already published this approval's decision.
    Relayed(ApprovalDecision),
    /// The caller's expected generation does not match the current one.
    Stale,
}

pub enum WaitOutcome {
    Woken(ApprovalDecision),
    Timeout(ApprovalDecision),
    Disconnected(ApprovalDecision),
}

/// Waits for the island to settle a question. Every exit that is not an explicit answer
/// falls back to `Cancelled`, which hands the question to the agent's own terminal.
pub fn wait_for_answer(
    cell: AnswerCell,
    wake_receiver: mpsc::Receiver<()>,
    question_timeout: Duration,
) -> QuestionSettlement {
    let _ = wake_receiver.recv_timeout(question_timeout);
    cell.get().cloned().unwrap_or(QuestionSettlement::Cancelled)
}

pub fn wait_for_decision(
    cell: DecisionCell,
    wake_receiver: mpsc::Receiver<()>,
    approval_timeout: Duration,
) -> WaitOutcome {
    match wake_receiver.recv_timeout(approval_timeout) {
        Ok(()) => WaitOutcome::Woken(cell.get().cloned().unwrap_or(ApprovalDecision::Deny)),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            WaitOutcome::Timeout(cell.get().cloned().unwrap_or(ApprovalDecision::Deny))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            WaitOutcome::Disconnected(cell.get().cloned().unwrap_or(ApprovalDecision::Deny))
        }
    }
}

pub fn claim_timeout(
    cell: &DecisionCell,
    claim: impl FnOnce() -> Result<Resolution, String>,
) -> Result<Resolution, String> {
    match claim() {
        Ok(outcome) => Ok(outcome),
        Err(error) => cell.get().cloned().map(Resolution::Relayed).ok_or(error),
    }
}

pub fn timeout_decision(
    cell: &DecisionCell,
    waited: ApprovalDecision,
    claim: impl FnOnce() -> Result<Resolution, String>,
) -> ApprovalDecision {
    match claim_timeout(cell, claim) {
        Ok(Resolution::Resolved(decision)) | Ok(Resolution::Relayed(decision)) => decision,
        Ok(Resolution::Stale) | Err(_) => cell.get().cloned().unwrap_or(waited),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_waiter_returns_published_winner_after_timeout() {
        let cell = Arc::new(OnceLock::new());
        cell.set(ApprovalDecision::Allow).unwrap();
        let (_wake_sender, wake_receiver) = mpsc::channel();
        let decision = wait_for_decision(cell, wake_receiver, Duration::ZERO);
        assert!(matches!(
            decision,
            WaitOutcome::Timeout(ApprovalDecision::Allow)
        ));
    }
}
