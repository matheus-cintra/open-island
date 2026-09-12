use super::autostart::island_candidates;
use crate::server::socket_path;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

/// The island holds one long-lived connection, so it never pays the accept tick; this
/// process is born and dies for a single keypress, which is why the reply matters more
/// than the exit code alone.
pub fn run_toggle() -> i32 {
    match ask_daemon("toggle", json!({"source": "hotkey"})) {
        Ok(data) if data["delivered"] == Value::Bool(true) => 0,
        Ok(_) => {
            eprintln!("open-islandd: no island is connected to the daemon");
            1
        }
        Err(error) => {
            eprintln!("open-islandd: {error}");
            1
        }
    }
}

pub fn ask_daemon(method: &str, params: Value) -> Result<Value, String> {
    let socket = socket_path();
    let mut stream = UnixStream::connect(&socket)
        .map_err(|error| format!("no daemon at {}: {error}", socket.display()))?;
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    let request = json!({"v": 1, "id": 1, "method": method, "params": params});
    writeln!(stream, "{request}")
        .and_then(|_| stream.flush())
        .map_err(|error| format!("unable to send {method}: {error}"))?;
    let reader = stream
        .try_clone()
        .map(BufReader::new)
        .map_err(|error| format!("unable to read the {method} reply: {error}"))?;
    for line in reader.lines() {
        let Ok(line) = line else { break };
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if message.get("id").and_then(Value::as_i64) != Some(1) {
            continue;
        }
        if message["ok"] != Value::Bool(true) {
            return Err(format!(
                "{method} failed: {}",
                message["error"].as_str().unwrap_or("unknown error")
            ));
        }
        return Ok(message["data"].clone());
    }
    Err("daemon closed the connection before replying".to_owned())
}

pub fn run_settings() -> i32 {
    match ask_daemon("settings", json!({"source": "cli"})) {
        Ok(data) if data["delivered"] == Value::Bool(true) => return 0,
        Ok(_) => {}
        Err(error) => eprintln!("open-islandd: {error}"),
    }
    for candidate in island_candidates() {
        if std::process::Command::new(&candidate)
            .arg("--settings")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .is_ok()
        {
            return 0;
        }
    }
    eprintln!("open-islandd: no island is running and none could be started");
    1
}

#[cfg(target_os = "macos")]
pub fn run_stop() -> i32 {
    let result = (|| -> Result<(), String> {
        // The private socket authenticates this same-user daemon; never search by name.
        match UnixStream::connect(socket_path()) {
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) =>
            {
                return Ok(())
            }
            Err(error) => return Err(error.to_string()),
            Ok(_) => (),
        }
        let reply = ask_daemon("ping", json!({}))?;
        let pid = reply["pid"]
            .as_u64()
            .filter(|pid| *pid > 1 && *pid <= i32::MAX as u64)
            .ok_or("PID inválido")? as i32;
        if unsafe { libc::kill(pid, libc::SIGTERM) } != 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}
