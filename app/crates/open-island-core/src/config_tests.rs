use super::*;

#[test]
fn an_absent_or_empty_document_is_the_shipped_default() {
    assert_eq!(Config::from_json_str(""), Config::default());
    assert_eq!(Config::from_json_str("{}"), Config::default());
    assert_eq!(Config::from_json_str("null"), Config::default());
    assert_eq!(Config::from_json_str("[1, 2, 3]"), Config::default());
    assert_eq!(Config::from_json_str("{ not json"), Config::default());
}

#[test]
fn every_default_sound_points_at_the_installed_theme() {
    let events = SoundEvents::default();
    assert_eq!(
        events.session_start,
        Some(PathBuf::from(
            "/usr/share/sounds/freedesktop/stereo/device-added.oga"
        ))
    );
    assert_eq!(
        events.task_complete,
        Some(PathBuf::from(
            "/usr/share/sounds/freedesktop/stereo/complete.oga"
        ))
    );
    assert_eq!(
        events.approval_needed,
        Some(PathBuf::from(
            "/usr/share/sounds/freedesktop/stereo/message.oga"
        ))
    );
    assert_eq!(events.task_acknowledge, None);
    assert_eq!(
        events.idle_reminder,
        Some(PathBuf::from(
            "/usr/share/sounds/freedesktop/stereo/dialog-warning.oga"
        ))
    );
    assert_eq!(
        events.context_limit,
        Some(PathBuf::from(
            "/usr/share/sounds/freedesktop/stereo/suspend-error.oga"
        ))
    );
}

#[test]
fn usage_defaults_ship_the_login_and_limit_switches_on() {
    let usage = UsageConfig::default();
    assert!(usage.show_limits);
    assert!(
        usage.use_claude_login,
        "the claude login switch defaults on"
    );
    assert!(usage.show_reset_cards);
    assert_eq!(usage.value_mode, UsageValueMode::Used);
    assert_eq!(usage.preferred_provider, UsageProviderChoice::Auto);
    assert_eq!(usage.codex_credit_display, CodexCreditDisplay::Credits);
    assert_eq!(usage.warn_threshold, DEFAULT_USAGE_WARN_THRESHOLD);
    assert_eq!(usage.refresh_interval, DEFAULT_USAGE_REFRESH);
}

#[test]
fn an_unknown_choice_falls_back_instead_of_dropping_the_setting() {
    let usage = |document: &str| Config::from_json_str(document).usage;
    assert_eq!(
        usage(r#"{"usage": {"value_mode": "sideways"}}"#).value_mode,
        UsageValueMode::Used
    );
    assert_eq!(
        usage(r#"{"usage": {"preferred_provider": 7}}"#).preferred_provider,
        UsageProviderChoice::Auto
    );
    assert_eq!(
        usage(r#"{"usage": {"codex_credit_display": null}}"#).codex_credit_display,
        CodexCreditDisplay::Credits
    );
    assert_eq!(
        usage(r#"{"usage": {"value_mode": "remaining"}}"#).value_mode,
        UsageValueMode::Remaining
    );
}

#[test]
fn the_warn_threshold_is_clamped_and_the_refresh_never_goes_under_a_minute() {
    let usage = |document: &str| Config::from_json_str(document).usage;
    assert_eq!(
        usage(r#"{"usage": {"warn_threshold": 0}}"#).warn_threshold,
        50.0
    );
    assert_eq!(
        usage(r#"{"usage": {"warn_threshold": 400}}"#).warn_threshold,
        100.0
    );
    assert_eq!(
        usage(r#"{"usage": {"warn_threshold": 80.5}}"#).warn_threshold,
        80.5
    );
    assert_eq!(
        usage(r#"{"usage": {"refresh_interval_ms": 1000}}"#).refresh_interval,
        MIN_USAGE_REFRESH
    );
    assert_eq!(
        usage(r#"{"usage": {"refresh_interval_ms": 900000}}"#).refresh_interval,
        Duration::from_secs(900)
    );
}

#[test]
fn turning_a_usage_switch_off_survives_a_save_fired_by_another_control() {
    let mut config = Config::default();
    config.usage.use_claude_login = false;
    config.usage.show_reset_cards = false;
    let written = config.to_json_value().to_string();
    let read_back = Config::from_json_str(&written);
    assert!(!read_back.usage.use_claude_login);
    assert!(!read_back.usage.show_reset_cards);
    assert!(read_back.usage.show_limits);
}

#[test]
fn one_key_of_the_wrong_type_never_discards_the_keys_around_it() {
    let config = Config::from_json_str(
        r#"{
            "notifications": { "idle_reminder_after_ms": "soon" },
            "sound": { "enabled": 3, "quiet": true, "volume": "loud" }
        }"#,
    );
    assert_eq!(
        config.notifications.idle_reminder_after,
        DEFAULT_IDLE_REMINDER_AFTER
    );
    assert!(config.sound.enabled);
    assert!(config.sound.quiet);
    assert_eq!(config.sound.volume, DEFAULT_VOLUME);
}

#[test]
fn a_section_of_the_wrong_shape_falls_back_whole() {
    let config = Config::from_json_str(r#"{"notifications": 7, "sound": ["complete.oga"]}"#);
    assert_eq!(config, Config::default());
}

#[test]
fn unknown_keys_and_unknown_sections_are_ignored() {
    let config = Config::from_json_str(
        r#"{"hotkey": "SUPER+I", "notifications": {"task_error": true}, "sound": {"packs": []}}"#,
    );
    assert_eq!(config, Config::default());
}

#[test]
fn a_missing_sound_keeps_the_default_and_an_explicit_null_turns_it_off() {
    let config = Config::from_json_str(
        r#"{"sound": {"events": {"task_complete": null, "task_acknowledge": "/tmp/ack.oga"}}}"#,
    );
    assert_eq!(config.sound.events.task_complete, None);
    assert_eq!(
        config.sound.events.task_acknowledge,
        Some(PathBuf::from("/tmp/ack.oga"))
    );
    assert_eq!(
        config.sound.events.approval_needed,
        SoundEvents::default().approval_needed
    );
}

#[test]
fn a_blank_sound_path_is_a_mistake_and_not_a_way_to_turn_a_sound_off() {
    let config = Config::from_json_str(r#"{"sound": {"events": {"task_complete": "   "}}}"#);
    assert_eq!(
        config.sound.events.task_complete,
        SoundEvents::default().task_complete
    );
}

#[test]
fn volume_is_clamped_and_a_non_finite_value_is_refused() {
    assert_eq!(
        Config::from_json_str(r#"{"sound": {"volume": 4.2}}"#)
            .sound
            .volume,
        1.0
    );
    assert_eq!(
        Config::from_json_str(r#"{"sound": {"volume": -1}}"#)
            .sound
            .volume,
        0.0
    );
    assert_eq!(
        Config::from_json_str(r#"{"sound": {"volume": 0.75}}"#)
            .sound
            .volume,
        0.75
    );
}

#[test]
fn the_idle_reminder_threshold_is_read_in_milliseconds() {
    let config = Config::from_json_str(r#"{"notifications": {"idle_reminder_after_ms": 45000}}"#);
    assert_eq!(
        config.notifications.idle_reminder_after,
        Duration::from_secs(45)
    );
    assert_eq!(
        Config::from_json_str(r#"{"notifications": {"idle_reminder_after_ms": -1}}"#)
            .notifications
            .idle_reminder_after,
        DEFAULT_IDLE_REMINDER_AFTER
    );
}

#[test]
fn every_sound_event_resolves_through_one_lookup() {
    let events = SoundEvents::default();
    assert_eq!(
        events.path(SoundEvent::TaskComplete),
        events.task_complete.as_ref()
    );
    assert_eq!(events.path(SoundEvent::TaskAcknowledge), None);
    assert_eq!(
        events.path(SoundEvent::IdleReminder),
        events.idle_reminder.as_ref()
    );
}

#[test]
fn the_island_timings_default_when_the_section_is_absent_wrong_or_partly_wrong() {
    assert_eq!(Config::from_json_str("{}").island, IslandConfig::default());
    assert_eq!(
        Config::from_json_str(r#"{"island": 7}"#).island,
        IslandConfig::default()
    );
    assert_eq!(
        Config::from_json_str(r#"{"island": ["250ms"]}"#).island,
        IslandConfig::default()
    );
    let partly_wrong =
        Config::from_json_str(r#"{"island": {"hover_dwell_ms": "soon", "idle_fade_ms": 30000}}"#)
            .island;
    assert_eq!(partly_wrong.hover_dwell, DEFAULT_HOVER_DWELL);
    assert_eq!(partly_wrong.auto_collapse, DEFAULT_AUTO_COLLAPSE);
    assert_eq!(partly_wrong.idle_fade, Duration::from_secs(30));
}

#[test]
fn the_island_timings_mirror_the_constants_the_front_end_ships_with() {
    let island = IslandConfig::default();
    assert_eq!(island.hover_dwell, Duration::from_millis(250));
    assert_eq!(island.auto_collapse, Duration::from_millis(2500));
    assert_eq!(island.idle_fade, Duration::from_millis(120_000));
}

#[test]
fn the_session_idle_threshold_defaults_when_the_section_is_absent_wrong_or_partly_wrong() {
    assert_eq!(
        Config::from_json_str("{}").sessions,
        SessionsConfig::default()
    );
    assert_eq!(
        Config::from_json_str(r#"{"sessions": "ten minutes"}"#).sessions,
        SessionsConfig::default()
    );
    assert_eq!(
        Config::from_json_str(r#"{"sessions": {"idle_after_ms": -1}}"#).sessions,
        SessionsConfig::default()
    );
    assert_eq!(
        Config::from_json_str(r#"{"sessions": {"idle_after_ms": 90000}}"#)
            .sessions
            .idle_after,
        Duration::from_secs(90)
    );
}

#[test]
fn the_session_idle_threshold_defaults_to_the_store_constant() {
    assert_eq!(SessionsConfig::default().idle_after, IDLE_AFTER);
}

fn a_config_with_every_field_moved_off_its_default() -> Config {
    Config {
        notifications: NotificationConfig {
            idle_reminder_after: Duration::from_millis(45_000),
            reminder_needs_response: true,
            reminder_completed_tasks: false,
            expand_on_completion: false,
            expand_on_question: false,
            subagent_timing: SubagentTiming::EveryCompletion,
        },
        sound: SoundConfig {
            enabled: false,
            volume: 0.75,
            quiet: true,
            follow_dnd: false,
            quiet_hours: true,
            quiet_hours_start: 23 * 60 + 15,
            quiet_hours_end: 7 * 60 + 45,
            spam_window: Duration::from_secs(25),
            spam_threshold: 7,
            events: SoundEvents {
                session_start: Some(PathBuf::from("/tmp/start.oga")),
                task_complete: None,
                approval_needed: Some(PathBuf::from("/tmp/approve.oga")),
                task_acknowledge: Some(PathBuf::from("/tmp/ack.oga")),
                idle_reminder: None,
                context_limit: Some(PathBuf::from("/tmp/limit.oga")),
                user_spam: Some(PathBuf::from("/tmp/spam.oga")),
            },
        },
        island: IslandConfig {
            hover_dwell: Duration::from_millis(400),
            auto_collapse: Duration::from_millis(9_000),
            idle_fade: Duration::from_millis(30_000),
            expand_on_hover: false,
            collapse_on_leave: false,
            hide_in_fullscreen: false,
            hide_when_idle: true,
            click_to_jump: false,
            smart_suppression: false,
        },
        sessions: SessionsConfig {
            idle_after: Duration::from_millis(90_000),
            cleanup_after: Duration::from_millis(45 * 60_000),
        },
        display: DisplayConfig {
            compact_layout: CompactLayout::Clean,
            monitor: Some("DP-2".to_owned()),
            notch_width_offset: -6,
            notch_height_offset: 3,
            island_height: 60,
            project: false,
            worktree: false,
            agent_icons: false,
            terminal_icons: false,
            model: false,
            effort: true,
            activity: false,
            subagents: false,
            tasks: false,
            ui_scale: 1.75,
            content_font: 13,
            panel_max_width: 800,
            panel_max_height: 900,
            completion_card_height: 120,
        },
        integrations: IntegrationsConfig {
            auto_configure: false,
            known_agents: vec!["claude".to_owned(), "codex".to_owned()],
        },
        usage: UsageConfig {
            show_limits: false,
            use_claude_login: false,
            value_mode: UsageValueMode::Remaining,
            preferred_provider: UsageProviderChoice::Codex,
            show_reset_cards: false,
            codex_credit_display: CodexCreditDisplay::Dollars,
            warn_threshold: 75.0,
            refresh_interval: Duration::from_secs(600),
        },
        filters: FiltersConfig {
            rules: vec![SilenceRule {
                field: RuleField::Prompt,
                match_type: MatchType::Equals,
                pattern: "/tmp/noise".to_owned(),
                name: "moved off its default".to_owned(),
                built_in: false,
                enabled: false,
            }],
            launchers: vec![LauncherRule {
                app_id: "Hyprland".to_owned(),
                name: "moved off its default".to_owned(),
                enabled: false,
            }],
            quiet_focus_mode: true,
            quiet_screen_off: true,
        },
    }
}

#[test]
fn the_defaults_survive_a_trip_through_json_and_back() {
    let written = Config::default().to_json_value().to_string();
    assert_eq!(Config::from_json_str(&written), Config::default());
}

#[test]
fn a_fully_customised_config_survives_a_trip_through_json_and_back() {
    let custom = a_config_with_every_field_moved_off_its_default();
    assert_ne!(custom, Config::default());
    let written = custom.to_json_value().to_string();
    assert_eq!(Config::from_json_str(&written), custom);
}

#[test]
fn a_sound_turned_off_is_written_as_null_and_not_dropped_from_the_document() {
    let mut config = Config::default();
    config.sound.events.task_complete = None;
    let written = config.to_json_value();
    assert_eq!(
        written["sound"]["events"]
            .as_object()
            .expect("events object")
            .get("task_complete"),
        Some(&Value::Null)
    );
    let read_back = Config::from_json_str(&written.to_string());
    assert_eq!(read_back.sound.events.task_complete, None);
    assert_ne!(
        read_back.sound.events.task_complete,
        SoundEvents::default().task_complete
    );
}

#[test]
fn the_config_path_prefers_the_override_then_xdg_then_home() {
    assert_eq!(
        path_from(
            Some(OsString::from("/tmp/explicit.json")),
            Some(OsString::from("/tmp/xdg")),
            Some(OsString::from("/tmp/home"))
        ),
        Some(PathBuf::from("/tmp/explicit.json"))
    );
    assert_eq!(
        path_from(
            None,
            Some(OsString::from("/tmp/xdg")),
            Some(OsString::from("/tmp/home"))
        ),
        Some(PathBuf::from("/tmp/xdg/open-island/config.json"))
    );
    assert_eq!(
        path_from(None, None, Some(OsString::from("/tmp/home"))),
        Some(PathBuf::from("/tmp/home/.config/open-island/config.json"))
    );
    assert_eq!(path_from(None, None, None), None);
}

#[test]
fn writing_a_config_creates_the_directory_it_lives_in() {
    let root = std::env::temp_dir().join(format!(
        "open-island-config-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&root);
    let path = root.join("open-island").join("config.json");
    let text = Config::default().to_json_value().to_string();

    write_atomic(&path, &text).expect("write");
    assert_eq!(fs::read_to_string(&path).expect("read"), text);

    write_atomic(&path, "{}").expect("overwrite");
    assert_eq!(fs::read_to_string(&path).expect("read"), "{}");
    assert_eq!(
        fs::read_dir(path.parent().expect("parent"))
            .expect("list")
            .count(),
        1
    );

    fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
fn agents_are_configured_without_asking_and_none_are_known_before_the_first_start() {
    let integrations = IntegrationsConfig::default();

    assert!(integrations.auto_configure, "C12 is opt-out, not opt-in");
    assert!(integrations.known_agents.is_empty());
}

#[test]
fn the_known_agents_survive_a_save_triggered_by_an_unrelated_control() {
    let mut config = Config::default();
    config.integrations.known_agents = vec!["codex".to_owned(), "claude".to_owned()];
    config.sound.enabled = false;
    let round_tripped = Config::from_json_str(&config.to_json_value().to_string());

    assert_eq!(
        round_tripped.integrations.known_agents,
        vec!["claude".to_owned(), "codex".to_owned()]
    );
    assert!(round_tripped.integrations.auto_configure);
}

#[test]
fn a_known_agent_list_that_is_not_a_list_of_names_falls_back_to_the_default() {
    let integrations = |document: &str| Config::from_json_str(document).integrations.known_agents;

    assert!(integrations(r#"{"integrations": {"known_agents": "claude"}}"#).is_empty());
    assert!(integrations(r#"{"integrations": {"known_agents": [7, null, "  "]}}"#).is_empty());
    assert_eq!(
        integrations(r#"{"integrations": {"known_agents": ["codex", "codex", "claude"]}}"#),
        vec!["claude".to_owned(), "codex".to_owned()]
    );
}

#[test]
fn the_default_volume_is_written_as_zero_point_three_and_not_as_a_float_artefact() {
    let written = Config::default().to_json_value();

    assert_eq!(written["sound"]["volume"], serde_json::json!(0.3));
    assert_eq!(
        Config::from_json_str(&written.to_string()).sound.volume,
        DEFAULT_VOLUME
    );
}

#[test]
fn the_row_shows_everything_that_has_data_except_the_reasoning_effort() {
    let display = DisplayConfig::default();

    assert!(
        display.project
            && display.worktree
            && display.model
            && display.activity
            && display.subagents
    );
    assert!(!display.effort, "the effort badge defaults off");
}

#[test]
fn the_display_section_survives_a_round_trip_and_a_partial_document() {
    let mut config = Config::default();
    config.display.model = false;
    config.display.effort = true;
    let round_tripped = Config::from_json_str(&config.to_json_value().to_string());

    assert_eq!(round_tripped, config);

    let partial = Config::from_json_str(r#"{"display":{"effort":true}}"#);
    assert!(partial.display.effort);
    assert!(partial.display.model, "an absent key keeps its default");
}

#[test]
fn a_notch_offset_is_signed_and_clamped_to_its_limit() {
    let inside =
        Config::from_json_str(r#"{"display":{"notch_width_offset":-7,"notch_height_offset":5}}"#);
    assert_eq!(inside.display.notch_width_offset, -7);
    assert_eq!(inside.display.notch_height_offset, 5);

    let outside = Config::from_json_str(
        r#"{"display":{"notch_width_offset":-900,"notch_height_offset":900}}"#,
    );
    assert_eq!(outside.display.notch_width_offset, -NOTCH_OFFSET_LIMIT);
    assert_eq!(outside.display.notch_height_offset, NOTCH_OFFSET_LIMIT);

    let absent = Config::from_json_str(r#"{"display":{}}"#);
    assert_eq!(absent.display.notch_width_offset, 0);
    assert_eq!(absent.display.notch_height_offset, 0);
}

#[test]
fn an_absolute_island_height_defaults_to_automatic_and_survives_a_round_trip() {
    assert_eq!(DisplayConfig::default().island_height, 0);
    assert_eq!(
        Config::from_json_str(r#"{"display":{}}"#)
            .display
            .island_height,
        0
    );

    let set = Config::from_json_str(r#"{"display":{"island_height":60}}"#);
    assert_eq!(set.display.island_height, 60);

    let round_tripped = Config::from_json_str(&set.to_json_value().to_string());
    assert_eq!(round_tripped, set);
    assert_eq!(round_tripped.display.island_height, 60);

    assert_eq!(
        Config::from_json_str(r#"{"display":{"island_height":9000}}"#)
            .display
            .island_height,
        MAX_ISLAND_HEIGHT
    );
    assert_eq!(
        Config::from_json_str(r#"{"display":{"island_height":-5}}"#)
            .display
            .island_height,
        0,
        "a negative value is not an unsigned integer and falls back to automatic"
    );
}

#[test]
fn every_pixel_range_is_the_one_its_slider_offers() {
    let display = |document: &str| Config::from_json_str(document).display;
    for (key, low, high) in [
        ("content_font", 9, 16),
        ("panel_max_width", 480, 1000),
        ("panel_max_height", 320, 1200),
        ("completion_card_height", 60, 240),
    ] {
        let read = |value: u32| {
            let parsed = display(&format!(r#"{{"display": {{"{key}": {value}}}}}"#));
            match key {
                "content_font" => parsed.content_font,
                "panel_max_width" => parsed.panel_max_width,
                "panel_max_height" => parsed.panel_max_height,
                _ => parsed.completion_card_height,
            }
        };
        assert_eq!(read(low - 1), low, "{key} floors at its slider's minimum");
        assert_eq!(read(high + 1), high, "{key} caps at its slider's maximum");
        assert_eq!(read(low), low);
        assert_eq!(read(high), high);
    }
}

#[test]
fn the_values_this_machine_already_has_survive_the_narrower_ranges() {
    let display = Config::from_json_str(
        r#"{"display": {"content_font": 11, "panel_max_width": 664,
             "panel_max_height": 720, "completion_card_height": 170}}"#,
    )
    .display;
    assert_eq!(display.content_font, 11);
    assert_eq!(display.panel_max_width, 664);
    assert_eq!(display.panel_max_height, 720);
    assert_eq!(display.completion_card_height, 170);
    assert_eq!(
        Config::from_json_str(r#"{"usage": {"warn_threshold": 90}}"#)
            .usage
            .warn_threshold,
        90.0
    );
}
