use crate::runner::CommandRunner;

#[cfg(target_os = "macos")]
pub fn focus_pid(pid: u32, _runner: &dyn CommandRunner) -> Result<(), String> {
    crate::process::activate(pid)
}
#[cfg(not(target_os = "macos"))]
pub fn focus_pid(pid: u32, runner: &dyn CommandRunner) -> Result<(), String> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() && hyprland(pid, runner).is_ok() {
        return Ok(());
    }
    let pid = pid.to_string();
    let search = runner.run("xdotool", &["search", "--pid", &pid])?;
    if !search.status.success() {
        return Err("window_not_found".into());
    }
    let window = String::from_utf8_lossy(&search.stdout)
        .split_whitespace()
        .next()
        .ok_or("window_not_found")?
        .to_owned();
    let activation = runner.run("xdotool", &["windowactivate", &window])?;
    if activation.status.success() {
        Ok(())
    } else {
        Err("window_activation_failed".into())
    }
}
#[cfg(not(target_os = "macos"))]
fn hyprland(pid: u32, runner: &dyn CommandRunner) -> Result<(), String> {
    let output = runner.run("hyprctl", &["clients", "-j"])?;
    if !output.status.success() {
        return Err("window_query_failed".into());
    }
    let clients: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|_| "invalid_window_list")?;
    let address = clients
        .as_array()
        .and_then(|clients| {
            clients.iter().find(|client| {
                client.get("pid").and_then(serde_json::Value::as_u64) == Some(pid.into())
            })
        })
        .and_then(|client| client.get("address"))
        .and_then(serde_json::Value::as_str)
        .ok_or("window_not_found")?;
    focus_address(address, runner)
}

pub fn focus_address(address: &str, runner: &dyn CommandRunner) -> Result<(), String> {
    let raw = address.strip_prefix("0x").unwrap_or(address);
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid_window_address".into());
    }
    let selector = format!("address:0x{raw}");
    let first = runner.run("hyprctl", &["dispatch", "focuswindow", &selector])?;
    if first.status.success() && String::from_utf8_lossy(&first.stdout).trim() == "ok" {
        return Ok(());
    }
    let lua = runner.run(
        "hyprctl",
        &[
            "dispatch",
            &format!("hl.dsp.focus{{window=\"{selector}\"}}"),
        ],
    )?;
    if lua.status.success() && String::from_utf8_lossy(&lua.stdout).trim() == "ok" {
        Ok(())
    } else {
        Err("window_activation_failed".into())
    }
}

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    use super::*;
    use crate::runner::FakeRunner;
    #[test]
    fn guarded_hyprland_checks_reply_and_validates_address_before_dispatch() {
        let runner = FakeRunner::new();
        runner.push_ok("hyprctl", r#"[{"pid":42,"address":"0x123"}]"#);
        runner.push_status("hyprctl", 1, "legacy unsupported", "");
        runner.push_ok("hyprctl", "ok\n");
        assert!(hyprland(42, &runner).is_ok());
        let invalid = FakeRunner::new();
        invalid.push_ok("hyprctl", r#"[{"pid":42,"address":"bad\"selector"}]"#);
        assert_eq!(
            hyprland(42, &invalid).unwrap_err(),
            "invalid_window_address"
        );
        assert_eq!(invalid.calls().len(), 1);
    }
}
