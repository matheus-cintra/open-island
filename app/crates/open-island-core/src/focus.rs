use serde::Deserialize;
use std::process::Command;

pub trait WindowFocus {
    fn focus_pid(&self, pid: u32) -> Result<(), String>;
}

pub struct HyprlandBackend;
pub struct X11Backend;

#[derive(Deserialize)]
struct HyprClient {
    pid: u32,
    address: String,
}

impl WindowFocus for HyprlandBackend {
    fn focus_pid(&self, pid: u32) -> Result<(), String> {
        let output = Command::new("hyprctl")
            .args(["clients", "-j"])
            .output()
            .map_err(|error| format!("failed to run hyprctl clients: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "hyprctl clients failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let clients: Vec<HyprClient> = serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("invalid hyprctl client JSON: {error}"))?;
        let client = clients
            .iter()
            .find(|client| client.pid == pid)
            .ok_or_else(|| format!("no Hyprland client found for pid {pid}"))?;
        let address = if client.address.starts_with("0x") {
            client.address.clone()
        } else {
            format!("0x{}", client.address)
        };
        let selector = format!("address:{address}");
        let legacy = dispatch(&["dispatch", "focuswindow", &selector]);
        if legacy.is_ok() {
            return Ok(());
        }
        dispatch(&[
            "dispatch",
            &format!("hl.dsp.focus{{window=\"{selector}\"}}"),
        ])
        .map_err(|lua_error| match legacy {
            Err(legacy_error) => format!(
                "hyprctl focuswindow failed ({legacy_error}); Lua dispatch failed ({lua_error})"
            ),
            Ok(()) => lua_error,
        })
    }
}

/// Hyprland >= 0.56 parses dispatch arguments as Lua and rejects the legacy
/// `focuswindow address:0x..` string, while older builds only accept that form -
/// so both are attempted. The Lua form also exits 0 when the window is gone and
/// only reports `warning: ... window not found` on stdout, which is why success
/// is `stdout == "ok"` rather than the exit status.
fn dispatch(args: &[&str]) -> Result<(), String> {
    let result = Command::new("hyprctl")
        .args(args)
        .output()
        .map_err(|error| format!("failed to run hyprctl: {error}"))?;
    let stdout = String::from_utf8_lossy(&result.stdout);
    if dispatch_succeeded(result.status.success(), &stdout) {
        return Ok(());
    }
    let detail = if stdout.trim().is_empty() {
        String::from_utf8_lossy(&result.stderr).trim().to_owned()
    } else {
        stdout.trim().to_owned()
    };
    Err(detail)
}

fn dispatch_succeeded(status_ok: bool, stdout: &str) -> bool {
    status_ok && stdout.trim() == "ok"
}

impl WindowFocus for X11Backend {
    fn focus_pid(&self, pid: u32) -> Result<(), String> {
        let search = Command::new("xdotool")
            .args(["search", "--pid", &pid.to_string()])
            .output()
            .map_err(|error| format!("failed to run xdotool search: {error}"))?;
        if !search.status.success() {
            return Err(format!(
                "xdotool search failed: {}",
                String::from_utf8_lossy(&search.stderr).trim()
            ));
        }
        let window = String::from_utf8_lossy(&search.stdout)
            .split_whitespace()
            .next()
            .map(str::to_owned)
            .ok_or_else(|| format!("no X11 window found for pid {pid}"))?;
        let result = Command::new("xdotool")
            .args(["windowactivate", &window])
            .output()
            .map_err(|error| format!("failed to run xdotool windowactivate: {error}"))?;
        if result.status.success() {
            Ok(())
        } else {
            Err(format!(
                "xdotool windowactivate failed: {}",
                String::from_utf8_lossy(&result.stderr).trim()
            ))
        }
    }
}

pub fn focus_pid(pid: u32) -> Result<(), String> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        HyprlandBackend.focus_pid(pid).or_else(|hypr_error| {
            X11Backend.focus_pid(pid).map_err(|x11_error| {
                format!("Hyprland focus failed ({hypr_error}); X11 fallback failed ({x11_error})")
            })
        })
    } else {
        X11Backend.focus_pid(pid)
    }
}

#[cfg(test)]
mod tests {
    use super::dispatch_succeeded;

    #[test]
    fn only_an_ok_body_counts_as_a_successful_dispatch() {
        assert!(dispatch_succeeded(true, "ok\n"));
        assert!(!dispatch_succeeded(false, "ok"));
        assert!(!dispatch_succeeded(true, ""));
    }

    #[test]
    fn lua_window_not_found_is_a_failure_despite_exiting_zero() {
        assert!(!dispatch_succeeded(
            true,
            "warning: =[C]:-1: hl.focus: window not found\n"
        ));
    }
}
