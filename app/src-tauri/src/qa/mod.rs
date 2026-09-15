pub mod service_control;

use serde::Serialize;
use service_control::{ServiceControl, ServiceControlGuard};
use std::sync::Arc;

struct SandboxServiceControl;

impl ServiceControl for SandboxServiceControl {
    fn stop_daemon_unit(&self) -> Result<(), String> {
        Ok(())
    }

    fn run_daemon(&self, _arguments: &[&str]) -> Result<String, String> {
        Err("QA service actions are disabled".to_owned())
    }

    fn detach_daemon(&self, _arguments: &[&str]) -> Result<(), String> {
        Err("QA detached service actions are disabled".to_owned())
    }

    fn run_update(&self, _prompt: &str) -> Result<(), String> {
        Err("QA updates are disabled".to_owned())
    }

    fn launch(&self) -> Result<(), String> {
        Err("QA external launch is disabled".to_owned())
    }

    fn register_shortcut(&self) -> Result<(), String> {
        Err("QA global shortcuts are disabled".to_owned())
    }

    fn autoconfigure(&self) -> Result<(), String> {
        Err("QA automatic configuration is disabled".to_owned())
    }
}

pub fn activate() -> Result<ServiceControlGuard, String> {
    validate_environment()?;
    service_control::install(Arc::new(SandboxServiceControl))
}

#[cfg(feature = "qa-webdriver")]
pub fn webdriver_port() -> Result<u16, String> {
    validate_environment()?;
    parse_webdriver_port(
        std::env::var("OPEN_ISLAND_QA_WEBDRIVER_PORT")
            .ok()
            .as_deref(),
    )
}

#[cfg(any(test, feature = "qa-webdriver"))]
fn parse_webdriver_port(value: Option<&str>) -> Result<u16, String> {
    value
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|port| *port >= 1024)
        .ok_or_else(|| {
            "OPEN_ISLAND_QA_WEBDRIVER_PORT must be an explicit unprivileged port".to_owned()
        })
}

fn validate_environment() -> Result<(), String> {
    if std::env::var_os("OPEN_ISLAND_QA") != Some("1".into()) {
        return Err("OPEN_ISLAND_QA=1 is required".to_owned());
    }
    let home = required_path("HOME")?;
    for variable in ["XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME"] {
        if !required_path(variable)?.starts_with(&home) {
            return Err(format!("{variable} must be inside the private QA HOME"));
        }
    }
    let socket = required_path("OPEN_ISLAND_SOCKET")?;
    if socket
        .parent()
        .and_then(|p| std::fs::canonicalize(p).ok())
        .is_none_or(|p| !p.starts_with(&home))
    {
        return Err("OPEN_ISLAND_SOCKET must be inside the private QA HOME".to_owned());
    }
    if std::env::var_os("OPEN_ISLAND_QA_CREDENTIALS_BLOCKED") != Some("1".into()) {
        return Err("QA credential blocking marker is required".to_owned());
    }
    Ok(())
}

fn required_path(variable: &str) -> Result<std::path::PathBuf, String> {
    let path = std::env::var_os(variable)
        .map(std::path::PathBuf::from)
        .ok_or_else(|| format!("{variable} is required"))?;
    if !path.is_absolute() {
        return Err(format!("{variable} must be absolute"));
    }
    if variable != "OPEN_ISLAND_SOCKET" {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&path).map_err(|_| "QA directory missing")?;
        if !metadata.is_dir()
            || metadata.permissions().mode() & 0o077 != 0
            || std::fs::canonicalize(&path).map_err(|_| "QA directory unavailable")? != path
        {
            return Err("QA directories must be private and canonical".to_owned());
        }
    }
    Ok(path)
}

pub fn bundle_identifier(attempt: &str) -> Result<String, String> {
    if attempt.is_empty()
        || !attempt
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err("QA attempt must contain only ASCII letters, digits, or hyphens".to_owned());
    }
    Ok(format!("app.open-island.qa.{attempt}"))
}

#[derive(Serialize)]
struct PreflightReport {
    isolated: bool,
    service_control: bool,
}

#[tauri::command]
fn qa_preflight() -> Result<PreflightReport, String> {
    validate_environment()?;
    service_control::preflight()?;
    Ok(PreflightReport {
        isolated: true,
        service_control: true,
    })
}

#[tauri::command]
fn qa_bundle_identifier(attempt: String) -> Result<String, String> {
    bundle_identifier(&attempt)
}

pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("qa-harness")
        .invoke_handler(tauri::generate_handler![qa_preflight, qa_bundle_identifier])
        .build()
}

#[cfg(test)]
mod tests {
    use super::{bundle_identifier, service_control};

    #[test]
    fn webdriver_requires_an_explicit_unprivileged_port() {
        for value in [
            None,
            Some(""),
            Some("0"),
            Some("80"),
            Some("65536"),
            Some("localhost:4445"),
        ] {
            assert!(super::parse_webdriver_port(value).is_err());
        }
        assert_eq!(super::parse_webdriver_port(Some("4445")), Ok(4445));
    }

    #[test]
    fn isolation_negative_refuses_dangerous_actions_without_adapter() {
        assert!(service_control::preflight().is_err());
        assert!(crate::settings::stop_daemon_unit().is_err());
        assert!(service_control::run_update("qa-isolation-negative").is_err());
        assert!(crate::settings::remove_auto_configuration().is_err());
        assert!(crate::settings::set_integration("autostart", true).is_err());
        assert!(service_control::register_shortcut().is_err());
        assert!(service_control::deny_launch().is_err());
    }

    #[test]
    fn bundle_identifier_uses_qa_namespace() {
        assert_eq!(
            bundle_identifier("attempt-1").expect("valid QA attempt"),
            "app.open-island.qa.attempt-1"
        );
        assert!(bundle_identifier("../real").is_err());
    }
}
