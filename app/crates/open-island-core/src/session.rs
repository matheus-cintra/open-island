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
        assert_eq!(session.mode.as_deref(), Some("default"));
        let subagents = session.subagents.as_ref().expect("subagents");
        assert_eq!(subagents.len(), 1);
        assert_eq!(subagents[0].kind, "Explore");
        assert_eq!(subagents[0].description.as_deref(), Some("look around"));
        assert!(!subagents[0].done);
        assert_eq!(session.permission_state, Some(PermissionState::Pending));
    }
}
