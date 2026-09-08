//! Protocol-specific ingress and decision contracts for agent adapters.

mod claude;
mod codex;
mod opencode;

use crate::protocol::{HookEvent, Question};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

pub use claude::{
    format_always_decision as format_claude_always, format_decision as format_claude_decision,
    format_question_decision as format_claude_answer, parse as parse_claude,
    parse_decision_output as parse_claude_decision, plan_return_mode as claude_plan_return_mode,
    PLAN_TOOL as CLAUDE_PLAN_TOOL, QUESTION_TOOL as CLAUDE_QUESTION_TOOL,
};
pub use codex::{
    format_decision as format_codex_decision, parse as parse_codex,
    parse_decision_output as parse_codex_decision,
    QUESTION_TIMEOUT_MS as CODEX_QUESTION_TIMEOUT_MS, QUESTION_TOOL as CODEX_QUESTION_TOOL,
};
pub use opencode::{
    format_decision as format_opencode_decision, parse as parse_opencode,
    parse_decision_output as parse_opencode_decision,
};

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedIngress {
    pub event: HookEvent,
    pub approval: Option<ApprovalInput>,
    pub question: Option<QuestionInput>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestionInput {
    pub question_id: String,
    pub session_id: crate::session::HookId,
    pub agent: String,
    pub questions: Vec<Question>,
    pub answerable: bool,
    pub expires_in_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ApprovalInput {
    pub approval_id: String,
    pub session_id: crate::session::HookId,
    pub tool_name: Option<String>,
    pub tool_input: Option<Value>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    InvalidJson(String),
    MissingField(&'static str),
    InvalidField(&'static str),
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(error) => write!(formatter, "invalid hook JSON: {error}"),
            Self::MissingField(field) => write!(formatter, "missing hook field: {field}"),
            Self::InvalidField(field) => write!(formatter, "invalid hook field: {field}"),
        }
    }
}

impl std::error::Error for ParseError {}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NativeDecision {
    Allow,
    Deny,
    #[serde(rename = "allow_always")]
    AllowAlways,
}

impl NativeDecision {
    pub const fn is_allow(self) -> bool {
        matches!(self, Self::Allow | Self::AllowAlways)
    }
}

pub(crate) fn parse_json(input: &str) -> Result<Value, ParseError> {
    serde_json::from_str(input).map_err(|error| ParseError::InvalidJson(error.to_string()))
}

pub(crate) fn string(value: &Value, field: &'static str) -> Result<String, ParseError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or(ParseError::MissingField(field))
}

pub(crate) fn optional_string(value: &Value, field: &'static str) -> Option<String> {
    value.get(field).and_then(Value::as_str).map(str::to_owned)
}

pub(crate) fn optional_pid(value: &Value) -> Option<u32> {
    value
        .get("pid")
        .and_then(Value::as_u64)
        .and_then(|pid| u32::try_from(pid).ok())
}

/// Decides whether a normalized event opens a question card and under what policy.
///
/// One place so the hook process and the daemon agree: the hook parses the payload and the
/// daemon only ever sees the normalized `HookEvent`.
pub fn question_from_event(event: &HookEvent) -> Option<QuestionInput> {
    use crate::protocol::HookEventKind;
    let question_id = event.question_id.clone()?;
    let questions = event.questions.clone()?;
    let (answerable, expires_in_ms) = match (event.agent.as_str(), &event.event) {
        ("claude", HookEventKind::PermissionRequest) => (true, None),
        ("opencode", HookEventKind::QuestionAsked) => (true, None),
        ("codex", HookEventKind::PreToolUse) => (false, Some(codex::QUESTION_TIMEOUT_MS)),
        _ => return None,
    };
    Some(QuestionInput {
        question_id,
        session_id: event.session_id.clone(),
        agent: event.agent.clone(),
        questions,
        answerable,
        expires_in_ms,
    })
}

/// The mirror of `question_from_event`: the event by which an agent reports that it
/// resolved the question itself, in its own terminal. It carries the same question id.
pub fn question_closed_by_event(event: &HookEvent) -> Option<String> {
    use crate::protocol::HookEventKind;
    let question_id = event.question_id.clone()?;
    matches!(
        event.event,
        HookEventKind::PostToolUse | HookEventKind::QuestionAnswered
    )
    .then_some(question_id)
}

pub(crate) fn parse_questions(value: Option<&Value>) -> Vec<Question> {
    value
        .and_then(Value::as_array)
        .map(|questions| {
            questions
                .iter()
                .filter_map(|question| serde_json::from_value(question.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn hash_questions(questions: Option<&Value>) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    questions.map(Value::to_string).hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn event_kind(name: &str) -> Option<crate::protocol::HookEventKind> {
    use crate::protocol::HookEventKind;
    match name {
        "question.asked" => Some(HookEventKind::QuestionAsked),
        "question.replied" => Some(HookEventKind::QuestionAnswered),
        "SessionStart" | "session_start" | "session.created" => Some(HookEventKind::SessionStart),
        "SessionEnd" | "session_end" | "session.deleted" => Some(HookEventKind::SessionEnd),
        "UserPromptSubmit" | "user_prompt_submit" | "open-island.prompt" => {
            Some(HookEventKind::UserPromptSubmit)
        }
        "PreToolUse" | "pre_tool_use" | "tool.execute.before" => Some(HookEventKind::PreToolUse),
        "PostToolUse" | "post_tool_use" | "tool.execute.after" => Some(HookEventKind::PostToolUse),
        "Stop" | "stop" | "session.idle" => Some(HookEventKind::Stop),
        "PermissionRequest" | "permission_request" | "permission.asked" => {
            Some(HookEventKind::PermissionRequest)
        }
        "Status" | "status" | "session.status" | "session.updated" => Some(HookEventKind::Status),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::HookEventKind;
    use serde_json::json;

    const CLAUDE_FIXTURE: &str = r#"{
        "session_id":"abc123","cwd":"/tmp/project","pid":42,
        "hook_event_name":"PreToolUse","permission_mode":"ask",
        "tool_name":"Bash","tool_input":{"command":"pwd"},"tool_use_id":"tool-1"
    }"#;
    const CLAUDE_REAL_PERMISSION_FIXTURE: &str = r#"{
        "session_id":"abc123","transcript_path":"/tmp/t.jsonl","cwd":"/tmp/project",
        "prompt_id":"prompt-9","permission_mode":"default","effort":{"level":"high"},
        "hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"pwd"},
        "permission_suggestions":[{"type":"setMode","mode":"acceptEdits","destination":"session"}]
    }"#;
    const CODEX_FIXTURE: &str = r#"{
        "session_id":"abc123","cwd":"/tmp/project","pid":42,"event":"PermissionRequest",
        "mode":"default","tool_name":"Bash","tool_input":{"command":"pwd"},"approval_id":"approval-1"
    }"#;
    const OPENCODE_FIXTURE: &str = r#"{
        "type":"permission.asked","properties":{"sessionID":"abc123","id":"permission-1",
        "permission":"bash","patterns":["pwd"],"metadata":{"command":"pwd"}},"cwd":"/tmp/project"
    }"#;
    const CLAUDE_QUESTION_FIXTURE: &str = r#"{
        "session_id":"abc123","cwd":"/tmp/project","prompt_id":"prompt-9",
        "permission_mode":"bypassPermissions","hook_event_name":"PermissionRequest",
        "tool_name":"AskUserQuestion","tool_input":{"questions":[{"question":"Qual cor?",
        "header":"Cor","options":[{"label":"Vermelho","description":"A cor vermelha"},
        {"label":"Azul","description":"A cor azul"}],"multiSelect":false}]}
    }"#;
    const CODEX_QUESTION_FIXTURE: &str = r#"{
        "session_id":"abc123","turn_id":"turn-1","cwd":"/tmp/project",
        "hook_event_name":"PreToolUse","permission_mode":"default",
        "tool_name":"request_user_input","tool_use_id":"call_1",
        "tool_input":{"questions":[{"header":"Colour","id":"colour","question":"Which colour?",
        "options":[{"label":"Vermelho","description":"Choose red."},
        {"label":"Azul","description":"Choose blue."}]}]}
    }"#;
    const OPENCODE_QUESTION_FIXTURE: &str = r#"{
        "type":"question.asked","properties":{"id":"que_1","sessionID":"abc123",
        "questions":[{"question":"Qual cor?","header":"Cor preferida",
        "options":[{"label":"Vermelho","description":"Escolher a cor vermelha"},
        {"label":"Azul","description":"Escolher a cor azul"}],"multiple":true,"custom":true}],
        "tool":{"messageID":"msg_1","callID":"call_1"}}
    }"#;

    #[test]
    fn representative_fixtures_normalize_identity_and_tool_context() {
        let claude = parse_claude(CLAUDE_FIXTURE)
            .expect("Claude parses")
            .expect("supported");
        assert_eq!(claude.event.session_id.as_str(), "claude:abc123");
        assert_eq!(claude.event.cwd.as_deref(), Some("/tmp/project"));
        assert_eq!(claude.event.pid, Some(42));
        assert_eq!(claude.event.tool_name.as_deref(), Some("Bash"));
        assert_eq!(claude.event.event, HookEventKind::PreToolUse);

        let codex = parse_codex(CODEX_FIXTURE)
            .expect("Codex parses")
            .expect("supported");
        assert_eq!(codex.event.session_id.as_str(), "codex:abc123");
        assert_eq!(
            codex
                .approval
                .as_ref()
                .map(|approval| approval.approval_id.as_str()),
            Some("approval-1")
        );

        let opencode = parse_opencode(OPENCODE_FIXTURE)
            .expect("OpenCode parses")
            .expect("supported");
        assert_eq!(opencode.event.session_id.as_str(), "opencode:abc123");
        assert_eq!(opencode.event.cwd.as_deref(), Some("/tmp/project"));
        assert_eq!(opencode.event.tool_name.as_deref(), Some("bash"));
        assert_eq!(opencode.event.approval_id.as_deref(), Some("permission-1"));
    }

    #[test]
    fn each_agent_carries_the_model_where_its_own_payload_puts_it() {
        let claude = r#"{"session_id":"abc123","cwd":"/tmp/p","hook_event_name":"SessionStart",
            "source":"startup","model":"claude-opus-5[1m]"}"#;
        let event = parse_claude(claude)
            .expect("parses")
            .expect("an event")
            .event;
        assert_eq!(event.agent, "claude");
        assert_eq!(event.model.as_deref(), Some("claude-opus-5[1m]"));

        let codex = r#"{"session_id":"abc123","cwd":"/tmp/p","hook_event_name":"PreToolUse",
            "model":"gpt-6-astra","tool_name":"Bash","tool_input":{"command":"pwd"}}"#;
        let event = parse_codex(codex).expect("parses").expect("an event").event;
        assert_eq!(event.model.as_deref(), Some("gpt-6-astra"));

        let opencode = r#"{"type":"session.updated","cwd":"/tmp/p","properties":{
            "sessionID":"ses_x","info":{"agent":"Sisyphus - ultraworker",
            "model":{"id":"gpt-5.6-sol","providerID":"openai","variant":"xhigh"}}}}"#;
        let event = parse_opencode(opencode)
            .expect("parses")
            .expect("an event")
            .event;
        assert_eq!(event.model.as_deref(), Some("gpt-5.6-sol"));
        assert_eq!(event.effort.as_deref(), Some("xhigh"));
    }

    #[test]
    fn claude_reasoning_effort_comes_from_the_nested_level() {
        let event = parse_claude(CLAUDE_REAL_PERMISSION_FIXTURE)
            .expect("parses")
            .expect("an event")
            .event;

        assert_eq!(event.effort.as_deref(), Some("high"));
        assert_eq!(event.model, None);
    }

    /// OpenCode runs our Claude hook through its own bridge. Without the `hook_source`
    /// switch the same session arrives twice under two identities and one of them is
    /// dropped by whichever loses the race for the process.
    #[test]
    fn the_opencode_bridge_keeps_the_opencode_identity() {
        let bridged = r#"{"session_id":"ses_f88567","cwd":"/tmp/p","hook_event_name":"PreToolUse",
            "hook_source":"opencode-plugin","tool_name":"TodoWrite","tool_input":{"todos":[]}}"#;
        let parsed = parse_claude(bridged).expect("parses").expect("an event");

        assert_eq!(parsed.event.agent, "opencode");
        assert_eq!(parsed.event.session_id.as_str(), "opencode:ses_f88567");
        assert_eq!(parsed.event.tool_name.as_deref(), Some("TodoWrite"));
    }

    #[test]
    fn a_payload_without_a_hook_source_is_still_claude() {
        let parsed = parse_claude(CLAUDE_FIXTURE)
            .expect("parses")
            .expect("an event");

        assert_eq!(parsed.event.agent, "claude");
        assert_eq!(parsed.event.session_id.as_str(), "claude:abc123");
    }

    #[test]
    fn allow_always_echoes_the_suggestions_the_request_offered() {
        let suggestions =
            json!([{"type": "setMode", "mode": "acceptEdits", "destination": "session"}]);
        let output: Value =
            serde_json::from_str(&format_claude_always(Some(&suggestions), None, None))
                .expect("valid JSON");

        assert_eq!(
            output["hookSpecificOutput"]["decision"]["behavior"],
            "allow"
        );
        assert_eq!(
            output["hookSpecificOutput"]["decision"]["updatedPermissions"],
            suggestions
        );
    }

    #[test]
    fn a_plan_returns_to_the_mode_the_session_was_in_before_it() {
        for mode in ["auto", "acceptEdits", "default"] {
            assert_eq!(claude_plan_return_mode(Some(mode)), mode);
        }
    }

    #[test]
    fn bypass_is_not_settable_from_a_hook_so_a_plan_returns_to_auto() {
        assert_eq!(claude_plan_return_mode(Some("bypassPermissions")), "auto");
        assert_eq!(claude_plan_return_mode(None), "auto");
        assert_eq!(claude_plan_return_mode(Some("plan")), "auto");
    }

    #[test]
    fn allow_always_without_suggestions_degrades_to_a_plain_allow() {
        let plain = format_claude_decision(NativeDecision::Allow, None, None);

        assert_eq!(format_claude_always(None, None, None), plain);
        assert_eq!(format_claude_always(Some(&json!([])), None, None), plain);
    }

    #[test]
    fn each_agent_has_its_own_word_for_always() {
        assert_eq!(
            format_opencode_decision(NativeDecision::AllowAlways),
            r#"{"reply":"always"}"#
        );
        assert_eq!(
            format_opencode_decision(NativeDecision::Allow),
            r#"{"reply":"once"}"#
        );
        assert!(NativeDecision::AllowAlways.is_allow());
    }

    #[test]
    fn claude_permission_request_without_approval_id_derives_a_stable_id() {
        let first = parse_claude(CLAUDE_REAL_PERMISSION_FIXTURE)
            .expect("Claude parses")
            .expect("supported");
        let approval = first.approval.as_ref().expect("approval");
        assert!(
            approval.approval_id.starts_with("claude:abc123:prompt-9:"),
            "{}",
            approval.approval_id
        );
        assert_eq!(
            first.event.approval_id.as_deref(),
            Some(approval.approval_id.as_str())
        );
        let again = parse_claude(CLAUDE_REAL_PERMISSION_FIXTURE)
            .expect("Claude parses")
            .expect("supported");
        assert_eq!(
            again.approval.map(|approval| approval.approval_id),
            Some(approval.approval_id.clone())
        );
        let other =
            CLAUDE_REAL_PERMISSION_FIXTURE.replace(r#""command":"pwd""#, r#""command":"ls""#);
        let other = parse_claude(&other)
            .expect("Claude parses")
            .expect("supported");
        assert_ne!(
            other.approval.expect("approval").approval_id,
            approval.approval_id
        );
    }

    #[test]
    fn malformed_and_unsupported_inputs_are_not_approvals() {
        assert!(matches!(parse_claude("{"), Err(ParseError::InvalidJson(_))));
        assert!(matches!(
            parse_codex(r#"{"hook_event_name":"PreToolUse"}"#),
            Err(ParseError::MissingField("session_id"))
        ));
        assert_eq!(
            parse_opencode(r#"{"type":"message.updated","properties":{"sessionID":"abc123"}}"#)
                .expect("valid unsupported event"),
            None
        );
    }

    #[test]
    fn question_fixtures_normalize_to_one_shape_across_agents() {
        let claude = parse_claude(CLAUDE_QUESTION_FIXTURE)
            .expect("Claude parses")
            .expect("supported")
            .question
            .expect("Claude opens a question");
        assert_eq!(claude.agent, "claude");
        assert!(claude.answerable);
        assert_eq!(claude.expires_in_ms, None);
        assert_eq!(claude.questions.len(), 1);
        assert_eq!(claude.questions[0].header.as_deref(), Some("Cor"));
        assert_eq!(claude.questions[0].options[0].label, "Vermelho");
        assert!(!claude.questions[0].multi_select);

        let codex = parse_codex(CODEX_QUESTION_FIXTURE)
            .expect("Codex parses")
            .expect("supported")
            .question
            .expect("Codex opens a question");
        assert!(!codex.answerable);
        assert_eq!(codex.expires_in_ms, Some(CODEX_QUESTION_TIMEOUT_MS));
        assert_eq!(codex.question_id, "codex:abc123:call_1");
        assert_eq!(codex.questions[0].id.as_deref(), Some("colour"));

        let opencode = parse_opencode(OPENCODE_QUESTION_FIXTURE)
            .expect("OpenCode parses")
            .expect("supported")
            .question
            .expect("OpenCode opens a question");
        assert!(opencode.answerable);
        assert_eq!(opencode.question_id, "que_1");
        assert!(opencode.questions[0].multi_select);
        assert!(opencode.questions[0].custom);
    }

    #[test]
    fn claude_ask_user_question_is_a_question_and_never_an_approval() {
        let parsed = parse_claude(CLAUDE_QUESTION_FIXTURE)
            .expect("Claude parses")
            .expect("supported");
        assert!(
            parsed.approval.is_none(),
            "AskUserQuestion must not open an allow/deny card"
        );
        assert!(parsed.event.approval_id.is_none());
        assert!(parsed.event.question_id.is_some());
    }

    #[test]
    fn claude_question_id_survives_the_answers_added_on_post_tool_use() {
        let asked = parse_claude(CLAUDE_QUESTION_FIXTURE)
            .expect("parses")
            .expect("supported");
        let answered = CLAUDE_QUESTION_FIXTURE
            .replace("PermissionRequest", "PostToolUse")
            .replace(
                r#""multiSelect":false}]}"#,
                r#""multiSelect":false}],"answers":{"Qual cor?":"Vermelho"}}"#,
            );
        let answered = parse_claude(&answered).expect("parses").expect("supported");
        assert_eq!(asked.event.question_id, answered.event.question_id);
        assert!(
            answered.question.is_none(),
            "PostToolUse closes the card, it does not open one"
        );
    }

    #[test]
    fn codex_post_tool_use_clears_the_card_the_pre_tool_use_opened() {
        let asked = parse_codex(CODEX_QUESTION_FIXTURE)
            .expect("parses")
            .expect("supported");
        let answered = CODEX_QUESTION_FIXTURE.replace("PreToolUse", "PostToolUse");
        let answered = parse_codex(&answered).expect("parses").expect("supported");
        assert_eq!(asked.event.question_id, answered.event.question_id);
        assert!(answered.question.is_none());
    }

    #[test]
    fn opencode_question_reply_closes_without_opening() {
        let replied = r#"{"type":"question.replied","properties":{"sessionID":"abc123",
            "requestID":"que_1","answers":[["Vermelho"]]}}"#;
        let parsed = parse_opencode(replied).expect("parses").expect("supported");
        assert_eq!(parsed.event.question_id.as_deref(), Some("que_1"));
        assert!(parsed.question.is_none());
        assert!(parsed.approval.is_none());
    }

    #[test]
    fn every_agent_reports_the_stop_through_the_same_event_kind() {
        let claude = parse_claude(
            r#"{"session_id":"abc123","cwd":"/tmp/project","hook_event_name":"Stop",
                "stop_hook_active":false,"last_assistant_message":"DONE=1"}"#,
        )
        .expect("parses")
        .expect("supported");
        let codex = parse_codex(
            r#"{"session_id":"abc123","cwd":"/tmp/project","hook_event_name":"Stop",
                "stop_hook_active":false,"last_assistant_message":"DONE=1"}"#,
        )
        .expect("parses")
        .expect("supported");
        let opencode =
            parse_opencode(r#"{"type":"session.idle","properties":{"sessionID":"abc123"}}"#)
                .expect("parses")
                .expect("supported");

        assert_eq!(claude.event.event, HookEventKind::Stop);
        assert_eq!(codex.event.event, HookEventKind::Stop);
        assert_eq!(opencode.event.event, HookEventKind::Stop);
        assert!(opencode.approval.is_none());
    }

    #[test]
    fn an_opencode_session_status_is_not_a_stop() {
        let busy = parse_opencode(
            r#"{"type":"session.status","properties":{"sessionID":"abc123","status":{"type":"idle"}}}"#,
        )
        .expect("parses")
        .expect("supported");

        assert_eq!(
            busy.event.event,
            HookEventKind::Status,
            "session.idle is the stop edge; session.status only bumps liveness"
        );
    }

    #[test]
    fn the_opencode_plugin_prompt_arrives_as_a_user_prompt() {
        let parsed = parse_opencode(
            r#"{"type":"open-island.prompt","properties":{"sessionID":"abc123",
                "text":"Answer agent questions from the island"}}"#,
        )
        .expect("parses")
        .expect("supported");

        assert_eq!(parsed.event.event, HookEventKind::UserPromptSubmit);
        assert_eq!(
            parsed.event.prompt.as_deref(),
            Some("Answer agent questions from the island")
        );
    }

    #[test]
    fn claude_answers_use_a_string_for_one_label_and_an_array_for_many() {
        let parsed = parse_claude(CLAUDE_QUESTION_FIXTURE)
            .expect("parses")
            .expect("supported");
        let questions = parsed.event.questions.clone().expect("questions");
        let tool_input = parsed.event.tool_input.clone();

        let single = format_claude_answer(
            tool_input.as_ref(),
            &questions,
            Some(&[vec!["Vermelho".to_owned()]]),
        );
        let single: Value = serde_json::from_str(&single).expect("JSON");
        let decision = &single["hookSpecificOutput"]["decision"];
        assert_eq!(decision["behavior"], "allow");
        assert_eq!(decision["updatedInput"]["answers"]["Qual cor?"], "Vermelho");
        assert!(decision["updatedInput"]["questions"].is_array());

        let mut multi = questions.clone();
        multi[0].multi_select = true;
        let many = format_claude_answer(
            tool_input.as_ref(),
            &multi,
            Some(&[vec!["Vermelho".to_owned(), "Azul".to_owned()]]),
        );
        let many: Value = serde_json::from_str(&many).expect("JSON");
        assert_eq!(
            many["hookSpecificOutput"]["decision"]["updatedInput"]["answers"]["Qual cor?"],
            json!(["Vermelho", "Azul"])
        );

        let custom = format_claude_answer(
            tool_input.as_ref(),
            &questions,
            Some(&[vec!["Roxo".to_owned()]]),
        );
        let custom: Value = serde_json::from_str(&custom).expect("JSON");
        assert_eq!(
            custom["hookSpecificOutput"]["decision"]["updatedInput"]["answers"]["Qual cor?"],
            "Roxo"
        );
    }

    #[test]
    fn an_unanswered_claude_question_allows_the_call_unchanged() {
        let plain = format_claude_decision(NativeDecision::Allow, None, None);
        for answers in [None, Some(&[][..]), Some(&[vec![]][..])] {
            assert_eq!(
                format_claude_answer(Some(&json!({"questions": []})), &[], answers),
                plain,
                "an unanswered question hands the call to the Claude TUI, it never denies"
            );
        }
    }

    #[test]
    fn native_decisions_remain_protocol_specific() {
        assert_eq!(
            serde_json::from_str::<Value>(&format_claude_decision(
                NativeDecision::Allow,
                None,
                None
            ))
            .expect("JSON"),
            json!({"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}})
        );
        assert_eq!(
            serde_json::from_str::<Value>(&format_codex_decision(NativeDecision::Deny))
                .expect("JSON"),
            json!({"decision":"deny"})
        );
        assert_eq!(
            serde_json::from_str::<Value>(&format_opencode_decision(NativeDecision::Allow))
                .expect("JSON"),
            json!({"reply":"once"})
        );
        assert_eq!(
            serde_json::from_str::<Value>(&format_opencode_decision(NativeDecision::Deny))
                .expect("JSON"),
            json!({"reply":"reject"})
        );
        assert_eq!(
            parse_claude_decision("").expect("empty Claude output"),
            None
        );
        assert_eq!(
            parse_codex_decision("\n").expect("empty Codex output"),
            None
        );
        assert_eq!(
            parse_opencode_decision(" ").expect("empty OpenCode output"),
            None
        );
    }
}
