use serde_json::{json, Value};
use std::{
    env, fs,
    io::{BufRead, BufReader, ErrorKind, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

mod support;

use support::isolated_home;

const CLAUDE_QUESTION: &str = r#"{"session_id":"abc123","cwd":"/tmp/project","prompt_id":"prompt-9","permission_mode":"bypassPermissions","hook_event_name":"PermissionRequest","tool_name":"AskUserQuestion","tool_input":{"questions":[{"question":"Qual cor?","header":"Cor","options":[{"label":"Vermelho","description":"A cor vermelha"},{"label":"Azul","description":"A cor azul"}],"multiSelect":false}]}}"#;
const CLAUDE_MULTI_QUESTION: &str = r#"{"session_id":"abc123","cwd":"/tmp/project","prompt_id":"prompt-9","permission_mode":"bypassPermissions","hook_event_name":"PermissionRequest","tool_name":"AskUserQuestion","tool_input":{"questions":[{"question":"Quais cores?","header":"Cor","options":[{"label":"Vermelho"},{"label":"Azul"}],"multiSelect":true}]}}"#;
const CODEX_QUESTION: &str = r#"{"session_id":"abc123","turn_id":"turn-1","cwd":"/tmp/project","hook_event_name":"PreToolUse","permission_mode":"default","tool_name":"request_user_input","tool_use_id":"call_1","tool_input":{"questions":[{"header":"Colour","id":"colour","question":"Which colour?","options":[{"label":"Vermelho"},{"label":"Azul"}]}]}}"#;
const CODEX_ANSWERED: &str = r#"{"session_id":"abc123","turn_id":"turn-1","cwd":"/tmp/project","hook_event_name":"PostToolUse","permission_mode":"default","tool_name":"request_user_input","tool_use_id":"call_1","tool_input":{"questions":[{"header":"Colour","id":"colour","question":"Which colour?","options":[{"label":"Vermelho"},{"label":"Azul"}]}]},"tool_response":"{\"answers\":{\"colour\":{\"answers\":[\"Vermelho\"]}}}"}"#;
const OPENCODE_QUESTION: &str = r#"{"type":"question.asked","properties":{"id":"que_1","sessionID":"abc123","questions":[{"question":"Qual cor?","header":"Cor","options":[{"label":"Vermelho"},{"label":"Azul"}],"multiple":false}]},"cwd":"/tmp/project"}"#;

/// The allow Claude receives when nothing answered: the call proceeds unchanged and its
/// own TUI renders the question.
const CLAUDE_UNANSWERED: &str = r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}"#;

fn socket(label: &str) -> PathBuf {
    env::temp_dir().join(format!(
        "open-island-question-{label}-{}-{}.sock",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos())
    ))
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

fn spawn_daemon(path: &Path, question_ms: u64) -> Daemon {
    let child = Command::new(env!("CARGO_BIN_EXE_open-islandd"))
        .arg("--socket")
        .arg(path)
        .env("HOME", isolated_home())
        .env("XDG_CONFIG_HOME", isolated_home())
        .env("XDG_STATE_HOME", isolated_home())
        .env("OPEN_ISLAND_POLL_MS", "25")
        .env("OPEN_ISLAND_QUESTION_TIMEOUT_MS", question_ms.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn daemon");
    Daemon {
        child,
        path: path.to_path_buf(),
    }
}

fn connect(path: &Path) -> (UnixStream, BufReader<UnixStream>) {
    for _ in 0..100 {
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

fn read_event(reader: &mut BufReader<UnixStream>, event: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "timed out waiting for {event}");
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => thread::sleep(Duration::from_millis(10)),
            Ok(_) => {
                let value: Value = serde_json::from_str(line.trim()).expect("event JSON");
                if value["event"] == event {
                    return value;
                }
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                thread::sleep(Duration::from_millis(10))
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

fn spawn_hook(agent: &str, path: &Path, input: &str) -> Child {
    let mut hook = Command::new(env!("CARGO_BIN_EXE_open-islandd"))
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
    let mut stdin = hook.stdin.take().expect("hook stdin");
    stdin.write_all(input.as_bytes()).expect("write hook stdin");
    hook
}

fn hook_stdout(hook: Child) -> String {
    let output = hook.wait_with_output().expect("hook output");
    assert!(
        output.status.success(),
        "hook failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn answer(stream: &mut UnixStream, question_id: &str, answers: Value) {
    send_request(
        stream,
        1,
        "answer_question",
        json!({"question_id": question_id, "answers": answers}),
    );
}

#[test]
fn claude_question_answered_on_the_island_reaches_the_agent_as_updated_input() {
    let path = socket("claude-answer");
    let daemon = spawn_daemon(&path, 10_000);
    let (mut island, mut events) = connect(&path);

    let hook = spawn_hook("claude", &path, CLAUDE_QUESTION);
    let asked = read_event(&mut events, "question-asked");
    let question_id = asked["data"]["question_id"]
        .as_str()
        .expect("question id")
        .to_owned();
    assert_eq!(asked["data"]["agent"], "claude");
    assert_eq!(asked["data"]["answerable"], true);
    assert_eq!(asked["data"]["questions"][0]["question"], "Qual cor?");
    assert_eq!(asked["data"]["questions"][0]["options"][1]["label"], "Azul");

    answer(&mut island, &question_id, json!([["Vermelho"]]));
    let resolved = read_event(&mut events, "question-resolved");
    assert_eq!(resolved["data"]["outcome"], "answered");

    let decision: Value = serde_json::from_str(&hook_stdout(hook)).expect("hook JSON");
    let decision = &decision["hookSpecificOutput"]["decision"];
    assert_eq!(decision["behavior"], "allow");
    assert_eq!(decision["updatedInput"]["answers"]["Qual cor?"], "Vermelho");
    assert_eq!(
        decision["updatedInput"]["questions"][0]["header"], "Cor",
        "the original input has to survive beside the answers"
    );
    drop(daemon);
}

#[test]
fn a_multi_select_claude_question_is_answered_with_an_array() {
    let path = socket("claude-multi");
    let daemon = spawn_daemon(&path, 10_000);
    let (mut island, mut events) = connect(&path);

    let hook = spawn_hook("claude", &path, CLAUDE_MULTI_QUESTION);
    let asked = read_event(&mut events, "question-asked");
    assert_eq!(asked["data"]["questions"][0]["multi_select"], true);
    let question_id = asked["data"]["question_id"]
        .as_str()
        .expect("id")
        .to_owned();

    answer(&mut island, &question_id, json!([["Vermelho", "Azul"]]));

    let decision: Value = serde_json::from_str(&hook_stdout(hook)).expect("hook JSON");
    assert_eq!(
        decision["hookSpecificOutput"]["decision"]["updatedInput"]["answers"]["Quais cores?"],
        json!(["Vermelho", "Azul"])
    );
    drop(daemon);
}

#[test]
fn an_unanswered_claude_question_times_out_into_the_terminal_and_never_denies() {
    let path = socket("claude-timeout");
    let daemon = spawn_daemon(&path, 150);
    let (_island, mut events) = connect(&path);

    let hook = spawn_hook("claude", &path, CLAUDE_QUESTION);
    read_event(&mut events, "question-asked");
    let resolved = read_event(&mut events, "question-resolved");

    assert_eq!(resolved["data"]["outcome"], "cancelled");
    assert_eq!(hook_stdout(hook), CLAUDE_UNANSWERED);
    drop(daemon);
}

#[test]
fn a_claude_question_with_no_daemon_leaves_the_call_untouched() {
    let path = socket("claude-offline");
    let hook = spawn_hook("claude", &path, CLAUDE_QUESTION);
    assert_eq!(hook_stdout(hook), CLAUDE_UNANSWERED);
}

#[test]
fn opencode_question_answered_on_the_island_comes_back_as_answers_for_the_plugin() {
    let path = socket("opencode-answer");
    let daemon = spawn_daemon(&path, 10_000);
    let (mut island, mut events) = connect(&path);

    let hook = spawn_hook("opencode", &path, OPENCODE_QUESTION);
    let asked = read_event(&mut events, "question-asked");
    assert_eq!(asked["data"]["question_id"], "que_1");
    assert_eq!(asked["data"]["agent"], "opencode");

    answer(&mut island, "que_1", json!([["Azul"]]));

    assert_eq!(
        serde_json::from_str::<Value>(&hook_stdout(hook)).expect("hook JSON"),
        json!({"answers": [["Azul"]]})
    );
    drop(daemon);
}

#[test]
fn an_unanswered_opencode_question_tells_the_plugin_to_stay_out_of_it() {
    let path = socket("opencode-timeout");
    let daemon = spawn_daemon(&path, 150);
    let (_island, mut events) = connect(&path);

    let hook = spawn_hook("opencode", &path, OPENCODE_QUESTION);
    read_event(&mut events, "question-asked");

    assert_eq!(hook_stdout(hook), "{}");
    drop(daemon);
}

#[test]
fn a_codex_question_does_not_block_the_hook_and_is_cleared_by_post_tool_use() {
    let path = socket("codex-fallback");
    let daemon = spawn_daemon(&path, 10_000);
    let (_island, mut events) = connect(&path);

    let started = Instant::now();
    let hook = spawn_hook("codex", &path, CODEX_QUESTION);
    assert_eq!(hook_stdout(hook), "");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the Codex hook must return at once: its TUI owns the answer"
    );

    let asked = read_event(&mut events, "question-asked");
    assert_eq!(asked["data"]["answerable"], false);
    assert_eq!(asked["data"]["expires_in_ms"], 60_000);
    let question_id = asked["data"]["question_id"]
        .as_str()
        .expect("id")
        .to_owned();
    assert_eq!(question_id, "codex:abc123:call_1");

    let cleared = spawn_hook("codex", &path, CODEX_ANSWERED);
    assert_eq!(hook_stdout(cleared), "");
    let resolved = read_event(&mut events, "question-resolved");
    assert_eq!(resolved["data"]["question_id"], question_id);
    assert_eq!(resolved["data"]["outcome"], "answered");
    drop(daemon);
}

#[test]
fn answering_an_unknown_question_is_an_error_and_not_a_silent_success() {
    let path = socket("unknown");
    let daemon = spawn_daemon(&path, 10_000);
    let (mut island, mut reader) = connect(&path);

    answer(&mut island, "missing", json!([["Vermelho"]]));

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(Instant::now() < deadline, "no response to answer_question");
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            thread::sleep(Duration::from_millis(10));
            continue;
        }
        let value: Value = serde_json::from_str(line.trim()).expect("JSON");
        if value["id"] == json!(1) {
            assert_eq!(value["ok"], false);
            assert!(value["error"]
                .as_str()
                .expect("error")
                .contains("not found"));
            break;
        }
    }
    drop(daemon);
}
