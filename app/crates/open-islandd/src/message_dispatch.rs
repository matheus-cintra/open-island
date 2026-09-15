use crate::{
    message_resources::{ResourceKey, Resources},
    message_runner::MessageRunner,
    notifications::lifecycle::SharedState,
};
use open_island_core::{
    discovery,
    input_bridge::{self, ExpectedIdentity, Outcome},
    jump::JumpPlanner,
    message_delivery::{DaemonEpoch, DeliveryState, MessageDelivery},
    process::{self, ProcessBirthIdentity},
    send::{self, Channel},
    session::Session,
};
use serde_json::{json, Value};
use std::{
    io::{self, Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

pub struct Target {
    pub session: Session,
    pub birth: ProcessBirthIdentity,
    pub guarded: bool,
}
pub fn deliver(
    state: &SharedState,
    target: &Target,
    message: &MessageDelivery,
    epoch: &DaemonEpoch,
    runner: &MessageRunner,
    resources: &Arc<Resources>,
) -> (DeliveryState, Option<String>) {
    let result = execute(state, target, message, epoch, runner, resources);
    match result {
        Ok(outcome) => (outcome, None),
        Err(error) => (
            if runner.effects.load(Ordering::Acquire) {
                DeliveryState::Unconfirmed
            } else {
                DeliveryState::Failed
            },
            Some(error),
        ),
    }
}
fn valid(state: &SharedState, target: &Target) -> Result<(), String> {
    if process::birth_identity(target.session.pid) != Some(target.birth)
        || !state
            .lock()
            .map_err(|_| "daemon_unavailable")?
            .store
            .delivery_target_current(&target.session)
    {
        return Err("target_changed".to_owned());
    }
    Ok(())
}
fn execute(
    state: &SharedState,
    target: &Target,
    message: &MessageDelivery,
    epoch: &DaemonEpoch,
    runner: &MessageRunner,
    resources: &Arc<Resources>,
) -> Result<DeliveryState, String> {
    runner.remaining()?;
    valid(state, target)?;
    if message.identity.as_ref().is_some_and(|expected| {
        expected != &crate::message_executor::identity(&target.session, target.birth, epoch)
    }) {
        return Err("target_changed".to_owned());
    }
    let host = discovery::terminal_for_delivery(&target.session, || runner.remaining().is_ok())
        .ok_or("target_changed")?;
    if let Some(socket) = host.env.get(input_bridge::ENV) {
        let socket = normalized(Path::new(socket));
        let _resource = resources.acquire(ResourceKey::Bridge(socket.clone()), runner)?;
        valid(state, target)?;
        return bridge(Path::new(&socket), target, &message.text, runner);
    }
    let plan =
        JumpPlanner::new(open_island_core::resolvers::default_resolvers()).plan(&host, runner);
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    let channel = send::channel_for(&host, &plan.steps, runtime.as_deref(), &Path::exists)
        .map_err(|blocked| blocked.code().to_owned())?;
    let key = match &channel {
        Channel::Tmux { socket, .. } => {
            crate::message_resources::tmux_key(socket.as_deref(), &host.env)
        }
        Channel::Zellij { session, .. } => {
            if target.guarded {
                return Err("host_unsupported".to_owned());
            }
            ResourceKey::Zellij(session.clone())
        }
        Channel::Wezterm { socket, pane_id } => {
            ResourceKey::Wezterm(normalized(Path::new(socket)), pane_id.clone())
        }
        Channel::Kitty { socket, window_id } => {
            ResourceKey::Kitty(socket.clone(), window_id.clone())
        }
    };
    let _resource = resources.acquire(key, runner)?;
    let buffer = format!(
        "open-island-{}-{}",
        epoch.0,
        message.attempt_id.ok_or("missing_attempt")?
    );
    let mut cleanup = BufferCleanup {
        channel: &channel,
        buffer: &buffer,
        runner,
        needed: false,
    };
    for step in send::plan_with_buffer(&channel, &message.text, &buffer) {
        runner.remaining()?;
        valid(state, target)?;
        let args = step.args.iter().map(String::as_str).collect::<Vec<_>>();
        let prepare = matches!(channel, Channel::Tmux { .. }) && args.contains(&"set-buffer");
        if prepare {
            cleanup.needed = true;
        }
        let output = if prepare {
            open_island_core::runner::CommandRunner::run(runner, &step.program, &args)
        } else {
            runner.run_effect(&step.program, &args)
        }?;
        if !output.status.success() {
            return Err("channel_command_failed".to_owned());
        }
    }
    Ok(DeliveryState::Delivered)
}
fn normalized(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}
struct BufferCleanup<'a> {
    channel: &'a Channel,
    buffer: &'a str,
    runner: &'a MessageRunner,
    needed: bool,
}
impl Drop for BufferCleanup<'_> {
    fn drop(&mut self) {
        if !self.needed {
            return;
        }
        if let Channel::Tmux { socket, .. } = self.channel {
            let mut args = Vec::new();
            if let Some(socket) = socket {
                args.extend(["-S", socket]);
            }
            args.extend(["delete-buffer", "-b", self.buffer]);
            let _ = self.runner.run_cleanup("tmux", &args);
        }
    }
}
struct TimedSocket<'a> {
    socket: UnixStream,
    runner: &'a MessageRunner,
    effect: bool,
}
impl<'a> TimedSocket<'a> {
    fn connect(path: &Path, runner: &'a MessageRunner, effect: bool) -> Result<Self, String> {
        let socket = open_island_core::unix_socket::connect(
            path,
            runner.remaining()?.min(Duration::from_millis(200)),
        )
        .map_err(|_| "bridge_unavailable")?;
        Ok(Self {
            socket,
            runner,
            effect,
        })
    }
    fn timeout(&self) -> io::Result<Duration> {
        self.runner
            .remaining()
            .map(|time| time.min(Duration::from_secs(3)))
            .map_err(|error| io::Error::new(io::ErrorKind::TimedOut, error))
    }
}
impl Read for TimedSocket<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.socket.set_read_timeout(Some(self.timeout()?))?;
        self.socket.read(bytes)
    }
}
impl Write for TimedSocket<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.socket.set_write_timeout(Some(self.timeout()?))?;
        let count = self.socket.write(bytes)?;
        if count > 0 && self.effect {
            self.runner.effects.store(true, Ordering::Release);
        }
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn bridge(
    path: &Path,
    target: &Target,
    text: &str,
    runner: &MessageRunner,
) -> Result<DeliveryState, String> {
    let mut probe = TimedSocket::connect(path, runner, false)?;
    input_bridge::write_frame(&mut probe, &json!({"probe":"capabilities"}))
        .map_err(|_| "bridge_probe_failed")?;
    let response =
        input_bridge::read_frame::<Value>(&mut probe).map_err(|_| "bridge_probe_failed")?;
    let supports = response["capabilities"].as_array().is_some_and(|caps| {
        caps.contains(&json!("expected_process_identity_v1")) && caps.contains(&json!("outcome_v1"))
    });
    if !supports && target.guarded {
        return Err("bridge_upgrade_required".to_owned());
    }
    let identity = if supports {
        Some(ExpectedIdentity {
            birth: target.birth,
            stdin_device: process::stdin_device(target.session.pid).ok_or("target_changed")?,
        })
    } else {
        None
    };
    if process::birth_identity(target.session.pid) != Some(target.birth) {
        return Err("target_changed".to_owned());
    }
    let mut socket = TimedSocket::connect(path, runner, true)?;
    input_bridge::write_frame(
        &mut socket,
        &input_bridge::Request {
            pid: target.session.pid,
            text: text.to_owned(),
            expected_process_identity: identity,
        },
    )
    .map_err(|_| "bridge_write_failed")?;
    let response: input_bridge::Response =
        input_bridge::read_frame(&mut socket).map_err(|_| "ack_lost")?;
    match response.outcome {
        Some(Outcome::Delivered) if response.error.is_none() => Ok(DeliveryState::Delivered),
        Some(Outcome::Rejected) => {
            runner.effects.store(false, Ordering::Release);
            Err(response
                .error_code
                .unwrap_or_else(|| "bridge_rejected".to_owned()))
        }
        Some(Outcome::Unconfirmed) => Err(response
            .error_code
            .unwrap_or_else(|| "delivery_unconfirmed".to_owned())),
        None if response.error.is_none() => Ok(DeliveryState::Delivered),
        _ => Err("delivery_unconfirmed".to_owned()),
    }
}
