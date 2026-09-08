use serde_json::{json, Value};
use std::{
    env, fs,
    io::{self, BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

mod support;

use support::isolated_home;

const PERMISSION_FIXTURE: &str = r#"{"session_id":"abc123","cwd":"/tmp/project","hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"pwd"},"approval_id":"approval-1"}"#;
const SESSION_START_FIXTURE: &str =
    r#"{"session_id":"abc123","cwd":"/tmp/project","hook_event_name":"SessionStart"}"#;
const ALLOW_OUTPUT: &str = r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow","updatedInput":{"command":"pwd"}}}}"#;
const DENY_OUTPUT: &str = r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"deny"}}}"#;

fn socket() -> PathBuf {
    env::temp_dir().join(format!(
        "open-island-hook-cli-{}-{}.sock",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos())
    ))
}

fn spawn_daemon(path: &Path, approval_ms: u64) -> Child {
    Command::new(env!("CARGO_BIN_EXE_open-islandd"))
        .arg("--socket")
        .arg(path)
        .env("HOME", isolated_home())
        .env("XDG_CONFIG_HOME", isolated_home())
        .env("XDG_STATE_HOME", isolated_home())
        .env("OPEN_ISLAND_POLL_MS", "25")
        .env("OPEN_ISLAND_APPROVAL_TIMEOUT_MS", approval_ms.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn daemon")
}

fn connect(path: &Path) -> (UnixStream, BufReader<UnixStream>) {
    for _ in 0..50 {
        if let Ok(stream) = UnixStream::connect(path) {
            let reader = BufReader::new(stream.try_clone().expect("clone"));
            return (stream, reader);
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("daemon did not bind");
}

fn read_event_or_response(reader: &mut BufReader<UnixStream>, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut line = String::new();
    loop {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        match reader.read_line(&mut line) {
            Ok(0) => thread::sleep(Duration::from_millis(10)),
            Ok(_) => return,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("{what}: {error}"),
        }
    }
}

fn read_event(reader: &mut BufReader<UnixStream>, event: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut line = String::new();
    loop {
        assert!(Instant::now() < deadline, "timed out waiting for {event}");
        match reader.read_line(&mut line) {
            Ok(0) => thread::sleep(Duration::from_millis(10)),
            Ok(_) => {
                let value: Value = serde_json::from_str(line.trim()).expect("event JSON");
                if value["event"] == event {
                    return value;
                }
                line.clear();
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("read event: {error}"),
        }
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

fn cleanup(child: &mut Child, path: &Path) {
    let _ = child.kill();
    let _ = child.wait();
    let _ = fs::remove_file(path);
}

fn run_hook(args: &[&str], input: &str) -> (String, String, ExitStatus) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_open-islandd"));
    command
        .arg("hook")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("spawn hook");
    {
        let mut stdin = child.stdin.take().expect("hook stdin");
        stdin.write_all(input.as_bytes()).expect("write hook stdin");
    }
    let output = child.wait_with_output().expect("hook output");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status,
    )
}

#[test]
fn cli_permission_allow_via_real_daemon_and_resolver() {
    let path = socket();
    let mut daemon = spawn_daemon(&path, 3000);
    let (mut resolver, mut resolver_reader) = connect(&path);
    resolver
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("resolver timeout");

    let mut hook = Command::new(env!("CARGO_BIN_EXE_open-islandd"))
        .arg("hook")
        .arg("--agent")
        .arg("claude")
        .arg("--socket")
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hook");
    {
        let mut stdin = hook.stdin.take().expect("hook stdin");
        stdin
            .write_all(PERMISSION_FIXTURE.as_bytes())
            .expect("write hook stdin");
    }

    let event = read_event(&mut resolver_reader, "approval-requested");
    let approval_id = event["data"]["approval_id"]
        .as_str()
        .expect("approval id")
        .to_owned();
    send_request(
        &mut resolver,
        1,
        "resolve_approval",
        json!({"approval_id": approval_id, "decision": "allow"}),
    );

    let output = hook.wait_with_output().expect("hook output");
    assert!(
        output.status.success(),
        "hook should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_str::<Value>(String::from_utf8_lossy(&output.stdout).trim())
            .expect("hook output is JSON"),
        serde_json::from_str::<Value>(ALLOW_OUTPUT).expect("expected output is JSON")
    );
    cleanup(&mut daemon, &path);
}

#[test]
fn cli_falls_through_when_daemon_unavailable() {
    let path = socket();
    let (stdout, stderr, status) = run_hook(
        &["--agent", "claude", "--socket", path.to_str().unwrap()],
        PERMISSION_FIXTURE,
    );
    assert!(
        status.success(),
        "hook should stay quiet, not fail: {stderr}"
    );
    assert!(stdout.trim().is_empty(), "unexpected decision: {stdout}");
}

#[test]
fn cli_returns_empty_output_for_non_approval_when_daemon_unavailable() {
    let path = socket();
    let (stdout, stderr, status) = run_hook(
        &["--agent", "claude", "--socket", path.to_str().unwrap()],
        SESSION_START_FIXTURE,
    );
    assert!(status.success(), "hook should succeed: {stderr}");
    assert!(
        stdout.is_empty(),
        "non-approval must write no stdout: {stdout:?}"
    );
}

/// The no-answer deny is the daemon's, and it only applies when an island was there to
/// answer on. With nobody watching the daemon says nothing at all, which
/// `cli_falls_through_when_daemon_unavailable` covers.
#[test]
fn cli_denies_when_an_island_watches_and_never_answers() {
    let path = socket();
    let mut daemon = spawn_daemon(&path, 50);
    let (mut island, mut island_reader) = connect(&path);
    island
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("island timeout");
    send_request(&mut island, 99, "ping", json!(null));
    read_event_or_response(&mut island_reader, "ping reply");

    let (stdout, stderr, status) = run_hook(
        &["--agent", "claude", "--socket", path.to_str().unwrap()],
        PERMISSION_FIXTURE,
    );

    assert!(status.success(), "hook should deny, not fail: {stderr}");
    assert_eq!(stdout.trim(), DENY_OUTPUT);
    cleanup(&mut daemon, &path);
}

#[test]
fn cli_malformed_stdin_fails_nonzero_without_stdout() {
    let path = socket();
    let (stdout, stderr, status) = run_hook(
        &["--agent", "claude", "--socket", path.to_str().unwrap()],
        "{",
    );
    assert!(!status.success(), "malformed input must fail");
    assert!(
        stdout.is_empty(),
        "malformed input writes no stdout: {stdout:?}"
    );
    assert!(!stderr.is_empty(), "malformed input reports on stderr");
}

#[test]
fn cli_empty_stdin_fails_nonzero() {
    let path = socket();
    let (stdout, _, status) = run_hook(
        &["--agent", "claude", "--socket", path.to_str().unwrap()],
        "",
    );
    assert!(!status.success(), "empty stdin must fail");
    assert!(stdout.is_empty(), "empty stdin writes no stdout");
}

#[test]
fn cli_rejects_unsupported_agent_without_stdout() {
    let (stdout, stderr, status) = run_hook(&["--agent", "gemini"], "");
    assert!(!status.success(), "unsupported agent must fail");
    assert!(stdout.is_empty(), "unsupported agent writes no stdout");
    assert!(
        stderr.contains("gemini"),
        "stderr names the agent: {stderr}"
    );
}
