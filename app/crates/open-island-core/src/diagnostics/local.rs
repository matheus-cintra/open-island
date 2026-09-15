use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Instant,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Service {
    pub installed: bool,
    pub active: Option<bool>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Binary {
    pub present: bool,
    pub version: Option<String>,
    pub probe: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Local {
    pub service: Service,
    pub daemon_binary: Binary,
    pub app_binary: Binary,
}
fn executable(name: &str) -> Option<PathBuf> {
    crate::paths::executable(name)
}
fn service(deadline: Instant) -> Service {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let installed = home.as_ref().is_some_and(|home| {
        if cfg!(target_os = "macos") {
            home.join("Library/LaunchAgents/app.open-island.daemon.plist")
                .is_file()
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"))
                .join("systemd/user/open-islandd.service")
                .is_file()
        }
    });
    let mut result = Service {
        installed,
        active: None,
    };
    if cfg!(feature = "qa-harness") {
        return result;
    }
    let program = if cfg!(target_os = "macos") {
        "launchctl"
    } else {
        "systemctl"
    };
    let Some(program) = executable(program) else {
        return result;
    };
    let args: &[&str] = if cfg!(target_os = "macos") {
        &["list"]
    } else {
        &[
            "--user",
            "show",
            "--property=ActiveState",
            "--property=LoadState",
            "open-islandd.service",
        ]
    };
    if let Ok((true, output)) = super::probe::run(&program, args, deadline) {
        if cfg!(target_os = "macos") {
            let row = output.lines().find_map(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                (fields.len() == 3 && fields[2] == "app.open-island.daemon").then_some(fields)
            });
            result.installed |= row.is_some();
            result.active = Some(row.is_some_and(|fields| fields[0].parse::<u32>().is_ok()));
        } else {
            if let Some(load) = output
                .lines()
                .find_map(|line| line.strip_prefix("LoadState="))
            {
                result.installed = load != "not-found";
            }
            result.active = match output
                .lines()
                .find_map(|line| line.strip_prefix("ActiveState="))
            {
                Some("active" | "reloading") => Some(true),
                Some("inactive" | "failed" | "activating" | "deactivating") => Some(false),
                _ => None,
            };
        }
    }
    result
}
fn same_executable(path: &Path) -> bool {
    path.canonicalize()
        .ok()
        .zip(
            std::env::current_exe()
                .ok()
                .and_then(|p| p.canonicalize().ok()),
        )
        .is_some_and(|(a, b)| a == b)
}
fn binary(name: &str, self_version: Option<&str>, deadline: Instant) -> Binary {
    let Some(path) = executable(name) else {
        return Binary {
            present: false,
            version: None,
            probe: "missing".into(),
        };
    };
    let mut report = Binary {
        present: true,
        version: None,
        probe: "unverified_legacy".into(),
    };
    if same_executable(&path) {
        if let Some(version) = self_version {
            report.version = Some(version.into());
            report.probe = "current_process".into();
            return report;
        }
    }
    if cfg!(feature = "qa-harness") || name != "open-islandd" {
        return report;
    }
    // Old daemon binaries start normally for unrecognized options. Their help is safe.
    let supported = match super::probe::run(&path, &["--help"], deadline) {
        Ok((true, output)) => output
            .lines()
            .any(|line| line.trim_start().starts_with("--version ")),
        Err(code) => {
            report.probe = code.into();
            return report;
        }
        _ => false,
    };
    if !supported {
        return report;
    }
    match super::probe::run(&path, &["--version"], deadline) {
        Ok((true, output)) => {
            report.version = output
                .trim()
                .strip_prefix("open-islandd ")
                .and_then(|text| super::version(&serde_json::Value::String(text.into())));
            report.probe = if report.version.is_some() {
                "verified"
            } else {
                "invalid_output"
            }
            .into();
        }
        Err(code) => report.probe = code.into(),
        _ => report.probe = "invalid_output".into(),
    }
    report
}
pub fn collect(app_version: Option<&str>, deadline: Instant) -> Local {
    Local {
        service: service(deadline),
        daemon_binary: binary(
            "open-islandd",
            app_version.is_none().then_some(env!("CARGO_PKG_VERSION")),
            deadline,
        ),
        app_binary: binary("open-island", app_version, deadline),
    }
}
