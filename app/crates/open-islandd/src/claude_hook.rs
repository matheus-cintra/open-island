use open_island_core::{
    adapters::{
        format_claude_always, format_claude_answer, format_claude_decision, parse_claude,
        NativeDecision, ParseError,
    },
    protocol::{Request, Response},
};
use serde_json::{json, Value};
use std::{
    fmt,
    io::{self, BufRead, BufReader, BufWriter, Write},
    os::unix::net::UnixStream,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub enum ClaudeHookError {
    Parse(ParseError),
    RequestSerialization(serde_json::Error),
    ResponseSerialization(serde_json::Error),
    InvalidResponse(String),
}

impl fmt::Display for ClaudeHookError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(formatter, "Claude hook input: {error}"),
            Self::RequestSerialization(error) => write!(formatter, "hook request: {error}"),
            Self::ResponseSerialization(error) => write!(formatter, "daemon response: {error}"),
            Self::InvalidResponse(error) => write!(formatter, "invalid daemon response: {error}"),
        }
    }
}

impl std::error::Error for ClaudeHookError {}

/// Handles one Claude hook payload and returns only Claude's optional JSON output.
///
/// Only an explicit decision from the daemon decides a permission. A missing socket, a
/// timeout, a disconnect, a daemon error and a malformed answer all return an empty
/// string, which leaves the request to Claude's own permission prompt exactly as if no
/// hook were installed — the island must never be able to block the agent. The daemon
/// owns the no-answer deny, and it arrives here as a real decision.
///
/// An `AskUserQuestion` permission hook is answered instead of decided: without an
/// answer it allows the call unchanged, which leaves the question to the Claude TUI.
pub fn handle(
    input: &str,
    socket_path: &Path,
    timeout: Duration,
) -> Result<String, ClaudeHookError> {
    let Some(parsed) = parse_claude(input).map_err(ClaudeHookError::Parse)? else {
        return Ok(String::new());
    };
    let is_approval = parsed.approval.is_some();
    let question = parsed.question.clone();
    let tool_input = parsed.event.tool_input.clone();
    let suggestions = is_approval
        .then(|| serde_json::from_str::<Value>(input).ok())
        .flatten()
        .and_then(|value| value.get("permission_suggestions").cloned());
    let request = Request {
        v: 1,
        id: json!(NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed)),
        method: "hook_event".to_owned(),
        params: Some(
            serde_json::to_value(parsed.event).map_err(ClaudeHookError::RequestSerialization)?,
        ),
    };
    let response = match exchange(socket_path, timeout, &request) {
        Ok(response) => response,
        Err(_) if question.is_some() => return Ok(unanswered(tool_input.as_ref())),
        Err(_) => return Ok(String::new()),
    };
    if let Some(question) = question {
        return Ok(format_claude_answer(
            tool_input.as_ref(),
            &question.questions,
            response_answers(&response).as_deref(),
        ));
    }
    if !is_approval {
        return Ok(String::new());
    }
    let set_mode = response_mode(&response);
    Ok(match response_decision(&response) {
        Some(NativeDecision::AllowAlways) => format_claude_always(
            suggestions.as_ref(),
            tool_input.as_ref(),
            set_mode.as_deref(),
        ),
        Some(decision) => {
            format_claude_decision(decision, tool_input.as_ref(), set_mode.as_deref())
        }
        None => String::new(),
    })
}

fn response_mode(response: &Response) -> Option<String> {
    if !response.ok {
        return None;
    }
    response
        .data
        .as_ref()
        .and_then(|data| data.get("mode"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn unanswered(tool_input: Option<&Value>) -> String {
    format_claude_answer(tool_input, &[], None)
}

fn response_answers(response: &Response) -> Option<Vec<Vec<String>>> {
    if !response.ok {
        return None;
    }
    response
        .data
        .as_ref()
        .and_then(|data| data.get("answers"))
        .cloned()
        .and_then(|answers| serde_json::from_value(answers).ok())
}

fn exchange(
    socket_path: &Path,
    timeout: Duration,
    request: &Request,
) -> Result<Response, ClaudeHookError> {
    let stream = UnixStream::connect(socket_path).map_err(|_| {
        ClaudeHookError::InvalidResponse(format!("daemon unavailable at {}", socket_path.display()))
    })?;
    stream.set_read_timeout(Some(timeout)).map_err(io_error)?;
    stream.set_write_timeout(Some(timeout)).map_err(io_error)?;
    let mut writer = BufWriter::new(stream.try_clone().map_err(io_error)?);
    let encoded = serde_json::to_string(request).map_err(ClaudeHookError::RequestSerialization)?;
    writeln!(writer, "{encoded}").map_err(io_error)?;
    writer.flush().map_err(io_error)?;
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line).map_err(io_error)?;
        if read == 0 {
            return Err(ClaudeHookError::InvalidResponse(
                "daemon disconnected before the matching response".to_owned(),
            ));
        }
        let value: Value =
            serde_json::from_str(line.trim()).map_err(ClaudeHookError::ResponseSerialization)?;
        if value.get("id") != Some(&request.id) {
            continue;
        }
        return serde_json::from_value(value).map_err(ClaudeHookError::ResponseSerialization);
    }
}

fn io_error(error: io::Error) -> ClaudeHookError {
    ClaudeHookError::InvalidResponse(error.to_string())
}

fn response_decision(response: &Response) -> Option<NativeDecision> {
    if !response.ok {
        return None;
    }
    response
        .data
        .as_ref()
        .and_then(|data| data.get("decision"))
        .cloned()
        .and_then(|decision| serde_json::from_value(decision).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        fs,
        os::unix::net::UnixListener,
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    const FIXTURES: &[(&str, &str)] = &[
        (
            "SessionStart",
            include_str!("../fixtures/claude-session-start.json"),
        ),
        ("Prompt", include_str!("../fixtures/claude-prompt.json")),
        (
            "PreToolUse",
            include_str!("../fixtures/claude-pre-tool-use.json"),
        ),
        (
            "PostToolUse",
            include_str!("../fixtures/claude-post-tool-use.json"),
        ),
        (
            "PermissionRequest",
            include_str!("../fixtures/claude-permission-request.json"),
        ),
    ];

    fn socket_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "open-island-claude-hook-{label}-{}-{}.sock",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    fn serve(path: &Path, response: Value) -> thread::JoinHandle<()> {
        let listener = UnixListener::bind(path).expect("bind test socket");
        thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept hook");
            let mut reader = BufReader::new(stream.try_clone().expect("clone test stream"));
            let mut line = String::new();
            reader.read_line(&mut line).expect("read hook request");
            let request: Request = serde_json::from_str(line.trim()).expect("request shape");
            let params = request.params.as_ref().expect("hook params");
            assert_eq!(params["session_id"], "claude:abc123");
            assert_eq!(params["cwd"], "/tmp/project");
            if params.get("tool_name") == Some(&json!("Bash")) {
                assert_eq!(params["tool_input"], json!({"command": "pwd"}));
            }
            let response = Response {
                v: 1,
                id: request.id,
                ok: true,
                data: Some(response),
                error: None,
            };
            let mut writer = BufWriter::new(stream);
            writeln!(
                writer,
                "{}",
                serde_json::to_string(&response).expect("response JSON")
            )
            .expect("write response");
            writer.flush().expect("flush response");
        })
    }

    #[test]
    fn fixture_events_are_forwarded_and_non_approval_output_is_empty() {
        for (name, fixture) in FIXTURES.iter().take(4) {
            let path = socket_path(name);
            let server = serve(&path, json!({"decision": "allow"}));
            let output = handle(fixture, &path, Duration::from_secs(1)).expect("hook handles");
            assert!(output.is_empty(), "{name} must not produce approval output");
            server.join().expect("server joins");
            fs::remove_file(path).expect("remove socket");
        }
    }

    #[test]
    fn permission_fixture_preserves_required_fields_and_allows() {
        let path = socket_path("allow");
        let server = serve(&path, json!({"decision": "allow"}));
        let output = handle(FIXTURES[4].1, &path, Duration::from_secs(1)).expect("hook handles");
        assert_eq!(
            serde_json::from_str::<Value>(&output).expect("valid JSON"),
            json!({"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow","updatedInput":{"command":"pwd"}}}})
        );
        server.join().expect("server joins");
        fs::remove_file(path).expect("remove socket");
    }

    #[test]
    fn real_permission_request_without_approval_id_is_answered() {
        let path = socket_path("real-allow");
        let server = serve(&path, json!({"decision": "allow"}));
        let real = r#"{"session_id":"abc123","transcript_path":"/tmp/t.jsonl","cwd":"/tmp/project","prompt_id":"prompt-9","permission_mode":"default","effort":{"level":"high"},"hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"pwd"},"permission_suggestions":[{"type":"setMode","mode":"acceptEdits","destination":"session"}]}"#;
        let output = handle(real, &path, Duration::from_secs(1)).expect("hook handles");
        assert_eq!(
            serde_json::from_str::<Value>(&output).expect("valid JSON"),
            json!({"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow","updatedInput":{"command":"pwd"}}}})
        );
        server.join().expect("server joins");
        fs::remove_file(path).expect("remove socket");
    }

    #[test]
    fn permission_fixture_maps_daemon_deny_to_claude_deny() {
        let path = socket_path("deny");
        let server = serve(&path, json!({"decision": "deny"}));
        let output = handle(FIXTURES[4].1, &path, Duration::from_secs(1)).expect("hook handles");
        assert_eq!(
            output,
            r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"deny"}}}"#
        );
        server.join().expect("server joins");
        fs::remove_file(path).expect("remove socket");
    }

    const EXIT_PLAN_MODE: &str = r##"{
        "session_id":"abc123","cwd":"/tmp/project","prompt_id":"f5816ad1",
        "permission_mode":"plan","hook_event_name":"PermissionRequest",
        "tool_name":"ExitPlanMode",
        "tool_input":{"plan":"# Plano\nAcrescentar uma linha ao README.","planFilePath":"/tmp/p.md"}
    }"##;

    #[test]
    fn allowing_a_plan_echoes_its_input_back_because_a_bare_allow_is_ignored() {
        let path = socket_path("plan-allow");
        let server = serve(&path, json!({"decision": "allow"}));
        let output = handle(EXIT_PLAN_MODE, &path, Duration::from_secs(1)).expect("hook handles");
        let value: Value = serde_json::from_str(&output).expect("valid JSON");

        assert_eq!(value["hookSpecificOutput"]["decision"]["behavior"], "allow");
        assert_eq!(
            value["hookSpecificOutput"]["decision"]["updatedInput"]["plan"],
            "# Plano\nAcrescentar uma linha ao README.",
            "without updatedInput Claude ignores the allow and asks in its own terminal"
        );
        server.join().expect("server joins");
        fs::remove_file(path).expect("remove socket");
    }

    #[test]
    fn allowing_a_plan_carries_the_mode_the_daemon_says_to_return_to() {
        let path = socket_path("plan-mode");
        let server = serve(&path, json!({"decision": "allow", "mode": "acceptEdits"}));
        let output = handle(EXIT_PLAN_MODE, &path, Duration::from_secs(1)).expect("hook handles");
        let value: Value = serde_json::from_str(&output).expect("valid JSON");
        let rule = &value["hookSpecificOutput"]["decision"]["updatedPermissions"][0];

        assert_eq!(rule["type"], "setMode");
        assert_eq!(rule["mode"], "acceptEdits");
        assert_eq!(rule["destination"], "session");
        server.join().expect("server joins");
        fs::remove_file(path).expect("remove socket");
    }

    #[test]
    fn an_ordinary_allow_carries_no_mode_because_the_daemon_sends_none() {
        let path = socket_path("no-mode");
        let server = serve(&path, json!({"decision": "allow"}));
        let output = handle(FIXTURES[4].1, &path, Duration::from_secs(1)).expect("hook handles");
        let value: Value = serde_json::from_str(&output).expect("valid JSON");

        assert!(
            value["hookSpecificOutput"]["decision"]["updatedPermissions"].is_null(),
            "only a plan may change the session's permission mode"
        );
        server.join().expect("server joins");
        fs::remove_file(path).expect("remove socket");
    }

    #[test]
    fn denying_a_plan_needs_no_input_because_deny_was_never_the_broken_half() {
        let path = socket_path("plan-deny");
        let server = serve(&path, json!({"decision": "deny"}));
        let output = handle(EXIT_PLAN_MODE, &path, Duration::from_secs(1)).expect("hook handles");

        assert_eq!(
            output,
            r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"deny"}}}"#
        );
        server.join().expect("server joins");
        fs::remove_file(path).expect("remove socket");
    }

    const PERMISSION_WITH_SUGGESTIONS: &str = r#"{
        "session_id":"abc123","cwd":"/tmp/project","hook_event_name":"PermissionRequest",
        "tool_name":"Bash","tool_input":{"command":"pwd"},"approval_id":"approval-1",
        "permission_suggestions":[{"type":"setMode","mode":"acceptEdits","destination":"session"}]
    }"#;

    #[test]
    fn allow_always_returns_the_persistent_shape_the_probe_proved() {
        let path = socket_path("always");
        let server = serve(&path, json!({"decision": "allow_always"}));
        let output = handle(PERMISSION_WITH_SUGGESTIONS, &path, Duration::from_secs(1))
            .expect("hook handles");
        let value: Value = serde_json::from_str(&output).expect("valid JSON");

        assert_eq!(value["hookSpecificOutput"]["decision"]["behavior"], "allow");
        assert_eq!(
            value["hookSpecificOutput"]["decision"]["updatedPermissions"][0]["mode"],
            "acceptEdits"
        );
        server.join().expect("server joins");
        fs::remove_file(path).expect("remove socket");
    }

    #[test]
    fn allow_always_on_a_request_that_suggested_nothing_degrades_to_a_bare_allow() {
        let path = socket_path("always-bare");
        let server = serve(&path, json!({"decision": "allow_always"}));
        let output = handle(FIXTURES[4].1, &path, Duration::from_secs(1)).expect("hook handles");

        assert_eq!(
            serde_json::from_str::<Value>(&output).expect("valid JSON"),
            json!({"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow","updatedInput":{"command":"pwd"}}}})
        );
        server.join().expect("server joins");
        fs::remove_file(path).expect("remove socket");
    }

    #[test]
    fn permission_falls_through_when_the_daemon_is_unavailable_or_disconnects() {
        let missing = socket_path("missing");
        assert_eq!(
            handle(FIXTURES[4].1, &missing, Duration::from_millis(10)).expect("hook handles"),
            ""
        );
        let path = socket_path("disconnect");
        let listener = UnixListener::bind(&path).expect("bind socket");
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            drop(stream);
        });
        assert_eq!(
            handle(FIXTURES[4].1, &path, Duration::from_millis(100)).expect("hook handles"),
            ""
        );
        server.join().expect("server joins");
        fs::remove_file(path).expect("remove socket");
    }

    #[test]
    fn permission_falls_through_when_the_daemon_times_out() {
        let path = socket_path("timeout");
        let listener = UnixListener::bind(&path).expect("bind socket");
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            thread::sleep(Duration::from_millis(100));
            drop(stream);
        });
        let output = handle(FIXTURES[4].1, &path, Duration::from_millis(10)).expect("hook handles");
        assert_eq!(output, "");
        server.join().expect("server joins");
        fs::remove_file(path).expect("remove socket");
    }

    #[test]
    fn permission_falls_through_when_the_daemon_errors_or_answers_nonsense() {
        let path = socket_path("daemon-error");
        let listener = UnixListener::bind(&path).expect("bind socket");
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut line = String::new();
            reader.read_line(&mut line).expect("read request");
            let id = serde_json::from_str::<Value>(&line).expect("request")["id"].clone();
            let mut writer = BufWriter::new(stream);
            writeln!(writer, r#"{{"v":1,"id":{id},"ok":false,"error":"boom"}}"#).expect("write");
            writer.flush().expect("flush");
            thread::sleep(Duration::from_millis(50));
        });
        assert_eq!(
            handle(FIXTURES[4].1, &path, Duration::from_secs(1)).expect("hook handles"),
            ""
        );
        server.join().expect("server joins");
        fs::remove_file(&path).expect("remove socket");

        let path = socket_path("bad-decision");
        let server = serve(&path, json!({"decision": "maybe"}));
        assert_eq!(
            handle(FIXTURES[4].1, &path, Duration::from_secs(1)).expect("hook handles"),
            ""
        );
        server.join().expect("server joins");
        fs::remove_file(path).expect("remove socket");
    }

    #[test]
    fn malformed_input_is_typed_error_and_empty_output_stays_empty() {
        assert!(matches!(
            handle("{", Path::new("/missing"), Duration::from_millis(1)),
            Err(ClaudeHookError::Parse(ParseError::InvalidJson(_)))
        ));
        assert!(handle(
            r#"{"session_id":"abc123","hook_event_name":"Unknown"}"#,
            Path::new("/missing"),
            Duration::from_millis(1),
        )
        .expect("unsupported hook handles")
        .is_empty());
        assert_eq!(
            open_island_core::adapters::format_claude_decision(NativeDecision::Deny, None, None),
            r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"deny"}}}"#
        );
    }
}
