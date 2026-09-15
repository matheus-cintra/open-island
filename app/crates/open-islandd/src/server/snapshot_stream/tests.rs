use super::*;
use open_island_core::{
    message_delivery::{DeliveryState, DeliveryStore},
    snapshot_page::{decode, PAGE_BYTES},
};
use serde_json::{json, Value};
use std::time::Duration;

#[test]
fn snapshot_over_four_mib_and_single_large_field() {
    let pool = SnapshotPool::new().unwrap();
    let original = Arc::new(json!({"tool_input": "á🦀\\".repeat(900_000)}));
    let id = pool.begin(7, original.clone()).unwrap();
    let mut count = 0;
    let result: Value = decode(id, |index| {
        let page = pool.page(7, id, index)?;
        assert!(page.bytes.len() <= PAGE_BYTES);
        assert!(
            serde_json::to_vec(&json!({"v":1,"id":4,"ok":true,"data":page}))
                .unwrap()
                .len()
                <= 256 * 1024
        );
        count += page.bytes.len();
        Ok(page)
    })
    .unwrap();
    assert!(count > 4 * 1024 * 1024);
    assert_eq!(&result, original.as_ref());
    assert_eq!(pool.page(7, id, 0).unwrap_err(), "snapshot_not_found");
}

#[test]
fn escaped_max_queue_preserves_all_admitted_text() {
    let mut store = DeliveryStore::default();
    for i in 0..64 {
        store
            .admit(
                if i < 32 { "a" } else { "b" },
                "\\".repeat(65536),
                1,
                false,
                None,
                None,
            )
            .unwrap();
    }
    assert_eq!(
        store
            .admit("c", "x".into(), 1, false, None, None)
            .unwrap_err(),
        "queue_full"
    );
    let original = Arc::new(store.snapshot());
    let pool = SnapshotPool::new().unwrap();
    let id = pool.begin(1, original.clone()).unwrap();
    let actual: Vec<open_island_core::message_delivery::MessageDelivery> =
        decode(id, |i| pool.page(1, id, i)).unwrap();
    assert_eq!(&actual, original.as_ref());
    assert!(actual
        .iter()
        .all(|m| m.state == DeliveryState::Queued && m.text.len() == 65536));
}

#[test]
fn tokens_are_owned_bounded_and_replacement_releases_old_producer() {
    let pool = SnapshotPool::new().unwrap();
    let doc = Arc::new(json!({"large":"x".repeat(200_000)}));
    let first = pool.begin(1, doc.clone()).unwrap();
    for connection in 2..=4 {
        pool.begin(connection, doc.clone()).unwrap();
    }
    assert_eq!(pool.begin(5, doc.clone()).unwrap_err(), "snapshot_busy");
    assert_eq!(pool.page(2, first, 0).unwrap_err(), "snapshot_not_found");
    assert_eq!(
        pool.page(1, first, 1).unwrap_err(),
        "snapshot_out_of_sequence"
    );
    assert_eq!(pool.page(1, first, 0).unwrap().page_index, 0);
    let second = pool.begin(1, doc.clone()).unwrap();
    assert_ne!(first, second);
    assert_eq!(pool.page(1, first, 1).unwrap_err(), "snapshot_not_found");
    let result: Value = decode(second, |i| pool.page(1, second, i)).unwrap();
    assert_eq!(&result, doc.as_ref());
    for c in 2..=4 {
        pool.cancel(c);
    }
}

#[test]
fn idle_deadline_renews_only_on_consumed_page() {
    let pool = SnapshotPool::new().unwrap();
    let doc = Arc::new(json!({"large":"x".repeat(200_000)}));
    let id = pool.begin(1, doc.clone()).unwrap();
    let slot = pool.tokens.lock().unwrap().connections[&1].clone();
    slot.age(Duration::from_secs(29));
    pool.page(1, id, 0).unwrap();
    assert!(!slot.expired(Instant::now() + Duration::from_secs(29)));
    slot.age(Duration::from_secs(31));
    assert_eq!(pool.page(1, id, 1).unwrap_err(), "snapshot_expired");
    assert_eq!(pool.page(1, id, 1).unwrap_err(), "snapshot_not_found");
    pool.begin(1, doc).unwrap();
}

#[test]
fn parser_rejects_dropped_page_wrong_total_and_trailing_json() {
    for corruption in 0..4 {
        let pool = SnapshotPool::new().unwrap();
        let id = pool
            .begin(1, Arc::new(json!({"text":"x".repeat(100_000)})))
            .unwrap();
        let result: Result<Value, _> = decode(id, |index| {
            if corruption == 0 && index == 1 {
                return Err("connection_lost".into());
            }
            let mut page = pool.page(1, id, index)?;
            if corruption == 1 {
                page.page_index += 1;
            }
            if page.end {
                if corruption == 2 {
                    page.total_bytes = Some(0);
                }
                if corruption == 3 {
                    page.bytes.extend_from_slice(b"{}");
                    page.total_bytes = page.total_bytes.map(|total| total + 2);
                }
            }
            Ok(page)
        });
        assert!(result.is_err(), "corruption {corruption}");
        pool.cancel(1);
    }
}
