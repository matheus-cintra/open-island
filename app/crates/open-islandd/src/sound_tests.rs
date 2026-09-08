use super::*;
use open_island_core::config::{Config, SoundEvents};

fn sound() -> SoundConfig {
    SoundConfig::default()
}

fn config_with_sound(sound: SoundConfig) -> Config {
    Config {
        sound,
        ..Config::default()
    }
}

#[test]
fn a_disabled_or_quiet_player_asks_for_nothing() {
    let mut config = sound();
    config.enabled = false;
    assert_eq!(requested_sound(&config, SoundEvent::TaskComplete), None);

    let mut config = sound();
    config.quiet = true;
    assert_eq!(requested_sound(&config, SoundEvent::TaskComplete), None);
}

#[test]
fn an_event_with_no_configured_file_asks_for_nothing() {
    let config = sound();
    assert_eq!(requested_sound(&config, SoundEvent::TaskAcknowledge), None);
    assert!(requested_sound(&config, SoundEvent::TaskComplete).is_some());
}

#[test]
fn an_event_whose_file_was_turned_off_asks_for_nothing() {
    let config = SoundConfig {
        events: SoundEvents {
            task_complete: None,
            ..SoundEvents::default()
        },
        ..sound()
    };
    assert_eq!(requested_sound(&config, SoundEvent::TaskComplete), None);
}

#[test]
fn do_not_disturb_silences_the_player_only_while_it_is_followed() {
    let config = sound();
    assert!(config.follow_dnd);
    assert!(!admits(&config, true, false, 0, 0));
    assert!(admits(&config, false, false, 0, 0));

    let config = SoundConfig {
        follow_dnd: false,
        ..sound()
    };
    assert!(admits(&config, true, false, 0, 0));
}

#[test]
fn a_quiet_scene_silences_the_player_whatever_the_sound_section_says() {
    let config = SoundConfig {
        follow_dnd: false,
        quiet_hours: false,
        ..sound()
    };
    assert!(admits(&config, false, false, 0, 0));
    assert!(!admits(&config, false, true, 0, 0));
}

#[test]
fn a_burst_is_dropped_at_the_concurrency_cap_and_never_queued_forever() {
    let config = sound();
    assert!(admits(&config, false, false, MAX_CONCURRENT - 1, 0));
    assert!(!admits(&config, false, false, MAX_CONCURRENT, 0));
    assert!(!admits(&config, false, false, MAX_CONCURRENT + 5, 0));
}

#[test]
fn the_player_is_invoked_with_the_volume_and_the_path_and_no_inherited_stdio() {
    let command = command(Path::new("/tmp/complete.oga"), 0.3);
    assert_eq!(command.get_program(), PLAYER);
    let args: Vec<_> = command.get_args().collect();
    assert_eq!(args, ["--volume", "0.300", "/tmp/complete.oga"]);
}

#[test]
fn a_disabled_player_still_starts_its_thread_so_a_reload_can_turn_sound_back_on() {
    let config = ConfigHandle::new(config_with_sound(SoundConfig {
        enabled: false,
        ..sound()
    }));
    let (player, thread) = SoundPlayer::start(config.clone());

    assert!(
        thread.is_some(),
        "without a thread a reload has nothing to send to"
    );
    player.play(SoundEvent::TaskComplete);

    config.set(config_with_sound(sound()));
    assert!(requested_sound(&config.get().sound, SoundEvent::TaskComplete).is_some());

    player.shutdown();
    if let Some(mut thread) = thread {
        let _ = thread.join_with_deadline(Duration::from_millis(500));
    }
}

fn quiet_hours(start: u32, end: u32) -> SoundConfig {
    let mut config = SoundConfig {
        quiet_hours: true,
        quiet_hours_start: start,
        quiet_hours_end: end,
        ..SoundConfig::default()
    };
    config.follow_dnd = false;
    config
}

#[test]
fn a_window_inside_one_day_silences_only_its_own_hours() {
    let config = quiet_hours(13 * 60, 14 * 60);
    assert!(!in_quiet_hours(&config, 12 * 60 + 59));
    assert!(in_quiet_hours(&config, 13 * 60));
    assert!(in_quiet_hours(&config, 13 * 60 + 59));
    assert!(!in_quiet_hours(&config, 14 * 60));
}

#[test]
fn a_window_that_ends_before_it_starts_wraps_past_midnight() {
    let config = quiet_hours(22 * 60, 8 * 60);
    assert!(in_quiet_hours(&config, 23 * 60));
    assert!(in_quiet_hours(&config, 0));
    assert!(in_quiet_hours(&config, 7 * 60 + 59));
    assert!(!in_quiet_hours(&config, 8 * 60));
    assert!(!in_quiet_hours(&config, 12 * 60));
    assert!(in_quiet_hours(&config, 22 * 60));
}

#[test]
fn quiet_hours_do_nothing_while_the_switch_is_off() {
    let mut config = quiet_hours(22 * 60, 8 * 60);
    config.quiet_hours = false;
    assert!(!in_quiet_hours(&config, 23 * 60));
}

#[test]
fn an_empty_window_silences_nothing() {
    let config = quiet_hours(9 * 60, 9 * 60);
    for minute in [0, 9 * 60, 12 * 60, 23 * 60] {
        assert!(!in_quiet_hours(&config, minute));
    }
}

#[test]
fn quiet_hours_close_the_gate_that_dnd_and_the_cap_share() {
    let config = quiet_hours(22 * 60, 8 * 60);
    assert!(!admits(&config, false, false, 0, 23 * 60));
    assert!(admits(&config, false, false, 0, 12 * 60));
}
