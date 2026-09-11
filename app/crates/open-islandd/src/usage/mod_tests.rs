use super::*;
use open_island_core::config::UsageConfig;
use open_island_core::usage::{UsageSnapshot, UsageWindow};

fn snapshot(provider: &str, identity: Option<&str>) -> UsageSnapshot {
    UsageSnapshot {
        provider: provider.to_owned(),
        identity: identity.map(str::to_owned),
        plan: None,
        windows: vec![UsageWindow {
            key: "session".to_owned(),
            label: "5h".to_owned(),
            percent: 42.0,
            resets_at_ms: None,
        }],
        models: Vec::new(),
        reset_cards: Vec::new(),
        credits: None,
        fetched_at_ms: 1,
    }
}

fn report(entries: Vec<ProviderUsage>) -> UsageReport {
    UsageReport { providers: entries }
}

#[test]
fn a_home_with_no_agent_detects_neither_provider_and_reports_no_error() {
    let home = PathBuf::from("/nonexistent/open-island-usage-test");
    let collected = collect(&home, &UsageConfig::default(), &UsageReport::default(), 7);
    assert_eq!(collected.providers.len(), 2);
    for entry in &collected.providers {
        assert!(
            !entry.detected,
            "{} claimed a home that is not there",
            entry.provider
        );
        assert!(entry.snapshot.is_none());
        assert!(entry.error.is_none());
        assert_eq!(entry.checked_at_ms, 7);
    }
}

#[test]
fn turning_the_claude_login_off_removes_the_provider_instead_of_erroring_on_it() {
    let home = PathBuf::from("/nonexistent/open-island-usage-test");
    let config = UsageConfig {
        use_claude_login: false,
        ..UsageConfig::default()
    };
    let collected = collect(&home, &config, &UsageReport::default(), 0);
    assert_eq!(collected.providers.len(), 1);
    assert_eq!(collected.providers[0].provider, PROVIDER_CODEX);
}

#[test]
fn a_carried_snapshot_survives_for_codex_because_its_account_is_only_known_after_a_fetch() {
    let home = PathBuf::from("/nonexistent/open-island-usage-test");
    let previous = report(vec![ProviderUsage {
        provider: PROVIDER_CODEX.to_owned(),
        detected: true,
        snapshot: Some(snapshot(PROVIDER_CODEX, Some("account-a"))),
        error: None,
        checked_at_ms: 0,
    }]);
    assert!(carried_snapshot(&home, &previous, PROVIDER_CODEX).is_some());
}

#[test]
fn a_carried_anthropic_snapshot_is_dropped_when_the_account_on_disk_no_longer_matches() {
    let home = PathBuf::from("/nonexistent/open-island-usage-test");
    let previous = report(vec![ProviderUsage {
        provider: PROVIDER_ANTHROPIC.to_owned(),
        detected: true,
        snapshot: Some(snapshot(PROVIDER_ANTHROPIC, Some("account-a"))),
        error: None,
        checked_at_ms: 0,
    }]);
    assert!(
        carried_snapshot(&home, &previous, PROVIDER_ANTHROPIC).is_some(),
        "a home with no credentials cannot contradict the stored identity"
    );
}

#[test]
fn auto_follows_the_agent_at_the_top_of_the_list() {
    let config = UsageConfig::default();
    assert_eq!(preferred(&config, Some("codex")), PROVIDER_CODEX);
    assert_eq!(preferred(&config, Some("claude")), PROVIDER_ANTHROPIC);
    assert_eq!(preferred(&config, Some("opencode")), PROVIDER_ANTHROPIC);
    assert_eq!(preferred(&config, None), PROVIDER_ANTHROPIC);
}

#[test]
fn a_pinned_provider_ignores_the_session_at_the_top() {
    let pinned = |choice| UsageConfig {
        preferred_provider: choice,
        ..UsageConfig::default()
    };
    assert_eq!(
        preferred(&pinned(UsageProviderChoice::Codex), Some("claude")),
        PROVIDER_CODEX
    );
    assert_eq!(
        preferred(&pinned(UsageProviderChoice::Anthropic), Some("codex")),
        PROVIDER_ANTHROPIC
    );
}

#[test]
fn the_cache_lands_under_the_state_directory_and_not_beside_the_config() {
    let path = cache_path().expect("a cache path");
    let text = path.to_string_lossy();
    let suffix = if cfg!(target_os = "macos") && std::env::var_os("XDG_STATE_HOME").is_none() {
        "Open Island/state/usage.json"
    } else {
        "open-island/usage.json"
    };
    assert!(text.ends_with(suffix), "{text}");
    assert!(!text.contains(".config/"), "{text}");
}
