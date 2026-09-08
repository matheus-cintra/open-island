use super::*;

fn admits_rules(rules: &[SilenceRule], cwd: &str, prompt: Option<&str>) -> bool {
    admits(rules, &[], Subject::new(cwd, prompt, None))
}

fn launcher(app_id: &str, enabled: bool) -> LauncherRule {
    LauncherRule {
        app_id: app_id.to_owned(),
        name: app_id.to_owned(),
        enabled,
    }
}

fn rule(field: RuleField, match_type: MatchType, pattern: &str) -> SilenceRule {
    SilenceRule {
        field,
        match_type,
        pattern: pattern.to_owned(),
        name: "test".to_owned(),
        built_in: false,
        enabled: true,
    }
}

#[test]
fn no_rules_admits_everything() {
    assert!(admits_rules(
        &[],
        "/home/me/project",
        Some("build the thing")
    ));
    assert!(admits_rules(&[], "", None));
}

#[test]
fn a_cwd_rule_matches_anywhere_in_the_path() {
    let rules = [rule(
        RuleField::Cwd,
        MatchType::Contains,
        "/.codex/memories",
    )];
    assert!(!admits_rules(
        &rules,
        "/home/me/.codex/memories/today",
        Some("anything")
    ));
    assert!(admits_rules(&rules, "/home/me/project", Some("anything")));
}

#[test]
fn a_prompt_rule_matches_the_start_only() {
    let rules = [rule(
        RuleField::Prompt,
        MatchType::Prefix,
        "## Memory Writing Agent",
    )];
    assert!(!admits_rules(
        &rules,
        "/home/me/project",
        Some("## Memory Writing Agent\nrecord this")
    ));
    assert!(admits_rules(
        &rules,
        "/home/me/project",
        Some("please use the ## Memory Writing Agent format")
    ));
}

#[test]
fn a_prompt_rule_never_matches_a_session_that_has_no_prompt() {
    let rules = [rule(RuleField::Prompt, MatchType::Contains, "anything")];
    assert!(admits_rules(&rules, "/home/me/project", None));
}

#[test]
fn a_disabled_rule_matches_nothing() {
    let mut disabled = rule(RuleField::Cwd, MatchType::Contains, "/.codex/memories");
    disabled.enabled = false;
    assert!(admits_rules(&[disabled], "/home/me/.codex/memories", None));
}

#[test]
fn an_empty_pattern_matches_nothing() {
    let empty = rule(RuleField::Cwd, MatchType::Contains, "");
    assert!(admits_rules(&[empty], "/home/me/project", Some("anything")));
}

#[test]
fn equals_is_the_whole_string_and_prefix_is_not() {
    let exact = [rule(RuleField::Cwd, MatchType::Equals, "/tmp")];
    assert!(!admits_rules(&exact, "/tmp", None));
    assert!(admits_rules(&exact, "/tmp/nested", None));

    let prefix = [rule(RuleField::Cwd, MatchType::Prefix, "/tmp")];
    assert!(!admits_rules(&prefix, "/tmp/nested", None));
}

#[test]
fn one_matching_rule_among_many_is_enough_to_reject() {
    let rules = [
        rule(RuleField::Cwd, MatchType::Contains, "/never-here"),
        rule(RuleField::Prompt, MatchType::Prefix, "## Memory"),
        rule(RuleField::Cwd, MatchType::Contains, "/nor-here"),
    ];
    assert!(!admits_rules(
        &rules,
        "/home/me/project",
        Some("## Memory Writing Agent")
    ));
}

#[test]
fn every_match_type_survives_a_round_trip_through_its_string() {
    for value in [MatchType::Contains, MatchType::Prefix, MatchType::Equals] {
        assert_eq!(MatchType::parse(value.as_str()), Some(value));
    }
    assert_eq!(MatchType::parse("nonsense"), None);
}

#[test]
fn every_rule_field_survives_a_round_trip_through_its_string() {
    for value in [RuleField::Cwd, RuleField::Prompt] {
        assert_eq!(RuleField::parse(value.as_str()), Some(value));
    }
    assert_eq!(RuleField::parse("nonsense"), None);
}

#[test]
fn the_built_ins_hide_memory_dirs_and_memory_writing_prompts() {
    let rules = built_in_rules();
    assert!(rules.iter().all(|rule| rule.built_in && rule.enabled));
    assert!(!admits_rules(&rules, "/home/me/.codex/memories", None));
    assert!(!admits_rules(&rules, "/home/me/.claude-mem/run", None));
    assert!(!admits_rules(
        &rules,
        "/home/me/project",
        Some("## Memory Writing Agent")
    ));
    assert!(admits_rules(
        &rules,
        "/home/me/project",
        Some("refactor the parser")
    ));
}

#[test]
fn a_launcher_rule_matches_the_whole_id_and_nothing_else() {
    let blocked = [launcher("kitty", true)];
    assert!(!admits(
        &[],
        &blocked,
        Subject::new("/home/me/project", None, Some("kitty"))
    ));
    assert!(admits(
        &[],
        &blocked,
        Subject::new("/home/me/project", None, Some("kitty-wrapper"))
    ));
    assert!(admits(
        &[],
        &blocked,
        Subject::new("/home/me/project", None, Some("alacritty"))
    ));
}

#[test]
fn a_session_with_no_launcher_is_never_blocked_by_a_launcher_rule() {
    let blocked = [launcher("kitty", true)];
    assert!(admits(&[], &blocked, Subject::new("/home/me", None, None)));
}

#[test]
fn a_disabled_launcher_rule_admits() {
    let blocked = [launcher("kitty", false)];
    assert!(admits(
        &[],
        &blocked,
        Subject::new("/home/me", None, Some("kitty"))
    ));
}

#[test]
fn a_launcher_rule_and_a_cwd_rule_both_reject_on_their_own() {
    let rules = [rule(
        RuleField::Cwd,
        MatchType::Contains,
        "/.codex/memories",
    )];
    let blocked = [launcher("Hyprland", true)];
    assert!(!admits(
        &rules,
        &blocked,
        Subject::new("/home/me/.codex/memories", None, Some("kitty"))
    ));
    assert!(!admits(
        &rules,
        &blocked,
        Subject::new("/home/me/project", None, Some("Hyprland"))
    ));
    assert!(admits(
        &rules,
        &blocked,
        Subject::new("/home/me/project", None, Some("kitty"))
    ));
}
