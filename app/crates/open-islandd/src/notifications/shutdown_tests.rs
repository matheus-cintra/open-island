//! Unit tests for the cooperative shutdown primitive.

use super::{wait_for_interval, BoundedThread, ShutdownFlag, SignalRegistrations};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Given one flag and a clone of it, when the original requests shutdown,
/// then both clones observe one requested state.
#[test]
fn clones_observe_one_requested_state() {
    let flag = ShutdownFlag::new();
    let peer = flag.clone();
    assert!(!flag.is_requested());
    assert!(!peer.is_requested());

    flag.request();

    assert!(flag.is_requested());
    assert!(peer.is_requested());
}

/// Given one flag, when SIGTERM and SIGINT are registered against it and the
/// guard is dropped, then the guard held both signal actions and unregistered
/// them without requesting the flag.
#[test]
fn registrations_own_both_signal_actions_and_unregister_on_drop() {
    let flag = ShutdownFlag::new();
    let shared = flag.share();
    let registrations = SignalRegistrations::register(&flag).expect("register SIGTERM and SIGINT");
    assert!(
        Arc::strong_count(&shared) >= 4,
        "both signal actions must hold the shared flag"
    );

    drop(registrations);

    assert_eq!(
        Arc::strong_count(&shared),
        2,
        "dropping the guard must unregister both signal actions"
    );
    assert!(!flag.is_requested());
}

/// Given a flag already requested, when a long interval is waited upon, then
/// the wait returns false immediately without sleeping the interval.
#[test]
fn requested_flag_aborts_long_interval_immediately() {
    let flag = ShutdownFlag::new();
    flag.request();
    let started = Instant::now();

    assert!(!wait_for_interval(&flag, Duration::from_secs(60)));

    assert!(
        started.elapsed() < Duration::from_millis(100),
        "a requested flag must abort the interval before sleeping"
    );
}

/// Given an unrequested flag, when a short interval is waited upon, then the
/// full interval elapses and the wait returns true.
#[test]
fn short_interval_elapses_when_not_requested() {
    let flag = ShutdownFlag::new();
    let started = Instant::now();

    assert!(wait_for_interval(&flag, Duration::from_millis(60)));

    assert!(
        started.elapsed() >= Duration::from_millis(55),
        "the interval must genuinely elapse"
    );
}

/// Given a thread whose body finishes immediately, when it is joined within a
/// deadline, then the join completes without waiting.
#[test]
fn finished_thread_joins() {
    let mut owned = BoundedThread::spawn("bounded-join", || {}).expect("spawn thread");

    assert!(owned.join_with_deadline(Duration::from_secs(1)));
}

/// Given a thread gated on an external event, when joined with a short
/// deadline, then the join times out and detaches; after the gate is released
/// the detached body is observed finishing under a bounded wait.
#[test]
fn gated_thread_detaches_then_is_observed_finished() {
    let (gate_tx, gate_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let mut owned = BoundedThread::spawn("bounded-detach", move || {
        let _ = gate_rx.recv();
        let _ = finished_tx.send(());
    })
    .expect("spawn thread");

    let started = Instant::now();
    assert!(!owned.join_with_deadline(Duration::from_millis(300)));
    assert!(
        started.elapsed() < Duration::from_millis(600),
        "a gated thread must time out near the deadline"
    );

    gate_tx.send(()).expect("release gate");
    assert!(
        finished_rx.recv_timeout(Duration::from_secs(1)).is_ok(),
        "the detached body must finish after release"
    );
}
