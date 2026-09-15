use super::*;
use std::{
    io::{BufRead, BufReader},
    os::unix::net::UnixListener,
    sync::mpsc,
    time::Instant,
};

fn setup() -> (
    tempfile::TempDir,
    UnixListener,
    Transport,
    mpsc::Receiver<Value>,
) {
    let directory = tempfile::Builder::new()
        .prefix("oi-ipc-")
        .tempdir_in("/tmp")
        .unwrap();
    let path = directory.path().join("socket");
    let listener = UnixListener::bind(&path).unwrap();
    let (sender, events) = mpsc::channel();
    let transport = Transport::start(
        path,
        Arc::new(move |_, event| {
            let _ = sender.send(event);
        }),
        Box::new(|| {}),
    )
    .unwrap();
    (directory, listener, transport, events)
}
fn reply(stream: &mut UnixStream, id: &Value) {
    writeln!(stream, "{}", json!({"v":1,"id":id,"ok":true,"data":"ok"})).unwrap();
}
#[test]
fn daemon_transport_parallel_requests() {
    let (_directory, listener, transport, events) = setup();
    let (mut stream, _) = listener.accept().unwrap();
    events.recv_timeout(Duration::from_secs(2)).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    std::thread::scope(|scope| {
        let slow = scope.spawn(|| transport.request("slow", Value::Null));
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let slow_id = serde_json::from_str::<Value>(&line).unwrap()["id"].clone();
        let started = Instant::now();
        let fast = scope.spawn(|| transport.request("ping", Value::Null));
        line.clear();
        reader.read_line(&mut line).unwrap();
        let fast_id = serde_json::from_str::<Value>(&line).unwrap()["id"].clone();
        reply(&mut stream, &fast_id);
        assert_eq!(fast.join().unwrap().unwrap(), "ok");
        assert!(started.elapsed() < Duration::from_millis(500));
        assert!(!slow.is_finished());
        reply(&mut stream, &slow_id);
        assert_eq!(slow.join().unwrap().unwrap(), "ok");
    });
}
#[test]
fn daemon_transport_timeout_cleanup() {
    let (_directory, listener, transport, events) = setup();
    let (stream, _) = listener.accept().unwrap();
    events.recv_timeout(Duration::from_secs(2)).unwrap();
    for _ in 0..100 {
        assert_eq!(
            transport.request_timeout("silent", Value::Null, Duration::from_millis(1)),
            Err("daemon_response_timeout".to_owned())
        );
    }
    assert_eq!(transport.shared.state.lock().unwrap().pending.len(), 0);
    drop(stream);
}
#[test]
fn daemon_transport_old_generation_and_disconnect() {
    let (_directory, listener, transport, events) = setup();
    let (stream, _) = listener.accept().unwrap();
    events.recv_timeout(Duration::from_secs(2)).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    std::thread::scope(|scope| {
        let pending = scope.spawn(|| transport.request("silent", Value::Null));
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        stream.shutdown(Shutdown::Both).unwrap();
        assert_eq!(
            pending.join().unwrap(),
            Err("daemon_unavailable".to_owned())
        );
    });
    let (sender, receiver) = mpsc::channel();
    let mut pending = Pending::default();
    pending.insert(1, 2, sender).unwrap();
    pending.settle(1, 1, Ok(Value::Null));
    assert!(receiver.try_recv().is_err());
    assert_eq!(pending.len(), 1);
    pending.settle(1, 2, Ok(Value::Null));
    assert_eq!(receiver.recv().unwrap(), Ok(Value::Null));
}
#[test]
fn daemon_transport_unavailable_boot_and_reconnect_without_respawn() {
    let directory = tempfile::Builder::new()
        .prefix("oi-ipc-")
        .tempdir_in("/tmp")
        .unwrap();
    let path = directory.path().join("socket");
    let spawned = Arc::new(AtomicU64::new(0));
    let counter = spawned.clone();
    let (sender, receiver) = mpsc::channel();
    let started = Instant::now();
    let transport = Transport::start(
        path.clone(),
        Arc::new(move |_, event| {
            let _ = sender.send(event);
        }),
        Box::new(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        }),
    )
    .unwrap();
    assert!(started.elapsed() < Duration::from_millis(100));
    assert_eq!(
        transport.request("ping", Value::Null),
        Err("daemon_unavailable".to_owned())
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    while spawned.load(Ordering::Relaxed) == 0 && Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert_eq!(spawned.load(Ordering::Relaxed), 1);
    let listener = UnixListener::bind(path).unwrap();
    let (stream, _) = listener.accept().unwrap();
    assert_eq!(
        receiver.recv_timeout(Duration::from_secs(2)).unwrap()["generation"],
        1
    );
    stream.shutdown(Shutdown::Both).unwrap();
    assert_eq!(
        receiver.recv_timeout(Duration::from_secs(2)).unwrap()["state"],
        "reconnecting"
    );
    let (_stream, _) = listener.accept().unwrap();
    assert_eq!(
        receiver.recv_timeout(Duration::from_secs(2)).unwrap()["generation"],
        2
    );
    assert_eq!(spawned.load(Ordering::Relaxed), 1);
}

#[test]
fn incremental_snapshot_does_not_block_reader_and_checks_epoch_before_returning() {
    for wrong_epoch in [false, true] {
        let (_directory, listener, transport, events) = setup();
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        events.recv_timeout(Duration::from_secs(2)).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let document = open_island_core::ui_state::UiSnapshot {
            schema_version: 1,
            discovering: false,
            daemon_epoch: open_island_core::message_delivery::DaemonEpoch(
                if wrong_epoch { "other" } else { "epoch" }.into(),
            ),
            publication_revision: 2,
            sessions: vec![],
            child_sessions: vec![],
            approvals: vec![],
            questions: vec![],
            message_deliveries: vec![],
            config: json!({"large": "é🦀".repeat(20_000)}),
            usage: Default::default(),
            update: None,
            quiet_scenes: Default::default(),
        };
        let bytes = serde_json::to_vec(&document).unwrap();
        std::thread::scope(|scope| {
            let sync = scope.spawn(|| transport.ui_snapshot());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["method"], "get_ui_state");
            writeln!(stream, "{}", json!({"v":1,"id":request["id"],"ok":true,"data":{"snapshot_id":1,"daemon_epoch":"epoch","publication_revision":2}})).unwrap();
            let count = bytes
                .len()
                .div_ceil(open_island_core::snapshot_page::PAGE_BYTES);
            for (index, chunk) in bytes
                .chunks(open_island_core::snapshot_page::PAGE_BYTES)
                .enumerate()
            {
                line.clear();
                reader.read_line(&mut line).unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                assert_eq!(request["params"]["expected_page"], index);
                writeln!(
                    stream,
                    "{}",
                    json!({"v":1,"event":"ui-state-invalidated","data":{"publication_revision":3}})
                )
                .unwrap();
                let end = index + 1 == count;
                let page = open_island_core::snapshot_page::SnapshotPage {
                    snapshot_id: 1,
                    page_index: index as u64,
                    bytes: chunk.to_vec(),
                    end,
                    total_bytes: end.then_some(bytes.len() as u64),
                };
                writeln!(
                    stream,
                    "{}",
                    json!({"v":1,"id":request["id"],"ok":true,"data":page})
                )
                .unwrap();
            }
            if wrong_epoch {
                line.clear();
                reader.read_line(&mut line).unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                assert_eq!(request["method"], "cancel_ui_state");
                reply(&mut stream, &request["id"]);
                assert_eq!(
                    sync.join().unwrap().unwrap_err(),
                    "invalid_snapshot_identity"
                );
            } else {
                assert_eq!(
                    sync.join().unwrap().unwrap().snapshot.config,
                    document.config
                );
            }
        });
    }
}
