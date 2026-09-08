use std::{
    env, fs,
    io::Read,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

mod support;

use support::isolated_home;

const EXIT_DEADLINE: Duration = Duration::from_secs(10);

fn socket(label: &str) -> PathBuf {
    env::temp_dir().join(format!(
        "open-island-test-help-{label}-{}-{}.sock",
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

fn spawn_cli(args: &[&str], socket: &PathBuf) -> Child {
    Command::new(env!("CARGO_BIN_EXE_open-islandd"))
        .args(args)
        .env("HOME", isolated_home())
        .env("XDG_CONFIG_HOME", isolated_home())
        .env("XDG_STATE_HOME", isolated_home())
        .env("XDG_RUNTIME_DIR", env::temp_dir())
        .env("OPEN_ISLAND_SOCKET", socket)
        .env("OPEN_ISLAND_POLL_MS", "1000")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn open-islandd")
}

fn assert_is_help(args: &[&str], label: &str) {
    let path = socket(label);
    let mut daemon = Daemon {
        child: spawn_cli(args, &path),
        path: path.clone(),
    };

    let deadline = Instant::now() + EXIT_DEADLINE;
    let status = loop {
        match daemon.child.try_wait().expect("poll open-islandd") {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                panic!("{args:?} never exited, so it did not print help")
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    };
    assert!(status.success(), "{args:?} exited with {:?}", status.code());

    let mut stdout = String::new();
    daemon
        .child
        .stdout
        .as_mut()
        .expect("stdout pipe")
        .read_to_string(&mut stdout)
        .expect("read stdout");
    assert!(!stdout.trim().is_empty(), "{args:?} printed nothing");
    for expected in [
        "Uso: open-islandd",
        "hook",
        "hooks",
        "hotkey",
        "toggle",
        "settings",
        "autostart",
        "--socket",
    ] {
        assert!(
            stdout.contains(expected),
            "{args:?} help is missing {expected}: {stdout}"
        );
    }
}

#[test]
fn long_flag_prints_help_and_exits_zero() {
    assert_is_help(&["--help"], "long");
}

#[test]
fn short_flag_prints_help_and_exits_zero() {
    assert_is_help(&["-h"], "short");
}

#[test]
fn bare_subcommand_prints_help_and_exits_zero() {
    assert_is_help(&["help"], "bare");
}

#[test]
fn socket_flag_still_starts_the_daemon() {
    let path = socket("daemon");
    let mut daemon = Daemon {
        child: spawn_cli(&["--socket", path.to_str().expect("socket path")], &path),
        path: path.clone(),
    };

    let deadline = Instant::now() + EXIT_DEADLINE;
    while Instant::now() < deadline && !path.exists() {
        assert!(
            daemon.child.try_wait().expect("poll daemon").is_none(),
            "--socket exited instead of binding {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(10));
    }

    assert!(path.exists(), "--socket never bound {}", path.display());
    assert!(
        daemon.child.try_wait().expect("poll daemon").is_none(),
        "--socket exited after binding {}",
        path.display()
    );
}
