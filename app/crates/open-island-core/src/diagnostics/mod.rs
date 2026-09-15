pub mod audio;
pub mod hooks;
pub mod local;
mod probe;
mod response;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    path::Path,
    time::{Duration, Instant},
};

pub const CAPABILITIES: [&str; 4] = [
    "ui_state_v1",
    "message_delivery_v1",
    "guarded_actions_v1",
    "diagnostics_v1",
];
#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub schema_version: u8,
    pub platform: String,
    pub arch: String,
    pub collector_version: String,
    pub app_version: Option<String>,
    pub daemon: Daemon,
    pub socket: Socket,
    pub model: String,
    #[serde(default)]
    pub audio: audio::Audio,
    pub counters: Option<Counters>,
    pub local: local::Local,
    pub hooks: Vec<hooks::HookStatus>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Daemon {
    pub state: String,
    pub version: Option<String>,
    pub pid: Option<u32>,
    pub epoch: Option<String>,
    pub capabilities: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Socket {
    pub exists: bool,
    pub connectable: bool,
}
impl Report {
    pub fn healthy(&self) -> bool {
        self.daemon.state == "ready"
            && (!self.local.service.installed || self.local.service.active == Some(true))
            && self
                .hooks
                .iter()
                .all(|hook| matches!(hook.state.as_str(), "missing" | "current"))
    }
}
fn version(value: &Value) -> Option<String> {
    let text = value.as_str()?;
    if text.len() > 48 {
        return None;
    }
    let base = text.split(['-', '+']).next()?;
    let parts: Vec<_> = base.split('.').collect();
    (parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-+".contains(&b)))
    .then(|| text.to_owned())
}
pub fn collect(path: &Path, app_version: Option<&str>) -> Report {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut report = transport(path, app_version);
    report.local = local::collect(app_version, deadline);
    report.audio = audio::collect();
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    report.hooks = hooks::collect(home.as_deref());
    report
}
fn transport(path: &Path, app_version: Option<&str>) -> Report {
    let mut report = Report {
        schema_version: 1,
        platform: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        collector_version: env!("CARGO_PKG_VERSION").into(),
        app_version: app_version.map(str::to_owned),
        daemon: Daemon {
            state: "unavailable".into(),
            version: None,
            pid: None,
            epoch: None,
            capabilities: vec![],
        },
        socket: Socket {
            exists: path.exists(),
            connectable: false,
        },
        model: "not_applicable".into(),
        audio: audio::Audio::default(),
        counters: None,
        hooks: Vec::new(),
        local: local::Local {
            service: local::Service {
                installed: false,
                active: None,
            },
            daemon_binary: local::Binary {
                present: false,
                version: None,
                probe: "not_checked".into(),
            },
            app_binary: local::Binary {
                present: false,
                version: None,
                probe: "not_checked".into(),
            },
        },
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    let Ok(mut stream) = crate::unix_socket::connect(path, Duration::from_millis(500)) else {
        return report;
    };
    report.socket.connectable = true;
    let result = (|| -> Result<Value, ()> {
        stream
            .set_write_timeout(Some(Duration::from_millis(500)))
            .map_err(|_| ())?;
        let request =
            json!({"v":1,"id":1,"method":"ping","params":{"client_role":"diagnostic"}}).to_string();
        stream
            .write_all(format!("{request}\n").as_bytes())
            .map_err(|_| ())?;
        response::read(
            |buffer, remaining| {
                stream.set_read_timeout(Some(remaining))?;
                stream.read(buffer)
            },
            deadline,
        )
    })();
    let Ok(data) = result else {
        report.daemon.state = "invalid_response".into();
        return report;
    };
    if data.get("daemon").and_then(Value::as_str) != Some("open-islandd") {
        report.daemon.state = "invalid_response".into();
        return report;
    }
    report.counters = data
        .get("counters")
        .and_then(|value| serde_json::from_value(value.clone()).ok());
    report.daemon.version = data.get("version").and_then(version);
    report.daemon.pid = data
        .get("pid")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok());
    report.daemon.epoch = data
        .get("daemon_epoch")
        .and_then(Value::as_str)
        .filter(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_owned);
    report.daemon.capabilities = CAPABILITIES
        .iter()
        .filter(|cap| {
            data.get("capabilities")
                .and_then(Value::as_array)
                .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(**cap)))
        })
        .map(|cap| (*cap).to_owned())
        .collect();
    report.daemon.state = if report.daemon.version.is_some()
        && report.daemon.epoch.is_some()
        && report.daemon.capabilities.len() == CAPABILITIES.len()
    {
        "ready"
    } else {
        "incompatible"
    }
    .into();
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn version_does_not_forward_error_paths_or_arbitrary_text() {
        for text in ["/home/private/secret", "TOKEN", "1.2", "1.2.3\nsecret"] {
            assert!(version(&json!(text)).is_none());
        }
        assert_eq!(version(&json!("0.6.3")), Some("0.6.3".into()));
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Capacity {
    pub active: u64,
    pub high_water: u64,
    pub rejected: u64,
    pub limit: u64,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Counters {
    pub outbox: OutboxCounters,
    pub connections_legacy: Capacity,
    pub connections_managed: Capacity,
    pub fast_requests: Capacity,
    pub blocking_requests: Capacity,
    pub bulk_requests: Capacity,
    pub no_island: u64,
    pub pending_approvals: u64,
    pub pending_questions: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OutboxCounters {
    pub max_connection_items: u64,
    pub max_connection_bytes: u64,
    pub overflow_disconnects: u64,
    pub item_limit: u64,
    pub byte_limit: u64,
}
