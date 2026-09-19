use open_island_core::{
    input_bridge,
    message_delivery::DeliveryState,
    process,
    protocol::QuietScenes,
    session::{Attention, Session},
    store::SessionStore,
};
use open_islandd::{
    config_handle::ConfigHandle,
    message_executor::{self, MessageExecutor},
    message_runner::MessageRunner,
    notifications::lifecycle::{DaemonContext, DaemonState},
    sound::SoundPlayer,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    os::unix::net::UnixListener,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

struct Helper(Child);
impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn context() -> DaemonContext {
    DaemonContext {
        discovery: Arc::new(open_islandd::discovery_cache::DiscoveryCache::default()),
        admission: Arc::new(open_islandd::server::admission::Admission::default()),
        snapshots: Arc::new(open_islandd::ui_state::UiSnapshots::new().unwrap()),
        state: Arc::new(Mutex::new(DaemonState {
            publication_revision: 0,
            no_island: 0,
            store: SessionStore::new(),
            pending: HashMap::new(),
            pending_questions: HashMap::new(),
            subscribers: Vec::new(),
            approval_generation: 0,
            usage: Default::default(),
            usage_watch: Default::default(),
            scenes: QuietScenes::default(),
            update: None,
        })),
        config: ConfigHandle::default(),
        sound: SoundPlayer::silent(),
        messages: Arc::new(MessageExecutor::new().unwrap()),
        scan_wakeup: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    }
}
fn helper(path: &std::path::Path) -> (Helper, Session) {
    #[cfg(target_os = "macos")]
    let mut command = {
        let bun = open_island_core::paths::executable("bun")
            .expect("Bun is required for the macOS process-environment fixture");
        let mut command = Command::new(bun);
        command.args(["-e", "setInterval(() => {}, 60000)"]);
        command
    };
    #[cfg(not(target_os = "macos"))]
    let mut command = {
        let mut command = Command::new("sleep");
        command.arg("60");
        command
    };
    let child = command
        .env(input_bridge::ENV, path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let session = Session::new("claude", "/tmp", child.id(), "kitty");
    (Helper(child), session)
}
fn enqueue(ctx: &DaemonContext, session: &Session) -> u64 {
    let birth = process::birth_identity(session.pid).unwrap();
    let identity = message_executor::identity(session, birth, &ctx.messages.epoch);
    ctx.state
        .lock()
        .unwrap()
        .store
        .deliveries
        .admit(
            &session.id,
            "texto preservado".into(),
            1,
            true,
            Some(identity),
            None,
        )
        .unwrap()
        .message_id
}
fn await_state(
    ctx: &DaemonContext,
    id: u64,
) -> open_island_core::message_delivery::MessageDelivery {
    let deadline = Instant::now() + Duration::from_secs(11);
    loop {
        let record = ctx
            .state
            .lock()
            .unwrap()
            .store
            .deliveries
            .snapshot()
            .into_iter()
            .find(|m| m.message_id == id)
            .unwrap();
        if !matches!(record.state, DeliveryState::Queued | DeliveryState::Sending) {
            return record;
        }
        assert!(Instant::now() < deadline, "delivery stuck");
        thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn bridge_confirm_reject_and_lost_ack_preserve_delivery_truth() {
    for outcome in ["delivered", "rejected", "lost"] {
        let directory = tempfile::tempdir_in("/tmp").unwrap();
        let socket = directory.path().join("input");
        let listener = UnixListener::bind(&socket).unwrap();
        let (_child, mut session) = helper(&socket);
        session.attention = Some(Attention::Idle);
        let ctx = context();
        let id = enqueue(&ctx, &session);
        let server = thread::spawn(move || {
            let (mut probe, _) = listener.accept().unwrap();
            let request: Value = input_bridge::read_frame(&probe).unwrap();
            assert_eq!(request, json!({"probe":"capabilities"}));
            input_bridge::write_frame(
                &mut probe,
                &json!({"capabilities":["outcome_v1","expected_process_identity_v1"]}),
            )
            .unwrap();
            let (mut connection, _) = listener.accept().unwrap();
            let request: input_bridge::Request = input_bridge::read_frame(&connection).unwrap();
            assert!(request.expected_process_identity.is_some());
            assert_eq!(request.text, "texto preservado");
            if outcome != "lost" {
                input_bridge::write_frame(&mut connection, &json!({"outcome":outcome,"error":if outcome == "rejected" {Some("reject")} else {None},"error_code":if outcome == "rejected" {Some("target_changed")} else {None}})).unwrap();
            }
        });
        assert!(ctx.messages.schedule(&ctx.state, &session, true).unwrap());
        let result = await_state(&ctx, id);
        server.join().unwrap();
        let expected = match outcome {
            "delivered" => DeliveryState::Delivered,
            "rejected" => DeliveryState::Failed,
            _ => DeliveryState::Unconfirmed,
        };
        assert_eq!(result.state, expected);
        assert_eq!(result.text.is_empty(), outcome == "delivered");
        assert!(!ctx.messages.schedule(&ctx.state, &session, true).unwrap());
    }
}
#[test]
fn saturation_keeps_queued_armed_and_cancel_releases_workers() {
    let ctx = context();
    let directory = tempfile::tempdir_in("/tmp").unwrap();
    let mut children = Vec::new();
    let mut servers = Vec::new();
    let (ready, notified) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    for index in 0..8 {
        let socket = directory.path().join(format!("s{index}"));
        let listener = UnixListener::bind(&socket).unwrap();
        let (child, mut session) = helper(&socket);
        session.attention = Some(Attention::Idle);
        children.push(child);
        let ready = ready.clone();
        let stop = stop.clone();
        servers.push(thread::spawn(move || {
            let (mut probe, _) = listener.accept().unwrap();
            let _: Value = input_bridge::read_frame(&probe).unwrap();
            input_bridge::write_frame(
                &mut probe,
                &json!({"capabilities":["outcome_v1","expected_process_identity_v1"]}),
            )
            .unwrap();
            let (connection, _) = listener.accept().unwrap();
            let _: Value = input_bridge::read_frame(&connection).unwrap();
            ready.send(()).unwrap();
            while !stop.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(10));
            }
        }));
        enqueue(&ctx, &session);
        assert!(ctx.messages.schedule(&ctx.state, &session, true).unwrap());
    }
    for _ in 0..8 {
        notified.recv_timeout(Duration::from_secs(10)).unwrap();
    }
    let (_ninth, mut session) = helper(&directory.path().join("missing"));
    session.attention = Some(Attention::Idle);
    let id = enqueue(&ctx, &session);
    assert_eq!(ctx.messages.active(), 8);
    assert!(!ctx.messages.schedule(&ctx.state, &session, true).unwrap());
    assert_eq!(
        ctx.state
            .lock()
            .unwrap()
            .store
            .deliveries
            .snapshot()
            .last()
            .unwrap()
            .state,
        DeliveryState::Queued
    );
    stop.store(true, Ordering::Release);
    for server in servers {
        server.join().unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while ctx.messages.active() != 0 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    assert!(ctx.messages.schedule(&ctx.state, &session, true).unwrap());
    assert_eq!(await_state(&ctx, id).state, DeliveryState::Failed);
}
#[test]
fn hanging_delivery_deadline_kills_group_and_bounds_output() {
    use open_island_core::runner::CommandRunner;
    let runner = MessageRunner::new(
        Instant::now() - Duration::from_secs(7),
        Arc::new(AtomicBool::new(false)),
    );
    let began = Instant::now();
    assert!(runner
        .run("sh", &["-c", "trap '' TERM; sleep 60 & wait"])
        .is_err());
    assert!(began.elapsed() < Duration::from_secs(4));
    let runner = MessageRunner::new(Instant::now(), Arc::new(AtomicBool::new(false)));
    let output = runner
        .run("sh", &["-c", "head -c 200000 /dev/zero"])
        .unwrap();
    assert!(output.status.success());
    assert!(output.stdout.len() <= 65536);
}

#[test]
fn recycled_target_is_rejected_without_contacting_the_bridge() {
    let ctx = context();
    let directory = tempfile::tempdir_in("/tmp").unwrap();
    let socket = directory.path().join("input");
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let (_child, mut session) = helper(&socket);
    session.attention = Some(Attention::Idle);
    let stale = message_executor::identity(
        &session,
        process::ProcessBirthIdentity::new(0),
        &ctx.messages.epoch,
    );
    let id = ctx
        .state
        .lock()
        .unwrap()
        .store
        .deliveries
        .admit(&session.id, "preservar".into(), 1, true, Some(stale), None)
        .unwrap()
        .message_id;
    assert!(ctx.messages.schedule(&ctx.state, &session, true).unwrap());
    let result = await_state(&ctx, id);
    assert_eq!(result.state, DeliveryState::Failed);
    assert_eq!(result.error_code.as_deref(), Some("target_changed"));
    assert_eq!(result.text, "preservar");
    assert!(listener.accept().is_err());
}

#[test]
fn resource_guard_serializes_and_releases_on_drop() {
    use open_islandd::message_resources::{ResourceKey, Resources};
    let resources = Arc::new(Resources::default());
    let runner = MessageRunner::new(Instant::now(), Arc::new(AtomicBool::new(false)));
    let first = resources
        .acquire(ResourceKey::Tmux("same-server".into()), &runner)
        .unwrap();
    let (ready, waiting) = mpsc::channel();
    let (acquired, observed) = mpsc::channel();
    thread::scope(|scope| {
        let resources = resources.clone();
        scope.spawn(move || {
            ready.send(()).unwrap();
            let runner = MessageRunner::new(Instant::now(), Arc::new(AtomicBool::new(false)));
            let _second = resources
                .acquire(ResourceKey::Tmux("same-server".into()), &runner)
                .unwrap();
            acquired.send(()).unwrap();
        });
        waiting.recv().unwrap();
        assert!(observed.recv_timeout(Duration::from_millis(50)).is_err());
        drop(first);
        observed.recv_timeout(Duration::from_secs(1)).unwrap();
    });
    assert!(resources
        .acquire(ResourceKey::Tmux("same-server".into()), &runner)
        .is_ok());
}

#[test]
fn guarded_rpc_rejects_stale_click_and_keeps_guard_when_scheduler_runs_later() {
    use open_island_core::{
        message_delivery::{ClientSubmissionId, DaemonEpoch, SessionInstanceId},
        protocol::{HookEvent, HookEventKind},
    };
    let directory = tempfile::tempdir_in("/tmp").unwrap();
    let socket = directory.path().join("input");
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let (_child, process_session) = helper(&socket);
    let ctx = context();
    let mut event = HookEvent::new("claude", "guarded", HookEventKind::SessionStart);
    event.pid = Some(process_session.pid);
    event.cwd = Some("/tmp".into());
    ctx.state.lock().unwrap().store.apply_hook_event(event);
    let hooks = ctx.state.lock().unwrap().store.discovery_targets();
    ctx.discovery.refresh(&hooks).unwrap();
    let snapshot = open_islandd::ui_state::freeze(&ctx).unwrap();
    let session = snapshot
        .sessions
        .iter()
        .find(|s| s.session.id == "claude:guarded")
        .unwrap();
    let expected = open_island_core::message_delivery::DeliveryIdentity {
        daemon_epoch: ctx.messages.epoch.clone(),
        session_instance_id: session.session_instance_id.clone().unwrap(),
    };
    for wrong_epoch in [true, false] {
        let mut identity = expected.clone();
        if wrong_epoch {
            identity.daemon_epoch = DaemonEpoch("old".into());
        } else {
            identity.session_instance_id = SessionInstanceId("recycled".into());
        }
        let jump_error = open_islandd::server::guarded_jump::jump(
            &ctx,
            open_islandd::server::guarded_jump::Jump {
                id: session.session.id.clone(),
                identity: identity.clone(),
            },
        )
        .unwrap_err();
        assert_eq!(
            jump_error,
            if wrong_epoch {
                "stale_epoch"
            } else {
                "stale_session"
            }
        );
        let error = open_islandd::poll::send_message_guarded(
            &ctx,
            open_islandd::server::guarded_actions::Send {
                id: session.session.id.clone(),
                text: "preservado".into(),
                identity,
                client_submission_id: ClientSubmissionId("stale".into()),
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            if wrong_epoch {
                "stale_epoch"
            } else {
                "stale_session"
            }
        );
        assert!(ctx
            .state
            .lock()
            .unwrap()
            .store
            .deliveries
            .snapshot()
            .is_empty());
        assert!(listener.accept().is_err());
    }
    let receipt = open_islandd::poll::send_message_guarded(
        &ctx,
        open_islandd::server::guarded_actions::Send {
            id: session.session.id.clone(),
            text: "preservado".into(),
            identity: expected,
            client_submission_id: ClientSubmissionId("new".into()),
        },
    )
    .unwrap();
    {
        let mut state = ctx.state.lock().unwrap();
        state
            .store
            .observe_deliveries(std::slice::from_ref(&session.session), 1);
        state
            .store
            .apply_hook_event(HookEvent::new("claude", "guarded", HookEventKind::Stop));
        state.store.mark_seen(
            &open_island_core::session::HookId::new("claude", "guarded"),
            Instant::now(),
        );
    }
    let stopped = open_islandd::ui_state::freeze(&ctx).unwrap();
    let target = &stopped
        .sessions
        .iter()
        .find(|s| s.session.id == "claude:guarded")
        .unwrap()
        .session;
    assert!(ctx.messages.schedule(&ctx.state, target, false).unwrap());
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut probe = loop {
        if let Ok((socket, _)) = listener.accept() {
            break socket;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    };
    probe
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let _: Value = input_bridge::read_frame(&probe).unwrap();
    input_bridge::write_frame(&mut probe, &json!({"capabilities":[]})).unwrap();
    drop(probe);
    let result = await_state(&ctx, receipt["message_id"].as_u64().unwrap());
    assert_eq!(result.state, DeliveryState::Failed);
    assert_eq!(
        result.error_code.as_deref(),
        Some("bridge_upgrade_required")
    );
    assert_eq!(result.text, "preservado");
    assert!(
        listener.accept().is_err(),
        "guarded delivery must not fall back to legacy text injection"
    );
    ctx.messages.shutdown();
}
