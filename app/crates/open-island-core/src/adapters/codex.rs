use super::{
    event_kind, hash_questions, optional_pid, optional_string, parse_json, parse_questions,
    question_from_event, string, ApprovalInput, NativeDecision, ParseError, ParsedIngress,
};
use crate::{HookEvent, HookId};
use serde_json::Value;

pub const QUESTION_TOOL: &str = "request_user_input";

/// The Codex TUI owns the answer and auto-resolves the question on its own after a minute
/// ("auto-resolves in 1m 00s"), so the island card carries the same deadline.
pub const QUESTION_TIMEOUT_MS: u64 = 60_000;

pub fn parse(input: &str) -> Result<Option<ParsedIngress>, ParseError> {
    let value = parse_json(input)?;
    let name = value
        .get("hook_event_name")
        .or_else(|| value.get("event"))
        .and_then(Value::as_str)
        .ok_or(ParseError::MissingField("hook_event_name"))?;
    let Some(event) = event_kind(name) else {
        return Ok(None);
    };
    let agent_session_id = string(&value, "session_id")?;
    let is_approval = matches!(&event, crate::protocol::HookEventKind::PermissionRequest);
    let mut normalized = HookEvent::new("codex", &agent_session_id, event);
    normalized.cwd = optional_string(&value, "cwd");
    normalized.pid = optional_pid(&value);
    normalized.mode = optional_string(&value, "mode");
    normalized.model = optional_string(&value, "model");
    normalized.status = optional_string(&value, "status");
    normalized.tool_name = optional_string(&value, "tool_name");
    normalized.tool_input = value.get("tool_input").cloned();
    normalized.tool_use_id = optional_string(&value, "tool_use_id");
    normalized.turn_id = optional_string(&value, "turn_id");
    normalized.prompt = optional_string(&value, "prompt");
    normalized.last_message = optional_string(&value, "last_assistant_message");
    normalized.approval_id = optional_string(&value, "approval_id");
    let approval = if is_approval {
        Some(ApprovalInput {
            approval_id: string(&value, "approval_id")?,
            session_id: HookId::new("codex", &agent_session_id),
            tool_name: normalized.tool_name.clone(),
            tool_input: normalized.tool_input.clone(),
            reason: optional_string(&value, "reason"),
        })
    } else {
        None
    };
    if normalized.tool_name.as_deref() == Some(QUESTION_TOOL) {
        let questions = normalized
            .tool_input
            .as_ref()
            .and_then(|input| input.get("questions"));
        normalized.question_id = Some(derived_question_id(
            &agent_session_id,
            normalized.tool_use_id.as_deref(),
            questions,
        ));
        normalized.questions = Some(parse_questions(questions));
    }
    let question = question_from_event(&normalized);
    Ok(Some(ParsedIngress {
        event: normalized,
        approval,
        question,
    }))
}

/// `tool_use_id` is the same value on the `PreToolUse` and `PostToolUse` of one
/// `request_user_input` call, so the card the first opens is the card the second clears.
fn derived_question_id(
    agent_session_id: &str,
    tool_use_id: Option<&str>,
    questions: Option<&Value>,
) -> String {
    match tool_use_id {
        Some(id) => format!("codex:{agent_session_id}:{id}"),
        None => format!(
            "codex:{agent_session_id}:{:016x}",
            hash_questions(questions)
        ),
    }
}

pub fn format_decision(decision: NativeDecision) -> String {
    let value = if decision.is_allow() { "allow" } else { "deny" };
    format!(r#"{{"decision":"{value}"}}"#)
}

pub fn parse_decision_output(input: &str) -> Result<Option<NativeDecision>, ParseError> {
    if input.trim().is_empty() {
        return Ok(None);
    }
    let value = parse_json(input)?;
    match value.get("decision").and_then(Value::as_str) {
        Some("allow") => Ok(Some(NativeDecision::Allow)),
        Some("deny") => Ok(Some(NativeDecision::Deny)),
        Some(_) => Err(ParseError::InvalidField("decision")),
        None => Ok(None),
    }
}
