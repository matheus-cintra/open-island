use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

pub fn socket_path() -> Option<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    let signature = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let socket = PathBuf::from(runtime)
        .join("hypr")
        .join(signature)
        .join(".socket.sock");
    socket.exists().then_some(socket)
}

pub fn request(command: &str) -> Option<String> {
    let socket = socket_path()?;
    let mut stream = UnixStream::connect(&socket).ok()?;
    stream.write_all(command.as_bytes()).ok()?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply).ok()?;
    Some(reply)
}

pub fn monitors() -> Option<serde_json::Value> {
    serde_json::from_str(&request("j/monitors")?).ok()
}

// Only the workspace on screen counts: a fullscreen window parked on another one is not
// covering anything the island would be in the way of.
pub fn any_fullscreen() -> Option<bool> {
    let active = serde_json::from_str::<serde_json::Value>(&request("j/activeworkspace")?).ok()?;
    let active_id = active["id"].as_i64()?;
    let clients = serde_json::from_str::<serde_json::Value>(&request("j/clients")?).ok()?;
    Some(clients.as_array()?.iter().any(|client| {
        client["fullscreen"].as_i64().is_some_and(|mode| mode > 0)
            && client["workspace"]["id"].as_i64() == Some(active_id)
    }))
}

// With no output attached the compositor answers `activewindow` with nulls, so the
// last-focused client is the only focus datum left. `focusHistoryID` 0 is that client.
pub fn focused_pid() -> Option<u32> {
    let active = serde_json::from_str::<serde_json::Value>(&request("j/activewindow")?).ok()?;
    if let Some(pid) = active["pid"].as_u64() {
        return u32::try_from(pid).ok();
    }
    let clients = serde_json::from_str::<serde_json::Value>(&request("j/clients")?).ok()?;
    clients
        .as_array()?
        .iter()
        .find(|client| client["focusHistoryID"].as_i64() == Some(0))
        .and_then(|client| client["pid"].as_u64())
        .and_then(|pid| u32::try_from(pid).ok())
}

pub const UI_SCALE_MIN: f64 = 1.0;
pub const UI_SCALE_MAX: f64 = 2.0;
const DPI_BASE: f64 = 96.0;
const DPI_SANE_MIN: f64 = 60.0;
const DPI_SANE_MAX: f64 = 250.0;
const MM_PER_INCH: f64 = 25.4;

pub fn ui_scale_from(width_px: u32, physical_width_mm: u32, compositor_scale: f64) -> f64 {
    if width_px == 0 || physical_width_mm == 0 || compositor_scale <= 0.0 {
        return UI_SCALE_MIN;
    }
    let dpi = f64::from(width_px) / (f64::from(physical_width_mm) / MM_PER_INCH);
    if !(DPI_SANE_MIN..=DPI_SANE_MAX).contains(&dpi) {
        return UI_SCALE_MIN;
    }
    let scale = dpi / DPI_BASE / compositor_scale;
    (scale.clamp(UI_SCALE_MIN, UI_SCALE_MAX) * 100.0).round() / 100.0
}

const COMPACT_HEIGHT_MIN: u64 = 16;
const COMPACT_HEIGHT_MAX: u64 = 200;
const COMPACT_OVERHANG: u32 = 4;

pub fn compact_height(name: Option<&str>) -> Option<u32> {
    reserved_top(name).map(|top| top + COMPACT_OVERHANG)
}

pub fn monitor_named(name: Option<&str>) -> Option<serde_json::Value> {
    let monitors = monitors()?;
    let list = monitors.as_array()?;
    if let Some(wanted) = name.filter(|value| !value.is_empty()) {
        if let Some(found) = list
            .iter()
            .find(|entry| entry.get("name").and_then(serde_json::Value::as_str) == Some(wanted))
        {
            return Some(found.clone());
        }
    }
    list.iter()
        .find(|entry| entry.get("focused").and_then(serde_json::Value::as_bool) == Some(true))
        .or_else(|| list.first())
        .cloned()
}

pub fn monitor_names() -> Vec<String> {
    monitors()
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|entry| entry.get("name").and_then(serde_json::Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub fn reserved_top(name: Option<&str>) -> Option<u32> {
    let monitor = monitor_named(name)?;
    let reserved = monitor.get("reserved")?.as_array()?;
    let top = reserved.get(1)?.as_u64()?;
    (COMPACT_HEIGHT_MIN..=COMPACT_HEIGHT_MAX)
        .contains(&top)
        .then_some(top as u32)
}

pub fn ui_scale(name: Option<&str>) -> f64 {
    let Some(monitor) = monitor_named(name) else {
        return UI_SCALE_MIN;
    };
    let width = monitor.get("width").and_then(serde_json::Value::as_u64);
    let physical = monitor
        .get("physicalWidth")
        .and_then(serde_json::Value::as_u64);
    let compositor = monitor
        .get("scale")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(1.0);
    match (width, physical) {
        (Some(width), Some(physical)) => ui_scale_from(width as u32, physical as u32, compositor),
        _ => UI_SCALE_MIN,
    }
}

pub fn cursor_position(socket: &Path) -> Option<(i32, i32)> {
    let mut stream = UnixStream::connect(socket).ok()?;
    stream.write_all(b"cursorpos").ok()?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply).ok()?;
    let (x, y) = reply.trim().split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::{ui_scale_from, UI_SCALE_MAX, UI_SCALE_MIN};

    #[test]
    fn ui_scale_tracks_dpi_not_resolution() {
        let cases = [
            ("1080p 24in", 1920u32, 531u32, 1.0, 1.00),
            ("1440p 27in", 2560, 597, 1.0, 1.13),
            ("4k 32in", 3840, 700, 1.0, 1.45),
            ("4k 27in", 3840, 597, 1.0, 1.70),
            ("4k 55in tv", 3840, 1210, 1.0, 1.00),
        ];
        for (label, width, physical, compositor, expected) in cases {
            let scale = ui_scale_from(width, physical, compositor);
            assert!(
                (scale - expected).abs() < 0.01,
                "{label}: expected {expected}, got {scale}"
            );
        }
    }

    #[test]
    fn a_4k_tv_and_a_4k_monitor_disagree() {
        let tv = ui_scale_from(3840, 1210, 1.0);
        let monitor = ui_scale_from(3840, 700, 1.0);
        assert!(monitor > tv, "same resolution must not mean the same scale");
    }

    #[test]
    fn a_missing_edid_size_falls_back() {
        assert_eq!(ui_scale_from(3840, 0, 1.0), UI_SCALE_MIN);
        assert_eq!(ui_scale_from(0, 700, 1.0), UI_SCALE_MIN);
        assert_eq!(ui_scale_from(3840, 700, 0.0), UI_SCALE_MIN);
    }

    #[test]
    fn an_absurd_dpi_falls_back() {
        assert_eq!(ui_scale_from(3840, 4000, 1.0), UI_SCALE_MIN);
        assert_eq!(ui_scale_from(3840, 100, 1.0), UI_SCALE_MIN);
    }

    #[test]
    fn the_compositor_scale_is_divided_out() {
        assert_eq!(ui_scale_from(3840, 700, 1.5), UI_SCALE_MIN);
        assert!(ui_scale_from(3840, 700, 1.0) > ui_scale_from(3840, 700, 1.25));
    }

    #[test]
    fn the_scale_is_bounded() {
        let dense = ui_scale_from(3840, 400, 1.0);
        assert!(dense <= UI_SCALE_MAX);
    }
}
