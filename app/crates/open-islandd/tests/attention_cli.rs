use serde_json::{json, Value};
use std::{
    env, fs,
    io::{BufRead, BufReader, ErrorKind, Write},
    os::unix::{net::UnixStream, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

mod support;

use support::{config_without_release_check, isolated_home};

fn socket(label: &str) -> PathBuf {
    env::temp_dir().join(format!(
        "open-island-test-attention-{label}-{}-{}.sock",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos())
    ))
}

struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Daemon {
    child: Child,
    path: PathBuf,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.path);
    }
}

fn spawn_daemon(path: &Path, idle_ms: u64) -> Daemon {
    let child = Command::new(env!("CARGO_BIN_EXE_open-islandd"))
        .arg("--socket")
        .arg(path)
        .env("HOME", isolated_home())
        .env("XDG_CONFIG_HOME", isolated_home())
        .env("XDG_STATE_HOME", isolated_home())
        .env("OPEN_ISLAND_CONFIG", config_without_release_check())
        .env("OPEN_ISLAND_POLL_MS", "25")
        .env("OPEN_ISLAND_IDLE_MS", idle_ms.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn daemon");
    Daemon {
        child,
        path: path.to_path_buf(),
    }
}

/// A process discovery accepts as an agent through its argv0 on Linux and macOS.
/// A `sleep` wearing the agent's name is enough to
/// give a hook session something to join.
fn spawn_agent_process(agent: &str, cwd: &Path) -> Killed {
    let child = Command::new("/bin/sleep")
        .arg0(agent)
        .arg("120")
        .current_dir(cwd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn fake agent");
    Killed(child)
}

fn connect(path: &Path) -> (UnixStream, BufReader<UnixStream>) {
    for _ in 0..200 {
        if let Ok(stream) = UnixStream::connect(path) {
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .expect("read timeout");
            let reader = BufReader::new(stream.try_clone().expect("clone"));
            return (stream, reader);
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("daemon did not bind {}", path.display());
}

fn request(
    stream: &mut UnixStream,
    reader: &mut BufReader<UnixStream>,
    id: i64,
    method: &str,
) -> Value {
    writeln!(
        stream,
        "{}",
        json!({"v":1,"id":id,"method":method,"params":{}})
    )
    .expect("write request");
    stream.flush().expect("flush request");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut line = String::new();
    loop {
        assert!(Instant::now() < deadline, "timed out waiting for {method}");
        match reader.read_line(&mut line) {
            Ok(0) => thread::sleep(Duration::from_millis(10)),
            Ok(_) => {
                let value: Value = serde_json::from_str(line.trim()).expect("response JSON");
                if value["id"] == json!(id) {
                    assert_eq!(value["ok"], json!(true), "{method} failed: {value}");
                    return value["data"].clone();
                }
                line.clear();
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                thread::sleep(Duration::from_millis(10))
            }
            Err(error) => panic!("read response: {error}"),
        }
    }
}

fn hook(agent: &str, path: &Path, input: &str) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_open-islandd"))
        .arg("hook")
        .arg("--agent")
        .arg(agent)
        .arg("--socket")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hook");
    child
        .stdin
        .take()
        .expect("hook stdin")
        .write_all(input.as_bytes())
        .expect("write hook stdin");
    let output = child.wait_with_output().expect("hook output");
    assert!(
        output.status.success(),
        "hook failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A hook writes to the socket and exits before the daemon has necessarily applied the
/// event, so every assertion polls until the state it is waiting for shows up.
struct Island {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
    id: i64,
}

impl Island {
    fn new(path: &Path) -> Self {
        let (stream, reader) = connect(path);
        Self {
            stream,
            reader,
            id: 0,
        }
    }

    fn session(&mut self, pid: u32) -> Value {
        self.id += 1;
        let sessions = request(&mut self.stream, &mut self.reader, self.id, "list_sessions");
        sessions
            .as_array()
            .expect("session array")
            .iter()
            .find(|session| session["pid"] == json!(pid))
            .cloned()
            .unwrap_or_else(|| panic!("no session for pid {pid} in {sessions}"))
    }

    fn session_where(&mut self, pid: u32, what: &str, ready: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut last = Value::Null;
        while Instant::now() < deadline {
            last = self.session(pid);
            if ready(&last) {
                return last;
            }
            thread::sleep(Duration::from_millis(20));
        }
        panic!("timed out waiting for {what}, last session was {last}")
    }

    fn attention(&mut self, pid: u32, expected: &str) -> Value {
        self.session_where(pid, expected, |session| {
            session["attention"] == json!(expected)
        })
    }
}

fn claude_event(name: &str, cwd: &Path, pid: u32, extra: &str) -> String {
    format!(
        r#"{{"session_id":"c3-attention","cwd":"{}","pid":{pid},"hook_event_name":"{name}"{extra}}}"#,
        cwd.display()
    )
}

#[test]
fn a_stop_reaches_the_island_as_needs_attention_and_the_next_prompt_takes_it_back() {
    let path = socket("stop");
    let cwd = env::temp_dir();
    let agent = spawn_agent_process("claude", &cwd);
    let pid = agent.0.id();
    let _daemon = spawn_daemon(&path, 60_000);
    let mut island = Island::new(&path);

    hook(
        "claude",
        &path,
        &claude_event(
            "UserPromptSubmit",
            &cwd,
            pid,
            r#","prompt":"Answer agent questions from the island""#,
        ),
    );
    let working = island.attention(pid, "working");
    assert_eq!(working["name"], json!("Answer agent questions from the…"));

    hook(
        "claude",
        &path,
        &claude_event("Stop", &cwd, pid, r#","last_assistant_message":"DONE=1""#),
    );
    let stopped = island.attention(pid, "needs_attention");
    assert_eq!(
        stopped["name"], working["name"],
        "the name is derived once and survives the stop"
    );

    hook(
        "claude",
        &path,
        &claude_event("UserPromptSubmit", &cwd, pid, r#","prompt":"And now this""#),
    );
    let resumed = island.attention(pid, "working");
    assert_eq!(resumed["name"], working["name"]);
}

#[test]
fn a_stopped_session_settles_into_idle_after_the_configured_delay() {
    let path = socket("idle");
    let cwd = env::temp_dir();
    let agent = spawn_agent_process("claude", &cwd);
    let pid = agent.0.id();
    let _daemon = spawn_daemon(&path, 300);
    let mut island = Island::new(&path);

    hook("claude", &path, &claude_event("Stop", &cwd, pid, ""));
    island.attention(pid, "needs_attention");

    thread::sleep(Duration::from_millis(400));
    assert_eq!(island.session(pid)["attention"], json!("idle"));
}

#[test]
fn an_opencode_session_idle_is_the_same_stop() {
    let path = socket("opencode");
    let cwd = env::temp_dir();
    let agent = spawn_agent_process("opencode", &cwd);
    let pid = agent.0.id();
    let _daemon = spawn_daemon(&path, 60_000);
    let mut island = Island::new(&path);

    hook(
        "opencode",
        &path,
        &format!(
            r#"{{"type":"open-island.prompt","properties":{{"sessionID":"oc-1","text":"Inspect the daemon socket"}},"cwd":"{}","pid":{pid}}}"#,
            cwd.display()
        ),
    );
    island.session_where(pid, "the derived name", |session| {
        session["name"] == json!("Inspect the daemon socket")
    });

    hook(
        "opencode",
        &path,
        &format!(
            r#"{{"type":"session.idle","properties":{{"sessionID":"oc-1"}},"cwd":"{}","pid":{pid}}}"#,
            cwd.display()
        ),
    );

    let session = island.attention(pid, "needs_attention");
    assert_eq!(session["name"], json!("Inspect the daemon socket"));
}

fn request_with(island: &mut Island, method: &str, params: Value) -> Value {
    island.id += 1;
    writeln!(
        island.stream,
        "{}",
        json!({"v":1,"id":island.id,"method":method,"params":params})
    )
    .expect("write request");
    island.stream.flush().expect("flush request");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut line = String::new();
    loop {
        assert!(Instant::now() < deadline, "timed out waiting for {method}");
        line.clear();
        match island.reader.read_line(&mut line) {
            Ok(0) => thread::sleep(Duration::from_millis(10)),
            Ok(_) => {
                let value: Value = serde_json::from_str(line.trim()).expect("response JSON");
                if value["id"] == json!(island.id) {
                    return value;
                }
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                thread::sleep(Duration::from_millis(10))
            }
            Err(error) => panic!("{method}: {error}"),
        }
    }
}

struct FakeKitty {
    child: Child,
    directory: PathBuf,
}

fn spawn_when_not_busy(command: &mut Command) -> Child {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match command.spawn() {
            Ok(child) => return child,
            Err(error)
                if error.kind() == ErrorKind::ExecutableFileBusy && Instant::now() < deadline =>
            {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("spawn fake kitty: {error}"),
        }
    }
}

impl Drop for FakeKitty {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn spawn_agent_under_fake_kitty(
    label: &str,
    agent: &str,
    cwd: &Path,
    listen_on: Option<&str>,
) -> (FakeKitty, u32) {
    let directory = env::temp_dir().join(format!(
        "open-island-fake-kitty-{}-{label}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).expect("fake kitty dir");
    let kitty = directory.join("kitty");
    fs::copy("/bin/bash", &kitty).expect("copy bash as kitty");
    let mut command = Command::new(&kitty);
    command
        .arg("-c")
        .arg(format!("(exec -a {agent} /bin/sleep 120) & wait"))
        .current_dir(cwd)
        .env_remove("KITTY_LISTEN_ON")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    if let Some(listen_on) = listen_on {
        command.env("KITTY_LISTEN_ON", listen_on);
    }
    let guard = FakeKitty {
        child: spawn_when_not_busy(&mut command),
        directory,
    };
    let parent = guard.child.id();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(pid) = open_island_core::process::pids().into_iter().find(|pid| {
            open_island_core::process::parent_and_comm(*pid).is_some_and(|(ppid, _)| ppid == parent)
        }) {
            return (guard, pid);
        }
        assert!(
            Instant::now() < deadline,
            "the fake kitty never spawned the agent"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_message_sent_while_the_agent_works_waits_and_leaves_when_it_stops() {
    let path = socket("message");
    let cwd = env::temp_dir();
    let (_kitty, pid) = spawn_agent_under_fake_kitty(
        "message",
        "claude",
        &cwd,
        Some("unix:/tmp/open-island-no-such-kitty"),
    );
    let _daemon = spawn_daemon(&path, 300);
    let mut island = Island::new(&path);

    hook(
        "claude",
        &path,
        &claude_event("UserPromptSubmit", &cwd, pid, r#","prompt":"trabalhe""#),
    );
    let working = island.attention(pid, "working");
    assert_eq!(working["send_channel"], json!("kitty"));
    let id = working["id"].as_str().expect("session id").to_owned();

    let first = request_with(
        &mut island,
        "send_message",
        json!({"id": id, "text": "primeira"}),
    );
    assert_eq!(first["ok"], json!(true), "{first}");
    assert_eq!(first["data"]["delivered"], json!(false));
    let second = request_with(
        &mut island,
        "send_message",
        json!({"id": id, "text": "segunda\n"}),
    );
    assert_eq!(second["data"]["delivered"], json!(false));
    let queued = island.session_where(pid, "two queued messages", |session| {
        session["queued_messages"].as_array().map(Vec::len) == Some(2)
    });
    assert_eq!(queued["queued_messages"][1]["text"], json!("segunda"));

    let cancel = request_with(
        &mut island,
        "cancel_message",
        json!({"id": id, "message_id": second["data"]["message_id"]}),
    );
    assert_eq!(cancel["ok"], json!(true), "{cancel}");
    island.session_where(pid, "one queued message", |session| {
        session["queued_messages"].as_array().map(Vec::len) == Some(1)
    });
    let twice = request_with(
        &mut island,
        "cancel_message",
        json!({"id": id, "message_id": second["data"]["message_id"]}),
    );
    assert_eq!(twice["ok"], json!(false));

    hook(
        "claude",
        &path,
        &claude_event("Stop", &cwd, pid, r#","last_assistant_message":"DONE=1""#),
    );
    island.attention(pid, "needs_attention");
    island.session_where(
        pid,
        "the queue still held while needs_attention",
        |session| session["queued_messages"].as_array().map(Vec::len) == Some(1),
    );
    let idle = island.attention(pid, "idle");
    let drained = island.session_where(pid, "the queue drained on idle", |session| {
        session["queued_messages"].is_null()
    });
    assert_eq!(drained["id"], idle["id"]);

    let blank = request_with(
        &mut island,
        "send_message",
        json!({"id": id, "text": "  \n"}),
    );
    assert_eq!(blank["ok"], json!(false));
    assert_eq!(blank["error"], json!("empty message"));
    let missing = request_with(
        &mut island,
        "send_message",
        json!({"id": "claude:nope", "text": "oi"}),
    );
    assert_eq!(missing["error"], json!("session 'claude:nope' not found"));
}

#[test]
fn a_kitty_without_remote_control_refuses_the_message_with_its_code() {
    let path = socket("blocked");
    let cwd = env::temp_dir();
    let (_kitty, pid) = spawn_agent_under_fake_kitty("blocked", "claude", &cwd, None);
    let _daemon = spawn_daemon(&path, 60_000);
    let mut island = Island::new(&path);
    let session = island.session_where(pid, "the process session", |_| true);
    assert_eq!(session["send_blocked"], json!("kitty_remote_control_off"));
    let refused = request_with(
        &mut island,
        "send_message",
        json!({"id": session["id"], "text": "oi"}),
    );
    assert_eq!(refused["ok"], json!(false));
    assert_eq!(refused["error"], json!("kitty_remote_control_off"));
}
