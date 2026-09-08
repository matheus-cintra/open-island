use super::{
    event_kind, optional_pid, optional_string, parse_json, parse_questions, question_from_event,
    string, ApprovalInput, NativeDecision, ParseError, ParsedIngress,
};
use crate::{protocol::HookEventKind, HookEvent, HookId};
use serde_json::Value;

fn properties(value: &Value) -> &Value {
    value.get("properties").unwrap_or(value)
}

pub fn parse(input: &str) -> Result<Option<ParsedIngress>, ParseError> {
    let value = parse_json(input)?;
    let name = string(&value, "type")?;
    let Some(event) = event_kind(&name) else {
        return Ok(None);
    };
    let data = properties(&value);
    let agent_session_id = data
        .get("sessionID")
        .or_else(|| data.get("session_id"))
        .and_then(Value::as_str)
        .ok_or(ParseError::MissingField("sessionID"))?;
    let is_approval = matches!(&event, HookEventKind::PermissionRequest);
    let is_question = matches!(
        &event,
        HookEventKind::QuestionAsked | HookEventKind::QuestionAnswered
    );
    let mut normalized = HookEvent::new("opencode", agent_session_id, event);
    normalized.cwd = optional_string(data, "cwd").or_else(|| optional_string(&value, "cwd"));
    normalized.pid = optional_pid(&value);
    normalized.tool_name =
        optional_string(data, "tool").or_else(|| optional_string(data, "permission"));
    normalized.tool_input = data
        .get("metadata")
        .cloned()
        .or_else(|| data.get("args").cloned());
    normalized.status = optional_string(data, "status");
    normalized.prompt = optional_string(data, "text");
    if let Some(model) = data.get("info").and_then(|info| info.get("model")) {
        normalized.model =
            optional_string(model, "id").or_else(|| optional_string(model, "modelID"));
        normalized.effort = optional_string(model, "variant");
    }
    if is_question {
        normalized.question_id =
            optional_string(data, "id").or_else(|| optional_string(data, "requestID"));
        normalized.questions = Some(parse_questions(data.get("questions")));
    } else {
        normalized.approval_id =
            optional_string(data, "id").or_else(|| optional_string(data, "requestID"));
    }
    if matches!(normalized.event, HookEventKind::QuestionAsked) && normalized.question_id.is_none()
    {
        return Err(ParseError::MissingField("id"));
    }
    let question = question_from_event(&normalized);
    let approval = if is_approval {
        Some(ApprovalInput {
            approval_id: normalized
                .approval_id
                .clone()
                .ok_or(ParseError::MissingField("id"))?,
            session_id: HookId::new("opencode", agent_session_id),
            tool_name: normalized.tool_name.clone(),
            tool_input: normalized.tool_input.clone(),
            reason: optional_string(data, "reason"),
        })
    } else {
        None
    };
    Ok(Some(ParsedIngress {
        event: normalized,
        approval,
        question,
    }))
}

/// `always` is declared by the installed SDK as one of `once | always | reject` on
/// `POST /session/{id}/permissions/{permissionID}`, and the plugin forwards it untouched.
pub fn format_decision(decision: NativeDecision) -> String {
    let value = match decision {
        NativeDecision::Allow => "once",
        NativeDecision::AllowAlways => "always",
        NativeDecision::Deny => "reject",
    };
    format!(r#"{{"reply":"{value}"}}"#)
}

pub fn parse_decision_output(input: &str) -> Result<Option<NativeDecision>, ParseError> {
    if input.trim().is_empty() {
        return Ok(None);
    }
    let value = parse_json(input)?;
    match value.get("reply").and_then(Value::as_str) {
        Some("once") => Ok(Some(NativeDecision::Allow)),
        Some("reject") => Ok(Some(NativeDecision::Deny)),
        Some(_) => Err(ParseError::InvalidField("reply")),
        None => Ok(None),
    }
}
