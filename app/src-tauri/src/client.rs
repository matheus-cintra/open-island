use open_island_core::{
    protocol::{ApprovalDecision, Request, Response},
    session::Session,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    env,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    process::Command,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::Duration,
};
use tauri::{AppHandle, Emitter};

type Pending = Arc<Mutex<HashMap<u64, mpsc::Sender<Result<Value, String>>>>>;

pub struct DaemonClient {
    stream: Arc<Mutex<UnixStream>>,
    pending: Pending,
    next_id: Mutex<u64>,
}

fn socket_path() -> PathBuf {
    env::var_os("OPEN_ISLAND_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("XDG_RUNTIME_DIR").unwrap_or_else(|| "/tmp".into()))
                .join("open-island.sock")
        })
}

pub fn daemon_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("OPEN_ISLANDD") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(current) = env::current_exe() {
        if let Some(dir) = current.parent() {
            candidates.push(dir.join("open-islandd"));
            candidates.push(dir.join("../target/debug/open-islandd"));
        }
    }
    candidates
}

fn connect(path: &PathBuf) -> Result<UnixStream, String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut spawned = false;
    loop {
        if let Ok(stream) = UnixStream::connect(path) {
            return Ok(stream);
        }
        if !spawned {
            for candidate in daemon_candidates() {
                if Command::new(candidate)
                    .arg("--socket")
                    .arg(path)
                    .spawn()
                    .is_ok()
                {
                    break;
                }
            }
            spawned = true;
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "unable to connect to open-islandd at {}",
                path.display()
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

impl DaemonClient {
    pub fn start(app: AppHandle) -> Result<Self, String> {
        let path = socket_path();
        let stream = connect(&path)?;
        let reader_stream = stream
            .try_clone()
            .map_err(|error| format!("clone daemon socket: {error}"))?;
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let shared_stream = Arc::new(Mutex::new(stream));
        let event_pending = Arc::clone(&pending);
        let reader_stream_ref = Arc::clone(&shared_stream);
        let reader_path = path.clone();
        thread::spawn(move || {
            reader_loop(
                reader_stream,
                reader_stream_ref,
                reader_path,
                event_pending,
                app,
            )
        });
        Ok(Self {
            stream: shared_stream,
            pending,
            next_id: Mutex::new(1),
        })
    }

    fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let mut id = self
            .next_id
            .lock()
            .map_err(|_| "request id lock poisoned".to_owned())?;
        let request_id = *id;
        *id = id.saturating_add(1);
        let (sender, receiver) = mpsc::channel();
        self.pending
            .lock()
            .map_err(|_| "pending lock poisoned".to_owned())?
            .insert(request_id, sender);
        let request = Request {
            v: 1,
            id: json!(request_id),
            method: method.to_owned(),
            params: Some(params),
        };
        let write_result = {
            let mut stream = self
                .stream
                .lock()
                .map_err(|_| "socket lock poisoned".to_owned())?;
            writeln!(
                stream,
                "{}",
                serde_json::to_string(&request).map_err(|error| error.to_string())?
            )
            .and_then(|_| stream.flush())
        };
        if let Err(error) = write_result {
            let _ = self.pending.lock().map(|mut map| map.remove(&request_id));
            return Err(format!("write daemon request: {error}"));
        }
        receiver
            .recv_timeout(Duration::from_secs(5))
            .map_err(|error| format!("daemon response timeout: {error}"))?
    }

    pub fn play_sound(&self, path: &str) -> Result<(), String> {
        self.request("play_sound", json!({ "path": path }))
            .map(|_| ())
    }

    pub fn list_sessions(&self) -> Result<Vec<Session>, String> {
        let data = self.request("list_sessions", json!({}))?;
        serde_json::from_value(data)
            .map_err(|error| format!("invalid daemon session list: {error}"))
    }

    pub fn get_config(&self) -> Result<Value, String> {
        self.request("get_config", json!({}))
    }

    pub fn get_usage(&self) -> Result<Value, String> {
        self.request("get_usage", json!({}))
    }

    pub fn get_update(&self) -> Result<Value, String> {
        self.request("get_update", json!({}))
    }

    pub fn send_message(&self, id: &str, text: &str) -> Result<Value, String> {
        self.request("send_message", json!({"id": id, "text": text}))
    }

    pub fn cancel_message(&self, id: &str, message_id: u64) -> Result<(), String> {
        let _ = self.request(
            "cancel_message",
            json!({"id": id, "message_id": message_id}),
        )?;
        Ok(())
    }

    pub fn jump(&self, id: &str) -> Result<(), String> {
        let _ = self.request("jump", json!({"id": id}))?;
        Ok(())
    }

    pub fn answer_question(
        &self,
        question_id: &str,
        answers: Vec<Vec<String>>,
    ) -> Result<(), String> {
        let _ = self.request(
            "answer_question",
            json!({"question_id": question_id, "answers": answers}),
        )?;
        Ok(())
    }

    pub fn resolve_approval(
        &self,
        approval_id: &str,
        decision: ApprovalDecision,
    ) -> Result<(), String> {
        let _ = self.request(
            "resolve_approval",
            json!({"approval_id": approval_id, "decision": decision}),
        )?;
        Ok(())
    }
}

fn reader_loop(
    mut stream: UnixStream,
    shared_stream: Arc<Mutex<UnixStream>>,
    path: PathBuf,
    pending: Pending,
    app: AppHandle,
) {
    loop {
        let mut disconnected = true;
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else {
                disconnected = true;
                break;
            };
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if let Some(event) = value.get("event").and_then(Value::as_str) {
                if let Some(data) = value.get("data") {
                    let _ = app.emit(event, data);
                }
                continue;
            }
            let Ok(response) = serde_json::from_value::<Response>(value) else {
                continue;
            };
            let result = if response.ok {
                Ok(response.data.unwrap_or(Value::Null))
            } else {
                Err(response
                    .error
                    .unwrap_or_else(|| "daemon request failed".to_owned()))
            };
            if let Some(id) = response.id.as_u64() {
                if let Ok(mut waiting) = pending.lock() {
                    if let Some(sender) = waiting.remove(&id) {
                        let _ = sender.send(result);
                    }
                }
            }
        }
        if !disconnected {
            return;
        }
        let mut delay = Duration::from_millis(100);
        loop {
            match UnixStream::connect(&path) {
                Ok(new_stream) => {
                    let replacement = match new_stream.try_clone() {
                        Ok(clone) => clone,
                        Err(_) => continue,
                    };
                    if let Ok(mut current) = shared_stream.lock() {
                        *current = replacement;
                    }
                    stream = new_stream;
                    break;
                }
                Err(_) => {
                    thread::sleep(delay);
                    delay = (delay * 2).min(Duration::from_secs(2));
                }
            }
        }
    }
}
