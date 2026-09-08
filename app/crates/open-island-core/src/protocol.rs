use crate::session::{HookId, Session};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Request {
    pub v: u8,
    pub id: Value,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Response {
    pub v: u8,
    pub id: Value,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Event<T = Vec<Session>> {
    pub v: u8,
    pub event: String,
    pub data: T,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalDecision {
    Allow,
    Deny,
    #[serde(rename = "allow_always")]
    AllowAlways,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HookEventKind {
    #[serde(alias = "SessionStart", alias = "session_start")]
    SessionStart,
    #[serde(alias = "SessionEnd", alias = "session_end")]
    SessionEnd,
    #[serde(
        alias = "UserPromptSubmit",
        alias = "user_prompt_submit",
        alias = "prompt"
    )]
    UserPromptSubmit,
    #[serde(alias = "PreToolUse", alias = "pre_tool_use")]
    PreToolUse,
    #[serde(alias = "PostToolUse", alias = "post_tool_use")]
    PostToolUse,
    #[serde(alias = "Stop", alias = "session.idle")]
    Stop,
    #[serde(alias = "SubagentStop", alias = "subagent_stop")]
    SubagentStop,
    Status,
    #[serde(
        alias = "PermissionRequest",
        alias = "permission_request",
        alias = "permission"
    )]
    PermissionRequest,
    #[serde(
        alias = "QuestionAsked",
        alias = "question_asked",
        alias = "question.asked"
    )]
    QuestionAsked,
    #[serde(
        alias = "QuestionAnswered",
        alias = "question_answered",
        alias = "question.replied"
    )]
    QuestionAnswered,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct QuestionOption {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Question {
    pub question: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    #[serde(default)]
    pub options: Vec<QuestionOption>,
    #[serde(default, alias = "multiple", alias = "multiSelect")]
    pub multi_select: bool,
    #[serde(default)]
    pub custom: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct HookEvent {
    pub agent: String,
    pub session_id: HookId,
    #[serde(alias = "kind")]
    pub event: HookEventKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "tool")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_response: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "last_assistant_message"
    )]
    pub last_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub questions: Option<Vec<Question>>,
}

impl HookEvent {
    pub fn new(agent: &str, agent_session_id: &str, event: HookEventKind) -> Self {
        Self {
            agent: agent.to_owned(),
            session_id: HookId::new(agent, agent_session_id),
            event,
            agent_session_id: Some(agent_session_id.to_owned()),
            cwd: None,
            pid: None,
            tool_name: None,
            tool_input: None,
            tool_response: None,
            tool_use_id: None,
            agent_id: None,
            agent_type: None,
            turn_id: None,
            prompt: None,
            summary: None,
            last_message: None,
            status: None,
            mode: None,
            permission_mode: None,
            model: None,
            effort: None,
            approval_id: None,
            question_id: None,
            questions: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ApprovalRequest {
    pub approval_id: String,
    pub session_id: HookId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ApprovalResolved {
    pub approval_id: String,
    pub session_id: HookId,
    #[serde(alias = "approval_decision")]
    pub decision: ApprovalDecision,
}

pub type ApprovalResolution = ApprovalResolved;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum QuestionOutcome {
    Answered,
    Cancelled,
    Expired,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct QuestionRequest {
    pub question_id: String,
    pub session_id: HookId,
    pub agent: String,
    pub questions: Vec<Question>,
    pub answerable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_in_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct QuestionResolved {
    pub question_id: String,
    pub session_id: HookId,
    pub outcome: QuestionOutcome,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct QuestionFocus {
    pub question_id: String,
    pub session_id: HookId,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct QuestionAnswer {
    pub question_id: String,
    pub answers: Vec<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct IslandToggle {
    pub source: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Default)]
pub struct QuietScenes {
    pub active: bool,
    pub focus_mode: bool,
    pub screen_off: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ConfigChanged {
    pub config: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct UpdateAvailable {
    pub version: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum EventData {
    ApprovalResolved(ApprovalResolved),
    ApprovalRequested(ApprovalRequest),
    QuestionRequested(QuestionRequest),
    QuestionResolved(QuestionResolved),
    QuestionFocus(QuestionFocus),
    IslandToggle(IslandToggle),
    QuietScenes(QuietScenes),
    Sessions(Vec<Session>),
    ConfigChanged(ConfigChanged),
    UsageUpdated(crate::usage::UsageReport),
    UpdateAvailable(UpdateAvailable),
}

pub type EventPayload = EventData;
pub type V1Event<T = EventData> = Event<T>;
pub type GenericEvent = Event<EventData>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::HookId;
    use serde_json::json;

    #[test]
    fn legacy_event_json_keeps_sessions_as_an_array() {
        let event = Event {
            v: 1,
            event: "sessions-updated".to_owned(),
            data: vec![Session::new("claude", "/tmp/project", 42, "kitty")],
        };

        assert_eq!(
            serde_json::to_value(event).expect("event serializes"),
            json!({
                "v": 1,
                "event": "sessions-updated",
                "data": [{
                    "id": "claude:42",
                    "agent": "claude",
                    "cwd": "/tmp/project",
                    "title": "project",
                    "pid": 42,
                    "terminal": "kitty"
                }]
            })
        );

        let decoded: GenericEvent = serde_json::from_value(json!({
            "v": 1,
            "event": "sessions-updated",
            "data": [{
                "id": "claude:42",
                "agent": "claude",
                "cwd": "/tmp/project",
                "title": "project",
                "pid": 42,
                "terminal": "kitty"
            }]
        }))
        .expect("legacy event deserializes through the generic payload");
        assert!(matches!(decoded.data, EventData::Sessions(_)));
    }

    #[test]
    fn a_one_session_array_is_a_session_list_and_never_a_config_document() {
        let decoded: GenericEvent = serde_json::from_value(json!({
            "v": 1,
            "event": "sessions-updated",
            "data": [{
                "id": "claude:42",
                "agent": "claude",
                "cwd": "/tmp/project",
                "title": "project",
                "pid": 42,
                "terminal": "kitty"
            }]
        }))
        .expect("event deserializes");

        assert!(
            matches!(decoded.data, EventData::Sessions(_)),
            "a single-element array also fits a one-field struct, so EventData::Sessions has to be tried first"
        );

        let decoded: GenericEvent = serde_json::from_value(json!({
            "v": 1,
            "event": "config-changed",
            "data": {"config": {"sound": {"quiet": true}}}
        }))
        .expect("event deserializes");

        assert!(matches!(decoded.data, EventData::ConfigChanged(_)));
    }

    #[test]
    fn legacy_request_and_response_envelopes_keep_their_wire_shape() {
        let ping = Request {
            v: 1,
            id: json!(1),
            method: "ping".to_owned(),
            params: Some(json!({})),
        };
        let list_sessions = Request {
            v: 1,
            id: json!(2),
            method: "list_sessions".to_owned(),
            params: Some(json!({})),
        };
        let request = Request {
            v: 1,
            id: json!(7),
            method: "jump".to_owned(),
            params: Some(json!({"id": "claude:42"})),
        };
        let response = Response {
            v: 1,
            id: json!(7),
            ok: true,
            data: Some(json!(null)),
            error: None,
        };

        assert_eq!(
            serde_json::to_value(ping).expect("ping request serializes"),
            json!({"v": 1, "id": 1, "method": "ping", "params": {}})
        );
        assert_eq!(
            serde_json::to_value(list_sessions).expect("list request serializes"),
            json!({"v": 1, "id": 2, "method": "list_sessions", "params": {}})
        );
        assert_eq!(
            serde_json::to_value(request).expect("request serializes"),
            json!({
                "v": 1,
                "id": 7,
                "method": "jump",
                "params": {"id": "claude:42"}
            })
        );
        assert_eq!(
            serde_json::to_value(response).expect("response serializes"),
            json!({"v": 1, "id": 7, "ok": true, "data": null})
        );
    }

    #[test]
    fn approval_events_carry_their_correlation_ids() {
        let event: V1Event<EventData> = Event {
            v: 1,
            event: "approval-requested".to_owned(),
            data: EventData::ApprovalRequested(ApprovalRequest {
                approval_id: "approval-1".to_owned(),
                session_id: HookId::new("claude", "abc123"),
                tool_name: Some("Bash".to_owned()),
                tool_input: Some(json!({"command": "pwd"})),
                reason: Some("Run a shell command".to_owned()),
            }),
        };

        let value = serde_json::to_value(event).expect("approval event serializes");
        assert_eq!(value["data"]["approval_id"], "approval-1");
        assert_eq!(value["data"]["session_id"], "claude:abc123");
    }

    #[test]
    fn approval_resolution_is_a_generic_event_payload() {
        let event: GenericEvent = Event {
            v: 1,
            event: "approval-resolved".to_owned(),
            data: EventData::ApprovalResolved(ApprovalResolved {
                approval_id: "approval-1".to_owned(),
                session_id: HookId::new("claude", "abc123"),
                decision: ApprovalDecision::Deny,
            }),
        };

        let encoded = serde_json::to_string(&event).expect("resolution event serializes");
        let decoded: GenericEvent = serde_json::from_str(&encoded).expect("event deserializes");

        assert_eq!(decoded, event);
    }

    #[test]
    fn approval_decisions_are_explicit_and_closed() {
        assert_eq!(
            serde_json::to_value(ApprovalDecision::Allow).expect("decision serializes"),
            json!("allow")
        );
        assert_eq!(
            serde_json::to_value(ApprovalDecision::Deny).expect("decision serializes"),
            json!("deny")
        );
        assert!(serde_json::from_value::<ApprovalDecision>(json!("maybe")).is_err());
    }

    #[test]
    fn approval_requests_require_both_correlation_ids() {
        assert!(serde_json::from_value::<ApprovalRequest>(json!({
            "approval_id": "approval-1"
        }))
        .is_err());
        assert!(serde_json::from_value::<ApprovalRequest>(json!({
            "session_id": "claude:abc123"
        }))
        .is_err());
    }

    fn question() -> Question {
        Question {
            question: "Qual cor?".to_owned(),
            header: Some("Cor".to_owned()),
            options: vec![QuestionOption {
                label: "Vermelho".to_owned(),
                description: None,
            }],
            multi_select: false,
            custom: false,
            id: None,
        }
    }

    #[test]
    fn every_question_event_round_trips_through_the_untagged_payload() {
        // EventData is untagged, so a payload that also matches an earlier variant would be
        // silently decoded as the wrong one. Every variant has to come back as itself.
        let session_id = HookId::new("claude", "abc123");
        let payloads = [
            EventData::QuestionRequested(QuestionRequest {
                question_id: "q-1".to_owned(),
                session_id: session_id.clone(),
                agent: "claude".to_owned(),
                questions: vec![question()],
                answerable: true,
                expires_in_ms: None,
            }),
            EventData::QuestionResolved(QuestionResolved {
                question_id: "q-1".to_owned(),
                session_id: session_id.clone(),
                outcome: QuestionOutcome::Answered,
            }),
            EventData::QuestionFocus(QuestionFocus {
                question_id: "q-1".to_owned(),
                session_id: session_id.clone(),
            }),
            EventData::ApprovalRequested(ApprovalRequest {
                approval_id: "approval-1".to_owned(),
                session_id: session_id.clone(),
                tool_name: Some("Bash".to_owned()),
                tool_input: None,
                reason: None,
            }),
            EventData::ApprovalResolved(ApprovalResolved {
                approval_id: "approval-1".to_owned(),
                session_id,
                decision: ApprovalDecision::Allow,
            }),
        ];
        for payload in payloads {
            let encoded = serde_json::to_string(&payload).expect("payload serializes");
            let decoded: EventData = serde_json::from_str(&encoded).expect("payload decodes");
            assert_eq!(decoded, payload, "{encoded}");
        }
    }

    #[test]
    fn question_shapes_of_all_three_agents_deserialize_into_one_type() {
        let claude: Question = serde_json::from_value(json!({
            "question": "Qual cor?", "header": "Cor",
            "options": [{"label": "Vermelho", "description": "A cor vermelha"}],
            "multiSelect": true
        }))
        .expect("Claude question");
        let codex: Question = serde_json::from_value(json!({
            "header": "Colour", "id": "colour", "question": "Which colour?",
            "options": [{"label": "Vermelho", "description": "Choose red."}]
        }))
        .expect("Codex question");
        let opencode: Question = serde_json::from_value(json!({
            "question": "Qual cor?", "header": "Cor",
            "options": [{"label": "Vermelho"}], "multiple": true, "custom": true
        }))
        .expect("OpenCode question");

        assert!(claude.multi_select);
        assert!(!codex.multi_select);
        assert_eq!(codex.id.as_deref(), Some("colour"));
        assert!(opencode.multi_select);
        assert!(opencode.custom);
        assert_eq!(opencode.options[0].description, None);
    }

    #[test]
    fn hook_events_without_a_question_keep_the_legacy_wire_shape() {
        let event = HookEvent::new("claude", "abc123", HookEventKind::PreToolUse);
        let value = serde_json::to_value(event).expect("event serializes");
        assert!(value.get("question_id").is_none());
        assert!(value.get("questions").is_none());
    }

    #[test]
    fn normalized_hook_events_use_canonical_hook_ids() {
        let event = HookEvent::new("claude", "abc123", HookEventKind::SessionStart);

        assert_eq!(event.session_id.as_str(), "claude:abc123");
        assert_eq!(
            serde_json::to_value(event).expect("hook event serializes")["event"],
            "session-start"
        );
    }

    #[test]
    fn an_available_update_round_trips_through_the_event_payload_and_captures_nothing_else() {
        let event: GenericEvent = Event {
            v: 1,
            event: "update-available".to_owned(),
            data: EventData::UpdateAvailable(UpdateAvailable {
                version: "v0.2.0".to_owned(),
            }),
        };
        let wire = serde_json::to_value(&event).expect("event serializes");
        assert_eq!(
            wire,
            json!({"v": 1, "event": "update-available", "data": {"version": "v0.2.0"}})
        );
        let decoded: GenericEvent = serde_json::from_value(wire).expect("event deserializes");
        assert_eq!(decoded, event);
        assert!(
            matches!(&decoded.data, EventData::UpdateAvailable(update) if update.version == "v0.2.0"),
            "an update payload was captured by an earlier variant: {:?}",
            decoded.data
        );

        let decoded: GenericEvent = serde_json::from_value(json!({
            "v": 1,
            "event": "config-changed",
            "data": {"config": {"updates": {"check_enabled": false}}}
        }))
        .expect("event deserializes");
        assert!(
            matches!(decoded.data, EventData::ConfigChanged(_)),
            "a config document was captured by the update variant: {:?}",
            decoded.data
        );
    }
}
