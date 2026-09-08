use serde_json::Value;
use std::{
    env, fs,
    io::{self, BufRead, BufReader},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

mod support;

use support::isolated_home;

fn socket(label: &str) -> PathBuf {
    env::temp_dir().join(format!(
        "open-island-test-toggle-{label}-{}-{}.sock",
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

fn spawn_daemon(path: &Path) -> Daemon {
    let child = Command::new(env!("CARGO_BIN_EXE_open-islandd"))
        .arg("--socket")
        .arg(path)
        .env("HOME", isolated_home())
        .env("XDG_CONFIG_HOME", isolated_home())
        .env("XDG_STATE_HOME", isolated_home())
        .env("OPEN_ISLAND_POLL_MS", "1000")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn daemon");
    Daemon {
        child,
        path: path.to_path_buf(),
    }
}

/// Stands in for the island: the daemon only has somewhere to deliver a toggle while a
/// client holds a connection open.
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

fn wait_for_bind(path: &Path) {
    let (stream, _) = connect(path);
    drop(stream);
}

fn toggle(path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_open-islandd"))
        .arg("toggle")
        .arg("--socket")
        .arg(path)
        .output()
        .expect("run toggle")
}

fn read_event(reader: &mut BufReader<UnixStream>, event: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "timed out waiting for {event}");
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => thread::sleep(Duration::from_millis(10)),
            Ok(_) => {
                let value: Value = serde_json::from_str(line.trim()).expect("message JSON");
                if value["event"] == event {
                    return value;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("read message: {error}"),
        }
    }
}

#[test]
fn a_toggle_reaches_the_island_over_the_socket() {
    let path = socket("delivered");
    let _daemon = spawn_daemon(&path);
    let (_island, mut island_reader) = connect(&path);

    let output = toggle(&path);
    assert!(
        output.status.success(),
        "toggle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let event = read_event(&mut island_reader, "island-toggle");
    assert_eq!(event["v"], 1);
    assert_eq!(event["data"]["source"], "hotkey");
}

#[test]
fn toggle_without_a_daemon_fails_loudly() {
    let path = socket("missing");
    let output = toggle(&path);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no daemon"), "stderr was: {stderr}");
}

#[test]
fn toggle_with_no_island_listening_says_so() {
    let path = socket("nobody");
    let _daemon = spawn_daemon(&path);
    wait_for_bind(&path);

    let output = toggle(&path);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no island is connected"),
        "stderr was: {stderr}"
    );
}
