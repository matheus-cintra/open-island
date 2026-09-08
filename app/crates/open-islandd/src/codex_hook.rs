use open_island_core::{
    adapters::{parse_codex, NativeDecision, ParseError},
    protocol::{Request, Response},
};
use serde_json::{json, Value};
use std::{
    fmt,
    io::{self, BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::Path,
    time::Duration,
};

const REQUEST_ID: u64 = 1;

#[derive(Debug)]
pub enum CodexHookError {
    Parse(ParseError),
    Encode(serde_json::Error),
    InvalidResponse(String),
}

impl fmt::Display for CodexHookError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(formatter, "unable to parse Codex hook: {error}"),
            Self::Encode(error) => write!(formatter, "unable to encode daemon request: {error}"),
            Self::InvalidResponse(error) => write!(formatter, "invalid daemon response: {error}"),
        }
    }
}

impl std::error::Error for CodexHookError {}

/// Converts one Codex hook payload into the optional Codex permission response.
///
/// Lifecycle and tool events are forwarded without waiting and produce empty output.
/// Permission requests wait for the daemon's matching response, and only an explicit
/// decision decides: transport failure, timeout, daemon error and a malformed answer all
/// produce empty output, which leaves the request to Codex's own prompt as if no hook
/// were installed. The island must never be able to block the agent.
pub fn run(input: &str, socket_path: &Path, timeout: Duration) -> Result<String, CodexHookError> {
    let Some(parsed) = parse_codex(input).map_err(CodexHookError::Parse)? else {
        return Ok(String::new());
    };

    let approval = parsed.approval.clone();
    let request = Request {
        v: 1,
        id: json!(REQUEST_ID),
        method: "hook_event".to_owned(),
        params: Some(serde_json::to_value(parsed.event).map_err(CodexHookError::Encode)?),
    };
    let encoded = serde_json::to_string(&request).map_err(CodexHookError::Encode)?;
    let stream = match UnixStream::connect(socket_path) {
        Ok(stream) => stream,
        Err(_) => return Ok(String::new()),
    };
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    if send_request(&stream, &encoded).is_err() {
        return Ok(String::new());
    }

    if approval.is_none() {
        return Ok(String::new());
    }

    let response = match read_matching_response(stream, REQUEST_ID) {
        Ok(response) => response,
        Err(_) => return Ok(String::new()),
    };
    if !response.ok {
        return Ok(String::new());
    }
    let decision = response
        .data
        .as_ref()
        .and_then(|data| data.get("decision"))
        .and_then(Value::as_str)
        .and_then(|value| match value {
            "allow" => Some(NativeDecision::Allow),
            "deny" => Some(NativeDecision::Deny),
            _ => None,
        });
    Ok(decision.map_or_else(String::new, format_decision))
}

fn send_request(stream: &UnixStream, request: &str) -> io::Result<()> {
    let mut stream = stream;
    writeln!(stream, "{request}")?;
    stream.flush()
}

fn read_matching_response(stream: UnixStream, request_id: u64) -> Result<Response, CodexHookError> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|error| CodexHookError::InvalidResponse(error.to_string()))?;
        if bytes == 0 {
            return Err(CodexHookError::InvalidResponse(
                "daemon closed the connection".to_owned(),
            ));
        }
        let value: Value = serde_json::from_str(line.trim())
            .map_err(|error| CodexHookError::InvalidResponse(error.to_string()))?;
        if value.get("id") != Some(&json!(request_id)) {
            continue;
        }
        let response: Response = serde_json::from_value(value)
            .map_err(|error| CodexHookError::InvalidResponse(error.to_string()))?;
        return Ok(response);
    }
}

fn format_decision(decision: NativeDecision) -> String {
    let value = if decision.is_allow() { "allow" } else { "deny" };
    format!(r#"{{"decision":"{value}"}}"#)
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_island_core::protocol::HookEventKind;
    use serde_json::json;
    use std::{
        fs,
        os::unix::net::UnixListener,
        sync::mpsc,
        thread,
        time::{SystemTime, UNIX_EPOCH},
    };

    const LIFECYCLE: &str =
        r#"{"session_id":"abc123","cwd":"/tmp/project","event":"SessionStart"}"#;
    const TOOL: &str = r#"{"session_id":"abc123","cwd":"/tmp/project","event":"PreToolUse","tool_name":"Bash","tool_input":{"command":"pwd"}}"#;
    const PERMISSION: &str = r#"{"session_id":"abc123","cwd":"/tmp/project","event":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"pwd"},"approval_id":"approval-1"}"#;

    fn socket_path(name: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        std::env::temp_dir().join(format!(
            "open-island-codex-{name}-{}-{stamp}.sock",
            std::process::id()
        ))
    }

    fn serve_once(path: &Path, response: Option<Value>) -> mpsc::Receiver<Request> {
        let listener = UnixListener::bind(path).expect("bind test socket");
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept test client");
            let mut reader = BufReader::new(stream.try_clone().expect("clone test stream"));
            let mut line = String::new();
            reader.read_line(&mut line).expect("read request");
            let request: Request = serde_json::from_str(line.trim()).expect("decode request");
            sender.send(request.clone()).expect("send request to test");
            if let Some(data) = response {
                let mut stream = stream;
                let reply = Response {
                    v: 1,
                    id: request.id,
                    ok: true,
                    data: Some(data),
                    error: None,
                };
                writeln!(
                    stream,
                    "{}",
                    serde_json::to_string(&reply).expect("encode response")
                )
                .expect("write response");
            }
        });
        receiver
    }

    fn cleanup(path: &Path) {
        let _ = fs::remove_file(path);
    }

    #[test]
    fn lifecycle_and_tool_events_forward_without_output() {
        let path = socket_path("events");
        let receiver = serve_once(&path, None);
        assert_eq!(
            run(LIFECYCLE, &path, Duration::from_secs(1)).expect("lifecycle runs"),
            ""
        );
        let request = receiver.recv().expect("lifecycle request");
        assert_eq!(request.method, "hook_event");
        let path_tool = socket_path("tool");
        let tool_receiver = serve_once(&path_tool, None);
        assert_eq!(
            run(TOOL, &path_tool, Duration::from_secs(1)).expect("tool runs"),
            ""
        );
        let tool_request = tool_receiver.recv().expect("tool request");
        let event: open_island_core::protocol::HookEvent =
            serde_json::from_value(tool_request.params.expect("params")).expect("event");
        assert_eq!(event.session_id.as_str(), "codex:abc123");
        assert_eq!(event.cwd.as_deref(), Some("/tmp/project"));
        assert_eq!(event.tool_name.as_deref(), Some("Bash"));
        assert_eq!(event.tool_input, Some(json!({"command":"pwd"})));
        assert_eq!(event.event, HookEventKind::PreToolUse);
        cleanup(&path);
        cleanup(&path_tool);
    }

    #[test]
    fn permission_maps_allow_and_deny() {
        for (name, value, expected) in [
            ("allow", "allow", r#"{"decision":"allow"}"#),
            ("deny", "deny", r#"{"decision":"deny"}"#),
        ] {
            let path = socket_path(name);
            let receiver = serve_once(&path, Some(json!({"accepted":true,"decision":value})));
            assert_eq!(
                run(PERMISSION, &path, Duration::from_secs(1)).expect("permission runs"),
                expected
            );
            let request = receiver.recv().expect("permission request");
            assert_eq!(request.method, "hook_event");
            cleanup(&path);
        }
    }

    #[test]
    fn malformed_input_is_a_typed_error() {
        let result = run("{", Path::new("/tmp/unused.sock"), Duration::from_millis(1));
        assert!(matches!(
            result,
            Err(CodexHookError::Parse(ParseError::InvalidJson(_)))
        ));
    }

    #[test]
    fn unavailable_or_disconnected_daemon_falls_through() {
        assert_eq!(
            run(
                PERMISSION,
                Path::new("/tmp/missing-open-island.sock"),
                Duration::from_millis(1)
            )
            .expect("unavailable daemon is handled"),
            ""
        );
        let path = socket_path("disconnect");
        let _receiver = serve_once(&path, None);
        assert_eq!(
            run(PERMISSION, &path, Duration::from_millis(50)).expect("disconnect is handled"),
            ""
        );
        cleanup(&path);
    }

    #[test]
    fn unsupported_input_has_empty_output_and_empty_input_is_invalid() {
        assert!(matches!(
            run(
                "",
                Path::new("/tmp/missing-open-island.sock"),
                Duration::from_millis(1)
            ),
            Err(CodexHookError::Parse(ParseError::InvalidJson(_)))
        ));
        assert_eq!(
            run(
                r#"{"session_id":"abc123","event":"Unknown"}"#,
                Path::new("/tmp/missing-open-island.sock"),
                Duration::from_millis(1)
            )
            .expect("unsupported input is handled"),
            ""
        );
    }
}
