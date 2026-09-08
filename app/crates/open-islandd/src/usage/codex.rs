use open_island_core::usage::{codex, redact, UsageError, UsageProvider, UsageSnapshot};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(25);
const CLIENT_NAME: &str = "open-island";

pub struct CodexUsage {
    home: PathBuf,
    binary: String,
}

impl CodexUsage {
    pub fn new(home: PathBuf) -> Self {
        Self {
            home,
            binary: "codex".to_owned(),
        }
    }

    pub fn with_binary(home: PathBuf, binary: String) -> Self {
        Self { home, binary }
    }
}

struct AppServer {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
}

impl AppServer {
    fn start(binary: &str) -> Result<Self, UsageError> {
        let mut child = Command::new(binary)
            .arg("app-server")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| UsageError::Transport(format!("start {binary}: {error}")))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| UsageError::Transport("app-server has no stdin".to_owned()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| UsageError::Transport("app-server has no stdout".to_owned()))?;
        let (sender, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    return;
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            lines,
        })
    }

    fn send(&mut self, message: &Value) -> Result<(), UsageError> {
        writeln!(self.stdin, "{message}")
            .and_then(|()| self.stdin.flush())
            .map_err(|error| UsageError::Transport(format!("write to app-server: {error}")))
    }

    fn wait_for(&self, id: u64, budget: Duration) -> Result<Value, UsageError> {
        let deadline = Instant::now() + budget;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(UsageError::Transport(format!(
                    "app-server did not answer request {id} in time"
                )));
            }
            let line = match self.lines.recv_timeout(remaining) {
                Ok(line) => line,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(UsageError::Transport("app-server closed stdout".to_owned()))
                }
            };
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if message.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                return Err(UsageError::Transport(redact(&error.to_string())));
            }
            return message.get("result").cloned().ok_or_else(|| {
                UsageError::Shape("app-server answered without a result".to_owned())
            });
        }
    }
}

impl Drop for AppServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl UsageProvider for CodexUsage {
    fn name(&self) -> &'static str {
        open_island_core::usage::PROVIDER_CODEX
    }

    fn discover(&self) -> bool {
        self.home.join(".codex/auth.json").is_file()
    }

    fn fetch(&self) -> Result<Value, UsageError> {
        let mut server = AppServer::start(&self.binary)?;
        server.send(&json!({
            "id": 1,
            "method": "initialize",
            "params": {
                "clientInfo": { "name": CLIENT_NAME, "version": env!("CARGO_PKG_VERSION") }
            }
        }))?;
        server.wait_for(1, HANDSHAKE_TIMEOUT)?;
        server.send(&json!({ "method": "initialized", "params": null }))?;
        server.send(&json!({ "id": 2, "method": "account/rateLimits/read" }))?;
        server.wait_for(2, READ_TIMEOUT)
    }

    fn identity(&self, raw: &Value) -> Option<String> {
        codex::identity(raw)
    }

    fn normalize(&self, raw: &Value, now_ms: u64) -> Result<UsageSnapshot, UsageError> {
        codex::normalize(raw, self.identity(raw), now_ms)
    }
}
