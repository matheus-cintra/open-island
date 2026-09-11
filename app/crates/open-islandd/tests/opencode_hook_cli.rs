use serde_json::{json, Value};
use std::{
    env, fs,
    io::{BufRead, BufReader, ErrorKind, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

mod support;

use support::{config_without_release_check, isolated_home};

const PERMISSION_FIXTURE: &str = include_str!("../fixtures/opencode-permission-asked.json");
const ALLOW_OUTPUT: &str = r#"{"reply":"once"}"#;
const DENY_OUTPUT: &str = r#"{"reply":"reject"}"#;

fn socket(name: &str) -> PathBuf {
    env::temp_dir().join(format!(
        "open-island-opencode-hook-{name}-{}-{}.sock",
        std::process::id(),
        support::unique_id()
    ))
}

fn spawn_daemon(path: &Path, approval_ms: u64) -> Child {
    Command::new(env!("CARGO_BIN_EXE_open-islandd"))
        .arg("--socket")
        .arg(path)
        .env("HOME", isolated_home())
        .env("XDG_CONFIG_HOME", isolated_home())
        .env("XDG_STATE_HOME", isolated_home())
        .env("OPEN_ISLAND_CONFIG", config_without_release_check())
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
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut line = String::new();
    loop {
        assert!(Instant::now() < deadline, "timed out waiting for {name}");
        match reader.read_line(&mut line) {
            Ok(0) => thread::sleep(Duration::from_millis(10)),
            Ok(_) => {
                let value: Value = serde_json::from_str(line.trim()).expect("event JSON");
                if value["event"] == name {
                    return value;
                }
                line.clear();
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                thread::sleep(Duration::from_millis(10))
            }
            Err(error) => panic!("read event: {error}"),
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

#[test]
fn opencode_permission_uses_real_daemon_and_native_reply() {
    resolve_through_daemon("allow", ALLOW_OUTPUT);
}

#[test]
fn opencode_permission_deny_from_the_daemon_still_rejects() {
    resolve_through_daemon("deny", DENY_OUTPUT);
}

fn resolve_through_daemon(decision: &str, expected: &str) {
    let path = socket(decision);
    let mut daemon = spawn_daemon(&path, 3000);
    let (mut resolver, mut reader) = connect(&path);
    reader
        .get_mut()
        .set_read_timeout(Some(Duration::from_secs(3)))
        .expect("set timeout");
    let mut hook = Command::new(env!("CARGO_BIN_EXE_open-islandd"))
        .args(["hook", "--agent", "opencode", "--socket"])
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn OpenCode hook");
    hook.stdin
        .take()
        .expect("hook stdin")
        .write_all(PERMISSION_FIXTURE.as_bytes())
        .expect("write fixture");
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
fn opencode_permission_falls_through_when_daemon_is_unavailable() {
    let mut hook = Command::new(env!("CARGO_BIN_EXE_open-islandd"))
        .args([
            "hook",
            "--agent",
            "opencode",
            "--socket",
            "/tmp/open-island-opencode-missing.sock",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run OpenCode hook");
    hook.stdin
        .take()
        .expect("hook stdin")
        .write_all(PERMISSION_FIXTURE.as_bytes())
        .expect("write fixture");
    let output = hook.wait_with_output().expect("hook output");
    assert!(output.status.success());
    assert!(
        output.stdout.is_empty(),
        "unexpected reply: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}
