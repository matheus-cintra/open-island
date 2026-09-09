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

#[cfg(test)]
mod tests {
    use super::{class_from, cursor_from, focused_pid_from, fullscreen_from, monitors_from};

    const REAL_MONITORS_REPLY: &str = r#"[{
    "id": 0,
    "name": "eDP-1",
    "description": "California Institute of Technology 0x142A",
    "make": "California Institute of Technology",
    "model": "0x142A",
    "serial": "",
    "width": 1920,
    "height": 1200,
    "physicalWidth": 300,
    "physicalHeight": 190,
    "refreshRate": 60.00000,
    "x": 0,
    "y": 0,
    "activeWorkspace": {
        "id": 2,
        "name": "2"
    },
    "specialWorkspace": {
        "id": 0,
        "name": ""
    },
    "reserved": [0, 32, 0, 0],
    "scale": 1,
    "transform": 0,
    "focused": true,
    "dpmsStatus": true,
    "vrr": false,
    "solitary": "0",
    "solitaryBlockedBy": ["WINDOWED","CANDIDATE"],
    "activelyTearing": false,
    "tearingBlockedBy": ["NOT_TORN","USER","CANDIDATE","HW_CURSOR"],
    "directScanoutTo": "0",
    "directScanoutBlockedBy": ["USER","CANDIDATE"],
    "disabled": false,
    "currentFormat": "XRGB8888",
    "mirrorOf": "none",
    "availableModes": ["1920x1200@60.00Hz","1920x1200@48.00Hz"],
    "colorManagementPreset": "srgb",
    "sdrBrightness": 1,
    "sdrSaturation": 1,
    "sdrMinLuminance": 0.2,
    "sdrMaxLuminance": 80,
    "hardwareCursorsInUse": true
}]"#;

    #[test]
    fn a_real_monitors_reply_becomes_monitor_info() {
        let monitors = monitors_from(REAL_MONITORS_REPLY);
        assert_eq!(monitors.len(), 1);
        let monitor = &monitors[0];
        assert_eq!(monitor.name, "eDP-1");
        assert_eq!(monitor.x, 0);
        assert_eq!(monitor.y, 0);
        assert_eq!(monitor.width, 1920);
        assert_eq!(monitor.physical_width_mm, 300);
        assert!((monitor.scale - 1.0).abs() < 0.01, "got {}", monitor.scale);
        assert_eq!(monitor.reserved_top, 32);
        assert!(monitor.focused);
    }

    #[test]
    fn a_missing_scale_defaults_to_one() {
        let monitors = monitors_from(
            r#"[{"name": "eDP-1", "x": 0, "y": 0, "width": 1920, "physicalWidth": 300}]"#,
        );
        assert_eq!(monitors.len(), 1);
        assert!(
            (monitors[0].scale - 1.0).abs() < 0.01,
            "got {}",
            monitors[0].scale
        );
    }

    #[test]
    fn a_missing_reserved_array_means_reserved_top_zero() {
        let monitors = monitors_from(r#"[{"name": "eDP-1", "x": 0, "y": 0, "width": 1920}]"#);
        assert_eq!(monitors.len(), 1);
        assert_eq!(monitors[0].reserved_top, 0);
        assert!(!monitors[0].focused);
        assert_eq!(monitors[0].physical_width_mm, 0);
    }

    #[test]
    fn an_entry_without_a_name_is_skipped() {
        let monitors = monitors_from(
            r#"[{"x": 0, "y": 0, "width": 1920}, {"name": "DP-1", "x": 0, "y": 0, "width": 2560}]"#,
        );
        assert_eq!(monitors.len(), 1);
        assert_eq!(monitors[0].name, "DP-1");
        assert_eq!(monitors[0].width, 2560);
    }

    #[test]
    fn garbage_json_means_no_monitors() {
        assert!(monitors_from("not json at all").is_empty());
        assert!(monitors_from("{}").is_empty());
        assert!(monitors_from("").is_empty());
    }

    #[test]
    fn only_a_fullscreen_client_on_the_active_workspace_counts() {
        let fullscreen_on_one = r#"[{"fullscreen": 2, "workspace": {"id": 1}}]"#;
        assert_eq!(
            fullscreen_from(r#"{"id": 1}"#, || Some(fullscreen_on_one.to_owned())),
            Some(true)
        );
        assert_eq!(
            fullscreen_from(r#"{"id": 2}"#, || Some(fullscreen_on_one.to_owned())),
            Some(false)
        );
        let windowed_on_one = r#"[{"fullscreen": 0, "workspace": {"id": 1}}]"#;
        assert_eq!(
            fullscreen_from(r#"{"id": 1}"#, || Some(windowed_on_one.to_owned())),
            Some(false)
        );
    }

    #[test]
    fn the_active_window_pid_wins_and_focus_history_is_the_fallback() {
        let clients = r#"[{"pid": 100, "focusHistoryID": 1}, {"pid": 200, "focusHistoryID": 0}]"#;
        assert_eq!(
            focused_pid_from(r#"{"pid": 42}"#, || Some(clients.to_owned())),
            Some(42)
        );
        assert_eq!(
            focused_pid_from("{}", || Some(clients.to_owned())),
            Some(200)
        );
        assert_eq!(
            focused_pid_from("{}", || Some(
                r#"[{"pid": 100, "focusHistoryID": 1}]"#.to_owned()
            )),
            None
        );
    }

    #[test]
    fn the_clients_are_not_requested_while_the_active_window_has_a_pid() {
        assert_eq!(
            focused_pid_from(r#"{"pid": 42}"#, || unreachable!()),
            Some(42)
        );
    }

    #[test]
    fn the_clients_are_not_requested_when_the_active_workspace_is_unreadable() {
        assert_eq!(fullscreen_from("{}", || unreachable!()), None);
        assert_eq!(fullscreen_from("garbage", || unreachable!()), None);
    }

    #[test]
    fn cursorpos_is_parsed_with_spaces() {
        assert_eq!(cursor_from("960, 540\n"), Some((960, 540)));
        assert_eq!(cursor_from("x"), None);
    }

    #[test]
    fn the_window_class_matches_the_pid_and_skips_empty_classes() {
        let clients = r#"[{"pid": 42, "class": ""}, {"pid": 42, "class": "kitty"}]"#;
        assert_eq!(class_from(clients, 42), Some("kitty".to_owned()));
        assert_eq!(class_from(clients, 7), None);
    }
}
