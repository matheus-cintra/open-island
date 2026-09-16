use open_island_core::config::FiltersConfig;
use serde_json::Value;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

const LOCK_PROBE_TTL: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Scenes {
    pub focus_mode: bool,
    pub screen_off: bool,
}

impl Scenes {
    pub fn active(self) -> bool {
        self.focus_mode || self.screen_off
    }
}

pub trait SceneSource {
    fn do_not_disturb(&mut self) -> bool;
    fn screen_off(&mut self) -> bool;
}

pub fn evaluate(filters: &FiltersConfig, source: &mut dyn SceneSource) -> Scenes {
    Scenes {
        focus_mode: filters.quiet_focus_mode && source.do_not_disturb(),
        screen_off: filters.quiet_screen_off && source.screen_off(),
    }
}

fn hypr_socket() -> Option<PathBuf> {
    let runtime = env::var_os("XDG_RUNTIME_DIR")?;
    let signature = env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let socket = PathBuf::from(runtime)
        .join("hypr")
        .join(signature)
        .join(".socket.sock");
    socket.exists().then_some(socket)
}

fn hypr_request(command: &str) -> Option<String> {
    let socket = hypr_socket()?;
    let mut stream = UnixStream::connect(&socket).ok()?;
    stream.write_all(command.as_bytes()).ok()?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply).ok()?;
    Some(reply)
}

pub fn monitors_dark(reply: Option<String>) -> bool {
    let Some(text) = reply else {
        return false;
    };
    let Ok(Value::Array(monitors)) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    let mut enabled = 0;
    let mut dark = 0;
    for monitor in &monitors {
        if monitor.get("disabled").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        enabled += 1;
        if monitor.get("dpmsStatus").and_then(Value::as_bool) == Some(false) {
            dark += 1;
        }
    }
    enabled > 0 && enabled == dark
}

fn graphical_session() -> Option<String> {
    let uid = fs::metadata("/proc/self").ok()?.uid().to_string();
    let output = Command::new("loginctl")
        .args(["show-user", &uid, "-p", "Display", "--value"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let id = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!id.is_empty()).then_some(id)
}

pub fn session_locked(reply: Option<String>) -> bool {
    reply.is_some_and(|text| {
        text.lines()
            .any(|line| line.trim() == "LockedHint=yes" || line.trim() == "IdleHint=yes")
    })
}

pub struct SystemScenes {
    dnd: Box<dyn FnMut() -> bool + Send>,
    session: Option<String>,
    session_probed: bool,
    locked_cache: Option<(std::time::Instant, bool)>,
}

impl SystemScenes {
    pub fn new(dnd: Box<dyn FnMut() -> bool + Send>) -> Self {
        Self {
            dnd,
            session: None,
            session_probed: false,
            locked_cache: None,
        }
    }

    fn locked(&mut self) -> bool {
        if !self.session_probed {
            self.session_probed = true;
            self.session = graphical_session();
        }
        let Some(id) = self.session.as_deref() else {
            return false;
        };
        if let Some((checked_at, locked)) = self.locked_cache {
            if checked_at.elapsed() < LOCK_PROBE_TTL {
                return locked;
            }
        }
        let output = Command::new("loginctl")
            .args(["show-session", id, "-p", "LockedHint", "-p", "IdleHint"])
            .output()
            .ok();
        let reply = output
            .filter(|out| out.status.success())
            .and_then(|out| String::from_utf8(out.stdout).ok());
        let locked = session_locked(reply);
        self.locked_cache = Some((std::time::Instant::now(), locked));
        locked
    }
}

impl SceneSource for SystemScenes {
    fn do_not_disturb(&mut self) -> bool {
        (self.dnd)()
    }

    fn screen_off(&mut self) -> bool {
        #[cfg(target_os = "linux")]
        {
            monitors_dark(hypr_request("j/monitors")) || self.locked()
        }
        #[cfg(target_os = "macos")]
        {
            open_island_core::process::displays_asleep().unwrap_or(false)
                || open_island_core::process::session_unavailable().unwrap_or(false)
        }
    }
}

#[cfg(test)]
#[path = "scenes_tests.rs"]
mod tests;
