use super::*;
use open_island_core::config::SoundConfig;
use std::thread;

/// `quiet` is only a marker here: these tests are about the handle swapping a value, not
/// about what the value means.
fn config_with_quiet(quiet: bool) -> Config {
    Config {
        sound: SoundConfig {
            quiet,
            ..SoundConfig::default()
        },
        ..Config::default()
    }
}

#[test]
fn a_fresh_handle_hands_out_the_config_it_was_built_with() {
    let handle = ConfigHandle::new(config_with_quiet(true));

    assert!(handle.get().sound.quiet);
}

#[test]
fn a_clone_of_the_handle_sees_a_value_the_original_set() {
    let handle = ConfigHandle::default();
    let clone = handle.clone();

    handle.set(config_with_quiet(true));

    assert!(clone.get().sound.quiet);
}

#[test]
fn a_snapshot_taken_before_a_swap_keeps_the_value_it_was_read_with() {
    let handle = ConfigHandle::default();
    let before = handle.get();

    handle.set(config_with_quiet(true));

    assert!(!before.sound.quiet);
    assert!(handle.get().sound.quiet);
}

#[test]
fn readers_on_other_threads_see_the_swap() {
    let handle = ConfigHandle::default();
    let reader = handle.clone();
    handle.set(config_with_quiet(true));

    let seen = thread::spawn(move || reader.get().sound.quiet)
        .join()
        .expect("reader thread");

    assert!(seen);
}
