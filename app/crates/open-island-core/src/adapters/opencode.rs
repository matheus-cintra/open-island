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
        .or_else(|| data.get("info").and_then(|info| info.get("id")))
        .and_then(Value::as_str)
        .ok_or(ParseError::MissingField("sessionID"))?;
    let is_approval = matches!(&event, HookEventKind::PermissionRequest);
    let is_question = matches!(
        &event,
        HookEventKind::QuestionAsked | HookEventKind::QuestionAnswered
    );
    let mut normalized = HookEvent::new("opencode", agent_session_id, event);
    normalized.session_metadata = value
        .get("session_metadata")
        .and_then(|metadata| serde_json::from_value(metadata.clone()).ok())
        .unwrap_or_default();
    if name.starts_with("session.") {
        if let Some(info) = data
            .get("info")
            .filter(|info| info.get("id").and_then(Value::as_str).is_some())
        {
            normalized
                .session_metadata
                .push(crate::protocol::SessionMetadata {
                    id: agent_session_id.to_owned(),
                    parent_id: optional_string(info, "parentID"),
                    title: optional_string(info, "title"),
                });
        }
    }
    normalized.cwd = optional_string(data, "cwd").or_else(|| optional_string(&value, "cwd"));
    normalized.pid = optional_pid(&value);
    normalized.tool_name =
        optional_string(data, "tool").or_else(|| optional_string(data, "permission"));
    normalized.tool_input = data
        .get("metadata")
        .cloned()
        .or_else(|| data.get("args").cloned());
    normalized.status = optional_string(data, "status").or_else(|| {
        data.get("status")
            .and_then(|status| optional_string(status, "type"))
    });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_info_confirms_ancestry_but_legacy_events_leave_it_unknown() {
        for kind in ["session.created", "session.updated", "session.deleted"] {
            let event = parse(&format!(r#"{{"type":"{kind}","properties":{{"info":{{"id":"child","parentID":"root","title":"Research"}}}}}}"#)).unwrap().unwrap().event;
            assert_eq!(event.session_id.as_str(), "opencode:child");
            assert_eq!(event.session_metadata[0].parent_id.as_deref(), Some("root"));
            assert_eq!(event.session_metadata[0].title.as_deref(), Some("Research"));
        }
        let legacy = parse(r#"{"type":"session.idle","properties":{"sessionID":"old"}}"#)
            .unwrap()
            .unwrap()
            .event;
        assert!(legacy.session_metadata.is_empty());
        let root = parse(
            r#"{"type":"session.created","properties":{"info":{"id":"root","title":"Main"}}}"#,
        )
        .unwrap()
        .unwrap()
        .event;
        assert_eq!(root.session_metadata.len(), 1);
        assert_eq!(root.session_metadata[0].parent_id, None);
    }
}
