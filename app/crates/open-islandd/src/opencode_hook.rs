use open_island_core::{
    adapters::{parse_opencode, NativeDecision, ParseError},
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
const NO_ANSWER: &str = "{}";

#[derive(Debug)]
pub enum OpenCodeHookError {
    Parse(ParseError),
    Encode(serde_json::Error),
    InvalidResponse(String),
}

impl fmt::Display for OpenCodeHookError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(formatter, "unable to parse OpenCode event: {error}"),
            Self::Encode(error) => write!(formatter, "unable to encode daemon request: {error}"),
            Self::InvalidResponse(error) => write!(formatter, "invalid daemon response: {error}"),
        }
    }
}

impl std::error::Error for OpenCodeHookError {}

pub fn run(
    input: &str,
    socket_path: &Path,
    timeout: Duration,
) -> Result<String, OpenCodeHookError> {
    let Some(parsed) = parse_opencode(input).map_err(OpenCodeHookError::Parse)? else {
        return Ok(String::new());
    };
    let approval = parsed.approval.is_some();
    let question = parsed.question.is_some();
    let request = Request {
        v: 1,
        id: json!(REQUEST_ID),
        method: "hook_event".to_owned(),
        params: Some(serde_json::to_value(parsed.event).map_err(OpenCodeHookError::Encode)?),
    };
    let stream = match UnixStream::connect(socket_path) {
        Ok(stream) => stream,
        Err(_) => return Ok(String::new()),
    };
    if stream.set_read_timeout(Some(timeout)).is_err()
        || stream.set_write_timeout(Some(timeout)).is_err()
    {
        return Ok(String::new());
    }
    let encoded = serde_json::to_string(&request).map_err(OpenCodeHookError::Encode)?;
    if send_request(&stream, &encoded).is_err() {
        return Ok(String::new());
    }
    if question {
        return Ok(match read_matching_response(stream, REQUEST_ID) {
            Ok(response) => format_answers(&response),
            Err(_) => NO_ANSWER.to_owned(),
        });
    }
    if !approval {
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

/// Answers the plugin so it can reply to the OpenCode server. An empty object means the
/// island produced no answer, and the plugin then leaves the question to the OpenCode TUI.
fn format_answers(response: &Response) -> String {
    if !response.ok {
        return NO_ANSWER.to_owned();
    }
    let answers = response
        .data
        .as_ref()
        .and_then(|data| data.get("answers"))
        .filter(|answers| answers.is_array());
    match answers {
        Some(answers) => json!({"answers": answers}).to_string(),
        None => NO_ANSWER.to_owned(),
    }
}

fn send_request(stream: &UnixStream, request: &str) -> io::Result<()> {
    let mut stream = stream;
    writeln!(stream, "{request}")?;
    stream.flush()
}

fn read_matching_response(
    stream: UnixStream,
    request_id: u64,
) -> Result<Response, OpenCodeHookError> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|error| OpenCodeHookError::InvalidResponse(error.to_string()))?;
        if bytes == 0 {
            return Err(OpenCodeHookError::InvalidResponse(
                "daemon closed the connection".to_owned(),
            ));
        }
        let value: Value = serde_json::from_str(line.trim())
            .map_err(|error| OpenCodeHookError::InvalidResponse(error.to_string()))?;
        if value.get("id") != Some(&json!(request_id)) {
            continue;
        }
        return serde_json::from_value(value)
            .map_err(|error| OpenCodeHookError::InvalidResponse(error.to_string()));
    }
}

fn format_decision(decision: NativeDecision) -> String {
    let reply = if decision.is_allow() {
        "once"
    } else {
        "reject"
    };
    format!(r#"{{"reply":"{reply}"}}"#)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        fs,
        os::unix::net::UnixListener,
        sync::mpsc,
        thread,
        time::{SystemTime, UNIX_EPOCH},
    };

    const PERMISSION: &str = r#"{"type":"permission.asked","properties":{"sessionID":"abc123","id":"permission-1","permission":"bash","metadata":{"command":"pwd"}},"cwd":"/tmp/project"}"#;
    const TOOL: &str = r#"{"type":"tool.execute.before","properties":{"sessionID":"abc123","tool":"bash","args":{"command":"pwd"}},"cwd":"/tmp/project"}"#;

    fn socket(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "open-island-opencode-{name}-{}-{}.sock",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    fn server(path: &Path, data: Option<Value>) -> mpsc::Receiver<Request> {
        let listener = UnixListener::bind(path).expect("bind");
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut line = String::new();
            reader.read_line(&mut line).expect("read");
            let request: Request = serde_json::from_str(line.trim()).expect("request");
            sender.send(request.clone()).expect("send");
            if let Some(data) = data {
                let mut writer = stream;
                let response = Response {
                    v: 1,
                    id: request.id,
                    ok: true,
                    data: Some(data),
                    error: None,
                };
                writeln!(
                    writer,
                    "{}",
                    serde_json::to_string(&response).expect("encode")
                )
                .expect("write");
            }
        });
        receiver
    }

    #[test]
    fn permission_maps_daemon_decisions_to_opencode_replies() {
        for (name, decision, expected) in [
            ("allow", "allow", r#"{"reply":"once"}"#),
            ("deny", "deny", r#"{"reply":"reject"}"#),
        ] {
            let path = socket(name);
            let requests = server(&path, Some(json!({"decision": decision})));
            assert_eq!(
                run(PERMISSION, &path, Duration::from_secs(1)).expect("run"),
                expected
            );
            assert_eq!(requests.recv().expect("request").method, "hook_event");
            fs::remove_file(path).expect("cleanup");
        }
    }

    #[test]
    fn non_permission_event_forwards_and_has_no_output() {
        let path = socket("tool");
        let requests = server(&path, None);
        assert_eq!(run(TOOL, &path, Duration::from_secs(1)).expect("run"), "");
        assert_eq!(requests.recv().expect("request").method, "hook_event");
        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn unavailable_permission_falls_through_and_unsupported_input_is_empty() {
        assert_eq!(
            run(
                PERMISSION,
                Path::new("/tmp/open-island-missing.sock"),
                Duration::from_millis(1)
            )
            .expect("fall through"),
            ""
        );
        assert_eq!(
            run(
                r#"{"type":"message.updated","properties":{"sessionID":"abc123"}}"#,
                Path::new("/tmp/open-island-missing.sock"),
                Duration::from_millis(1)
            )
            .expect("unsupported"),
            ""
        );
    }
}
