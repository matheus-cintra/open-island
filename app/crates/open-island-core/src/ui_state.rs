use crate::{
    message_delivery::{DaemonEpoch, MessageDelivery, PendingGeneration, SessionInstanceId},
    protocol::{ApprovalRequest, QuestionRequest, QuietScenes, UpdateAvailable},
    session::Session,
    usage::UsageReport,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UiSession {
    #[serde(flatten)]
    pub session: Session,
    pub session_instance_id: Option<SessionInstanceId>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UiApproval {
    #[serde(skip)]
    pub session_generation: u64,
    #[serde(default)]
    pub session_instance_id: Option<SessionInstanceId>,
    #[serde(flatten)]
    pub request: ApprovalRequest,
    pub pending_generation: PendingGeneration,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UiQuestion {
    #[serde(skip)]
    pub session_generation: u64,
    #[serde(default)]
    pub session_instance_id: Option<SessionInstanceId>,
    #[serde(flatten)]
    pub request: QuestionRequest,
    pub pending_generation: PendingGeneration,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UiSnapshot {
    pub schema_version: u8,
    #[serde(default)]
    pub discovering: bool,
    pub daemon_epoch: DaemonEpoch,
    pub publication_revision: u64,
    pub sessions: Vec<UiSession>,
    #[serde(default)]
    pub child_sessions: Vec<UiSession>,
    pub approvals: Vec<UiApproval>,
    pub questions: Vec<UiQuestion>,
    pub message_deliveries: Vec<MessageDelivery>,
    pub config: serde_json::Value,
    pub usage: UsageReport,
    pub update: Option<UpdateAvailable>,
    pub quiet_scenes: QuietScenes,
}

impl UiSnapshot {
    pub fn targets(&self) -> impl Iterator<Item = &UiSession> {
        self.sessions.iter().chain(&self.child_sessions)
    }
}
