use super::{
    refresh::{Phase, Refresh},
    Transport,
};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    net::Shutdown,
    os::unix::net::{UnixListener, UnixStream},
    sync::{mpsc, Arc},
    time::Duration,
};
struct Close(UnixStream);
impl Drop for Close {
    fn drop(&mut self) {
        let _ = self.0.shutdown(Shutdown::Both);
    }
}
fn reply(stream: &mut UnixStream, id: &Value, data: Value) {
    writeln!(stream, "{}", json!({"v":1,"id":id,"ok":true,"data":data})).unwrap();
}
fn fixture(compatible: bool) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("socket");
    let listener = UnixListener::bind(&path).unwrap();
    let control = Arc::new(Refresh::default());
    let events_control = control.clone();
    let transport = Arc::new(
        Transport::start(
            path,
            Arc::new(move |event, data| events_control.event(event, &data)),
            Box::new(|| panic!("server already exists")),
        )
        .unwrap(),
    );
    let (mut stream, _) = listener.accept().unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let (updates, received) = mpsc::channel();
    let (requests, observed) = mpsc::channel();
    std::thread::scope(|scope| {
        let closer = Close(stream.try_clone().unwrap());
        let worker = control
            .start(
                transport.clone(),
                Arc::new(move |cache| {
                    let _ = updates.send(cache);
                }),
            )
            .unwrap();
        scope.spawn(move || {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut begin = 0;
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) { Ok(0) | Err(_) => break, _ => {} }
                let request: Value = serde_json::from_str(&line).unwrap();
                let method = request["method"].as_str().unwrap();
                requests.send(method.to_owned()).unwrap();
                match method {
                    "ping" => reply(&mut stream, &request["id"], if compatible {json!({"daemon_epoch":"epoch","capabilities":["ui_state_v1","message_delivery_v1","guarded_actions_v1","diagnostics_v1"]})} else {json!({"version":"old"})}),
                    "subscribe_ui" => reply(&mut stream, &request["id"], json!({"daemon_epoch":"epoch"})),
                    "get_ui_state" => {
                        begin += 1;
                        if begin == 1 { writeln!(stream, "{}", json!({"v":1,"event":"ui-state-invalidated","data":{"daemon_epoch":"epoch","publication_revision":1}})).unwrap(); }
                        reply(&mut stream, &request["id"], json!({"snapshot_id":begin,"daemon_epoch":"epoch","publication_revision":1}));
                    }
                    "get_ui_state_page" => {
                        let document = open_island_core::ui_state::UiSnapshot { schema_version:1, discovering:false, daemon_epoch:open_island_core::message_delivery::DaemonEpoch("epoch".into()), publication_revision:1, sessions:vec![], child_sessions:vec![], approvals:vec![], questions:vec![], message_deliveries:vec![], config:json!({"read":begin}), usage:Default::default(), update:None, quiet_scenes:Default::default() };
                        let bytes = serde_json::to_vec(&document).unwrap();
                        reply(&mut stream, &request["id"], json!({"snapshot_id":begin,"page_index":0,"total_bytes":bytes.len(),"bytes":bytes,"end":true}));
                    }
                    _ => panic!("unexpected RPC {method}"),
                }
            }
        });
        let first = received.recv_timeout(Duration::from_secs(3)).unwrap();
        if compatible {
            assert_eq!(first.phase, Phase::Connected);
            assert_eq!(first.snapshot.unwrap().snapshot.config["read"], 1);
            let second = received.recv_timeout(Duration::from_secs(3)).unwrap();
            assert_eq!(second.snapshot.unwrap().snapshot.config["read"], 2);
            assert_eq!(
                control.cache().snapshot.unwrap().snapshot.config["read"],
                2,
                "late WebView reads cache without another RPC"
            );
            assert_eq!(
                observed.try_iter().collect::<Vec<_>>(),
                [
                    "ping",
                    "subscribe_ui",
                    "get_ui_state",
                    "get_ui_state_page",
                    "get_ui_state",
                    "get_ui_state_page"
                ]
            );
        } else {
            assert_eq!(first.phase, Phase::Incompatible);
            assert_eq!(
                observed.recv_timeout(Duration::from_secs(1)).unwrap(),
                "ping"
            );
        }
        assert!(received.recv_timeout(Duration::from_millis(150)).is_err());
        assert!(
            observed.try_recv().is_err(),
            "no repeated poll when clean or incompatible"
        );
        drop(worker);
        drop(closer);
    });
}
#[test]
fn refresh_dirty_during_snapshot_and_late_webview() {
    fixture(true);
}
#[test]
fn incompatible_daemon_is_not_restarted_or_polled_repeatedly() {
    fixture(false);
}
