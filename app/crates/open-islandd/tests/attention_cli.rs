use serde_json::{json, Value};
use std::{
    env, fs,
    io::{BufRead, BufReader, Write},
    os::unix::{net::UnixStream, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

mod support;

use support::isolated_home;

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

/// A process the daemon's procfs discovery accepts as an agent: `discovery::scan` reads
/// argv0 from `/proc/<pid>/cmdline`, so a `sleep` wearing the agent's name is enough to
/// give a hook session something to join.
fn spawn_agent_process(agent: &str, cwd: &Path) -> Killed {
    let child = Command::new("/usr/bin/sleep")
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
    loop {
        assert!(Instant::now() < deadline, "timed out waiting for {method}");
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => thread::sleep(Duration::from_millis(10)),
            Ok(_) => {
                let value: Value = serde_json::from_str(line.trim()).expect("response JSON");
                if value["id"] == json!(id) {
                    assert_eq!(value["ok"], json!(true), "{method} failed: {value}");
                    return value["data"].clone();
                }
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
