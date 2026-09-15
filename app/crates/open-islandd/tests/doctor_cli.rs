use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixListener,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn start(home: &std::path::Path, args: &[&str]) -> Owned {
    use std::os::unix::fs::PermissionsExt;
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for name in ["systemctl", "launchctl", "open-islandd", "open-island"] {
        let path = bin.join(name);
        if !path.exists() {
            std::fs::write(&path, "#!/bin/sh\nprintf 'LoadState=loaded\\nActiveState=active\\n1 0 app.open-island.daemon\\n'\n").unwrap();
        }
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    Owned(
        Command::new(env!("CARGO_BIN_EXE_open-islandd"))
            .args(args)
            .env("HOME", home)
            .env("PATH", &bin)
            .env("OPEN_ISLAND_SOCKET", home.join("socket"))
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_STATE_HOME", home.join("state"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    )
}
fn finish(mut child: Owned) -> (i32, String) {
    let end = Instant::now() + Duration::from_secs(6);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < end,
            "doctor must exit without starting daemon"
        );
        thread::sleep(Duration::from_millis(5));
    };
    let mut text = String::new();
    child
        .0
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    (status.code().unwrap(), text)
}
#[test]
fn offline_doctor_and_version_do_not_start_daemon_or_create_configuration() {
    let home = tempfile::tempdir().unwrap();
    let (code, text) = finish(start(home.path(), &["doctor", "--json"]));
    assert_eq!(code, 1);
    let report: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(report["daemon"]["state"], "unavailable");
    assert!(!text.contains(home.path().to_str().unwrap()));
    assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 1);
    let (code, text) = finish(start(home.path(), &["--version"]));
    assert_eq!(code, 0);
    assert!(text.starts_with("open-islandd "));
    assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 1);
}
#[test]
fn doctor_allowlist_discards_unknown_fields_and_identifies_itself() {
    let home = tempfile::tempdir().unwrap();
    let listener = UnixListener::bind(home.path().join("socket")).unwrap();
    listener.set_nonblocking(true).unwrap();
    let child = start(home.path(), &["doctor", "--json"]);
    let end = Instant::now() + Duration::from_secs(2);
    let (mut socket, _) = loop {
        if let Ok(pair) = listener.accept() {
            break pair;
        }
        assert!(Instant::now() < end);
        thread::sleep(Duration::from_millis(5));
    };
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut request = String::new();
    BufReader::new(socket.try_clone().unwrap())
        .read_line(&mut request)
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&request).unwrap()["params"]["client_role"],
        "diagnostic"
    );
    writeln!(socket, "{}", json!({"id":1,"ok":true,"data":{
        "daemon":"open-islandd", "version":"0.6.3", "pid":42,"daemon_epoch":"0123456789abcdef0123456789abcdef",
        "capabilities":open_island_core::diagnostics::CAPABILITIES,
        "prompt":"SECRET_SENTINEL", "home":home.path(), "credentials":"SECRET_SENTINEL"
    }})).unwrap();
    let (code, text) = finish(child);
    assert_eq!(code, 0);
    assert!(!text.contains("SECRET_SENTINEL"));
    assert!(!text.contains(home.path().to_str().unwrap()));
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap()["daemon"]["version"],
        "0.6.3"
    );
}

#[test]
fn silent_daemon_is_bounded_and_does_not_trigger_a_restart() {
    let home = tempfile::tempdir().unwrap();
    let listener = UnixListener::bind(home.path().join("socket")).unwrap();
    listener.set_nonblocking(true).unwrap();
    let started = Instant::now();
    let child = start(home.path(), &["doctor", "--json"]);
    let socket = loop {
        if let Ok((socket, _)) = listener.accept() {
            break socket;
        }
        assert!(started.elapsed() < Duration::from_secs(2));
        thread::sleep(Duration::from_millis(5));
    };
    let (code, text) = finish(child);
    assert_eq!(code, 1);
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap()["daemon"]["state"],
        "invalid_response"
    );
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 2);
    drop(socket);
}

fn fake(home: &std::path::Path, name: &str, source: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(home.join("bin")).unwrap();
    let path = home.join("bin").join(name);
    std::fs::write(&path, source).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}
#[test]
fn legacy_help_is_checked_before_attempting_version_and_output_is_filtered() {
    let home = tempfile::tempdir().unwrap();
    fake(home.path(), "open-islandd", "#!/bin/sh\nprintf '%s\\n' \"$1\" >> \"$HOME/arguments\"\nprintf 'legacy help SECRET_SENTINEL\\n'\n");
    let (_, text) = finish(start(home.path(), &["doctor", "--json"]));
    assert_eq!(
        std::fs::read_to_string(home.path().join("arguments")).unwrap(),
        "--help\n"
    );
    let report: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        report["local"]["daemon_binary"]["probe"],
        "unverified_legacy"
    );
    assert!(report["local"]["daemon_binary"]["version"].is_null());
    assert!(!text.contains("SECRET_SENTINEL"));
}
#[test]
fn supported_binary_version_is_parsed_without_raw_stderr() {
    let home = tempfile::tempdir().unwrap();
    fake(home.path(), "open-islandd", "#!/bin/sh\ncase \"$1\" in --help) printf '  --version prints version\\n';; --version) printf 'open-islandd 0.6.3\\n';; *) exit 1;; esac\nprintf 'SECRET_SENTINEL' >&2\n");
    let (_, text) = finish(start(home.path(), &["doctor", "--json"]));
    let report: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(report["local"]["daemon_binary"]["version"], "0.6.3");
    assert_eq!(report["local"]["daemon_binary"]["probe"], "verified");
    assert!(!text.contains("SECRET_SENTINEL"));
}
#[test]
fn hanging_probe_and_its_stdout_child_are_killed_at_deadline() {
    let home = tempfile::tempdir().unwrap();
    fake(
        home.path(),
        "open-islandd",
        "#!/bin/sh\n/bin/sleep 60 &\nprintf '%s\\n' \"$!\" > \"$HOME/helper-pid\"\nexit 0\n",
    );
    let started = Instant::now();
    let (_, text) = finish(start(home.path(), &["doctor", "--json"]));
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap()["local"]["daemon_binary"]["probe"],
        "timeout"
    );
    let pid: u32 = std::fs::read_to_string(home.path().join("helper-pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let end = Instant::now() + Duration::from_secs(1);
    while open_island_core::process::command(pid).is_some_and(|command| !command.is_empty()) {
        assert!(Instant::now() < end, "probe child still running");
        thread::sleep(Duration::from_millis(10));
    }
}
