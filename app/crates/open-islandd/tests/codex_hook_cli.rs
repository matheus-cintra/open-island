use serde_json::{json, Value};
use std::{
    env, fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

mod support;

use support::isolated_home;

const PERMISSION_FIXTURE: &str = include_str!("../fixtures/codex-permission-request.json");
const SESSION_START_FIXTURE: &str = include_str!("../fixtures/codex-session-start.json");
const TOOL_FIXTURE: &str = include_str!("../fixtures/codex-pre-tool-use.json");
const ALLOW_OUTPUT: &str = r#"{"decision":"allow"}"#;
const DENY_OUTPUT: &str = r#"{"decision":"deny"}"#;

fn socket(name: &str) -> PathBuf {
    env::temp_dir().join(format!(
        "open-island-codex-hook-{name}-{}-{}.sock",
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
        .env("OPEN_ISLAND_POLL_MS", "1000")
        .env("OPEN_ISLAND_APPROVAL_TIMEOUT_MS", approval_ms.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn daemon")
}

fn connect(path: &Path) -> (UnixStream, BufReader<UnixStream>) {
    for _ in 0..100 {
        if let Ok(stream) = UnixStream::connect(path) {
            let reader = BufReader::new(stream.try_clone().expect("clone stream"));
            return (stream, reader);
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("daemon did not bind");
}

fn read_event(reader: &mut BufReader<UnixStream>, name: &str) -> Value {
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("read event");
        let value: Value = serde_json::from_str(line.trim()).expect("event JSON");
        if value["event"] == name {
            return value;
        }
    }
}

fn send_request(stream: &mut UnixStream, id: u64, method: &str, params: Value) {
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

fn run_hook(path: &Path, fixture: &str) -> std::process::Output {
    let mut hook = Command::new(env!("CARGO_BIN_EXE_open-islandd"))
        .args(["hook", "--agent", "codex", "--socket"])
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn Codex hook");
    hook.stdin
        .take()
        .expect("hook stdin")
        .write_all(fixture.as_bytes())
        .expect("write fixture");
    hook.wait_with_output().expect("hook output")
}

#[test]
fn codex_cli_permission_allow_uses_real_daemon() {
    resolve_through_daemon("allow", "allow", ALLOW_OUTPUT);
}

#[test]
fn codex_cli_permission_deny_from_the_daemon_still_denies() {
    resolve_through_daemon("deny", "deny", DENY_OUTPUT);
}

fn resolve_through_daemon(name: &str, decision: &str, expected: &str) {
    let path = socket(name);
    let mut daemon = spawn_daemon(&path, 3000);
    let (mut resolver, mut reader) = connect(&path);
    reader
        .get_mut()
        .set_read_timeout(Some(Duration::from_secs(3)))
        .expect("set timeout");
    let mut hook = Command::new(env!("CARGO_BIN_EXE_open-islandd"))
        .args(["hook", "--agent", "codex", "--socket"])
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn Codex hook");
    hook.stdin
        .take()
        .expect("hook stdin")
        .write_all(PERMISSION_FIXTURE.as_bytes())
        .expect("write permission fixture");
    let requested = read_event(&mut reader, "approval-requested");
    let approval_id = requested["data"]["approval_id"]
        .as_str()
        .expect("approval id")
        .to_owned();
    send_request(
        &mut resolver,
        1,
        "resolve_approval",
        json!({"approval_id": approval_id, "decision": decision}),
    );
    let output = hook.wait_with_output().expect("hook output");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), expected);
    cleanup(&mut daemon, &path);
}

#[test]
fn codex_cli_forwards_non_approval_and_keeps_stdout_empty() {
    let path = socket("tool");
    let mut daemon = spawn_daemon(&path, 3000);
    let output = run_hook(&path, TOOL_FIXTURE);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    cleanup(&mut daemon, &path);
}

#[test]
fn codex_cli_non_approval_is_empty_when_daemon_is_unavailable() {
    let output = run_hook(&socket("missing"), SESSION_START_FIXTURE);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn codex_cli_unavailable_permission_falls_through_without_stdout() {
    let output = run_hook(&socket("unavailable"), PERMISSION_FIXTURE);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn codex_cli_malformed_input_fails_without_stdout() {
    let output = run_hook(&socket("malformed"), "{");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}
