use open_island_core::config::{self, Config, SOUND_THEME_DIR};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::client::daemon_candidates;

pub fn save(config: Value) -> Result<(), String> {
    let parsed = Config::from_json_str(&config.to_string());
    let path = config::path().ok_or_else(|| "no config directory for this user".to_owned())?;
    config::save(&path, &parsed)
}

pub fn user_sound_dir() -> Option<PathBuf> {
    open_island_core::paths::data_dir().map(|dir| dir.join("sounds"))
}

fn sounds_in(directory: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut sounds = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|extension| {
                matches!(
                    extension.to_string_lossy().as_ref(),
                    "oga" | "ogg" | "wav" | "flac" | "mp3"
                )
            })
        })
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    sounds.sort();
    sounds
}

pub fn theme_sounds() -> Vec<String> {
    let mut sounds = user_sound_dir()
        .map(|directory| sounds_in(&directory))
        .unwrap_or_default();
    if cfg!(target_os = "macos") {
        if let Some(dir) = open_island_core::paths::bundled_sounds() {
            sounds.extend(sounds_in(&dir));
        }
    } else {
        sounds.extend(sounds_in(Path::new(SOUND_THEME_DIR)));
    }
    sounds
}

pub fn integration_status() -> Result<Value, String> {
    let mut report = serde_json::Map::new();
    report.insert(
        "autostart".to_owned(),
        always_detected(is_installed(&["autostart", "status"])?),
    );
    #[cfg(target_os = "linux")]
    report.insert(
        "hyprland".to_owned(),
        always_detected(is_installed(&["hotkey", "status"])?),
    );
    let output = run_daemon(&["hooks", "status"])?;
    let agents = serde_json::from_str::<Value>(output.trim())
        .map_err(|error| format!("invalid status from open-islandd: {error}"))?;
    let Value::Object(agents) = agents else {
        return Err("open-islandd returned no agent status".to_owned());
    };
    report.extend(agents);
    Ok(Value::Object(report))
}

pub fn set_integration(name: &str, enabled: bool) -> Result<Value, String> {
    let action = if enabled { "install" } else { "uninstall" };
    match name {
        "autostart" => run_daemon(&["autostart", action])?,
        "hyprland" => run_daemon(&["hotkey", action])?,
        agent => {
            run_daemon(&["hooks", action, "--agent", agent])?;
            remember_agent(agent)?;
            String::new()
        }
    };
    integration_status()
}

pub fn remove_auto_configuration() -> Result<Vec<String>, String> {
    if let Some(path) = config::path() {
        let mut config = Config::from_json_str(&std::fs::read_to_string(&path).unwrap_or_default());
        config.integrations.auto_configure = false;
        config::save(&path, &config)?;
    }
    let mut removed = Vec::new();
    let mut actions = vec![&["hooks", "uninstall"][..]];
    if cfg!(target_os = "linux") {
        actions.push(&["hotkey", "uninstall"]);
    }
    for args in actions {
        for line in run_daemon(args)?.lines() {
            removed.push(line.trim().to_owned());
        }
    }
    detach_daemon(&["autostart", "uninstall"])?;
    Ok(removed)
}

#[cfg(target_os = "linux")]
pub fn stop_daemon_unit() -> Result<(), String> {
    let status = Command::new("systemctl")
        .args(["--user", "stop", "open-islandd.service"])
        .status()
        .map_err(|error| format!("systemctl: {error}"))?;
    if status.success() {
        return Ok(());
    }
    Err(format!("systemctl stop exited with {status}"))
}

#[cfg(target_os = "linux")]
fn detach_daemon(args: &[&str]) -> Result<(), String> {
    let executable = executables()
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| "open-islandd not found".to_owned())?;
    let status = Command::new("systemd-run")
        .args(["--user", "--collect", "--quiet"])
        .arg(&executable)
        .args(args)
        .status()
        .map_err(|error| format!("systemd-run: {error}"))?;
    if status.success() {
        return Ok(());
    }
    Err(format!("systemd-run exited with {status}"))
}

fn always_detected(installed: bool) -> Value {
    json!({"detected": true, "installed": installed})
}

fn remember_agent(name: &str) -> Result<(), String> {
    let path = config::path().ok_or_else(|| "no config directory for this user".to_owned())?;
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut config = Config::from_json_str(&text);
    if config
        .integrations
        .known_agents
        .iter()
        .any(|known| known == name)
    {
        return Ok(());
    }
    config.integrations.known_agents.push(name.to_owned());
    config.integrations.known_agents.sort();
    config::save(&path, &config)
}

fn is_installed(args: &[&str]) -> Result<bool, String> {
    let output = run_daemon(args)?;
    let reply = serde_json::from_str::<Value>(output.trim())
        .map_err(|error| format!("invalid status from open-islandd: {error}"))?;
    Ok(reply["installed"] == Value::Bool(true))
}

fn run_daemon(args: &[&str]) -> Result<String, String> {
    let mut last = "open-islandd not found".to_owned();
    for candidate in executables() {
        let output = match Command::new(&candidate).args(args).output() {
            Ok(output) => output,
            Err(error) => {
                last = format!("{}: {error}", candidate.display());
                continue;
            }
        };
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    Err(last)
}

fn executables() -> Vec<PathBuf> {
    let mut candidates = daemon_candidates();
    candidates.push(PathBuf::from("open-islandd"));
    candidates
}

#[cfg(target_os = "macos")]
pub fn stop_daemon_unit() -> Result<(), String> {
    // Resolve the daemon through its private socket.
    run_daemon(&["stop"]).map(|_| ())
}
#[cfg(target_os = "macos")]
fn detach_daemon(args: &[&str]) -> Result<(), String> {
    let executable = executables()
        .into_iter()
        .find(|p| p.is_file())
        .ok_or("open-islandd não encontrado")?;
    let mut child = Command::new(executable)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
