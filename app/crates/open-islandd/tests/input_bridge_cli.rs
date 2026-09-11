use open_island_core::input_bridge;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

struct Session {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    _directory: tempfile::TempDir,
    info: PathBuf,
    received: PathBuf,
}
impl Drop for Session {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_some() {
            return;
        }
        let foreground = self.master.process_group_leader();
        let _ = self.child.kill();
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            self.drain_output();
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        if let Some(group) = foreground.filter(|group| *group > 1) {
            unsafe {
                libc::kill(-group, libc::SIGKILL);
            }
        }
        if let Some(pid) = self.child.process_id() {
            unsafe {
                libc::kill(pid as i32, libc::SIGKILL);
            }
        }
        self.drain_output();
        let _ = self.child.wait();
    }
}
impl Session {
    fn finish(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut output = String::new();
        loop {
            // Darwin can wait for even a small pending TTY output buffer before
            // completing process exit. A real terminal continuously drains it.
            output.push_str(&self.drain_output());
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success(), "session exit: {status:?}");
                return;
            }
            assert!(
                Instant::now() < deadline,
                "session did not exit, terminal output: {output} {}",
                self.output()
            );
            thread::sleep(Duration::from_millis(20));
        }
    }
    fn drain_output(&self) -> String {
        let fd = self.master.as_raw_fd().unwrap();
        let mut bytes = [0u8; 8192];
        unsafe {
            libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK);
        }
        let mut output = String::new();
        while output.len() < 65536 {
            let count = unsafe { libc::read(fd, bytes.as_mut_ptr().cast(), bytes.len()) };
            if count <= 0 {
                break;
            }
            output.push_str(&String::from_utf8_lossy(&bytes[..count as usize]));
        }
        output
    }
    fn output(&self) -> String {
        let mut output = self.drain_output();
        let root = self.child.process_id().unwrap_or(0);
        let mut ids = vec![root];
        for pid in open_island_core::process::pids() {
            let mut current = pid;
            for _ in 0..32 {
                let Some((parent, _)) = open_island_core::process::parent_and_comm(current) else {
                    break;
                };
                if parent == root {
                    ids.push(pid);
                    break;
                }
                if parent <= 1 || parent == current {
                    break;
                }
                current = parent;
            }
        }
        let ids = ids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
        if let Ok(state) = std::process::Command::new("/bin/ps")
            .args(["-o", "pid,ppid,pgid,tpgid,stat,comm", "-p", &ids])
            .stdin(std::process::Stdio::null())
            .output()
        {
            output.push_str(&String::from_utf8_lossy(&state.stdout));
            output.push_str(&String::from_utf8_lossy(&state.stderr));
        }
        output
    }
}
fn wait_file(path: &Path) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(value) = fs::read_to_string(path) {
            if !value.is_empty() {
                return value;
            }
        }
        assert!(
            Instant::now() < deadline,
            "timeout waiting for {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(20));
    }
}
fn session() -> Session {
    session_with_shell(None)
}
fn session_with_shell(shell: Option<&str>) -> Session {
    session_fixture(shell, false)
}
fn session_fixture(shell: Option<&str>, background: bool) -> Session {
    let directory = tempfile::tempdir().unwrap();
    let info = directory.path().join("info");
    let received = directory.path().join("received");
    let agent = directory.path().join("claude");
    std::os::unix::fs::symlink(
        open_island_core::paths::executable("python3").unwrap(),
        &agent,
    )
    .unwrap();
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 30,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut command = if let Some(shell) = shell {
        open_islandd::input_install::configure(
            directory.path(),
            Path::new(env!("CARGO_BIN_EXE_open-islandd")),
            true,
            false,
        )
        .unwrap();
        let mut command = CommandBuilder::new(shell);
        command.args(["-fic", ". \"$1\"; shift; claude \"$@\"", "test"]);
        command.arg(directory.path().join(".config/open-island/input.sh"));
        let mut paths = vec![directory.path().to_path_buf()];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        command.env("PATH", std::env::join_paths(paths).unwrap());
        command.env("HOME", directory.path());
        command
    } else {
        let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_open-islandd"));
        command.args(["run", "--"]);
        command.arg(&agent);
        command
    };
    command.cwd(directory.path());
    if background {
        command.env("OPEN_ISLAND_TEST_BACKGROUND", "1");
    }
    command.args([
        "-c",
        r#"
import os, sys, tty, json, time
tty.setraw(0)
os.write(1, b'\x1b[?2004h')
background = 0
if os.environ.get('OPEN_ISLAND_TEST_BACKGROUND') == '1':
    ready_r, ready_w = os.pipe()
    background = os.fork()
    if background == 0:
        os.close(ready_r)
        fd = os.open('/dev/null', os.O_RDONLY)
        os.dup2(fd, 0)
        os.close(fd)
        os.write(ready_w, b'1')
        os.close(ready_w)
        time.sleep(60)
        os._exit(0)
    os.close(ready_w)
    os.read(ready_r, 1)
    os.close(ready_r)
with open(sys.argv[1], 'w') as f:
    json.dump({'pid': os.getpid(), 'socket': os.environ['OPEN_ISLAND_INPUT_SOCKET'], 'cwd': os.getcwd(), 'background': background}, f)
data = b''
while not data.endswith(b'\r'):
    data += os.read(0, 8192)
with open(sys.argv[2], 'w') as f:
    json.dump({'text': data.decode(), 'size': list(os.get_terminal_size(0))}, f)
if background:
    os.kill(background, 9)
    os.waitpid(background, 0)
time.sleep(0.2)
"#,
    ]);
    command.arg(&info);
    command.arg(&received);
    let child = pair.slave.spawn_command(command).unwrap();
    drop(pair.slave);
    Session {
        child,
        master: pair.master,
        _directory: directory,
        info,
        received,
    }
}
fn address(session: &Session) -> (PathBuf, u32) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !session.info.exists() {
        if Instant::now() >= deadline {
            let fd = session.master.as_raw_fd().unwrap();
            let mut bytes = [0u8; 8192];
            unsafe {
                libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK);
            }
            let n = unsafe { libc::read(fd, bytes.as_mut_ptr().cast(), bytes.len()) };
            panic!(
                "bridge did not start: {}",
                String::from_utf8_lossy(&bytes[..n.max(0) as usize])
            );
        }
        thread::sleep(Duration::from_millis(20));
    }
    let info: serde_json::Value = serde_json::from_str(&wait_file(&session.info)).unwrap();
    assert_eq!(
        PathBuf::from(info["cwd"].as_str().unwrap()),
        fs::canonicalize(session._directory.path()).unwrap()
    );
    (
        PathBuf::from(info["socket"].as_str().unwrap()),
        info["pid"].as_u64().unwrap() as u32,
    )
}

#[test]
fn two_real_ptys_receive_only_their_own_unicode_messages_and_reject_cross_session_input() {
    let mut first = session();
    let mut second = session();
    let (socket_a, pid_a) = address(&first);
    let (socket_b, pid_b) = address(&second);
    assert!(input_bridge::send(&socket_a, pid_b, "wrong session")
        .unwrap_err()
        .contains("não pertence"));
    assert!(input_bridge::send(&socket_a, pid_a, "\x1b[201~").is_err());
    assert!(!first.received.exists());
    assert!(!second.received.exists());
    input_bridge::send(&socket_a, pid_a, "olá\nsegunda linha\t'$(literal)'").unwrap();
    input_bridge::send(&socket_b, pid_b, "outro agente").unwrap();
    let result_a: serde_json::Value = serde_json::from_str(&wait_file(&first.received)).unwrap();
    let result_b: serde_json::Value = serde_json::from_str(&wait_file(&second.received)).unwrap();
    assert_eq!(
        result_a["text"],
        "\x1b[200~olá\nsegunda linha\t'$(literal)'\x1b[201~\r"
    );
    assert_eq!(result_b["text"], "\x1b[200~outro agente\x1b[201~\r");
    first.finish();
    second.finish();
    assert!(!socket_a.exists());
    assert!(!socket_b.exists());
    assert!(input_bridge::send(&socket_a, pid_a, "closed").is_err());
}

#[test]
fn keyboard_and_resize_survive_the_bridge() {
    use std::io::Write;
    let mut session = session();
    let _ = address(&session);
    session
        .master
        .resize(PtySize {
            rows: 10,
            cols: 40,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    thread::sleep(Duration::from_millis(150));
    let mut keyboard = session.master.take_writer().unwrap();
    keyboard.write_all(b"keyboard\r").unwrap();
    let result: serde_json::Value = serde_json::from_str(&wait_file(&session.received)).unwrap();
    assert_eq!(result["text"], "keyboard\r");
    assert_eq!(result["size"], serde_json::json!([40, 10]));
    session.finish();
}

#[test]
fn interactive_shell_integration_wraps_commands_and_accepts_messages() {
    for shell in ["bash", "zsh"] {
        eprintln!("input shell fixture: {shell}");
        let Some(executable) = open_island_core::paths::executable(shell) else {
            continue;
        };
        let mut session = session_with_shell(Some(executable.to_str().unwrap()));
        let (socket, pid) = address(&session);
        eprintln!("input shell fixture ready: {shell}");
        input_bridge::send(&socket, pid, "pelo comando do shell").unwrap();
        let received: serde_json::Value =
            serde_json::from_str(&wait_file(&session.received)).unwrap();
        assert_eq!(
            received["text"],
            "\x1b[200~pelo comando do shell\x1b[201~\r"
        );
        eprintln!("input shell fixture delivered: {shell}");
        session.finish();
    }
}

#[test]
fn hanging_up_the_bridge_reaps_the_agent_and_removes_its_socket() {
    let mut session = session();
    let (socket, pid) = address(&session);
    session.child.kill().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        session.drain_output();
        if session.child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(Instant::now() < deadline, "bridge did not stop");
        thread::sleep(Duration::from_millis(20));
    }
    assert!(!open_island_core::process::exists(pid));
    assert!(!socket.exists());
}

#[test]
fn a_background_helper_in_the_same_process_group_cannot_receive_the_parent_input() {
    let mut session = session_fixture(None, true);
    let (socket, pid) = address(&session);
    let info: serde_json::Value = serde_json::from_str(&wait_file(&session.info)).unwrap();
    let background = info["background"].as_u64().unwrap() as u32;
    assert_eq!(unsafe { libc::getpgid(pid as i32) }, unsafe {
        libc::getpgid(background as i32)
    });
    assert!(input_bridge::send(&socket, background, "não deve chegar ao pai").is_err());
    assert!(!session.received.exists());
    input_bridge::send(&socket, pid, "mensagem ao agente correto").unwrap();
    session.finish();
    assert!(!open_island_core::process::exists(background));
}

#[test]
fn a_connection_can_arrive_before_its_message_without_losing_the_request() {
    use std::os::unix::net::UnixStream;
    let mut session = session();
    let (socket, pid) = address(&session);
    let mut connection = UnixStream::connect(socket).unwrap();
    connection
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    thread::sleep(Duration::from_millis(150));
    input_bridge::write_frame(
        &mut connection,
        &input_bridge::Request {
            pid,
            text: "chegou depois da conexão".into(),
        },
    )
    .unwrap();
    let response: input_bridge::Response = input_bridge::read_frame(connection).unwrap();
    assert!(response.error.is_none(), "{response:?}");
    session.finish();
}

#[test]
fn daemon_discovers_the_bridge_and_delivers_through_the_existing_message_protocol() {
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixStream,
        process::{Command, Stdio},
    };
    let mut agent = session();
    let (_, pid) = address(&agent);
    let home = tempfile::tempdir().unwrap();
    let socket = home.path().join("daemon.sock");
    let config = home.path().join("config.json");
    fs::write(&config, r#"{"updates":{"check_enabled":false},"integrations":{"auto_configure":false},"sound":{"enabled":false}}"#).unwrap();
    struct Daemon(std::process::Child);
    impl Drop for Daemon {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let _daemon = Daemon(
        Command::new(env!("CARGO_BIN_EXE_open-islandd"))
            .args(["--socket", socket.to_str().unwrap()])
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", home.path())
            .env("XDG_STATE_HOME", home.path())
            .env("OPEN_ISLAND_CONFIG", config)
            .env("OPEN_ISLAND_IDLE_MS", "1")
            .env("OPEN_ISLAND_POLL_MS", "25")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut stream = loop {
        if let Ok(stream) = UnixStream::connect(&socket) {
            break stream;
        }
        assert!(Instant::now() < deadline, "daemon did not bind");
        thread::sleep(Duration::from_millis(20));
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request = |method: &str, params: serde_json::Value| -> serde_json::Value {
        writeln!(
            stream,
            "{}",
            serde_json::json!({"v":1,"id":1,"method":method,"params":params})
        )
        .unwrap();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let response: serde_json::Value = serde_json::from_str(&line).unwrap();
            if response["id"] == 1 {
                assert_eq!(response["ok"], true, "{response}");
                return response["data"].clone();
            }
        }
    };
    request(
        "hook_event",
        serde_json::json!({"agent":"claude","session_id":"claude:bridge-test","event":"stop","pid":pid,"cwd":std::env::current_dir().unwrap()}),
    );
    let found = loop {
        let sessions = request("list_sessions", serde_json::json!({}));
        if let Some(found) = sessions
            .as_array()
            .unwrap()
            .iter()
            .find(|session| session["pid"] == pid && session["attention"] == "idle")
        {
            break found.clone();
        }
        assert!(
            Instant::now() < deadline,
            "session not discovered: {sessions}"
        );
        thread::sleep(Duration::from_millis(25));
    };
    assert_eq!(found["send_channel"], "island");
    let reply = request(
        "send_message",
        serde_json::json!({"id": found["id"], "text":"mensagem da ilha"}),
    );
    assert_eq!(reply["delivered"], true);
    let received: serde_json::Value = serde_json::from_str(&wait_file(&agent.received)).unwrap();
    assert_eq!(received["text"], "\x1b[200~mensagem da ilha\x1b[201~\r");
    agent.finish();
}
