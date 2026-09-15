use crate::message_delivery::*;

#[test]
fn delivery_queue_transitions_and_stale_attempt() {
    let mut store = DeliveryStore::default();
    let first = store
        .admit("one", "preserve".into(), 0, true, None, None)
        .unwrap();
    store
        .admit("one", "next".into(), 1, true, None, None)
        .unwrap();
    let attempt = store.reserve("one").unwrap().unwrap().attempt_id.unwrap();
    assert!(store.reserve("one").unwrap().is_none());
    assert_eq!(
        store.cancel("one", first.message_id),
        Err("delivery_in_progress")
    );
    assert!(!store.settle(
        first.message_id,
        attempt + 1,
        DeliveryState::Delivered,
        None,
        2
    ));
    assert!(store.settle(
        first.message_id,
        attempt,
        DeliveryState::Unconfirmed,
        Some("ack_lost".into()),
        2
    ));
    assert_eq!(store.snapshot()[0].text, "preserve");
    assert!(!store.settle(first.message_id, attempt, DeliveryState::Delivered, None, 3));
    assert!(store.reserve("one").unwrap().is_none());
    store.observe(&[("one", false)], 3);
    let next = store.reserve("one").unwrap().unwrap();
    assert!(store.settle(
        next.message_id,
        next.attempt_id.unwrap(),
        DeliveryState::Delivered,
        None,
        4
    ));
    assert_eq!(store.snapshot()[1].text, "");
    assert_eq!(store.cancel("one", first.message_id), Ok(true));
}
#[test]
fn delivery_queue_capacity_preserves_first_entries() {
    let mut store = DeliveryStore::default();
    for i in 0..32 {
        store
            .admit("one", i.to_string(), i, false, None, None)
            .unwrap();
    }
    assert_eq!(
        store.admit("one", "overflow".into(), 33, false, None, None),
        Err("queue_full")
    );
    assert_eq!(store.snapshot().len(), 32);
    assert_eq!(store.snapshot()[0].text, "0");
    let mut store = DeliveryStore::default();
    for i in 0..64 {
        store
            .admit(
                if i < 32 { "one" } else { "two" },
                "\\".repeat(65536),
                i,
                false,
                None,
                None,
            )
            .unwrap();
    }
    assert_eq!(
        store.admit("three", "overflow".into(), 65, false, None, None),
        Err("queue_full")
    );
    assert_eq!(
        store.snapshot().iter().map(|r| r.text.len()).sum::<usize>(),
        MAX_LIVE_BYTES
    );
}
#[test]
fn delivery_queue_orphan_retention_and_legacy_projection() {
    let mut store = DeliveryStore::default();
    let message = store
        .admit("one", "recover".into(), 0, true, None, None)
        .unwrap();
    assert_eq!(store.queued("one")[0].id, message.message_id);
    let attempt = store.reserve("one").unwrap().unwrap().attempt_id.unwrap();
    assert!(store.queued("one").is_empty());
    store.settle(message.message_id, attempt, DeliveryState::Failed, None, 1);
    store.observe(&[], 2);
    store.observe(&[("one", true)], 10);
    assert!(store.reserve("one").unwrap().is_none());
    store.observe(&[], 600001);
    assert_eq!(store.snapshot()[0].text, "recover");
    store.observe(&[], 600002);
    assert!(store.snapshot().is_empty());
}
#[test]
fn delivery_queue_rollback_restores_armed_and_dedupes_submission() {
    let mut store = DeliveryStore::default();
    let submission = Some(ClientSubmissionId("submission".into()));
    let message = store
        .admit("one", "text".into(), 0, true, None, submission.clone())
        .unwrap();
    assert_eq!(
        store
            .admit("one", "text".into(), 0, true, None, submission)
            .unwrap()
            .message_id,
        message.message_id
    );
    let first = store.reserve("one").unwrap().unwrap();
    assert!(store.rollback(first.message_id, first.attempt_id.unwrap()));
    let second = store.reserve("one").unwrap().unwrap();
    assert_ne!(first.attempt_id, second.attempt_id);
    assert!(!store.rollback(first.message_id, first.attempt_id.unwrap()));
}

#[test]
fn guarded_cancel_preserves_text_for_recycled_identity_and_inflight_attempt() {
    let identity = DeliveryIdentity {
        daemon_epoch: DaemonEpoch("epoch".into()),
        session_instance_id: SessionInstanceId("current".into()),
    };
    let mut store = DeliveryStore::default();
    let admitted = store
        .admit(
            "session",
            "original".into(),
            0,
            true,
            Some(identity.clone()),
            None,
        )
        .unwrap();
    let mut old = identity.clone();
    old.session_instance_id = SessionInstanceId("old".into());
    assert_eq!(
        store.cancel_guarded("session", admitted.message_id, &old),
        Err("stale_session")
    );
    assert_eq!(store.snapshot()[0].text, "original");
    let attempt = store
        .reserve("session")
        .unwrap()
        .unwrap()
        .attempt_id
        .unwrap();
    assert_eq!(
        store.cancel_guarded("session", admitted.message_id, &identity),
        Err("delivery_in_progress")
    );
    assert!(store.settle(
        admitted.message_id,
        attempt,
        DeliveryState::Unconfirmed,
        None,
        1
    ));
    assert!(store
        .cancel_guarded("session", admitted.message_id, &identity)
        .is_ok());
    assert!(store.snapshot().is_empty());
}
