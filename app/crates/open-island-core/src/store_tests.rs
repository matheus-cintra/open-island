use super::*;
use crate::{protocol::HookEventKind, session::HookId};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

struct TemporaryRepository(PathBuf);

impl TemporaryRepository {
    fn on_branch(branch: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "open-island-store-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock after Unix epoch")
                .as_nanos()
        ));
        fs::create_dir_all(path.join(".git")).expect("create repository metadata");
        fs::write(
            path.join(".git/HEAD"),
            format!("ref: refs/heads/{branch}\n"),
        )
        .expect("write branch head");
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TemporaryRepository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn event(kind: HookEventKind, id: &str, cwd: &str, pid: u32) -> HookEvent {
    let mut event = HookEvent::new("claude", id, kind);
    event.cwd = Some(cwd.to_owned());
    event.pid = Some(pid);
    event
}

fn process(pid: u32, cwd: &str) -> Session {
    Session::new("claude", cwd, pid, "kitty")
}

fn assert_process_fallback(snapshot: &[Session], pid: u32, cwd: &str) {
    assert_eq!(snapshot.len(), 1);
    let session = &snapshot[0];
    assert_eq!(session.id, format!("claude:{pid}"));
    assert_eq!(session.hook_id, None);
    assert_eq!(session.agent, "claude");
    assert_eq!(session.pid, pid);
    assert_eq!(session.cwd, cwd);
}

#[test]
fn lifecycle_prompt_tool_permission_and_end_update_snapshot() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(
        event(HookEventKind::SessionStart, "one", "/work", 4242),
        now,
    );
    let mut prompt = event(HookEventKind::UserPromptSubmit, "one", "/work", 4242);
    prompt.prompt = Some("inspect".to_owned());
    store.apply_hook_event_at(prompt, now);
    let mut tool = event(HookEventKind::PreToolUse, "one", "/work", 4242);
    tool.tool_name = Some("Bash".to_owned());
    store.apply_hook_event_at(tool, now);
    assert_eq!(
        store.snapshot_at(&[process(4242, "/work")], now)[0]
            .current_tool
            .as_deref(),
        Some("Bash")
    );
    let mut status = event(HookEventKind::Status, "one", "/work", 4242);
    status.status = Some("working".to_owned());
    store.apply_hook_event_at(status, now);
    let mut post_tool = event(HookEventKind::PostToolUse, "one", "/work", 4242);
    post_tool.tool_name = Some("Bash".to_owned());
    store.apply_hook_event_at(post_tool, now);
    let mut permission = event(HookEventKind::PermissionRequest, "one", "/work", 4242);
    permission.approval_id = Some("approval".to_owned());
    store.apply_hook_event_at(permission, now);
    let snapshot = store.snapshot_at(&[process(4242, "/work")], now);
    let session = &snapshot[0];
    assert_eq!(session.id, "claude:one");
    assert_eq!(session.summary.as_deref(), Some("inspect"));
    assert_eq!(session.current_tool, None);
    assert_eq!(session.status.as_deref(), Some("working"));
    assert_eq!(session.permission_state, Some(PermissionState::Pending));

    store.apply_hook_event_at(event(HookEventKind::SessionEnd, "one", "/work", 4242), now);
    assert_process_fallback(
        &store.snapshot_at(&[process(4242, "/work")], now),
        4242,
        "/work",
    );
}

#[test]
fn disconnect_clears_hook_state() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(
        event(HookEventKind::SessionStart, "one", "/work", 4242),
        now,
    );
    store.disconnect_hook(&HookId::new("claude", "one"));
    assert_process_fallback(
        &store.snapshot_at(&[process(4242, "/work")], now),
        4242,
        "/work",
    );
}

#[test]
fn permission_resolution_and_cap_are_explicit() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    for index in 0..=MAX_PENDING_APPROVALS {
        let mut request = event(HookEventKind::PermissionRequest, "one", "/work", 4242);
        request.approval_id = Some(format!("a{index}"));
        store.apply_hook_event_at(request, now);
    }
    assert_eq!(store.pending_approvals.len(), MAX_PENDING_APPROVALS);
    store.resolve_approval("a32", ApprovalDecision::Allow);
    let snapshot = store.snapshot_at(&[process(4242, "/work")], now);
    let session = snapshot
        .iter()
        .find(|session| session.id == "claude:one")
        .expect("resolved hook session is present");
    assert_eq!(session.permission_state, Some(PermissionState::Allowed));
}

#[test]
fn pid_change_retains_hook_identity() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(
        event(HookEventKind::SessionStart, "one", "/work", 4242),
        now,
    );
    let mut update = event(HookEventKind::Status, "one", "/work", 4243);
    update.status = Some("working".to_owned());
    store.apply_hook_event_at(update, now);
    let session = &store.snapshot_at(&[process(4243, "/work")], now)[0];
    assert_eq!(session.id, "claude:one");
    assert_eq!(session.pid, 4243);
    assert_eq!(
        session.hook_id.as_ref().map(HookId::as_str),
        Some("claude:one")
    );
}

#[test]
fn pid_match_precedes_cwd_match() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(
        event(HookEventKind::SessionStart, "one", "/work", 4242),
        now,
    );
    let processes = [process(4242, "/other"), process(4243, "/work")];
    let snapshot = store.snapshot_at(&processes, now);
    let session = snapshot
        .iter()
        .find(|session| session.id == "claude:one")
        .expect("joined hook session is present");
    assert_eq!(session.pid, 4242);
    assert_eq!(session.cwd, "/other");
}

#[test]
fn cwd_match_joins_when_pid_is_missing() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    let mut start = HookEvent::new("claude", "one", HookEventKind::SessionStart);
    start.cwd = Some("/work".to_owned());
    store.apply_hook_event_at(start, now);
    let snapshot = store.snapshot_at(&[process(4242, "/work")], now);
    assert_eq!(snapshot[0].id, "claude:one");
    assert_eq!(snapshot[0].pid, 4242);
}

#[test]
fn snapshots_are_deterministic_and_remove_stale_or_missing_hooks() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(event(HookEventKind::SessionStart, "z", "/z", 2), now);
    store.apply_hook_event_at(event(HookEventKind::SessionStart, "a", "/a", 1), now);
    let processes = [process(2, "/z"), process(1, "/a")];
    let snapshot = store.snapshot_at(&processes, now);
    assert_eq!(
        snapshot.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        vec!["claude:a", "claude:z"]
    );
    assert!(!store.snapshot_at(&[process(1, "/a")], now).is_empty());
    assert!(store.snapshot_with_liveness(&[], now, |_| false).is_empty());
}

#[test]
fn a_hook_whose_process_is_gone_is_dropped_but_a_silent_one_is_kept() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(
        event(HookEventKind::SessionStart, "quiet", "/work", 4242),
        now,
    );
    store.apply_hook_event_at(event(HookEventKind::Stop, "quiet", "/work", 4242), now);

    let much_later = now + Duration::from_secs(6 * 60 * 60);
    let snapshot = store.snapshot_at(&[process(4242, "/work")], much_later);
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].id, "claude:quiet");
    assert_eq!(snapshot[0].attention, Some(Attention::Idle));

    assert!(store.snapshot_at(&[], much_later).is_empty());
    assert_process_fallback(
        &store.snapshot_at(&[process(4242, "/work")], much_later),
        4242,
        "/work",
    );
}

#[test]
fn process_only_sessions_are_preserved_as_fallback() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    let snapshot = store.snapshot_at(&[process(4, "/work")], now);
    assert_eq!(snapshot, vec![process(4, "/work")]);
}

fn question_event(kind: HookEventKind, id: &str, question_id: &str) -> HookEvent {
    let mut event = event(kind, id, "/work", 4242);
    event.question_id = Some(question_id.to_owned());
    event.questions = Some(vec![crate::protocol::Question {
        question: "Qual cor?".to_owned(),
        header: None,
        options: Vec::new(),
        multi_select: false,
        custom: false,
        id: None,
    }]);
    event
}

#[test]
fn a_pending_question_is_its_own_state_and_never_a_pending_permission() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(
        question_event(HookEventKind::PermissionRequest, "one", "q-1"),
        now,
    );

    let session = &store.snapshot_at(&[process(4242, "/work")], now)[0];
    assert_eq!(session.question_state, Some(QuestionState::Pending));
    assert_eq!(
        session.permission_state, None,
        "an AskUserQuestion permission hook must not raise an approval card"
    );
}

#[test]
fn answering_and_expiring_a_question_move_it_out_of_pending() {
    let now = Instant::now();
    for (outcome, expected) in [
        (QuestionOutcome::Answered, QuestionState::Answered),
        (QuestionOutcome::Cancelled, QuestionState::Expired),
        (QuestionOutcome::Expired, QuestionState::Expired),
    ] {
        let mut store = SessionStore::new();
        store.apply_hook_event_at(
            question_event(HookEventKind::QuestionAsked, "one", "q-1"),
            now,
        );
        store.resolve_question("q-1", outcome);
        assert_eq!(
            store.snapshot_at(&[process(4242, "/work")], now)[0].question_state,
            Some(expected)
        );
        store.resolve_question("q-1", QuestionOutcome::Answered);
    }
}

#[test]
fn the_agent_closing_the_question_itself_clears_the_card() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(question_event(HookEventKind::PreToolUse, "one", "q-1"), now);
    store.apply_hook_event_at(
        question_event(HookEventKind::PostToolUse, "one", "q-1"),
        now,
    );

    assert_eq!(
        store.snapshot_at(&[process(4242, "/work")], now)[0].question_state,
        Some(QuestionState::Answered)
    );
}

#[test]
fn pending_questions_are_capped_and_dropped_with_their_session() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    for index in 0..MAX_PENDING_APPROVALS + 4 {
        store.apply_hook_event_at(
            question_event(HookEventKind::QuestionAsked, "one", &format!("q-{index}")),
            now,
        );
    }
    store.resolve_question("q-0", QuestionOutcome::Answered);
    assert_eq!(
        store.snapshot_at(&[process(4242, "/work")], now)[0].question_state,
        Some(QuestionState::Pending),
        "the oldest question was evicted by the cap, so resolving it changes nothing"
    );

    store.disconnect_hook(&HookId::new("claude", "one"));
    store.apply_hook_event_at(
        question_event(HookEventKind::QuestionAsked, "one", "q-0"),
        now,
    );
    store.resolve_question("q-0", QuestionOutcome::Answered);
    assert_eq!(
        store.snapshot_at(&[process(4242, "/work")], now)[0].question_state,
        Some(QuestionState::Answered),
        "the id is free again once the session went away"
    );
}

fn attention_of(store: &mut SessionStore, now: Instant) -> Option<Attention> {
    store.snapshot_at(&[process(4242, "/work")], now)[0].attention
}

#[test]
fn a_stop_makes_the_session_need_attention_and_the_next_event_takes_it_back() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(
        event(HookEventKind::UserPromptSubmit, "one", "/work", 4242),
        now,
    );
    assert_eq!(attention_of(&mut store, now), Some(Attention::Working));

    store.apply_hook_event_at(event(HookEventKind::Stop, "one", "/work", 4242), now);
    assert_eq!(
        attention_of(&mut store, now),
        Some(Attention::NeedsAttention)
    );

    let later = now + Duration::from_secs(30);
    store.apply_hook_event_at(
        event(HookEventKind::UserPromptSubmit, "one", "/work", 4242),
        later,
    );
    assert_eq!(attention_of(&mut store, later), Some(Attention::Working));
}

#[test]
fn a_stop_clears_the_running_tool() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    let mut running = event(HookEventKind::PreToolUse, "one", "/work", 4242);
    running.tool_name = Some("Bash".to_owned());
    store.apply_hook_event_at(running, now);
    assert_eq!(
        store.snapshot_at(&[process(4242, "/work")], now)[0].current_tool,
        Some("Bash".to_owned())
    );

    store.apply_hook_event_at(event(HookEventKind::Stop, "one", "/work", 4242), now);
    assert_eq!(
        store.snapshot_at(&[process(4242, "/work")], now)[0].current_tool,
        None
    );
}

#[test]
fn looking_at_a_stopped_session_settles_it() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(event(HookEventKind::Stop, "one", "/work", 4242), now);
    assert_eq!(
        attention_of(&mut store, now),
        Some(Attention::NeedsAttention)
    );

    store.mark_seen(&HookId::new("claude", "one"), now + Duration::from_secs(1));
    assert_eq!(attention_of(&mut store, now), Some(Attention::Idle));
}

#[test]
fn a_look_before_the_stop_does_not_settle_the_one_that_comes_after() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(
        event(HookEventKind::SessionStart, "one", "/work", 4242),
        now,
    );
    store.mark_seen(&HookId::new("claude", "one"), now);

    let later = now + Duration::from_secs(5);
    store.apply_hook_event_at(event(HookEventKind::Stop, "one", "/work", 4242), later);
    assert_eq!(
        attention_of(&mut store, later),
        Some(Attention::NeedsAttention)
    );
}

#[test]
fn attention_decays_to_idle_after_the_configured_delay() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.set_idle_after(Duration::from_secs(60));
    store.apply_hook_event_at(event(HookEventKind::Stop, "one", "/work", 4242), now);

    assert_eq!(
        attention_of(&mut store, now + Duration::from_secs(59)),
        Some(Attention::NeedsAttention)
    );
    assert_eq!(
        attention_of(&mut store, now + Duration::from_secs(60)),
        Some(Attention::Idle)
    );
}

#[test]
fn a_pending_question_or_approval_outranks_a_stop() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(event(HookEventKind::Stop, "one", "/work", 4242), now);
    store.apply_hook_event_at(
        question_event(HookEventKind::PermissionRequest, "one", "q-1"),
        now,
    );
    assert_eq!(
        attention_of(&mut store, now),
        Some(Attention::WaitingForInput)
    );

    store.resolve_question("q-1", QuestionOutcome::Answered);
    assert_eq!(attention_of(&mut store, now), Some(Attention::Working));

    let mut approval = event(HookEventKind::PermissionRequest, "one", "/work", 4242);
    approval.approval_id = Some("a-1".to_owned());
    store.apply_hook_event_at(approval, now);
    assert_eq!(
        attention_of(&mut store, now),
        Some(Attention::WaitingForInput)
    );
}

#[test]
fn the_snapshot_is_sorted_by_attention_first() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    for id in ["a-working", "b-stopped", "c-asking"] {
        store.apply_hook_event_at(
            event(HookEventKind::SessionStart, id, &format!("/{id}"), 0),
            now,
        );
    }
    store.apply_hook_event_at(
        event(HookEventKind::Stop, "b-stopped", "/b-stopped", 0),
        now,
    );
    let mut asking = event(HookEventKind::PermissionRequest, "c-asking", "/c-asking", 0);
    asking.question_id = Some("q-1".to_owned());
    asking.questions = Some(Vec::new());
    store.apply_hook_event_at(asking, now);

    let processes = [
        process(1, "/a-working"),
        process(2, "/b-stopped"),
        process(3, "/c-asking"),
    ];
    let snapshot = store.snapshot_at(&processes, now);

    assert_eq!(
        snapshot
            .iter()
            .map(|session| session.id.as_str())
            .collect::<Vec<_>>(),
        vec!["claude:c-asking", "claude:b-stopped", "claude:a-working"]
    );
}

#[test]
fn the_first_prompt_names_the_session_and_later_prompts_never_rename_it() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    let mut first = event(HookEventKind::UserPromptSubmit, "one", "/work", 4242);
    first.prompt = Some("Answer agent questions from the island".to_owned());
    store.apply_hook_event_at(first, now);

    assert_eq!(
        store.snapshot_at(&[process(4242, "/work")], now)[0].name,
        Some("Answer agent questions from the…".to_owned())
    );

    let mut second = event(HookEventKind::UserPromptSubmit, "one", "/work", 4242);
    second.prompt = Some("Now do something completely different".to_owned());
    store.apply_hook_event_at(second, now);

    assert_eq!(
        store.snapshot_at(&[process(4242, "/work")], now)[0].name,
        Some("Answer agent questions from the…".to_owned()),
        "the name is derived once, so it never flaps under the user"
    );
}

#[test]
fn the_branch_is_resolved_from_the_session_cwd() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    let repository = TemporaryRepository::on_branch("session-fixture");
    let repository = repository.path().to_string_lossy();
    store.apply_hook_event_at(
        event(HookEventKind::SessionStart, "one", &repository, 4242),
        now,
    );

    let session = &store.snapshot_at(&[process(4242, &repository)], now)[0];
    assert_eq!(session.branch.as_deref(), Some("session-fixture"));
}

fn stopped_store(id: &str, cwd: &str, pid: u32) -> (SessionStore, Instant, HookId) {
    let mut store = SessionStore::new();
    let start = Instant::now();
    store.apply_hook_event_at(event(HookEventKind::UserPromptSubmit, id, cwd, pid), start);
    store.apply_hook_event_at(event(HookEventKind::Stop, id, cwd, pid), start);
    (store, start, HookId::new("claude", id))
}

#[test]
fn an_idle_reminder_waits_out_the_threshold_and_then_fires_exactly_once() {
    let (mut store, start, hook_id) = stopped_store("reminder-1", "/tmp/one", 11);
    let after = Duration::from_secs(300);

    assert!(store
        .take_idle_reminders(
            start + Duration::from_secs(299),
            after,
            ReminderScopes::default()
        )
        .is_empty());

    let due = store.take_idle_reminders(
        start + Duration::from_secs(301),
        after,
        ReminderScopes::default(),
    );
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].session_id, hook_id);
    assert_eq!(due[0].agent, "claude");
    assert_eq!(due[0].waited, Duration::from_secs(301));

    assert!(store
        .take_idle_reminders(
            start + Duration::from_secs(900),
            after,
            ReminderScopes::default()
        )
        .is_empty());
}

#[test]
fn a_session_the_user_looked_at_is_never_reminded() {
    let (mut store, start, hook_id) = stopped_store("reminder-2", "/tmp/two", 12);
    store.mark_seen(&hook_id, start + Duration::from_secs(5));
    assert!(store
        .take_idle_reminders(
            start + Duration::from_secs(600),
            Duration::from_secs(300),
            ReminderScopes::default()
        )
        .is_empty());
}

#[test]
fn a_session_that_went_back_to_work_is_never_reminded() {
    let (mut store, start, _) = stopped_store("reminder-3", "/tmp/three", 13);
    store.apply_hook_event_at(
        event(
            HookEventKind::UserPromptSubmit,
            "reminder-3",
            "/tmp/three",
            13,
        ),
        start + Duration::from_secs(10),
    );
    assert!(store
        .take_idle_reminders(
            start + Duration::from_secs(600),
            Duration::from_secs(300),
            ReminderScopes::default()
        )
        .is_empty());
}

#[test]
fn a_second_stop_earns_a_second_reminder() {
    let (mut store, start, _) = stopped_store("reminder-4", "/tmp/four", 14);
    let after = Duration::from_secs(300);
    assert_eq!(
        store
            .take_idle_reminders(
                start + Duration::from_secs(301),
                after,
                ReminderScopes::default()
            )
            .len(),
        1
    );
    let second_stop = start + Duration::from_secs(400);
    store.apply_hook_event_at(
        event(
            HookEventKind::UserPromptSubmit,
            "reminder-4",
            "/tmp/four",
            14,
        ),
        second_stop,
    );
    store.apply_hook_event_at(
        event(HookEventKind::Stop, "reminder-4", "/tmp/four", 14),
        second_stop,
    );
    assert_eq!(
        store
            .take_idle_reminders(
                second_stop + Duration::from_secs(301),
                after,
                ReminderScopes::default()
            )
            .len(),
        1
    );
}

#[test]
fn a_session_waiting_on_an_approval_is_never_reminded() {
    let mut store = SessionStore::new();
    let start = Instant::now();
    store.apply_hook_event_at(
        event(HookEventKind::Stop, "reminder-5", "/tmp/five", 15),
        start,
    );
    let mut pending = event(
        HookEventKind::PermissionRequest,
        "reminder-5",
        "/tmp/five",
        15,
    );
    pending.approval_id = Some("approval-5".to_owned());
    store.apply_hook_event_at(pending, start);
    store
        .hooks
        .get_mut(&HookId::new("claude", "reminder-5"))
        .expect("hook")
        .stopped_at = Some(start);
    assert!(store
        .take_idle_reminders(
            start + Duration::from_secs(600),
            Duration::from_secs(300),
            ReminderScopes::default()
        )
        .is_empty());
}

#[test]
fn reminders_come_back_in_a_stable_order() {
    let mut store = SessionStore::new();
    let start = Instant::now();
    for (id, cwd, pid) in [
        ("zulu", "/tmp/z", 21),
        ("alpha", "/tmp/a", 22),
        ("mike", "/tmp/m", 23),
    ] {
        store.apply_hook_event_at(event(HookEventKind::Stop, id, cwd, pid), start);
    }
    let due = store.take_idle_reminders(
        start + Duration::from_secs(600),
        Duration::from_secs(300),
        ReminderScopes::default(),
    );
    let ids: Vec<_> = due
        .iter()
        .map(|entry| entry.session_id.as_str().to_owned())
        .collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted);
    assert_eq!(ids.len(), 3);
}

/// The head of a real background-agent notice, as the daemon received it on 2026-09-06.
/// Its whole body reached `summary` and its first line reached `name`, because Claude
/// fires `UserPromptSubmit` for harness-injected turns too.
const TASK_NOTIFICATION: &str = concat!(
    "<task-notification>\n<task-id>a56230c066fddfd83</task-id>\n",
    "<status>completed</status>\n<summary>Agent \"Describe three agent icons\" finished</summary>\n",
    "<result>## 1. session-icon.png\n\n1. **Shape**: rounded square, not a circle, "
);

#[test]
fn a_harness_notification_never_names_or_summarises_a_session() {
    let now = Instant::now();
    let mut store = SessionStore::new();

    let mut injected = event(HookEventKind::UserPromptSubmit, "one", "/work", 4242);
    injected.prompt = Some(TASK_NOTIFICATION.to_owned());
    store.apply_hook_event_at(injected, now);

    let session = &store.snapshot_at(&[process(4242, "/work")], now)[0];
    assert_eq!(session.name, None);
    assert_eq!(session.summary, None);

    let mut typed = event(HookEventKind::UserPromptSubmit, "one", "/work", 4242);
    typed.prompt = Some("Começar a Phase C7 do open-island".to_owned());
    store.apply_hook_event_at(typed, now);

    let session = &store.snapshot_at(&[process(4242, "/work")], now)[0];
    assert_eq!(
        session.name.as_deref(),
        Some("Começar a Phase C7 do open-island")
    );
    assert_eq!(
        session.summary.as_deref(),
        Some("Começar a Phase C7 do open-island")
    );
}

#[test]
fn a_long_prompt_rides_the_snapshot_cut_to_the_summary_ruler() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    let mut typed = event(HookEventKind::UserPromptSubmit, "one", "/work", 4242);
    typed.prompt = Some("palavra ".repeat(400));
    store.apply_hook_event_at(typed, now);

    let summary = store.snapshot_at(&[process(4242, "/work")], now)[0]
        .summary
        .clone()
        .expect("a summary");

    assert!(summary.chars().count() <= naming::SUMMARY_MAX_CHARS);
}

#[test]
fn the_model_and_the_effort_survive_the_events_that_do_not_carry_them() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    let mut start = event(HookEventKind::SessionStart, "one", "/work", 4242);
    start.model = Some("claude-opus-5[1m]".to_owned());
    store.apply_hook_event_at(start, now);

    let mut tool = event(HookEventKind::PreToolUse, "one", "/work", 4242);
    tool.tool_name = Some("Bash".to_owned());
    tool.effort = Some("high".to_owned());
    store.apply_hook_event_at(tool, now);

    let session = &store.snapshot_at(&[process(4242, "/work")], now)[0];
    assert_eq!(session.model.as_deref(), Some("claude-opus-5[1m]"));
    assert_eq!(session.effort.as_deref(), Some("high"));

    store.apply_hook_event_at(event(HookEventKind::Stop, "one", "/work", 4242), now);

    let session = &store.snapshot_at(&[process(4242, "/work")], now)[0];
    assert_eq!(session.model.as_deref(), Some("claude-opus-5[1m]"));
    assert_eq!(session.effort.as_deref(), Some("high"));
}

#[test]
fn the_activity_stamp_moves_with_the_activity_and_never_with_a_status() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at_wall(
        event(HookEventKind::SessionStart, "one", "/work", 4242),
        now,
        1_000,
    );
    assert_eq!(
        store.snapshot_at(&[process(4242, "/work")], now)[0].since_ms,
        Some(1_000)
    );

    let mut tool = event(HookEventKind::PreToolUse, "one", "/work", 4242);
    tool.tool_name = Some("Bash".to_owned());
    store.apply_hook_event_at_wall(tool, now, 2_000);
    assert_eq!(
        store.snapshot_at(&[process(4242, "/work")], now)[0].since_ms,
        Some(2_000)
    );

    let mut status = event(HookEventKind::Status, "one", "/work", 4242);
    status.status = Some("working".to_owned());
    store.apply_hook_event_at_wall(status, now, 3_000);
    assert_eq!(
        store.snapshot_at(&[process(4242, "/work")], now)[0].since_ms,
        Some(2_000)
    );

    store.apply_hook_event_at_wall(event(HookEventKind::Stop, "one", "/work", 4242), now, 4_000);
    assert_eq!(
        store.snapshot_at(&[process(4242, "/work")], now)[0].since_ms,
        Some(4_000)
    );
}

/// OpenCode fires `session.updated` within milliseconds of `session.idle`, and that maps to
/// `Status`. Before the guard it cleared `stopped_at` and the stop edge C3 and C5 are built
/// on disappeared for OpenCode alone.
#[test]
fn a_status_arriving_after_a_stop_does_not_undo_the_stop() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(event(HookEventKind::Stop, "one", "/work", 4242), now);
    let mut status = event(HookEventKind::Status, "one", "/work", 4242);
    status.status = Some("idle".to_owned());
    store.apply_hook_event_at(status, now);

    assert_eq!(
        store.snapshot_at(&[process(4242, "/work")], now)[0].attention,
        Some(Attention::NeedsAttention)
    );
}

#[test]
fn a_daemon_that_never_saw_a_hook_reports_no_stamp_and_no_model() {
    let mut store = SessionStore::new();
    let session = &store.snapshot(&[process(4242, "/work")])[0];

    assert_eq!(session.since_ms, None);
    assert_eq!(session.model, None);
    assert_eq!(session.effort, None);
}

#[test]
fn the_stop_hooks_last_message_becomes_the_activity_line_and_the_next_prompt_clears_it() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    let mut stop = event(HookEventKind::Stop, "one", "/work", 4242);
    stop.last_message =
        Some("**Eae!** Tudo certo por aqui.\n\nO que a gente vai fazer hoje?".to_owned());
    store.apply_hook_event_at(stop, now);
    assert_eq!(
        store.snapshot_at(&[process(4242, "/work")], now)[0]
            .last_message
            .as_deref(),
        Some("Eae! Tudo certo por aqui.")
    );

    let later = now + Duration::from_secs(5);
    store.apply_hook_event_at(
        event(HookEventKind::UserPromptSubmit, "one", "/work", 4242),
        later,
    );
    assert_eq!(
        store.snapshot_at(&[process(4242, "/work")], later)[0].last_message,
        None
    );
}

#[test]
fn the_transcript_body_keeps_the_whole_answer_and_the_next_prompt_clears_it() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    let answer = "Purge completo. Liberou ~246 MB.\n\n## O que foi removido\n\n| Categoria | Itens |\n|---|---|";
    let mut stop = event(HookEventKind::Stop, "one", "/work", 4242);
    stop.last_message = Some(answer.to_owned());
    store.apply_hook_event_at(stop, now);

    let session = &store.snapshot_at(&[process(4242, "/work")], now)[0];
    assert_eq!(
        session.last_message.as_deref(),
        Some("Purge completo. Liberou ~246 MB.")
    );
    assert_eq!(session.last_message_body.as_deref(), Some(answer));

    let later = now + Duration::from_secs(5);
    store.apply_hook_event_at(
        event(HookEventKind::UserPromptSubmit, "one", "/work", 4242),
        later,
    );
    assert_eq!(
        store.snapshot_at(&[process(4242, "/work")], later)[0].last_message_body,
        None
    );
}

fn agent_spawn(id: &str, kind: &str, description: &str, agent_id: &str) -> HookEvent {
    let mut spawn = event(HookEventKind::PostToolUse, id, "/work", 4242);
    spawn.tool_name = Some("Agent".to_owned());
    spawn.tool_input = Some(serde_json::json!({
        "subagent_type": kind,
        "description": description,
        "prompt": "do the thing",
    }));
    spawn.tool_response = Some(serde_json::json!({
        "isAsync": true,
        "status": "async_launched",
        "agentId": agent_id,
    }));
    spawn
}

fn subagent_tool(kind: HookEventKind, id: &str, agent_id: &str, tool: &str) -> HookEvent {
    let mut inner = event(kind, id, "/work", 4242);
    inner.agent_id = Some(agent_id.to_owned());
    inner.agent_type = Some("Explore".to_owned());
    inner.tool_name = Some(tool.to_owned());
    inner.tool_input = Some(serde_json::json!({"command": "sleep 15"}));
    inner
}

#[test]
fn the_agent_tools_post_hook_opens_a_subagent_and_subagent_stop_closes_it() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(
        agent_spawn(
            "one",
            "Explore",
            "count the rust files",
            "aa9be983d610683e3",
        ),
        now,
    );

    let session = &store.snapshot_at(&[process(4242, "/work")], now)[0];
    let subagents = session
        .subagents
        .as_ref()
        .expect("subagents survive the merge");
    assert_eq!(subagents.len(), 1);
    assert_eq!(subagents[0].id, "aa9be983d610683e3");
    assert_eq!(subagents[0].kind, "Explore");
    assert_eq!(
        subagents[0].description.as_deref(),
        Some("count the rust files")
    );
    assert!(!subagents[0].done);

    let mut stop = event(HookEventKind::SubagentStop, "one", "/work", 4242);
    stop.agent_id = Some("aa9be983d610683e3".to_owned());
    store.apply_hook_event_at(stop, now + Duration::from_secs(15));

    let session = &store.snapshot_at(&[process(4242, "/work")], now + Duration::from_secs(15))[0];
    let subagents = session.subagents.as_ref().expect("subagents");
    assert!(subagents[0].done);
    assert_eq!(subagents[0].tool, None);
}

#[test]
fn a_subagents_own_tool_calls_describe_the_subagent_and_never_the_parent() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(
        agent_spawn(
            "one",
            "Explore",
            "count the rust files",
            "aa9be983d610683e3",
        ),
        now,
    );
    let mut stopped = event(HookEventKind::Stop, "one", "/work", 4242);
    stopped.last_message = Some("Fui atras dos arquivos.".to_owned());
    store.apply_hook_event_at(stopped, now + Duration::from_secs(1));
    let before =
        store.snapshot_at(&[process(4242, "/work")], now + Duration::from_secs(1))[0].clone();
    assert_eq!(before.attention, Some(Attention::NeedsAttention));

    let later = now + Duration::from_secs(2);
    store.apply_hook_event_at(
        subagent_tool(
            HookEventKind::PreToolUse,
            "one",
            "aa9be983d610683e3",
            "Bash",
        ),
        later,
    );

    let after = store.snapshot_at(&[process(4242, "/work")], later)[0].clone();
    assert_eq!(after.current_tool, None, "the parent is not running Bash");
    assert_eq!(
        after.since_ms, before.since_ms,
        "the elapsed badge does not restart"
    );
    assert_eq!(
        after.attention,
        Some(Attention::NeedsAttention),
        "a stopped session stays stopped while its subagents work"
    );
    let subagents = after.subagents.as_ref().expect("subagents");
    assert_eq!(subagents[0].tool.as_deref(), Some("Bash"));
    assert_eq!(subagents[0].summary.as_deref(), Some("sleep 15"));

    store.apply_hook_event_at(
        subagent_tool(
            HookEventKind::PostToolUse,
            "one",
            "aa9be983d610683e3",
            "Bash",
        ),
        later,
    );
    let done = store.snapshot_at(&[process(4242, "/work")], later)[0].clone();
    let idle = &done.subagents.as_ref().expect("subagents")[0];
    assert_eq!(idle.tool, None);
    assert_eq!(idle.summary, None);
}

#[test]
fn an_unknown_agent_id_never_invents_a_subagent_and_the_next_prompt_clears_them() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(
        agent_spawn(
            "one",
            "Explore",
            "count the rust files",
            "aa9be983d610683e3",
        ),
        now,
    );

    let mut orphan = event(HookEventKind::SubagentStop, "one", "/work", 4242);
    orphan.agent_id = Some("a4bdfdd1a194bc858".to_owned());
    store.apply_hook_event_at(orphan, now);
    store.apply_hook_event_at(
        subagent_tool(
            HookEventKind::PreToolUse,
            "one",
            "a4bdfdd1a194bc858",
            "Bash",
        ),
        now,
    );

    let session = &store.snapshot_at(&[process(4242, "/work")], now)[0];
    let subagents = session.subagents.as_ref().expect("subagents");
    assert_eq!(subagents.len(), 1, "the harness own agents are not ours");
    assert!(!subagents[0].done);

    let later = now + Duration::from_secs(5);
    store.apply_hook_event_at(
        event(HookEventKind::UserPromptSubmit, "one", "/work", 4242),
        later,
    );
    assert_eq!(
        store.snapshot_at(&[process(4242, "/work")], later)[0].subagents,
        None
    );
}

#[test]
fn the_transcript_body_is_bounded_by_lines_and_by_characters() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    let mut stop = event(HookEventKind::Stop, "one", "/work", 4242);
    stop.last_message = Some(
        (1..=40)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    store.apply_hook_event_at(stop, now);

    let body = store.snapshot_at(&[process(4242, "/work")], now)[0]
        .last_message_body
        .clone()
        .expect("body");
    assert_eq!(body.lines().count(), naming::TRANSCRIPT_MAX_LINES);
    assert_eq!(body.lines().last(), Some("line 12"));

    let mut wordy_store = SessionStore::new();
    let mut wordy = event(HookEventKind::Stop, "two", "/work", 4243);
    wordy.last_message = Some(vec!["palavra"; 400].join(" "));
    wordy_store.apply_hook_event_at(wordy, now);
    let long = wordy_store.snapshot_at(&[process(4243, "/work")], now)[0]
        .last_message_body
        .clone()
        .expect("body");
    assert!(long.chars().count() <= naming::TRANSCRIPT_MAX_CHARS);
    assert!(long.ends_with('\u{2026}'));
}

fn cwd_rule(pattern: &str) -> SilenceRule {
    SilenceRule {
        field: crate::filters::RuleField::Cwd,
        match_type: crate::filters::MatchType::Contains,
        pattern: pattern.to_owned(),
        name: pattern.to_owned(),
        built_in: false,
        enabled: true,
    }
}

fn prompt_rule(pattern: &str) -> SilenceRule {
    SilenceRule {
        field: crate::filters::RuleField::Prompt,
        match_type: crate::filters::MatchType::Prefix,
        pattern: pattern.to_owned(),
        name: pattern.to_owned(),
        built_in: false,
        enabled: true,
    }
}

#[test]
fn a_filtered_directory_never_reaches_the_snapshot_as_a_hook_session() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.set_filter_rules(vec![cwd_rule("/.codex/memories")], Vec::new());
    store.apply_hook_event_at(
        event(
            HookEventKind::SessionStart,
            "one",
            "/home/me/.codex/memories",
            10,
        ),
        now,
    );
    assert!(store
        .snapshot_at(&[process(10, "/home/me/.codex/memories")], now)
        .is_empty());
}

#[test]
fn a_filtered_directory_is_dropped_from_a_process_only_session_too() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.set_filter_rules(vec![cwd_rule("/.codex/memories")], Vec::new());
    let snapshot = store.snapshot_at(
        &[
            process(10, "/home/me/.codex/memories"),
            process(11, "/home/me/project"),
        ],
        now,
    );
    assert_process_fallback(&snapshot, 11, "/home/me/project");
}

#[test]
fn an_unfiltered_session_still_arrives() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.set_filter_rules(vec![cwd_rule("/.codex/memories")], Vec::new());
    store.apply_hook_event_at(
        event(HookEventKind::SessionStart, "one", "/home/me/project", 10),
        now,
    );
    assert_eq!(
        store
            .snapshot_at(&[process(10, "/home/me/project")], now)
            .len(),
        1
    );
}

#[test]
fn a_prompt_rule_matches_the_raw_first_prompt_that_derive_name_would_have_thrown_away() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.set_filter_rules(vec![prompt_rule("<system-reminder>")], Vec::new());
    store.apply_hook_event_at(
        event(HookEventKind::SessionStart, "one", "/home/me/project", 10),
        now,
    );
    let mut prompt = event(
        HookEventKind::UserPromptSubmit,
        "one",
        "/home/me/project",
        10,
    );
    prompt.prompt = Some("<system-reminder>\nbackground work".to_owned());
    store.apply_hook_event_at(prompt, now);
    assert!(store
        .snapshot_at(&[process(10, "/home/me/project")], now)
        .is_empty());
}

#[test]
fn a_filtered_session_registers_no_approval() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.set_filter_rules(vec![cwd_rule("/.codex/memories")], Vec::new());
    let mut request = event(
        HookEventKind::PermissionRequest,
        "one",
        "/home/me/.codex/memories",
        10,
    );
    request.approval_id = Some("approval-1".to_owned());
    store.apply_hook_event_at(request, now);
    assert!(store.pending_approvals.is_empty());
}

#[test]
fn turning_a_rule_on_evicts_the_session_it_now_matches() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(
        event(
            HookEventKind::SessionStart,
            "one",
            "/home/me/.codex/memories",
            10,
        ),
        now,
    );
    assert_eq!(
        store
            .snapshot_at(&[process(10, "/home/me/.codex/memories")], now)
            .len(),
        1
    );
    store.set_filter_rules(vec![cwd_rule("/.codex/memories")], Vec::new());
    assert!(store
        .snapshot_at(&[process(10, "/home/me/.codex/memories")], now)
        .is_empty());
}

#[test]
fn a_stopped_session_is_hidden_once_it_is_older_than_the_cleanup_window() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.set_cleanup_after(Duration::from_secs(60));
    store.apply_hook_event_at(event(HookEventKind::Stop, "one", "/work", 4242), now);
    assert_eq!(store.snapshot_at(&[process(4242, "/work")], now).len(), 1);
    let later = now + Duration::from_secs(61);
    assert!(store
        .snapshot_at(&[process(4242, "/work")], later)
        .is_empty());
}

#[test]
fn a_hidden_session_comes_back_whole_on_the_next_event() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.set_cleanup_after(Duration::from_secs(60));
    let mut prompt = event(HookEventKind::UserPromptSubmit, "one", "/work", 4242);
    prompt.prompt = Some("inspect the parser".to_owned());
    store.apply_hook_event_at(prompt, now);
    store.apply_hook_event_at(event(HookEventKind::Stop, "one", "/work", 4242), now);
    let later = now + Duration::from_secs(61);
    assert!(store
        .snapshot_at(&[process(4242, "/work")], later)
        .is_empty());

    store.apply_hook_event_at(
        event(HookEventKind::UserPromptSubmit, "one", "/work", 4242),
        later,
    );
    let back = store.snapshot_at(&[process(4242, "/work")], later);
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].name.as_deref(), Some("inspect the parser"));
}

#[test]
fn a_working_session_is_never_hidden_however_old() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.set_cleanup_after(Duration::from_secs(60));
    store.apply_hook_event_at(
        event(HookEventKind::SessionStart, "one", "/work", 4242),
        now,
    );
    let much_later = now + Duration::from_secs(24 * 60 * 60);
    assert_eq!(
        store
            .snapshot_at(&[process(4242, "/work")], much_later)
            .len(),
        1
    );
}

#[test]
fn a_stopped_session_holding_an_approval_is_never_hidden() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.set_cleanup_after(Duration::from_secs(60));
    let mut approval = event(HookEventKind::PermissionRequest, "one", "/work", 4242);
    approval.approval_id = Some("a-1".to_owned());
    store.apply_hook_event_at(approval, now);
    store.apply_hook_event_at(event(HookEventKind::Stop, "one", "/work", 4242), now);
    let later = now + Duration::from_secs(24 * 60 * 60);
    assert_eq!(store.snapshot_at(&[process(4242, "/work")], later).len(), 1);
}

#[test]
fn a_zero_cleanup_window_hides_nothing() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(event(HookEventKind::Stop, "one", "/work", 4242), now);
    let later = now + Duration::from_secs(24 * 60 * 60);
    assert_eq!(store.snapshot_at(&[process(4242, "/work")], later).len(), 1);
}

#[test]
fn three_prompts_inside_the_window_cross_once_and_only_once() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    let w = Duration::from_secs(10);
    assert!(!store.note_prompt(now, w, 3));
    assert!(!store.note_prompt(now + Duration::from_secs(1), w, 3));
    assert!(store.note_prompt(now + Duration::from_secs(2), w, 3));
    assert!(!store.note_prompt(now + Duration::from_secs(3), w, 3));
}

#[test]
fn primary_stops_get_daemon_completion_ids_and_duplicate_turns_do_not_replace_them() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(
        event(HookEventKind::UserPromptSubmit, "one", "/work", 4242),
        now,
    );
    let mut stop = event(HookEventKind::Stop, "one", "/work", 4242);
    stop.turn_id = Some("turn-1".to_owned());
    store.apply_hook_event_at(stop.clone(), now);
    let first = store.snapshot_at(&[process(4242, "/work")], now)[0]
        .completion_id
        .clone()
        .expect("accepted stop has an id");

    store.apply_hook_event_at(stop, now + Duration::from_secs(1));
    assert_eq!(
        store.snapshot_at(&[process(4242, "/work")], now)[0]
            .completion_id
            .as_deref(),
        Some(first.as_str())
    );

    store.apply_hook_event_at(
        event(HookEventKind::UserPromptSubmit, "one", "/work", 4242),
        now + Duration::from_secs(2),
    );
    let mut next = event(HookEventKind::Stop, "one", "/work", 4242);
    next.turn_id = Some("turn-2".to_owned());
    store.apply_hook_event_at(next, now + Duration::from_secs(3));
    let second = store.snapshot_at(&[process(4242, "/work")], now)[0]
        .completion_id
        .clone()
        .expect("new turn has an id");
    assert_ne!(first, second);

    let mut delayed = event(HookEventKind::Stop, "one", "/work", 4242);
    delayed.turn_id = Some("turn-1".to_owned());
    store.apply_hook_event_at(delayed, now + Duration::from_secs(4));
    assert_eq!(
        store.snapshot_at(&[process(4242, "/work")], now)[0]
            .completion_id
            .as_deref(),
        Some(second.as_str())
    );
}

#[test]
fn legacy_stops_need_primary_activity_before_they_can_complete_again() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(event(HookEventKind::Stop, "one", "/work", 4242), now);
    let first = store.snapshot_at(&[process(4242, "/work")], now)[0]
        .completion_id
        .clone()
        .expect("first legacy stop is accepted");

    store.apply_hook_event_at(
        event(HookEventKind::Stop, "one", "/work", 4242),
        now + Duration::from_secs(1),
    );
    assert_eq!(
        store.snapshot_at(&[process(4242, "/work")], now)[0]
            .completion_id
            .as_deref(),
        Some(first.as_str())
    );

    store.apply_hook_event_at(
        event(HookEventKind::PreToolUse, "one", "/work", 4242),
        now + Duration::from_secs(2),
    );
    store.apply_hook_event_at(
        event(HookEventKind::Stop, "one", "/work", 4242),
        now + Duration::from_secs(3),
    );
    assert_ne!(
        store.snapshot_at(&[process(4242, "/work")], now)[0]
            .completion_id
            .as_deref(),
        Some(first.as_str())
    );
}

#[test]
fn prompts_spread_wider_than_the_window_never_cross() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    let w = Duration::from_secs(10);
    for i in 0..6 {
        assert!(!store.note_prompt(now + Duration::from_secs(i * 11), w, 3));
    }
}

#[test]
fn the_crossing_can_happen_again_after_the_window_empties() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    let w = Duration::from_secs(10);
    store.note_prompt(now, w, 3);
    store.note_prompt(now + Duration::from_secs(1), w, 3);
    assert!(store.note_prompt(now + Duration::from_secs(2), w, 3));
    let later = now + Duration::from_secs(60);
    assert!(!store.note_prompt(later, w, 3));
    assert!(!store.note_prompt(later + Duration::from_secs(1), w, 3));
    assert!(store.note_prompt(later + Duration::from_secs(2), w, 3));
}

#[test]
fn a_threshold_below_two_or_a_zero_window_detects_nothing() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    for i in 0..8 {
        let at = now + Duration::from_millis(i * 100);
        assert!(!store.note_prompt(at, Duration::from_secs(10), 1));
        assert!(!store.note_prompt(at, Duration::ZERO, 3));
    }
}

#[test]
fn a_real_opencode_todowrite_payload_becomes_the_session_task_list() {
    let payload = r#"{
        "session_id": "ses_f827e8c52ffeRFAGlk7b3KBh73",
        "cwd": "/work",
        "permission_mode": "bypassPermissions",
        "hook_event_name": "PostToolUse",
        "tool_name": "TodoWrite",
        "tool_input": {
            "todos": [
                {"content": "Extract validation logic", "status": "completed", "priority": "high"},
                {"content": "Break generateReport() apart", "status": "in_progress", "priority": "high"},
                {"content": "Replace hardcoded config", "status": "pending", "priority": "medium"}
            ]
        },
        "tool_use_id": "toolu_01N5riW5AagCsCqMeZM6WMTd",
        "hook_source": "opencode-plugin"
    }"#;
    let parsed = crate::adapters::parse_claude(payload)
        .expect("parses")
        .expect("an event");
    assert_eq!(parsed.event.agent, "opencode");

    let now = Instant::now();
    let mut store = SessionStore::new();
    let mut start = HookEvent::new("opencode", "ses_f827", HookEventKind::SessionStart);
    start.cwd = Some("/work".to_owned());
    start.pid = Some(4242);
    store.apply_hook_event_at(start, now);

    let mut todo = parsed.event;
    todo.session_id = HookId::new("opencode", "ses_f827");
    todo.cwd = Some("/work".to_owned());
    todo.pid = Some(4242);
    store.apply_hook_event_at(todo, now);

    let process = Session::new("opencode", "/work", 4242, "kitty");
    let snapshot = store.snapshot_at(&[process], now);
    let tasks = snapshot[0].tasks.as_ref().expect("the task list survived");
    assert_eq!(tasks.len(), 3);
    assert_eq!(tasks[0].content, "Extract validation logic");
    assert_eq!(tasks[0].status, TaskStatus::Completed);
    assert_eq!(tasks[1].status, TaskStatus::InProgress);
    assert_eq!(tasks[2].status, TaskStatus::Pending);
}

#[test]
fn a_todo_list_is_bounded_and_drops_empty_and_unknown_entries() {
    let mut items = String::new();
    for index in 0..(MAX_TASKS + 5) {
        items.push_str(&format!(
            r#"{{"content":"task {index}","status":"pending"}},"#
        ));
    }
    let payload = format!(
        r#"{{"todos":[{items}{{"content":"   ","status":"pending"}},{{"content":"odd","status":"nope"}}]}}"#
    );
    let value: serde_json::Value = serde_json::from_str(&payload).expect("json");
    let tasks = todo_list(Some("todowrite"), Some(&value)).expect("a list");
    assert_eq!(tasks.len(), MAX_TASKS);
    assert_eq!(tasks[0].content, "task 0");

    let short = serde_json::json!({"todos":[{"content":"only","status":"nope"}]});
    let fallback = todo_list(Some("TodoWrite"), Some(&short)).expect("a list");
    assert_eq!(fallback[0].status, TaskStatus::Pending);

    assert!(todo_list(Some("Bash"), Some(&short)).is_none());
    assert!(todo_list(None, Some(&short)).is_none());
}

#[test]
fn plan_mode_never_becomes_the_mode_a_plan_approval_returns_to() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    let id = HookId::new("claude", "one");

    let mut start = event(HookEventKind::SessionStart, "one", "/work", 4242);
    start.permission_mode = Some("bypassPermissions".to_owned());
    store.apply_hook_event_at(start, now);
    assert_eq!(store.prior_mode(&id).as_deref(), Some("bypassPermissions"));

    let mut planning = event(HookEventKind::PreToolUse, "one", "/work", 4242);
    planning.permission_mode = Some("plan".to_owned());
    store.apply_hook_event_at(planning, now);
    assert_eq!(
        store.prior_mode(&id).as_deref(),
        Some("bypassPermissions"),
        "the mode the user was in has to survive the whole plan"
    );
    assert_eq!(
        store.snapshot_at(&[process(4242, "/work")], now)[0]
            .mode
            .as_deref(),
        Some("plan")
    );
}

#[test]
fn a_session_that_only_ever_planned_has_no_mode_to_return_to() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    let mut start = event(HookEventKind::SessionStart, "one", "/work", 4242);
    start.permission_mode = Some("plan".to_owned());
    store.apply_hook_event_at(start, now);
    assert_eq!(store.prior_mode(&HookId::new("claude", "one")), None);
}

fn both_scopes() -> ReminderScopes {
    ReminderScopes {
        needs_response: true,
        completed_tasks: true,
    }
}

fn needs_response_only() -> ReminderScopes {
    ReminderScopes {
        needs_response: true,
        completed_tasks: false,
    }
}

fn waiting_store(id: &str, cwd: &str, pid: u32) -> (SessionStore, Instant, HookId) {
    let mut store = SessionStore::new();
    let start = Instant::now();
    store.apply_hook_event_at(event(HookEventKind::UserPromptSubmit, id, cwd, pid), start);
    let mut request = event(HookEventKind::PermissionRequest, id, cwd, pid);
    request.approval_id = Some(format!("{id}-approval"));
    store.apply_hook_event_at(request, start);
    (store, start, HookId::new("claude", id))
}

#[test]
fn a_session_blocked_on_you_reminds_once_and_not_once_per_tick() {
    let (mut store, start, hook_id) = waiting_store("waiting-1", "/tmp/wait", 71);
    let after = Duration::from_secs(300);

    assert!(store
        .take_idle_reminders(start + Duration::from_secs(299), after, both_scopes())
        .is_empty());

    let due = store.take_idle_reminders(start + Duration::from_secs(301), after, both_scopes());
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].session_id, hook_id);

    let mut fired = 0;
    for tick in 1..=10 {
        let at = start + Duration::from_secs(301) + Duration::from_secs(2 * tick);
        fired += store.take_idle_reminders(at, after, both_scopes()).len();
    }
    assert_eq!(fired, 0);
}

#[test]
fn the_two_scopes_cover_different_sessions_and_neither_covers_the_other() {
    let (mut stopped, start, _) = stopped_store("scope-stop", "/tmp/stop", 72);
    let at = start + Duration::from_secs(400);
    let after = Duration::from_secs(300);
    assert!(stopped
        .take_idle_reminders(at, after, needs_response_only())
        .is_empty());
    assert_eq!(
        stopped
            .take_idle_reminders(at, after, ReminderScopes::default())
            .len(),
        1
    );

    let (mut waiting, start, _) = waiting_store("scope-wait", "/tmp/wait", 73);
    let at = start + Duration::from_secs(400);
    assert!(waiting
        .take_idle_reminders(at, after, ReminderScopes::default())
        .is_empty());
    assert_eq!(
        waiting.take_idle_reminders(at, after, both_scopes()).len(),
        1
    );
}

#[test]
fn a_new_approval_after_the_first_was_answered_earns_a_second_reminder() {
    let (mut store, start, _) = waiting_store("waiting-2", "/tmp/wait", 74);
    let after = Duration::from_secs(300);
    let first = start + Duration::from_secs(301);
    assert_eq!(
        store.take_idle_reminders(first, after, both_scopes()).len(),
        1
    );

    store.resolve_approval("waiting-2-approval", ApprovalDecision::Allow);
    let second_ask = first + Duration::from_secs(10);
    let mut request = event(
        HookEventKind::PermissionRequest,
        "waiting-2",
        "/tmp/wait",
        74,
    );
    request.approval_id = Some("waiting-2-again".to_owned());
    store.apply_hook_event_at(request, second_ask);

    assert!(store
        .take_idle_reminders(second_ask + Duration::from_secs(299), after, both_scopes())
        .is_empty());
    assert_eq!(
        store
            .take_idle_reminders(second_ask + Duration::from_secs(301), after, both_scopes())
            .len(),
        1
    );
}

#[test]
fn a_delay_of_zero_is_off_and_no_scope_at_all_is_off() {
    let (mut store, start, _) = stopped_store("off-1", "/tmp/off", 75);
    let at = start + Duration::from_secs(600);
    assert!(store
        .take_idle_reminders(at, Duration::ZERO, both_scopes())
        .is_empty());
    assert!(store
        .take_idle_reminders(
            at,
            Duration::from_secs(300),
            ReminderScopes {
                needs_response: false,
                completed_tasks: false,
            }
        )
        .is_empty());
}

fn launched_process(pid: u32, cwd: &str, launcher: &str) -> Session {
    let mut session = process(pid, cwd);
    session.launcher = Some(launcher.to_owned());
    session
}

fn blocked(app_id: &str) -> Vec<LauncherRule> {
    vec![LauncherRule {
        app_id: app_id.to_owned(),
        name: app_id.to_owned(),
        enabled: true,
    }]
}

#[test]
fn a_blocked_launcher_hides_a_hook_session_on_the_very_first_snapshot() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.set_filter_rules(Vec::new(), blocked("Hyprland"));
    store.apply_hook_event_at(
        event(HookEventKind::UserPromptSubmit, "probe", "/work", 900),
        now,
    );
    let processes = [launched_process(900, "/work", "Hyprland")];
    assert!(store.snapshot_at(&processes, now).is_empty());
    assert!(store.snapshot_at(&processes, now).is_empty());
}

#[test]
fn a_blocked_launcher_hides_a_bare_process_row_too() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.set_filter_rules(Vec::new(), blocked("Hyprland"));
    assert!(store
        .snapshot_at(&[launched_process(901, "/work", "Hyprland")], now)
        .is_empty());
    assert_eq!(
        store
            .snapshot_at(&[launched_process(902, "/work", "kitty")], now)
            .len(),
        1
    );
}

#[test]
fn a_blocked_launcher_never_reminds_you_about_the_session_it_hid() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.set_filter_rules(Vec::new(), blocked("Hyprland"));
    store.apply_hook_event_at(
        event(HookEventKind::UserPromptSubmit, "probe", "/work", 903),
        now,
    );
    store.apply_hook_event_at(event(HookEventKind::Stop, "probe", "/work", 903), now);
    let processes = [launched_process(903, "/work", "Hyprland")];
    assert!(store.snapshot_at(&processes, now).is_empty());
    assert!(store
        .take_idle_reminders(
            now + Duration::from_secs(600),
            Duration::from_secs(300),
            both_scopes()
        )
        .is_empty());
}

#[test]
fn unblocking_a_launcher_brings_the_hook_session_back_with_its_id() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.set_filter_rules(Vec::new(), blocked("Hyprland"));
    store.apply_hook_event_at(
        event(HookEventKind::UserPromptSubmit, "probe", "/work", 904),
        now,
    );
    let processes = [launched_process(904, "/work", "Hyprland")];
    assert!(store.snapshot_at(&processes, now).is_empty());

    store.set_filter_rules(Vec::new(), Vec::new());
    let back = store.snapshot_at(&processes, now);
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].hook_id, Some(HookId::new("claude", "probe")));
}

fn queued_ids(store: &mut SessionStore, sessions: &[Session], pid: u32) -> Vec<u64> {
    store
        .snapshot_at(sessions, Instant::now())
        .into_iter()
        .find(|session| session.pid == pid)
        .and_then(|session| session.queued_messages)
        .map(|messages| messages.into_iter().map(|message| message.id).collect())
        .unwrap_or_default()
}

fn with_attention(mut session: Session, attention: Option<Attention>) -> Session {
    session.attention = attention;
    session
}

#[test]
fn a_message_waits_while_the_agent_works_and_leaves_once_per_stop() {
    let mut store = SessionStore::new();
    let working = with_attention(process(7, "/tmp/p"), Some(Attention::Working));
    let idle = with_attention(process(7, "/tmp/p"), Some(Attention::Idle));
    let first = store.enqueue_message(&working.id, "primeira".into(), 1, working.attention);
    let second = store.enqueue_message(&working.id, "segunda".into(), 2, working.attention);
    assert_eq!(first.id, 1);
    assert_eq!(second.id, 2);
    assert!(store
        .take_due_messages(std::slice::from_ref(&working))
        .is_empty());
    assert_eq!(
        queued_ids(&mut store, std::slice::from_ref(&working), 7),
        vec![1, 2]
    );

    let due = store.take_due_messages(std::slice::from_ref(&idle));
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].1.text, "primeira");
    assert_eq!(due[0].0.id, idle.id);
    assert!(
        store
            .take_due_messages(std::slice::from_ref(&idle))
            .is_empty(),
        "the second one waits for another working phase"
    );
    assert!(store
        .take_due_messages(std::slice::from_ref(&working))
        .is_empty());
    let due = store.take_due_messages(std::slice::from_ref(&idle));
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].1.text, "segunda");
    assert!(queued_ids(&mut store, std::slice::from_ref(&idle), 7).is_empty());
}

#[test]
fn a_message_sent_to_a_stopped_session_is_due_at_once() {
    let mut store = SessionStore::new();
    let idle = with_attention(process(7, "/tmp/p"), Some(Attention::WaitingForInput));
    store.enqueue_message(&idle.id, "agora".into(), 1, idle.attention);
    assert_eq!(
        store.take_due_messages(std::slice::from_ref(&idle)).len(),
        1
    );
    let unknown = process(8, "/tmp/q");
    store.enqueue_message(&unknown.id, "sem hook".into(), 2, None);
    assert_eq!(
        store
            .take_due_messages(std::slice::from_ref(&unknown))
            .len(),
        1
    );
}

#[test]
fn a_queued_message_can_be_cancelled_and_a_vanished_session_drops_its_queue() {
    let mut store = SessionStore::new();
    let working = with_attention(process(7, "/tmp/p"), Some(Attention::Working));
    let kept = store.enqueue_message(&working.id, "fica".into(), 1, working.attention);
    let gone = store.enqueue_message(&working.id, "sai".into(), 2, working.attention);
    assert!(store.cancel_message(&working.id, gone.id));
    assert!(!store.cancel_message(&working.id, gone.id));
    assert!(!store.cancel_message("claude:999", kept.id));
    assert_eq!(
        queued_ids(&mut store, std::slice::from_ref(&working), 7),
        vec![kept.id]
    );
    assert!(store.take_due_messages(&[]).is_empty());
    assert!(queued_ids(&mut store, &[working], 7).is_empty());
}

#[test]
fn the_queue_is_bounded_and_keeps_the_newest() {
    let mut store = SessionStore::new();
    let working = with_attention(process(7, "/tmp/p"), Some(Attention::Working));
    for index in 0..(MAX_QUEUED_MESSAGES + 3) {
        store.enqueue_message(
            &working.id,
            format!("m{index}"),
            index as u64,
            working.attention,
        );
    }
    let ids = queued_ids(&mut store, &[working], 7);
    assert_eq!(ids.len(), MAX_QUEUED_MESSAGES);
    assert_eq!(ids[0], 4);
}

#[test]
fn inaccessible_live_hook_is_kept_until_the_process_exits() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(
        event(HookEventKind::SessionStart, "private", "/work", 4242),
        now,
    );
    let sessions = store.snapshot_with_liveness(&[], now, |pid| pid == 4242);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "claude:private");
    assert!(store.snapshot_with_liveness(&[], now, |_| false).is_empty());
}

fn oc_event(id: &str, parent: Option<&str>, kind: HookEventKind) -> HookEvent {
    let mut event = HookEvent::new("opencode", id, kind);
    event.pid = Some(4242);
    event.cwd = Some("/work".into());
    event
        .session_metadata
        .push(crate::protocol::SessionMetadata {
            id: id.into(),
            parent_id: parent.map(str::to_owned),
            title: Some(format!("Title {id}")),
        });
    if let Some(parent) = parent {
        event
            .session_metadata
            .push(crate::protocol::SessionMetadata {
                id: parent.into(),
                parent_id: None,
                title: Some(format!("Title {parent}")),
            });
    }
    event
}

fn oc_snapshot(store: &mut SessionStore, now: Instant) -> Vec<Session> {
    store.snapshot_with_liveness(
        &[Session::new("opencode", "/work", 4242, "kitty")],
        now,
        |_| true,
    )
}

#[test]
fn opencode_family_keeps_original_identities_and_parent_content() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.apply_hook_event_at(oc_event("a", Some("root"), HookEventKind::PreToolUse), now);
    store.apply_hook_event_at(oc_event("b", Some("root"), HookEventKind::PreToolUse), now);
    let mut root = oc_event("root", None, HookEventKind::UserPromptSubmit);
    root.prompt = Some("Main prompt".into());
    root.model = Some("main-model".into());
    store.apply_hook_event_at(root, now);
    let sessions = oc_snapshot(&mut store, now);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "opencode:root");
    assert_eq!(sessions[0].subagents.as_ref().unwrap().len(), 2);
    assert_eq!(sessions[0].model.as_deref(), Some("main-model"));
    assert_eq!(sessions[0].summary.as_deref(), Some("Main prompt"));
    assert_eq!(sessions[0].terminal, "kitty");
    assert_eq!(store.hooks.len(), 3);
}

#[test]
fn opencode_late_metadata_reconciles_bridge_and_multilevel_children_without_pid_grouping() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    let bridge = crate::adapters::parse_claude(r#"{"hook_source":"opencode-plugin","session_id":"leaf","pid":4242,"cwd":"/work","hook_event_name":"PreToolUse","tool_name":"Bash"}"#).unwrap().unwrap().event;
    store.apply_hook_event_at(bridge.clone(), now);
    store.apply_hook_event_at(oc_event("other", None, HookEventKind::SessionStart), now);
    assert_eq!(oc_snapshot(&mut store, now).len(), 2);
    let mut info = oc_event("leaf", Some("middle"), HookEventKind::Status);
    info.session_metadata[1].parent_id = Some("root".into());
    info.session_metadata
        .push(crate::protocol::SessionMetadata {
            id: "root".into(),
            parent_id: None,
            title: Some("Root".into()),
        });
    store.apply_hook_event_at(info, now);
    store.apply_hook_event_at(bridge, now);
    let sessions = oc_snapshot(&mut store, now);
    assert_eq!(sessions.len(), 2);
    let root = sessions.iter().find(|s| s.id == "opencode:root").unwrap();
    assert_eq!(root.subagents.as_ref().unwrap().len(), 2);
    assert_eq!(root.subagents.as_ref().unwrap()[0].id, "leaf");
    let mut cycle = oc_event("root", Some("leaf"), HookEventKind::Status);
    cycle.session_metadata.truncate(1);
    store.apply_hook_event_at(cycle, now);
    assert_eq!(
        store.root_of(&HookId::new("opencode", "leaf")).as_str(),
        "opencode:root"
    );
}

#[test]
fn opencode_lifecycle_preserves_pending_children_and_removes_family() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.set_cleanup_after(Duration::from_secs(1));
    store.apply_hook_event_at(oc_event("a", Some("root"), HookEventKind::PreToolUse), now);
    store.apply_hook_event_at(oc_event("b", Some("root"), HookEventKind::Stop), now);
    store.apply_hook_event_at(oc_event("root", None, HookEventKind::Stop), now);
    let later = now + Duration::from_secs(1000);
    let sessions = oc_snapshot(&mut store, later);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].attention, Some(Attention::Working));
    let mut question = oc_event("a", Some("root"), HookEventKind::QuestionAsked);
    question.question_id = Some("q-a".into());
    store.apply_hook_event_at(question, later);
    let mut permission = oc_event("b", Some("root"), HookEventKind::PermissionRequest);
    permission.approval_id = Some("p-b".into());
    store.apply_hook_event_at(permission, later);
    assert_eq!(
        oc_snapshot(&mut store, later)[0].attention,
        Some(Attention::WaitingForInput)
    );
    store.apply_hook_event_at(
        oc_event("root", None, HookEventKind::UserPromptSubmit),
        later,
    );
    assert_eq!(
        oc_snapshot(&mut store, later)[0]
            .subagents
            .as_ref()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        store.pending_questions["q-a"].session_id.as_str(),
        "opencode:a"
    );
    assert_eq!(
        store.pending_approvals["p-b"].session_id.as_str(),
        "opencode:b"
    );
    store.resolve_approval("p-b", ApprovalDecision::Allow);
    store.apply_hook_event_at(
        oc_event("b", Some("root"), HookEventKind::PreToolUse),
        later,
    );
    store.apply_hook_event_at(oc_event("b", Some("root"), HookEventKind::Stop), later);
    store.apply_hook_event_at(
        oc_event("root", None, HookEventKind::UserPromptSubmit),
        later,
    );
    assert_eq!(
        oc_snapshot(&mut store, later)[0]
            .subagents
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    store.apply_hook_event_at(
        oc_event("b", Some("root"), HookEventKind::PreToolUse),
        later,
    );
    assert_eq!(
        oc_snapshot(&mut store, later)[0]
            .subagents
            .as_ref()
            .unwrap()
            .len(),
        2
    );
    assert!(store
        .snapshot_with_liveness(&[], later, |_| false)
        .is_empty());
    assert!(store.hooks.is_empty());
    assert!(store.pending_questions.is_empty());
}

#[test]
fn opencode_cleanup_hides_a_finished_family_together_and_child_deletion_keeps_parent() {
    let now = Instant::now();
    let mut store = SessionStore::new();
    store.set_cleanup_after(Duration::from_secs(1));
    store.apply_hook_event_at(oc_event("child", Some("root"), HookEventKind::Stop), now);
    store.apply_hook_event_at(oc_event("root", None, HookEventKind::Stop), now);
    assert_eq!(
        oc_snapshot(&mut store, now)[0]
            .subagents
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    assert!(oc_snapshot(&mut store, now + Duration::from_secs(2)).is_empty());
    assert_eq!(store.hooks.len(), 2);
    store.apply_hook_event_at(
        oc_event("child", Some("root"), HookEventKind::PreToolUse),
        now,
    );
    assert_eq!(oc_snapshot(&mut store, now).len(), 1);
    store.apply_hook_event_at(
        oc_event("child", Some("root"), HookEventKind::SessionEnd),
        now,
    );
    assert_eq!(oc_snapshot(&mut store, now)[0].id, "opencode:root");
    assert_eq!(store.hooks.len(), 1);
}
