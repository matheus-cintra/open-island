use super::{
    event_kind, hash_questions, optional_pid, optional_string, parse_json, parse_questions,
    question_from_event, string, ApprovalInput, NativeDecision, ParseError, ParsedIngress,
};
use crate::{HookEvent, HookId};
use serde_json::{Map, Value};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub const QUESTION_TOOL: &str = "AskUserQuestion";
pub const PLAN_TOOL: &str = "ExitPlanMode";

pub fn plan_return_mode(prior: Option<&str>) -> &str {
    match prior {
        Some(mode @ ("auto" | "acceptEdits" | "default")) => mode,
        _ => "auto",
    }
}

pub fn parse(input: &str) -> Result<Option<ParsedIngress>, ParseError> {
    let value = parse_json(input)?;
    let name = value
        .get("hook_event_name")
        .and_then(Value::as_str)
        .ok_or(ParseError::MissingField("hook_event_name"))?;
    let Some(event) = event_kind(name) else {
        return Ok(None);
    };
    let agent_session_id = string(&value, "session_id")?;
    let agent = source_agent(&value);
    let is_question = value.get("tool_name").and_then(Value::as_str) == Some(QUESTION_TOOL);
    let is_approval =
        matches!(&event, crate::protocol::HookEventKind::PermissionRequest) && !is_question;
    let mut normalized = HookEvent::new(agent, &agent_session_id, event);
    normalized.cwd = optional_string(&value, "cwd");
    normalized.pid = optional_pid(&value);
    normalized.permission_mode = optional_string(&value, "permission_mode");
    normalized.model = optional_string(&value, "model");
    normalized.effort = value
        .get("effort")
        .and_then(|effort| effort.get("level"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    normalized.tool_name = optional_string(&value, "tool_name");
    normalized.tool_input = value.get("tool_input").cloned();
    normalized.tool_response = value.get("tool_response").cloned();
    normalized.tool_use_id = optional_string(&value, "tool_use_id");
    normalized.agent_id = optional_string(&value, "agent_id");
    normalized.agent_type = optional_string(&value, "agent_type").filter(|kind| !kind.is_empty());
    normalized.prompt = optional_string(&value, "prompt");
    normalized.last_message = optional_string(&value, "last_assistant_message");
    normalized.approval_id = optional_string(&value, "approval_id").filter(|id| !id.is_empty());
    if is_approval && normalized.approval_id.is_none() {
        normalized.approval_id = Some(derived_approval_id(
            &value,
            &agent_session_id,
            normalized.tool_name.as_deref(),
            normalized.tool_input.as_ref(),
        ));
    }
    let approval = if is_approval {
        Some(ApprovalInput {
            approval_id: normalized.approval_id.clone().unwrap_or_default(),
            session_id: HookId::new(agent, &agent_session_id),
            tool_name: normalized.tool_name.clone(),
            tool_input: normalized.tool_input.clone(),
            reason: optional_string(&value, "reason"),
        })
    } else {
        None
    };
    if is_question {
        let questions = normalized
            .tool_input
            .as_ref()
            .and_then(|input| input.get("questions"));
        normalized.question_id = Some(derived_question_id(&value, &agent_session_id, questions));
        normalized.questions = Some(parse_questions(questions));
    }
    let question = question_from_event(&normalized);
    Ok(Some(ParsedIngress {
        event: normalized,
        approval,
        question,
    }))
}

/// OpenCode ships a Claude-Code-compatible hook bridge: it reads `~/.claude/settings.json`
/// and runs our hook with `--agent claude`, stamping `hook_source`. Its `session_id` is the
/// OpenCode one, so without this the same OpenCode session arrives twice — once as
/// `claude:ses_x` from the bridge and once as `opencode:ses_x` from the plugin — and
/// `snapshot_at` drops whichever loses the race for the process. The bridge is also the only
/// source of a tool name for OpenCode, because `tool.execute.*` never reaches the plugin.
fn source_agent(value: &Value) -> &'static str {
    match value.get("hook_source").and_then(Value::as_str) {
        Some("opencode-plugin") => "opencode",
        _ => "claude",
    }
}

/// Stable across the `PreToolUse`, `PermissionRequest` and `PostToolUse` payloads of one
/// `AskUserQuestion` call: only `tool_input.questions` is hashed, and `PostToolUse` adds
/// `answers` beside it without changing it.
fn derived_question_id(value: &Value, agent_session_id: &str, questions: Option<&Value>) -> String {
    let prompt_id = optional_string(value, "prompt_id").unwrap_or_default();
    format!(
        "claude:{agent_session_id}:{prompt_id}:{:016x}",
        hash_questions(questions)
    )
}

fn derived_approval_id(
    value: &Value,
    agent_session_id: &str,
    tool_name: Option<&str>,
    tool_input: Option<&Value>,
) -> String {
    let prompt_id = optional_string(value, "prompt_id").unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    tool_name.hash(&mut hasher);
    tool_input.map(Value::to_string).hash(&mut hasher);
    format!(
        "claude:{agent_session_id}:{prompt_id}:{:016x}",
        hasher.finish()
    )
}

/// Builds the `PermissionRequest` output that answers an `AskUserQuestion` call.
///
/// `answers` absent, empty or unusable produces a plain `allow`, which hands the question
/// back to the Claude TUI instead of denying it. A single selection is written as a string
/// and a multi-select one as an array of labels, the two shapes Claude echoes back.
pub fn format_question_decision(
    tool_input: Option<&Value>,
    questions: &[crate::protocol::Question],
    answers: Option<&[Vec<String>]>,
) -> String {
    let Some(rows) = answers.filter(|rows| !rows.is_empty()) else {
        return format_decision(NativeDecision::Allow, None, None);
    };
    let Some(input) = tool_input.and_then(Value::as_object) else {
        return format_decision(NativeDecision::Allow, None, None);
    };
    let mut answered = Map::new();
    for (question, labels) in questions.iter().zip(rows) {
        if labels.is_empty() {
            continue;
        }
        let value = if question.multi_select {
            Value::from(labels.clone())
        } else {
            Value::from(labels[0].clone())
        };
        answered.insert(question.question.clone(), value);
    }
    if answered.is_empty() {
        return format_decision(NativeDecision::Allow, None, None);
    }
    let mut updated = input.clone();
    updated.insert("answers".to_owned(), Value::Object(answered));
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": {"behavior": "allow", "updatedInput": Value::Object(updated)}
        }
    })
    .to_string()
}

pub fn format_decision(
    decision: NativeDecision,
    tool_input: Option<&Value>,
    set_mode: Option<&str>,
) -> String {
    if !decision.is_allow() {
        return r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"deny"}}}"#
            .to_owned();
    }
    if tool_input.is_none() && set_mode.is_none() {
        return r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}"#
            .to_owned();
    }
    let mut body = Map::new();
    body.insert("behavior".to_owned(), Value::from("allow"));
    if let Some(input) = tool_input {
        body.insert("updatedInput".to_owned(), input.clone());
    }
    if let Some(mode) = set_mode {
        body.insert(
            "updatedPermissions".to_owned(),
            serde_json::json!([{"type": "setMode", "mode": mode, "destination": "session"}]),
        );
    }
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": Value::Object(body)
        }
    })
    .to_string()
}

pub fn format_always_decision(
    suggestions: Option<&Value>,
    tool_input: Option<&Value>,
    set_mode: Option<&str>,
) -> String {
    let Some(rules) = suggestions.filter(|value| value.as_array().is_some_and(|it| !it.is_empty()))
    else {
        return format_decision(NativeDecision::Allow, tool_input, set_mode);
    };
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": {"behavior": "allow", "updatedPermissions": rules}
        }
    })
    .to_string()
}

pub fn parse_decision_output(input: &str) -> Result<Option<NativeDecision>, ParseError> {
    if input.trim().is_empty() {
        return Ok(None);
    }
    let value = parse_json(input)?;
    let behavior = value
        .get("hookSpecificOutput")
        .and_then(|output| output.get("decision"))
        .and_then(|decision| decision.get("behavior"))
        .and_then(Value::as_str);
    match behavior {
        Some("allow") => Ok(Some(NativeDecision::Allow)),
        Some("deny") => Ok(Some(NativeDecision::Deny)),
        Some(_) => Err(ParseError::InvalidField("behavior")),
        None => Ok(None),
    }
}
