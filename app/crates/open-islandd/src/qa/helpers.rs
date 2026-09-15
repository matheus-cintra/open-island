use open_island_core::{
    input_bridge::{self, ExpectedIdentity, Outcome, Request, Response},
    process::{register_process_for_qa, system_process_source_for_qa},
    protocol::{HookEvent, HookEventKind},
    store::SessionStore,
};
use serde_json::json;
use std::{
    env,
    fs::OpenOptions,
    io,
    os::unix::{
        fs::{OpenOptionsExt, PermissionsExt},
        net::UnixListener,
        process::CommandExt,
    },
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub struct Helper {
    child: Child,
    socket: PathBuf,
}
impl Drop for Helper {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            unsafe { libc::kill(-(self.child.id() as i32), libc::SIGKILL) };
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket);
    }
}

fn index(value: &str) -> io::Result<usize> {
    value
        .parse::<usize>()
        .ok()
        .filter(|n| *n < 50)
        .ok_or_else(|| io::Error::other("invalid QA helper index"))
}

fn path(number: usize, suffix: &str) -> io::Result<PathBuf> {
    let home = env::var_os("HOME").ok_or_else(|| io::Error::other("private HOME required"))?;
    Ok(PathBuf::from(home).join(format!("qa-session-{number}.{suffix}")))
}

pub fn spawn(store: &mut SessionStore) -> io::Result<Vec<Helper>> {
    let count = match env::var("OPEN_ISLAND_QA_SESSIONS") {
        Err(env::VarError::NotPresent) => 0,
        Ok(value) => value
            .parse::<usize>()
            .ok()
            .filter(|n| *n <= 50)
            .ok_or_else(|| io::Error::other("QA session count must be between 0 and 50"))?,
        Err(_) => return Err(io::Error::other("invalid QA session count")),
    };
    let mut helpers = Vec::with_capacity(count);
    for number in 0..count {
        let socket = path(number, "sock")?;
        let helper = Helper {
            child: Command::new(env::current_exe()?)
                .args(["--qa-session-helper", &number.to_string()])
                .env(input_bridge::ENV, &socket)
                .process_group(0)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
            socket: socket.clone(),
        };
        let pid = helper.child.id();
        helpers.push(helper);
        let deadline = Instant::now() + Duration::from_secs(3);
        while !socket.exists() {
            if helpers.last_mut().unwrap().child.try_wait()?.is_some() || Instant::now() >= deadline
            {
                return Err(io::Error::other("QA helper did not become ready"));
            }
            thread::sleep(Duration::from_millis(10));
        }
        let birth = system_process_source_for_qa()
            .birth_identity(pid)
            .ok_or_else(|| io::Error::other("QA helper birth unavailable"))?;
        register_process_for_qa(pid, birth).map_err(io::Error::other)?;
        let mut event = HookEvent::new(
            "claude",
            &format!("qa-session-{number}"),
            HookEventKind::SessionStart,
        );
        event.pid = Some(pid);
        event.cwd = Some(env::var("HOME").map_err(io::Error::other)?);
        store.apply_hook_event(event.clone());
        event.event = HookEventKind::UserPromptSubmit;
        store.apply_hook_event(event);
    }
    Ok(helpers)
}

pub fn serve(value: &str) -> io::Result<()> {
    super::validate_environment(&open_island_core::paths::socket())?;
    let outcome = env::var("OPEN_ISLAND_QA_DELIVERY").unwrap_or_else(|_| "delivered".into());
    if !matches!(outcome.as_str(), "delivered" | "rejected" | "lost_ack") {
        return Err(io::Error::other("invalid QA delivery outcome"));
    }
    let number = index(value)?;
    let socket = path(number, "sock")?;
    if env::var_os(input_bridge::ENV).map(PathBuf::from).as_ref() != Some(&socket) {
        return Err(io::Error::other("private QA input socket required"));
    }
    let source = system_process_source_for_qa();
    let pid = std::process::id();
    let expected = ExpectedIdentity {
        birth: source
            .birth_identity(pid)
            .ok_or_else(|| io::Error::other("own birth unavailable"))?,
        stdin_device: source.stdin_device(pid).unwrap_or(0),
    };
    let listener = UnixListener::bind(&socket)?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    for stream in listener.incoming() {
        let mut stream = stream?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        let Ok(value) = input_bridge::read_frame::<serde_json::Value>(&stream) else {
            continue;
        };
        if value == json!({"probe":"capabilities"}) {
            let _ = input_bridge::write_frame(
                &mut stream,
                &json!({"capabilities":["outcome_v1","expected_process_identity_v1"]}),
            );
            continue;
        }
        let Ok(request) = serde_json::from_value::<Request>(value) else {
            continue;
        };
        let response = if request.pid == pid
            && request.expected_process_identity == Some(expected)
            && input_bridge::validate_text(&request.text).is_ok()
        {
            if outcome == "rejected" {
                let _ = input_bridge::write_frame(
                    &mut stream,
                    &Response::failure(
                        Outcome::Rejected,
                        "qa_rejected",
                        "QA refusal before delivery".into(),
                    ),
                );
                continue;
            }
            let mut record = OpenOptions::new()
                .create(true)
                .append(true)
                .mode(0o600)
                .open(path(number, "jsonl")?)?;
            input_bridge::write_frame(&mut record, &request)?;
            record.sync_data()?;
            if outcome == "lost_ack" {
                // The owned fixture received the text but drops its acknowledgement.
                // Exercise the real executor's uncertain outcome; never fabricate success.
                continue;
            }
            Response::success()
        } else {
            Response::failure(
                Outcome::Rejected,
                "stale_session",
                "QA target rejected".into(),
            )
        };
        let _ = input_bridge::write_frame(&mut stream, &response);
    }
    Ok(())
}
