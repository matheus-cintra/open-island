use super::*;
use serde_json::json;

fn anthropic_payload() -> Value {
    json!({
        "five_hour": {
            "utilization": 12.0,
            "resets_at": "2026-09-07T07:00:00.289344+00:00",
            "limit_dollars": null
        },
        "seven_day": {
            "utilization": 3.0,
            "resets_at": "2026-09-14T01:00:00.289371+00:00"
        },
        "seven_day_opus": null,
        "extra_usage": { "is_enabled": false },
        "limits": [
            {
                "kind": "session",
                "group": "session",
                "percent": 12,
                "severity": "normal",
                "resets_at": "2026-09-07T07:00:00.289344+00:00",
                "scope": null,
                "is_active": true
            },
            {
                "kind": "weekly_all",
                "group": "weekly",
                "percent": 3,
                "severity": "normal",
                "resets_at": "2026-09-14T01:00:00.289371+00:00",
                "scope": null,
                "is_active": false
            },
            {
                "kind": "weekly_scoped",
                "group": "weekly",
                "percent": 60,
                "severity": "normal",
                "resets_at": null,
                "scope": { "model": { "id": null, "display_name": "Fable" }, "surface": null },
                "is_active": false
            }
        ]
    })
}

fn codex_payload() -> Value {
    json!({
        "rateLimits": {
            "limitId": "codex",
            "limitName": null,
            "primary": { "usedPercent": 7, "windowDurationMins": 10080, "resetsAt": 1788881122 },
            "secondary": null,
            "credits": { "hasCredits": false, "unlimited": false, "balance": "0" },
            "individualLimit": null,
            "spendControlReached": false,
            "planType": "pro",
            "rateLimitReachedType": null
        },
        "rateLimitsByLimitId": {
            "codex": {
                "limitId": "codex",
                "primary": { "usedPercent": 7, "windowDurationMins": 10080, "resetsAt": 1788881122 }
            },
            "codex_bengalfox": {
                "limitId": "codex_bengalfox",
                "limitName": "GPT-5.3-Codex-Spark",
                "primary": { "usedPercent": 4, "windowDurationMins": 300, "resetsAt": 1788776517 },
                "secondary": { "usedPercent": 0, "windowDurationMins": 10080, "resetsAt": 1789363317 }
            }
        },
        "rateLimitResetCredits": {
            "availableCount": 2,
            "credits": [
                {
                    "id": "RateLimitResetCredit_ddfd18528a9c81919e58e5dc00d3e8b8",
                    "resetType": "codexRateLimits",
                    "status": "available",
                    "grantedAt": 1788487933,
                    "expiresAt": 1791079933,
                    "title": "Full reset",
                    "description": "Thanks for using Codex!"
                },
                {
                    "id": "RateLimitResetCredit_redeemed",
                    "resetType": "codexRateLimits",
                    "status": "redeemed",
                    "grantedAt": 1788581944,
                    "expiresAt": 1791173944,
                    "title": "Full reset",
                    "description": null
                }
            ]
        },
        "accountId": "a412213a-40e4-4367-b303-0d36a4fe7543",
        "rateLimitUpsell": null
    })
}

#[test]
fn the_anthropic_windows_come_from_limits_and_carry_the_5h_and_7d_labels() {
    let snapshot = anthropic::normalize(&anthropic_payload(), None, 1_000).unwrap();
    assert_eq!(snapshot.provider, PROVIDER_ANTHROPIC);
    assert_eq!(
        snapshot.windows,
        vec![
            UsageWindow {
                key: "session".to_owned(),
                label: "5h".to_owned(),
                percent: 12.0,
                resets_at_ms: Some(1_788_764_400_000),
            },
            UsageWindow {
                key: "weekly".to_owned(),
                label: "7d".to_owned(),
                percent: 3.0,
                resets_at_ms: Some(1_789_347_600_000),
            },
        ]
    );
    assert_eq!(snapshot.fetched_at_ms, 1_000);
}

#[test]
fn a_scoped_limit_becomes_the_per_model_window_the_header_draws_on_the_right() {
    let snapshot = anthropic::normalize(&anthropic_payload(), None, 0).unwrap();
    assert_eq!(
        snapshot.models,
        vec![UsageModelWindow {
            model: "Fable".to_owned(),
            percent: 60.0,
            resets_at_ms: None,
        }]
    );
}

#[test]
fn the_old_top_level_fields_answer_when_limits_is_missing() {
    let mut payload = anthropic_payload();
    payload.as_object_mut().unwrap().remove("limits");
    let snapshot = anthropic::normalize(&payload, None, 0).unwrap();
    assert_eq!(
        snapshot
            .windows
            .iter()
            .map(|window| (window.label.as_str(), window.percent))
            .collect::<Vec<_>>(),
        vec![("5h", 12.0), ("7d", 3.0)]
    );
    assert!(snapshot.models.is_empty());
}

#[test]
fn a_response_with_no_window_at_all_is_an_error_and_never_an_empty_snapshot() {
    let error = anthropic::normalize(&json!({ "limits": [] }), None, 0).unwrap_err();
    assert!(matches!(error, UsageError::Shape(_)));
    let error = codex::normalize(&json!({ "accountId": "x" }), None, 0).unwrap_err();
    assert!(matches!(error, UsageError::Shape(_)));
}

#[test]
fn the_codex_window_label_comes_from_the_duration_the_app_server_reports() {
    let snapshot = codex::normalize(&codex_payload(), None, 5).unwrap();
    assert_eq!(
        snapshot.windows,
        vec![UsageWindow {
            key: "primary".to_owned(),
            label: "7d".to_owned(),
            percent: 7.0,
            resets_at_ms: Some(1_788_881_122_000),
        }]
    );
    assert_eq!(snapshot.plan.as_deref(), Some("pro"));
}

#[test]
fn the_spark_bucket_is_a_model_row_and_the_primary_bucket_is_not_repeated_as_one() {
    let snapshot = codex::normalize(&codex_payload(), None, 0).unwrap();
    assert_eq!(
        snapshot.models,
        vec![UsageModelWindow {
            model: "GPT-5.3-Codex-Spark".to_owned(),
            percent: 4.0,
            resets_at_ms: Some(1_788_776_517_000),
        }]
    );
}

#[test]
fn only_an_available_reset_card_is_a_reset_card() {
    let snapshot = codex::normalize(&codex_payload(), None, 0).unwrap();
    assert_eq!(snapshot.reset_cards.len(), 1);
    assert_eq!(
        snapshot.reset_cards[0].id,
        "RateLimitResetCredit_ddfd18528a9c81919e58e5dc00d3e8b8"
    );
    assert_eq!(snapshot.reset_cards[0].title.as_deref(), Some("Full reset"));
    assert_eq!(
        snapshot.reset_cards[0].expires_at_ms,
        Some(1_791_079_933_000)
    );
}

#[test]
fn a_credit_balance_arrives_as_a_string_and_is_read_as_a_number() {
    let snapshot = codex::normalize(&codex_payload(), None, 0).unwrap();
    assert_eq!(
        snapshot.credits,
        Some(UsageCredits {
            balance: 0.0,
            unlimited: false
        })
    );
}

#[test]
fn identity_prefers_the_account_uuid_and_falls_back_to_the_plan() {
    let credentials = json!({ "claudeAiOauth": { "subscriptionType": "max" } });
    let account =
        json!({ "oauthAccount": { "accountUuid": "cacffe8a-2633-47ac-9d98-2a08aec23c76" } });
    assert_eq!(
        anthropic::identity(&credentials, Some(&account)).as_deref(),
        Some("cacffe8a-2633-47ac-9d98-2a08aec23c76")
    );
    assert_eq!(
        anthropic::identity(&credentials, None).as_deref(),
        Some("max")
    );
    assert_eq!(
        codex::identity(&codex_payload()).as_deref(),
        Some("a412213a-40e4-4367-b303-0d36a4fe7543")
    );
}

#[test]
fn redact_removes_the_four_shapes_the_credential_files_actually_carry() {
    assert_eq!(
        redact("Authorization: Bearer sk-ant-oat01-AbCdEfGhIjKlMnOpQrSt failed"),
        "Authorization: Bearer <redacted> failed"
    );
    assert_eq!(
        redact("token eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9 expired"),
        "token <redacted> expired"
    );
    assert_eq!(
        redact("account user@example.com is locked"),
        "account <redacted> is locked"
    );
    assert_eq!(redact("sk-proj-0123456789abcdef"), "<redacted>");
}

#[test]
fn redact_leaves_a_message_that_carries_no_secret_untouched() {
    let message = "the codex app-server answered without rateLimits";
    assert_eq!(redact(message), message);
    assert_eq!(redact("http 429 rate limited"), "http 429 rate limited");
}

#[test]
fn the_threshold_fires_on_the_crossing_and_stays_quiet_on_the_level() {
    let mut watch = ThresholdWatch::new();
    assert!(!watch.observe(PROVIDER_ANTHROPIC, 50.0, 90.0));
    assert!(watch.observe(PROVIDER_ANTHROPIC, 91.0, 90.0));
    assert!(!watch.observe(PROVIDER_ANTHROPIC, 92.0, 90.0));
    assert!(!watch.observe(PROVIDER_ANTHROPIC, 99.0, 90.0));
    assert!(!watch.observe(PROVIDER_ANTHROPIC, 4.0, 90.0));
    assert!(watch.observe(PROVIDER_ANTHROPIC, 90.0, 90.0));
}

#[test]
fn a_provider_that_is_already_over_the_threshold_when_first_seen_fires_once() {
    let mut watch = ThresholdWatch::new();
    assert!(watch.observe(PROVIDER_CODEX, 95.0, 90.0));
    assert!(!watch.observe(PROVIDER_CODEX, 95.0, 90.0));
    watch.forget(PROVIDER_CODEX);
    assert!(watch.observe(PROVIDER_CODEX, 95.0, 90.0));
}

#[test]
fn the_two_providers_cross_independently() {
    let mut watch = ThresholdWatch::new();
    assert!(watch.observe(PROVIDER_ANTHROPIC, 91.0, 90.0));
    assert!(!watch.observe(PROVIDER_ANTHROPIC, 91.0, 90.0));
    assert!(watch.observe(PROVIDER_CODEX, 91.0, 90.0));
}

#[test]
fn peak_percent_reads_the_models_as_well_as_the_windows() {
    let snapshot = anthropic::normalize(&anthropic_payload(), None, 0).unwrap();
    assert_eq!(snapshot.peak_percent(), 60.0);
}

#[test]
fn a_snapshot_is_stale_once_it_is_older_than_the_window() {
    let snapshot = codex::normalize(&codex_payload(), None, 1_000_000).unwrap();
    assert!(!snapshot.is_stale(1_000_000));
    assert!(!snapshot.is_stale(1_000_000 + USAGE_STALE_AFTER.as_millis() as u64));
    assert!(snapshot.is_stale(1_000_001 + USAGE_STALE_AFTER.as_millis() as u64));
}

#[test]
fn rfc3339_reads_the_shape_the_endpoint_emits() {
    assert_eq!(
        rfc3339_to_ms("2026-09-07T07:00:00.289344+00:00"),
        Some(1_788_764_400_000)
    );
    assert_eq!(
        rfc3339_to_ms("2026-09-07T07:00:00Z"),
        Some(1_788_764_400_000)
    );
    assert_eq!(
        rfc3339_to_ms("2026-09-07T04:00:00-03:00"),
        Some(1_788_764_400_000)
    );
    assert_eq!(rfc3339_to_ms("1970-01-01T00:00:00Z"), Some(0));
}

#[test]
fn rfc3339_refuses_anything_it_does_not_understand_instead_of_guessing() {
    assert_eq!(rfc3339_to_ms(""), None);
    assert_eq!(rfc3339_to_ms("not a date"), None);
    assert_eq!(rfc3339_to_ms("2026-13-07T07:00:00Z"), None);
    assert_eq!(rfc3339_to_ms("2026-09-07 07:00"), None);
    assert_eq!(rfc3339_to_ms("1969-12-31T23:59:59Z"), None);
}

#[test]
fn window_label_names_the_durations_the_two_providers_report() {
    assert_eq!(window_label(300), "5h");
    assert_eq!(window_label(10080), "7d");
    assert_eq!(window_label(1440), "1d");
    assert_eq!(window_label(90), "90m");
    assert_eq!(window_label(0), "");
}

#[test]
fn a_report_answers_by_provider_name() {
    let snapshot = codex::normalize(&codex_payload(), None, 0).unwrap();
    let report = UsageReport {
        providers: vec![ProviderUsage {
            provider: PROVIDER_CODEX.to_owned(),
            detected: true,
            snapshot: Some(snapshot),
            error: None,
            checked_at_ms: 0,
        }],
    };
    assert!(report.snapshot(PROVIDER_CODEX).is_some());
    assert!(report.snapshot(PROVIDER_ANTHROPIC).is_none());
    assert!(report.provider(PROVIDER_ANTHROPIC).is_none());
}

#[test]
fn a_snapshot_survives_the_json_round_trip_the_cache_file_uses() {
    let snapshot = codex::normalize(&codex_payload(), Some("account".to_owned()), 42).unwrap();
    let text = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(
        serde_json::from_str::<UsageSnapshot>(&text).unwrap(),
        snapshot
    );
}
