use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct HookId(String);

impl HookId {
    pub fn new(agent: &str, agent_session_id: &str) -> Self {
        Self(format!("{agent}:{agent_session_id}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HookId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

pub type HookSessionId = HookId;

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    Unknown,
    Pending,
    #[serde(alias = "approved")]
    Allowed,
    Denied,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuestionState {
    Pending,
    Answered,
    Expired,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Attention {
    WaitingForInput,
    NeedsAttention,
    Working,
    Idle,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

impl TaskStatus {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "pending" => Some(Self::Pending),
            "in_progress" => Some(Self::InProgress),
            "completed" => Some(Self::Completed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Task {
    pub content: String,
    pub status: TaskStatus,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Subagent {
    pub id: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since_ms: Option<u64>,
    #[serde(default)]
    pub done: bool,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub agent: String,
    pub cwd: String,
    pub title: String,
    pub pid: u32,
    pub terminal: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "hook_session_id",
        alias = "agent_session_id"
    )]
    pub hook_id: Option<HookId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "tool")]
    pub current_tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_message_body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagents: Option<Vec<Subagent>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tasks: Option<Vec<Task>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "permission", alias = "approval")]
    pub permission_state: Option<PermissionState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "question", alias = "pending_question")]
    pub question_state: Option<QuestionState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attention: Option<Attention>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// Unix milliseconds at which the state the activity line describes began. Absolute
    /// rather than a duration, so a stale snapshot cannot age and the poller does not
    /// rebroadcast the whole list on every tick. `None` after a daemon restart, because
    /// the hook state that held it is gone and the daemon genuinely does not know.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raise_pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launcher: Option<String>,
    /// Opaque daemon-local edge token for one accepted primary Stop event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queued_messages: Option<Vec<QueuedMessage>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_channel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_blocked: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct QueuedMessage {
    pub id: u64,
    pub text: String,
    pub queued_at_ms: u64,
}

impl Session {
    pub fn new(agent: &str, cwd: &str, pid: u32, terminal: &str) -> Self {
        let title = cwd
            .rsplit('/')
            .next()
            .filter(|part| !part.is_empty())
            .unwrap_or("/");
        Self {
            id: format!("{agent}:{pid}"),
            agent: agent.to_owned(),
            cwd: cwd.to_owned(),
            title: title.to_owned(),
            pid,
            terminal: terminal.to_owned(),
            hook_id: None,
            last_message: None,
            last_message_body: None,
            status: None,
            current_tool: None,
            summary: None,
            mode: None,
            subagents: None,
            tasks: None,
            permission_state: None,
            question_state: None,
            attention: None,
            name: None,
            branch: None,
            model: None,
            effort: None,
            since_ms: None,
            raise_pid: None,
            launcher: None,
            completion_id: None,
            queued_messages: None,
            send_channel: None,
            send_blocked: None,
        }
    }

    pub fn with_hook_session_id(mut self, agent_session_id: &str) -> Self {
        let hook_id = HookId::new(&self.agent, agent_session_id);
        self.id = hook_id.to_string();
        self.hook_id = Some(hook_id);
        self
    }

    pub fn with_hook_id(self, agent_session_id: &str) -> Self {
        self.with_hook_session_id(agent_session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_session_without_the_message_fields_still_deserialises_and_a_queue_round_trips() {
        let legacy: Session = serde_json::from_value(json!({
            "id": "claude:42", "agent": "claude", "cwd": "/tmp/project",
            "title": "project", "pid": 42, "terminal": "kitty"
        }))
        .expect("legacy session deserialises");
        assert_eq!(legacy.queued_messages, None);
        assert_eq!(legacy.send_channel, None);
        assert_eq!(legacy.send_blocked, None);
        let wire = serde_json::to_value(&legacy).expect("serialises");
        assert!(wire.get("queued_messages").is_none());

        let mut session = Session::new("claude", "/tmp/project", 42, "kitty");
        session.send_channel = Some("tmux".to_owned());
        session.queued_messages = Some(vec![QueuedMessage {
            id: 7,
            text: "oi\ntudo".to_owned(),
            queued_at_ms: 1,
        }]);
        let wire = serde_json::to_value(&session).expect("serialises");
        assert_eq!(wire["send_channel"], json!("tmux"));
        assert_eq!(wire["queued_messages"][0]["id"], json!(7));
        let back: Session = serde_json::from_value(wire).expect("round trip");
        assert_eq!(back, session);
    }

    #[test]
    fn process_sessions_keep_the_legacy_identity_shape() {
        let session = Session::new("claude", "/tmp/project", 42, "kitty");

        assert_eq!(session.id, "claude:42");
    }

    #[test]
    fn hook_sessions_use_the_agent_session_identity() {
        let session =
            Session::new("claude", "/tmp/project", 42, "kitty").with_hook_session_id("abc123");

        assert_eq!(session.id, "claude:abc123");
        assert_eq!(
            session.hook_id.as_ref().map(HookId::as_str),
            Some("claude:abc123")
        );
    }

    #[test]
    fn legacy_session_json_does_not_gain_rich_fields() {
        let session = Session::new("claude", "/tmp/project", 42, "kitty");

        assert_eq!(
            serde_json::to_value(session).expect("session serializes"),
            json!({
                "id": "claude:42",
                "agent": "claude",
                "cwd": "/tmp/project",
                "title": "project",
                "pid": 42,
                "terminal": "kitty"
            })
        );
    }

    #[test]
    fn rich_session_fields_deserialize_when_present() {
        let session: Session = serde_json::from_value(json!({
            "id": "claude:abc123",
            "agent": "claude",
            "cwd": "/tmp/project",
            "title": "project",
            "pid": 42,
            "terminal": "kitty",
            "hook_id": "claude:abc123",
            "status": "working",
            "current_tool": "Bash",
            "summary": "Inspect the project",
            "completion_id": "daemon-1",
            "mode": "default",
            "subagents": [{"id": "a1", "kind": "Explore", "description": "look around"}],
            "permission_state": "pending"
        }))
        .expect("rich session deserializes");

        assert_eq!(
            session.hook_id.as_ref().map(HookId::as_str),
            Some("claude:abc123")
        );
        assert_eq!(session.status.as_deref(), Some("working"));
        assert_eq!(session.current_tool.as_deref(), Some("Bash"));
        assert_eq!(session.summary.as_deref(), Some("Inspect the project"));
        assert_eq!(session.completion_id.as_deref(), Some("daemon-1"));
        assert_eq!(session.mode.as_deref(), Some("default"));
        let subagents = session.subagents.as_ref().expect("subagents");
        assert_eq!(subagents.len(), 1);
        assert_eq!(subagents[0].kind, "Explore");
        assert_eq!(subagents[0].description.as_deref(), Some("look around"));
        assert!(!subagents[0].done);
        assert_eq!(session.permission_state, Some(PermissionState::Pending));
    }
}
