use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use super::{Compositor, MonitorInfo};

fn socket_path() -> Option<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    let signature = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let socket = PathBuf::from(runtime)
        .join("hypr")
        .join(signature)
        .join(".socket.sock");
    socket.exists().then_some(socket)
}

fn request(command: &str) -> Option<String> {
    let socket = socket_path()?;
    let mut stream = UnixStream::connect(&socket).ok()?;
    stream.write_all(command.as_bytes()).ok()?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply).ok()?;
    Some(reply)
}

// Only the workspace on screen counts: a fullscreen window parked on another one is not
// covering anything the island would be in the way of.
fn fullscreen_from(
    active_workspace: &str,
    clients: impl FnOnce() -> Option<String>,
) -> Option<bool> {
    let active = serde_json::from_str::<serde_json::Value>(active_workspace).ok()?;
    let active_id = active["id"].as_i64()?;
    let clients = serde_json::from_str::<serde_json::Value>(&clients()?).ok()?;
    Some(clients.as_array()?.iter().any(|client| {
        client["fullscreen"].as_i64().is_some_and(|mode| mode > 0)
            && client["workspace"]["id"].as_i64() == Some(active_id)
    }))
}

// With no output attached the compositor answers `activewindow` with nulls, so the
// last-focused client is the only focus datum left. `focusHistoryID` 0 is that client.
fn focused_pid_from(active_window: &str, clients: impl FnOnce() -> Option<String>) -> Option<u32> {
    let active = serde_json::from_str::<serde_json::Value>(active_window).ok()?;
    if let Some(pid) = active["pid"].as_u64() {
        return u32::try_from(pid).ok();
    }
    let clients = serde_json::from_str::<serde_json::Value>(&clients()?).ok()?;
    clients
        .as_array()?
        .iter()
        .find(|client| client["focusHistoryID"].as_i64() == Some(0))
        .and_then(|client| client["pid"].as_u64())
        .and_then(|pid| u32::try_from(pid).ok())
}

fn cursor_from(reply: &str) -> Option<(i32, i32)> {
    let (x, y) = reply.trim().split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

fn class_from(clients: &str, pid: u32) -> Option<String> {
    let clients = serde_json::from_str::<serde_json::Value>(clients).ok()?;
    clients.as_array()?.iter().find_map(|client| {
        (client.get("pid").and_then(serde_json::Value::as_u64) == Some(u64::from(pid)))
            .then(|| client.get("class").and_then(serde_json::Value::as_str))
            .flatten()
            .filter(|class| !class.is_empty())
            .map(str::to_owned)
    })
}

fn monitor_info(entry: &serde_json::Value) -> Option<MonitorInfo> {
    Some(MonitorInfo {
        name: entry.get("name")?.as_str()?.to_owned(),
        x: entry.get("x")?.as_i64()? as i32,
        y: entry.get("y")?.as_i64()? as i32,
        width: entry.get("width")?.as_u64()? as u32,
        physical_width_mm: entry
            .get("physicalWidth")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32,
        scale: entry
            .get("scale")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(1.0),
        reserved_top: entry
            .get("reserved")
            .and_then(serde_json::Value::as_array)
            .and_then(|reserved| reserved.get(1))
            .and_then(serde_json::Value::as_u64)
            .and_then(|top| u32::try_from(top).ok())
            .unwrap_or(0),
        focused: entry
            .get("focused")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    })
}

fn monitors_from(json: &str) -> Vec<MonitorInfo> {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .map(|list| list.iter().filter_map(monitor_info).collect())
        .unwrap_or_default()
}

pub struct HyprlandBackend;

impl Compositor for HyprlandBackend {
    fn available(&self) -> bool {
        socket_path().is_some()
    }

    fn monitors(&self) -> Vec<MonitorInfo> {
        request("j/monitors")
            .map(|reply| monitors_from(&reply))
            .unwrap_or_default()
    }

    fn cursor_position(&self) -> Option<(i32, i32)> {
        let socket = socket_path()?;
        let mut stream = UnixStream::connect(&socket).ok()?;
        stream.write_all(b"cursorpos").ok()?;
        let mut reply = String::new();
        stream.read_to_string(&mut reply).ok()?;
        cursor_from(&reply)
    }

    fn any_fullscreen(&self) -> Option<bool> {
        fullscreen_from(&request("j/activeworkspace")?, || request("j/clients"))
    }

    fn focused_pid(&self) -> Option<u32> {
        focused_pid_from(&request("j/activewindow")?, || request("j/clients"))
    }

    fn window_class(&self, pid: u32) -> Option<String> {
        class_from(&request("j/clients")?, pid)
    }
}
