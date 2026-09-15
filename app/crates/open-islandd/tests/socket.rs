use serde_json::{json, Value};
use std::{
    env,
    io::{BufRead, BufReader, ErrorKind, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

mod support;

use support::{config_without_release_check, isolated_home};

fn socket() -> PathBuf {
    static NEXT_SOCKET: AtomicU64 = AtomicU64::new(0);
    env::temp_dir().join(format!(
        "open-island-test-{}-{}.sock",
        std::process::id(),
        NEXT_SOCKET.fetch_add(1, Ordering::Relaxed)
    ))
}

fn request(
    reader: &mut BufReader<UnixStream>,
    stream: &mut UnixStream,
    id: i64,
    method: &str,
    params: Value,
) -> Value {
    send_request(stream, id, method, params);
    read_until_id(reader, id)
}

fn connect(path: &PathBuf) -> (UnixStream, BufReader<UnixStream>) {
    for _ in 0..50 {
        if let Ok(stream) = UnixStream::connect(path) {
            let reader = BufReader::new(stream.try_clone().expect("clone"));
            return (stream, reader);
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("daemon did not bind");
}

struct Daemon {
    child: Child,
    path: PathBuf,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.path);
    }
}

fn spawn_daemon(path: &PathBuf, env: &[(&str, &str)]) -> Daemon {
    let mut command = Command::new(env!("CARGO_BIN_EXE_open-islandd"));
    command
        .arg("--socket")
        .arg(path)
        .env("HOME", isolated_home())
        .env("XDG_CONFIG_HOME", isolated_home())
        .env("XDG_STATE_HOME", isolated_home())
        .env("OPEN_ISLAND_CONFIG", config_without_release_check())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (key, value) in env {
        command.env(key, value);
    }
    Daemon {
        child: command.spawn().expect("spawn daemon"),
        path: path.clone(),
    }
}

fn send_request(stream: &mut UnixStream, id: i64, method: &str, params: Value) {
    writeln!(
        stream,
        "{}",
        json!({"v":1,"id":id,"method":method,"params":params})
    )
    .expect("write request");
    stream.flush().expect("flush request");
}

fn read_message(reader: &mut BufReader<UnixStream>, deadline: Instant, what: &str) -> Value {
    let mut line = String::new();
    loop {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        match reader.read_line(&mut line) {
            Ok(0) => thread::sleep(Duration::from_millis(10)),
            Ok(_) => return serde_json::from_str(line.trim()).expect("message JSON"),
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                thread::sleep(Duration::from_millis(10))
            }
            Err(error) => panic!("{what}: {error}"),
        }
    }
}

fn read_until_id(reader: &mut BufReader<UnixStream>, id: i64) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let value = read_message(reader, deadline, "read message");
        if value.get("id").and_then(Value::as_i64) == Some(id) && value.get("ok").is_some() {
            return value;
        }
    }
}

fn read_response_and_event(
    reader: &mut BufReader<UnixStream>,
    response_id: i64,
    event: &str,
) -> (Value, Value) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut response = None;
    let mut event_message = None;
    while response.is_none() || event_message.is_none() {
        let value = read_message(reader, deadline, "read message");
        if value.get("id").and_then(Value::as_i64) == Some(response_id) {
            response = Some(value.clone());
        }
        if value.get("event").and_then(Value::as_str) == Some(event) {
            event_message = Some(value);
        }
    }
    (
        response.expect("response"),
        event_message.expect("event message"),
    )
}

fn read_event(reader: &mut BufReader<UnixStream>, event: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let value = read_message(reader, deadline, "read event");
        if value["event"] == event {
            return value;
        }
    }
}

fn hook(agent_session_id: &str, approval_id: Option<&str>) -> Value {
    let mut event = json!({
        "agent": "claude",
        "session_id": format!("claude:{agent_session_id}"),
        "agent_session_id": agent_session_id,
        "event": "permission-request",
        "cwd": "/tmp/project",
        "pid": 42,
        "tool_name": "Bash",
        "tool_input": {"command": "pwd"}
    });
    if let Some(approval_id) = approval_id {
        event["approval_id"] = json!(approval_id);
    }
    event
}

#[test]
fn daemon_serves_two_clients_and_pushes_updates() {
    let path = socket();
    let _daemon = spawn_daemon(&path, &[("OPEN_ISLAND_POLL_MS", "25")]);
    let (mut first, mut first_reader) = connect(&path);
    let (second, mut second_reader) = connect(&path);
    first
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    second
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let event = read_event(&mut first_reader, "sessions-updated");
    let other_event = read_event(&mut second_reader, "sessions-updated");
    assert_eq!(event["event"], "sessions-updated");
    assert_eq!(other_event["event"], "sessions-updated");
    send_request(&mut first, 1, "ping", json!({}));
    let ping = read_until_id(&mut first_reader, 1);
    assert_eq!(ping["ok"], true);
    assert_eq!(ping["data"]["daemon"], "open-islandd");
    assert!(ping["data"]["pid"].as_u64().is_some());
    send_request(&mut first, 2, "list_sessions", json!({}));
    let sessions = read_until_id(&mut first_reader, 2);
    assert_eq!(sessions["ok"], true);
    assert!(sessions["data"].is_array());
    send_request(&mut first, 3, "jump", json!({"id":"missing:0"}));
    let jump = read_until_id(&mut first_reader, 3);
    assert_eq!(jump["ok"], false);
    assert_eq!(jump["error"], "session 'missing:0' not found");
}

#[test]
fn approval_waits_for_second_client_and_fans_out_resolution() {
    let path = socket();
    let _daemon = spawn_daemon(&path, &[("OPEN_ISLAND_POLL_MS", "1000")]);
    let (mut hook_stream, mut hook_reader) = connect(&path);
    let (mut gui_stream, mut gui_reader) = connect(&path);
    hook_stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    gui_stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");

    send_request(
        &mut hook_stream,
        10,
        "hook_event",
        hook("approval-session", Some("approval-1")),
    );
    let requested = read_event(&mut gui_reader, "approval-requested");
    assert_eq!(requested["event"], "approval-requested");
    assert_eq!(requested["data"]["approval_id"], "approval-1");

    send_request(
        &mut gui_stream,
        20,
        "resolve_approval",
        json!({"approval_id":"approval-1","decision":"allow"}),
    );
    let (resolution, resolved) = read_response_and_event(&mut gui_reader, 20, "approval-resolved");
    assert_eq!(resolved["event"], "approval-resolved");
    assert_eq!(resolved["data"]["decision"], "allow");
    assert_eq!(resolution["ok"], true);
    let (hook_response, hook_resolved) =
        read_response_and_event(&mut hook_reader, 10, "approval-resolved");
    assert_eq!(hook_resolved["data"]["decision"], "allow");
    assert_eq!(hook_response["data"]["decision"], "allow");
}

/// Every connection is a subscriber, the hook's own included, so "did an island see this"
/// has to leave the caller out. With nobody else listening there is no surface to decide on,
/// and the daemon answers without a decision instead of holding the agent for the full
/// approval timeout only to deny it. The hook reads that as "no opinion" and the agent asks
/// in its own terminal.
#[test]
fn an_approval_with_no_island_listening_answers_without_a_decision() {
    let path = socket();
    let _daemon = spawn_daemon(&path, &[("OPEN_ISLAND_POLL_MS", "1000")]);
    let (mut hook_stream, mut hook_reader) = connect(&path);
    hook_stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");

    let started = std::time::Instant::now();
    send_request(
        &mut hook_stream,
        10,
        "hook_event",
        hook("lonely-session", Some("approval-lonely")),
    );
    let response = read_until_id(&mut hook_reader, 10);

    assert_eq!(response["ok"], true);
    assert_eq!(response["data"]["accepted"], false);
    assert!(
        response["data"].get("decision").is_none(),
        "no island means no decision, not a denial: {}",
        response["data"]
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the agent must not be held for the approval timeout"
    );
}

/// The withdrawal has to take the approval back out, or the agent's retry of the same
/// request would be deduplicated into a denial.
#[test]
fn a_withdrawn_approval_can_be_admitted_again_when_an_island_arrives() {
    let path = socket();
    let _daemon = spawn_daemon(&path, &[("OPEN_ISLAND_POLL_MS", "1000")]);
    let (mut hook_stream, mut hook_reader) = connect(&path);
    hook_stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    send_request(
        &mut hook_stream,
        10,
        "hook_event",
        hook("retry-session", Some("approval-retry")),
    );
    assert_eq!(
        read_until_id(&mut hook_reader, 10)["data"]["accepted"],
        false
    );

    let (mut gui_stream, mut gui_reader) = connect(&path);
    gui_stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    // A connection only becomes a subscriber once the accept loop has taken it, so prove
    // it is registered before asking whether an island is listening.
    send_request(&mut gui_stream, 1, "ping", Value::Null);
    read_until_id(&mut gui_reader, 1);
    send_request(
        &mut hook_stream,
        11,
        "hook_event",
        hook("retry-session", Some("approval-retry")),
    );

    let requested = read_event(&mut gui_reader, "approval-requested");
    assert_eq!(requested["data"]["approval_id"], "approval-retry");
    send_request(
        &mut gui_stream,
        20,
        "resolve_approval",
        json!({"approval_id":"approval-retry","decision":"allow"}),
    );
    assert_eq!(
        read_until_id(&mut hook_reader, 11)["data"]["decision"],
        "allow"
    );
}

#[test]
fn unknown_approval_id_is_an_error() {
    let path = socket();
    let _daemon = spawn_daemon(&path, &[]);
    let (mut stream, mut reader) = connect(&path);
    let response = request(
        &mut reader,
        &mut stream,
        1,
        "resolve_approval",
        json!({"approval_id":"missing","decision":"deny"}),
    );
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"], "approval 'missing' not found");
}

#[test]
fn approval_timeout_denies_without_unbounded_wait() {
    let path = socket();
    let _daemon = spawn_daemon(&path, &[("OPEN_ISLAND_APPROVAL_TIMEOUT_MS", "50")]);
    let (mut stream, mut reader) = connect(&path);
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    // An island that watches and never answers: the timeout deny is the daemon's, and it
    // only applies when there was a surface to answer on.
    let (mut island, mut island_reader) = connect(&path);
    island
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    send_request(&mut island, 99, "ping", Value::Null);
    read_until_id(&mut island_reader, 99);
    send_request(
        &mut stream,
        1,
        "hook_event",
        hook("timeout-session", Some("timeout-1")),
    );
    let response = read_until_id(&mut reader, 1);
    assert_eq!(response["data"]["decision"], "deny");
}

#[test]
fn thirty_third_pending_approval_is_denied_immediately() {
    let path = socket();
    let _daemon = spawn_daemon(&path, &[("OPEN_ISLAND_APPROVAL_TIMEOUT_MS", "30000")]);
    let (mut island, mut island_reader) = connect(&path);
    island
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    request(&mut island_reader, &mut island, 99, "ping", Value::Null);
    let mut clients = Vec::new();
    for _ in 0..31 {
        let (stream, reader) = connect(&path);
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        clients.push((stream, reader));
    }
    // Thirty-two pending requests fit within the legacy connection budget;
    // one socket owns two requests. Only the separate island counts as a UI.
    for index in 0..32 {
        send_request(
            &mut clients[index % 31].0,
            index as i64,
            "hook_event",
            hook(&format!("cap-{index}"), Some(&format!("cap-{index}"))),
        );
        let event = read_event(&mut island_reader, "approval-requested");
        assert_eq!(event["data"]["approval_id"], format!("cap-{index}"));
    }
    let started = Instant::now();
    let (stream, reader) = &mut clients[0];
    let response = request(
        reader,
        stream,
        32,
        "hook_event",
        hook("cap-32", Some("cap-32")),
    );
    assert_eq!(response["data"]["decision"], "deny");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn disconnect_denies_originating_approval_and_notifies_other_client() {
    let path = socket();
    let _daemon = spawn_daemon(&path, &[("OPEN_ISLAND_POLL_MS", "1000")]);
    let (mut hook_stream, _) = connect(&path);
    let (gui_stream, mut gui_reader) = connect(&path);
    gui_stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    send_request(
        &mut hook_stream,
        1,
        "hook_event",
        hook("disconnect-session", Some("disconnect-1")),
    );
    let requested = read_event(&mut gui_reader, "approval-requested");
    assert_eq!(requested["event"], "approval-requested");
    drop(hook_stream);
    let resolved = read_event(&mut gui_reader, "approval-resolved");
    assert_eq!(resolved["event"], "approval-resolved");
    assert_eq!(resolved["data"]["decision"], "deny");
    let _ = gui_stream.shutdown(std::net::Shutdown::Both);
}

#[test]
fn oversized_input_closes_connection_and_daemon_still_answers_ping() {
    let path = socket();
    let _daemon = spawn_daemon(&path, &[]);
    let (mut stream, _) = connect(&path);
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    send_request(&mut stream, 1, "ping", json!({}));
    let _ = stream.write_all(&vec![b'x'; 1024 * 1024 + 8192]);
    let mut bytes = [0; 8192];
    use std::io::Read;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut received = 0;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "oversized connection did not close");
        stream.set_read_timeout(Some(remaining)).unwrap();
        match stream.read(&mut bytes) {
            Ok(0) => break,
            Ok(count) => {
                received += count;
                assert!(received <= 4 * 1024 * 1024 + 256 * 1024);
            }
            Err(error) if error.kind() == ErrorKind::ConnectionReset => break,
            Err(error) => panic!("oversized connection did not close: {error}"),
        }
    }
    let (mut stream, mut reader) = connect(&path);
    assert_eq!(
        request(&mut reader, &mut stream, 1, "ping", json!({}))["ok"],
        true
    );
}

#[test]
fn incomplete_first_frame_expires_even_while_peer_remains_open() {
    let path = socket();
    let _daemon = spawn_daemon(&path, &[]);
    let (mut stream, _) = connect(&path);
    stream
        .set_read_timeout(Some(Duration::from_secs(7)))
        .unwrap();
    stream.write_all(b"{\"v\":1").unwrap();
    use std::io::Read;
    let started = Instant::now();
    while stream.read(&mut [0; 8192]).unwrap() != 0 {
        assert!(started.elapsed() < Duration::from_secs(7));
    }
    assert!(started.elapsed() < Duration::from_secs(7));
    let (mut stream, mut reader) = connect(&path);
    assert_eq!(
        request(&mut reader, &mut stream, 1, "ping", json!({}))["ok"],
        true
    );
}

#[test]
fn paged_snapshot_roundtrips_over_socket_and_is_connection_owned() {
    let path = socket();
    let _daemon = spawn_daemon(&path, &[]);
    let (mut stream, mut reader) = connect(&path);
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let ping = request(&mut reader, &mut stream, 1, "ping", json!({}));
    assert_eq!(
        ping["data"]["capabilities"],
        json!(open_island_core::diagnostics::CAPABILITIES)
    );
    let begin = request(&mut reader, &mut stream, 2, "get_ui_state", json!({}));
    assert_eq!(begin["ok"], true);
    let id = begin["data"]["snapshot_id"].as_u64().unwrap();
    let (mut stranger, mut stranger_reader) = connect(&path);
    let denied = request(
        &mut stranger_reader,
        &mut stranger,
        1,
        "get_ui_state_page",
        json!({"snapshot_id":id,"expected_page":0}),
    );
    assert_eq!(denied["error"], "snapshot_not_found");
    let snapshot: open_island_core::ui_state::UiSnapshot =
        open_island_core::snapshot_page::decode(id, |page| {
            let reply = request(
                &mut reader,
                &mut stream,
                10 + page as i64,
                "get_ui_state_page",
                json!({"snapshot_id":id,"expected_page":page}),
            );
            if reply["ok"] != true {
                return Err(reply.to_string());
            }
            serde_json::from_value(reply["data"].clone()).map_err(|e| e.to_string())
        })
        .unwrap();
    assert_eq!(snapshot.schema_version, 1);
    assert_eq!(
        snapshot.daemon_epoch.0,
        ping["data"]["daemon_epoch"].as_str().unwrap()
    );
    assert!(snapshot
        .sessions
        .iter()
        .all(|s| s.session.queued_messages.is_none()));
    assert!(snapshot.approvals.is_empty());
    assert!(snapshot.questions.is_empty());
}

#[test]
fn ui_subscription_receives_invalidation_and_transient_commands() {
    let path = socket();
    let _daemon = spawn_daemon(&path, &[]);
    let (mut stream, mut reader) = connect(&path);
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let subscription = request(&mut reader, &mut stream, 1, "subscribe_ui", json!({}));
    assert_eq!(subscription["ok"], true);
    assert_eq!(
        subscription["data"]["capabilities"],
        json!(open_island_core::diagnostics::CAPABILITIES)
    );
    let (mut hook, mut hook_reader) = connect(&path);
    assert_eq!(
        request(
            &mut hook_reader,
            &mut hook,
            1,
            "hook_event",
            json!({"agent":"claude","session_id":"snapshot-test","event":"session-start","pid":std::process::id()})
        )["ok"],
        true
    );
    let mut line = String::new();
    loop {
        line.clear();
        reader.read_line(&mut line).unwrap();
        let event: Value = serde_json::from_str(&line).unwrap();
        if event["event"] == "ui-state-invalidated" {
            assert!(event["data"]["publication_revision"].as_u64().unwrap() > 0);
            assert!(event["data"]["daemon_epoch"].as_str().is_some());
            break;
        }
        assert_ne!(event["event"], "sessions-updated");
    }
    request(&mut hook_reader, &mut hook, 2, "settings", json!({}));
    loop {
        line.clear();
        reader.read_line(&mut line).unwrap();
        let event: Value = serde_json::from_str(&line).unwrap();
        if event["event"] == "open-settings" {
            break;
        }
    }
}

#[test]
fn a_diagnostic_connection_neither_holds_approval_nor_receives_prompt_events() {
    let path = socket();
    let _daemon = spawn_daemon(&path, &[("OPEN_ISLAND_POLL_MS", "1000")]);
    let (mut doctor, mut doctor_reader) = connect(&path);
    doctor
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    assert_eq!(
        request(
            &mut doctor_reader,
            &mut doctor,
            1,
            "ping",
            json!({"client_role":"diagnostic"})
        )["ok"],
        true
    );
    let (mut source, mut source_reader) = connect(&path);
    source
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let started = Instant::now();
    let result = request(
        &mut source_reader,
        &mut source,
        2,
        "hook_event",
        hook("private-prompt", Some("diagnostic-alone")),
    );
    assert_eq!(result["data"]["accepted"], false);
    assert!(result["data"].get("decision").is_none());
    assert!(started.elapsed() < Duration::from_secs(2));
    // The next frame on the doctor socket must be its own reply, not a session event.
    send_request(&mut doctor, 3, "ping", json!({"client_role":"diagnostic"}));
    let next = read_message(
        &mut doctor_reader,
        Instant::now() + Duration::from_secs(2),
        "doctor reply",
    );
    assert_eq!(next["id"], 3);
    assert_eq!(next["data"]["counters"]["no_island"], 1);
    assert!(next.get("event").is_none());
}

#[test]
fn managed_and_legacy_connections_have_separate_32_slot_budgets() {
    let path = socket();
    let _daemon = spawn_daemon(&path, &[("OPEN_ISLAND_POLL_MS", "10000")]);
    let mut managed = Vec::new();
    for _ in 0..32 {
        let (mut stream, mut reader) = connect(&path);
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        assert_eq!(
            request(&mut reader, &mut stream, 1, "subscribe_ui", json!({}))["ok"],
            true
        );
        managed.push((stream, reader));
    }
    let mut legacy = Vec::new();
    for _ in 0..32 {
        let (mut stream, mut reader) = connect(&path);
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        assert_eq!(
            request(
                &mut reader,
                &mut stream,
                2,
                "ping",
                json!({"client_role":"diagnostic"})
            )["ok"],
            true
        );
        legacy.push((stream, reader));
    }
    let (stream, reader) = &mut legacy[0];
    assert_eq!(
        request(reader, stream, 3, "subscribe_ui", json!({}))["error"],
        "server_busy"
    );
    let mut excess = UnixStream::connect(&path).unwrap();
    excess
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut byte = [0u8];
    match std::io::Read::read(&mut excess, &mut byte) {
        Ok(0) => {}
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::ConnectionReset | ErrorKind::BrokenPipe
            ) => {}
        other => panic!("connection 65 must close: {other:?}"),
    }
    // Saturation does not prevent replies on a previously admitted UI connection.
    let (stream, reader) = &mut managed[0];
    assert_eq!(request(reader, stream, 4, "ping", json!({}))["ok"], true);
    let (stream, reader) = &mut legacy[0];
    let report = request(
        reader,
        stream,
        5,
        "ping",
        json!({"client_role":"diagnostic"}),
    );
    let counters = &report["data"]["counters"];
    assert_eq!(counters["connections_legacy"]["active"], 32);
    assert_eq!(counters["connections_managed"]["active"], 32);
    assert_eq!(counters["connections_legacy"]["rejected"], 1);
    assert_eq!(counters["connections_managed"]["rejected"], 1);
    assert_eq!(counters["connections_managed"]["high_water"], 32);
}
