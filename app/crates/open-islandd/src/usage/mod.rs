pub mod anthropic;
pub mod codex;

use crate::installer;
use open_island_core::{
    config::{UsageConfig, UsageProviderChoice},
    usage::{ProviderUsage, UsageProvider, UsageReport, PROVIDER_ANTHROPIC, PROVIDER_CODEX},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub use anthropic::now_ms;

pub fn cache_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    Some(base.join("open-island").join("usage.json"))
}

pub fn load_cached() -> UsageReport {
    let Some(path) = cache_path() else {
        return UsageReport::default();
    };
    fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<UsageReport>(&text).ok())
        .unwrap_or_default()
}

pub fn save_cached(report: &UsageReport) {
    if report
        .providers
        .iter()
        .all(|entry| entry.snapshot.is_none())
    {
        return;
    }
    let Some(path) = cache_path() else {
        return;
    };
    let Ok(text) = serde_json::to_string(report) else {
        return;
    };
    let _ = open_island_core::config::write_atomic(&path, &text);
}

pub fn providers(home: &Path, config: &UsageConfig) -> Vec<Box<dyn UsageProvider>> {
    let mut providers: Vec<Box<dyn UsageProvider>> = Vec::new();
    if config.use_claude_login {
        providers.push(Box::new(anthropic::AnthropicUsage::new(home.to_path_buf())));
    }
    providers.push(Box::new(codex::CodexUsage::new(home.to_path_buf())));
    providers
}

pub fn collect(
    home: &Path,
    config: &UsageConfig,
    previous: &UsageReport,
    now_ms: u64,
) -> UsageReport {
    let mut providers = Vec::new();
    for provider in self::providers(home, config) {
        let name = provider.name();
        let carried = carried_snapshot(home, previous, name);
        if !provider.discover() {
            providers.push(ProviderUsage {
                provider: name.to_owned(),
                detected: false,
                snapshot: None,
                error: None,
                checked_at_ms: now_ms,
            });
            continue;
        }
        let fetched = provider
            .fetch()
            .and_then(|raw| provider.normalize(&raw, now_ms));
        providers.push(match fetched {
            Ok(snapshot) => ProviderUsage {
                provider: name.to_owned(),
                detected: true,
                snapshot: Some(snapshot),
                error: None,
                checked_at_ms: now_ms,
            },
            Err(error) => ProviderUsage {
                provider: name.to_owned(),
                detected: true,
                snapshot: carried,
                error: Some(error.message().to_owned()),
                checked_at_ms: now_ms,
            },
        });
    }
    UsageReport { providers }
}

fn carried_snapshot(
    home: &Path,
    previous: &UsageReport,
    name: &str,
) -> Option<open_island_core::usage::UsageSnapshot> {
    let snapshot = previous.snapshot(name)?.clone();
    if name != PROVIDER_ANTHROPIC {
        return Some(snapshot);
    }
    let current = anthropic::AnthropicUsage::new(home.to_path_buf()).local_identity();
    match (current, snapshot.identity.as_deref()) {
        (Some(current), Some(stored)) if current != stored => None,
        _ => Some(snapshot),
    }
}

pub fn preferred(config: &UsageConfig, leading_agent: Option<&str>) -> &'static str {
    match config.preferred_provider {
        UsageProviderChoice::Anthropic => PROVIDER_ANTHROPIC,
        UsageProviderChoice::Codex => PROVIDER_CODEX,
        UsageProviderChoice::Auto => match leading_agent {
            Some("codex") => PROVIDER_CODEX,
            _ => PROVIDER_ANTHROPIC,
        },
    }
}

pub fn home() -> Option<PathBuf> {
    installer::home_dir().ok()
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
