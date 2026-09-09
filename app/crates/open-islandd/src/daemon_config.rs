use crate::installer;
use crate::notifications::lifecycle::{self, DaemonContext};
use open_island_core::config::{self, Config};
use serde_json::Value;
use std::{env, fs, io, path::Path, time::Duration};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const DEFAULT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(90);

pub fn approval_timeout() -> Duration {
    env::var("OPEN_ISLAND_APPROVAL_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map_or(DEFAULT_APPROVAL_TIMEOUT, Duration::from_millis)
}

pub fn question_timeout() -> Duration {
    env::var("OPEN_ISLAND_QUESTION_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map_or(DEFAULT_APPROVAL_TIMEOUT, Duration::from_millis)
}

pub const IDLE_AFTER_VARIABLE: &str = "OPEN_ISLAND_IDLE_MS";

pub fn idle_after_override() -> Option<Duration> {
    env::var(IDLE_AFTER_VARIABLE)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_millis)
}

pub fn idle_after(config: &Config) -> Duration {
    idle_after_override().unwrap_or(config.sessions.idle_after)
}

pub fn env_locked() -> Value {
    let mut locked = serde_json::Map::new();
    if idle_after_override().is_some() {
        locked.insert(
            "sessions.idle_after_ms".to_owned(),
            Value::String(IDLE_AFTER_VARIABLE.to_owned()),
        );
    }
    Value::Object(locked)
}

pub fn config_stamp(path: &Path) -> Option<(std::time::SystemTime, u64)> {
    let metadata = fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

pub fn configure_detected_agents(config: &mut Config) {
    if !config.integrations.auto_configure {
        return;
    }
    let (Ok(home), Ok(executable)) = (installer::home_dir(), env::current_exe()) else {
        return;
    };
    let (configured, errors) =
        installer::auto_configure(&home, &executable, &config.integrations.known_agents);
    for error in errors {
        eprintln!("open-islandd: auto-configure {error}");
    }
    if configured.is_empty() {
        return;
    }
    for agent in &configured {
        println!("open-islandd: configured the {agent} hooks");
    }
    config.integrations.known_agents.extend(configured);
    config.integrations.known_agents.sort();
    config.integrations.known_agents.dedup();
    let Some(path) = config::path() else {
        return;
    };
    if let Err(error) = config::save(&path, config) {
        eprintln!("open-islandd: {error}");
    }
}

pub fn load_config() -> Config {
    let Some(path) = config::path() else {
        return Config::default();
    };
    match fs::read_to_string(&path) {
        Ok(text) => Config::from_json_str(&text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Config::default(),
        Err(error) => {
            eprintln!("open-islandd: {}: {error}", path.display());
            Config::default()
        }
    }
}

pub fn hook_timeout() -> Duration {
    env::var("OPEN_ISLAND_HOOK_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map_or(
            DEFAULT_APPROVAL_TIMEOUT + Duration::from_secs(30),
            Duration::from_millis,
        )
}

pub fn reload_config(ctx: &DaemonContext, broadcast: impl Fn(String) -> bool) {
    let config = load_config();
    if *ctx.config.get() == config {
        return;
    }
    let idle_after = idle_after(&config);
    let rules = config.filters.rules.clone();
    let launchers = config.filters.launchers.clone();
    let cleanup_after = config.sessions.cleanup_after;
    let payload = config.to_json_value();
    ctx.config.set(config);
    if let Ok(mut state) = ctx.state.lock() {
        state.store.set_idle_after(idle_after);
        state.store.set_cleanup_after(cleanup_after);
        state.store.set_filter_rules(rules, launchers);
    }
    broadcast(lifecycle::config_changed_message(payload));
}
