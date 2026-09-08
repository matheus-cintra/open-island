use super::*;

struct Fake {
    dnd: bool,
    dark: bool,
}

impl SceneSource for Fake {
    fn do_not_disturb(&mut self) -> bool {
        self.dnd
    }

    fn screen_off(&mut self) -> bool {
        self.dark
    }
}

fn filters(focus_mode: bool, screen_off: bool) -> FiltersConfig {
    FiltersConfig {
        quiet_focus_mode: focus_mode,
        quiet_screen_off: screen_off,
        ..FiltersConfig::default()
    }
}

#[test]
fn a_scene_only_counts_while_its_own_switch_is_on() {
    let mut source = Fake {
        dnd: true,
        dark: true,
    };
    assert_eq!(
        evaluate(&filters(false, false), &mut source),
        Scenes::default()
    );
    assert!(!evaluate(&filters(false, false), &mut source).active());
    assert!(evaluate(&filters(true, false), &mut source).focus_mode);
    assert!(!evaluate(&filters(true, false), &mut source).screen_off);
    assert!(evaluate(&filters(false, true), &mut source).screen_off);
}

#[test]
fn a_switch_that_is_on_over_a_condition_that_is_off_is_not_a_scene() {
    let mut source = Fake {
        dnd: false,
        dark: false,
    };
    assert!(!evaluate(&filters(true, true), &mut source).active());
}

#[test]
fn every_enabled_monitor_has_to_be_dark() {
    let both_off = r#"[{"name":"DP-1","dpmsStatus":false},{"name":"HDMI-A-1","dpmsStatus":false}]"#;
    let one_on = r#"[{"name":"DP-1","dpmsStatus":false},{"name":"HDMI-A-1","dpmsStatus":true}]"#;
    assert!(monitors_dark(Some(both_off.to_owned())));
    assert!(!monitors_dark(Some(one_on.to_owned())));
}

#[test]
fn a_disabled_monitor_is_not_a_dark_one() {
    let only_disabled = r#"[{"name":"DP-1","dpmsStatus":true,"disabled":true}]"#;
    let live_beside_it = r#"[{"name":"DP-1","dpmsStatus":true,"disabled":true},{"name":"HDMI-A-1","dpmsStatus":false}]"#;
    assert!(!monitors_dark(Some(only_disabled.to_owned())));
    assert!(monitors_dark(Some(live_beside_it.to_owned())));
}

#[test]
fn no_compositor_and_no_monitors_are_both_awake() {
    assert!(!monitors_dark(None));
    assert!(!monitors_dark(Some("[]".to_owned())));
    assert!(!monitors_dark(Some("not json".to_owned())));
}

#[test]
fn either_logind_hint_counts_and_a_missing_reply_does_not() {
    assert!(session_locked(Some(
        "LockedHint=yes\nIdleHint=no\n".to_owned()
    )));
    assert!(session_locked(Some(
        "LockedHint=no\nIdleHint=yes\n".to_owned()
    )));
    assert!(!session_locked(Some(
        "LockedHint=no\nIdleHint=no\n".to_owned()
    )));
    assert!(!session_locked(None));
}
