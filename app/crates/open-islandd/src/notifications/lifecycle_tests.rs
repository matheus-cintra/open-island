use super::{
    admit_locked, admit_question_locked, expire_questions, pending_question, resolve_if_current,
    settle_question, withdraw, Admission, AdmissionRequest, DaemonContext, DaemonState,
    PendingApproval, QuestionAdmission, QuestionAdmissionRequest,
};
use crate::config_handle::ConfigHandle;
use crate::notifications::approval::Resolution;
use crate::notifications::approval::{PendingQuestion, QuestionSettlement};
use crate::sound::SoundPlayer;
use open_island_core::{
    adapters::QuestionInput,
    protocol::{
        ApprovalDecision, HookEvent, HookEventKind, Question, QuestionOption, QuestionOutcome,
    },
    session::{HookId, PermissionState, QuestionState, Session},
    store::{SessionStore, MAX_PENDING_APPROVALS},
};
use std::{
    collections::HashMap,
    sync::{mpsc, Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

fn approval_event(session: &str, approval_id: &str) -> HookEvent {
    let mut event = HookEvent::new("claude", session, HookEventKind::PermissionRequest);
    event.approval_id = Some(approval_id.to_owned());
    event.cwd = Some("/tmp/project".to_owned());
    event.pid = Some(42);
    event.tool_name = Some("Bash".to_owned());
    event
}

fn pending(session: &str, generation: u64) -> PendingApproval {
    let (wake_sender, _wake_receiver) = mpsc::channel();
    PendingApproval {
        connection_id: 1,
        session_id: HookId::new("claude", session),
        approval_generation: generation,
        cell: Arc::new(OnceLock::new()),
        wake_sender,
    }
}

fn daemon_state(pending: HashMap<String, PendingApproval>) -> DaemonState {
    DaemonState {
        store: SessionStore::new(),
        pending,
        pending_questions: HashMap::new(),
        subscribers: Vec::new(),
        approval_generation: MAX_PENDING_APPROVALS as u64,
        usage: open_island_core::usage::UsageReport::default(),
        usage_watch: open_island_core::usage::ThresholdWatch::new(),
        scenes: open_island_core::protocol::QuietScenes::default(),
    }
}

#[test]
fn an_accepted_admission_takes_the_next_generation_and_becomes_pending() {
    // Given
    let mut state = daemon_state(HashMap::new());
    let (wake_sender, _approval_receiver) = mpsc::channel();
    let cell = Arc::new(OnceLock::new());

    // When
    let outcome = admit_locked(
        &mut state,
        AdmissionRequest {
            connection_id: 7,
            approval_id: "accepted-id".to_owned(),
            event: approval_event("accepted-session", "accepted-id"),
            cell,
            wake_sender,
        },
    );

    // Then
    assert!(matches!(outcome, Admission::Accepted(32, _)));
    assert!(state.pending.contains_key("accepted-id"));
}

#[test]
fn duplicate_at_capacity_is_mutation_free_and_keeps_first_authoritative() {
    // Given
    let first_cell = Arc::new(OnceLock::new());
    let mut pending_approvals = HashMap::new();
    for index in 0..MAX_PENDING_APPROVALS {
        let id = if index == 0 {
            "duplicate-id".to_owned()
        } else {
            format!("pending-{index}")
        };
        let mut value = pending(&format!("session-{index}"), index as u64);
        if index == 0 {
            value.cell = Arc::clone(&first_cell);
        }
        pending_approvals.insert(id, value);
    }
    let mut state = daemon_state(pending_approvals);
    let (wake_sender, _approval_receiver) = mpsc::channel();

    // When
    let outcome = admit_locked(
        &mut state,
        AdmissionRequest {
            connection_id: 9,
            approval_id: "duplicate-id".to_owned(),
            event: approval_event("duplicate-session", "duplicate-id"),
            cell: Arc::new(OnceLock::new()),
            wake_sender,
        },
    );

    // Then
    assert!(matches!(outcome, Admission::Duplicate));
    assert_eq!(state.pending.len(), MAX_PENDING_APPROVALS);
    assert!(Arc::ptr_eq(
        &state.pending["duplicate-id"].cell,
        &first_cell
    ));
    assert!(state.store.snapshot(&[]).is_empty());
}

#[test]
fn a_capacity_rejection_applies_deny_and_leaves_the_state_untouched() {
    // Given
    let pending_approvals = (0..MAX_PENDING_APPROVALS)
        .map(|index| {
            (
                format!("pending-{index}"),
                pending(&format!("session-{index}"), index as u64),
            )
        })
        .collect();
    let mut state = daemon_state(pending_approvals);
    let (wake_sender, _approval_receiver) = mpsc::channel();

    // When
    let outcome = admit_locked(
        &mut state,
        AdmissionRequest {
            connection_id: 10,
            approval_id: "capacity-id".to_owned(),
            event: approval_event("capacity-session", "capacity-id"),
            cell: Arc::new(OnceLock::new()),
            wake_sender,
        },
    );

    // Then
    assert!(matches!(outcome, Admission::Capacity(_)));
    assert!(!state.pending.contains_key("capacity-id"));
    let sessions = state
        .store
        .snapshot(&[Session::new("claude", "/tmp/project", 42, "")]);
    assert_eq!(sessions[0].permission_state, Some(PermissionState::Denied));
}

#[test]
fn a_withdrawn_approval_leaves_no_session_waiting_on_a_decision_nobody_will_make() {
    // Given
    let mut state = daemon_state(HashMap::new());
    let (wake_sender, _wake_receiver) = mpsc::channel();
    let outcome = admit_locked(
        &mut state,
        AdmissionRequest {
            connection_id: 7,
            approval_id: "withdrawn-id".to_owned(),
            event: approval_event("withdrawn-session", "withdrawn-id"),
            cell: Arc::new(OnceLock::new()),
            wake_sender,
        },
    );
    assert!(matches!(outcome, Admission::Accepted(_, _)));
    let context = question_ctx(state);

    // When
    withdraw(&context, "withdrawn-id");

    // Then
    let mut state = context.state.lock().expect("state");
    assert!(!state.pending.contains_key("withdrawn-id"));
    let sessions = state
        .store
        .snapshot(&[Session::new("claude", "/tmp/project", 42, "")]);
    assert_eq!(sessions[0].permission_state, Some(PermissionState::Unknown));
}

#[test]
fn a_resolution_publishes_before_the_broadcast_and_never_holds_the_lock() {
    // Given
    let cell = Arc::new(OnceLock::new());
    let (approval_wake_sender, approval_wake_receiver) = mpsc::channel();
    let event = approval_event("resolution-session", "resolution-id");
    let mut daemon_state = daemon_state(HashMap::from([(
        "resolution-id".to_owned(),
        PendingApproval {
            connection_id: 3,
            session_id: event.session_id.clone(),
            approval_generation: 4,
            cell: Arc::clone(&cell),
            wake_sender: approval_wake_sender,
        },
    )]));
    daemon_state.store.apply_hook_event(event);
    let state = Arc::new(std::sync::Mutex::new(daemon_state));
    let context = DaemonContext {
        state: Arc::clone(&state),
        config: ConfigHandle::default(),
        sound: SoundPlayer::silent(),
    };
    let state_during_broadcast = Arc::clone(&state);

    // When
    let outcome = resolve_if_current(
        &context,
        "resolution-id",
        Some(4),
        ApprovalDecision::Allow,
        move |_| {
            assert!(state_during_broadcast.try_lock().is_ok());
            assert!(approval_wake_receiver.try_recv().is_ok());
            true
        },
    )
    .unwrap();

    // Then
    assert!(matches!(
        outcome,
        Resolution::Resolved(ApprovalDecision::Allow)
    ));
    assert_eq!(cell.get(), Some(&ApprovalDecision::Allow));
    assert!(state.lock().unwrap().pending.is_empty());
}

fn one_question() -> Vec<Question> {
    vec![Question {
        question: "Qual cor?".to_owned(),
        header: Some("Cor".to_owned()),
        options: vec![QuestionOption {
            label: "Vermelho".to_owned(),
            description: None,
        }],
        multi_select: false,
        custom: false,
        id: None,
    }]
}

fn question_input(question_id: &str, answerable: bool) -> QuestionInput {
    QuestionInput {
        question_id: question_id.to_owned(),
        session_id: HookId::new("claude", "question-session"),
        agent: "claude".to_owned(),
        questions: one_question(),
        answerable,
        expires_in_ms: (!answerable).then_some(60_000),
    }
}

fn question_hook_event(question_id: &str) -> HookEvent {
    let mut event = HookEvent::new(
        "claude",
        "question-session",
        HookEventKind::PermissionRequest,
    );
    event.cwd = Some("/tmp/project".to_owned());
    event.pid = Some(42);
    event.tool_name = Some("AskUserQuestion".to_owned());
    event.question_id = Some(question_id.to_owned());
    event.questions = Some(one_question());
    event
}

fn admit_question(
    state: &mut DaemonState,
    question_id: &str,
    answerable: bool,
) -> (
    QuestionAdmission,
    Arc<OnceLock<QuestionSettlement>>,
    mpsc::Receiver<()>,
) {
    let cell = Arc::new(OnceLock::new());
    let (wake_sender, wake_receiver) = mpsc::channel();
    let outcome = admit_question_locked(
        state,
        QuestionAdmissionRequest {
            connection_id: 7,
            event: question_hook_event(question_id),
            question: question_input(question_id, answerable),
            cell: Arc::clone(&cell),
            wake_sender,
        },
    );
    (outcome, cell, wake_receiver)
}

fn question_ctx(state: DaemonState) -> DaemonContext {
    DaemonContext {
        state: Arc::new(Mutex::new(state)),
        config: ConfigHandle::default(),
        sound: SoundPlayer::silent(),
    }
}

#[test]
fn an_admitted_question_is_never_a_pending_approval() {
    let mut state = daemon_state(HashMap::new());

    let (outcome, _cell, _wake) = admit_question(&mut state, "q-1", true);

    assert!(matches!(outcome, QuestionAdmission::Accepted(32, _)));
    assert!(state.pending_questions.contains_key("q-1"));
    assert!(state.pending.is_empty(), "a question is not an approval");
    assert_eq!(
        state
            .store
            .snapshot(&[Session::new("claude", "/tmp/project", 42, "kitty")])[0]
            .question_state,
        Some(QuestionState::Pending)
    );
}

#[test]
fn a_question_the_hook_cannot_answer_outlives_its_connection_and_carries_a_deadline() {
    let mut state = daemon_state(HashMap::new());

    admit_question(&mut state, "q-codex", false);

    let pending = state.pending_questions.get("q-codex").expect("pending");
    assert_eq!(
        pending.connection_id, None,
        "Codex sends its PreToolUse and exits, so the card cannot hang off that connection"
    );
    assert!(!pending.answerable);
    assert!(pending.expires_at.is_some());
}

#[test]
fn a_duplicate_question_is_mutation_free_and_capacity_cancels_the_newcomer() {
    let mut state = daemon_state(HashMap::new());
    admit_question(&mut state, "q-1", true);

    let (duplicate, _, _) = admit_question(&mut state, "q-1", true);
    assert!(matches!(duplicate, QuestionAdmission::Duplicate));
    assert_eq!(state.pending_questions.len(), 1);

    for index in 1..MAX_PENDING_APPROVALS {
        admit_question(&mut state, &format!("q-fill-{index}"), true);
    }
    let (over, cell, _) = admit_question(&mut state, "q-over", true);
    assert!(matches!(
        over,
        QuestionAdmission::Capacity(resolved) if resolved.outcome == QuestionOutcome::Cancelled
    ));
    assert!(!state.pending_questions.contains_key("q-over"));
    assert!(cell.get().is_none(), "capacity never publishes an answer");
}

#[test]
fn answering_publishes_once_and_a_second_claim_is_refused() {
    let mut state = daemon_state(HashMap::new());
    let (_, cell, wake) = admit_question(&mut state, "q-1", true);
    let ctx = question_ctx(state);
    let broadcasts: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let recorder = {
        let broadcasts = Arc::clone(&broadcasts);
        move |message: String| {
            broadcasts.lock().expect("broadcasts").push(message);
            true
        }
    };

    let settled = settle_question(
        &ctx,
        "q-1",
        None,
        QuestionSettlement::Answered(vec![vec!["Vermelho".to_owned()]]),
        recorder,
    )
    .expect("first claim wins");
    let broadcasts = broadcasts.lock().expect("broadcasts").clone();

    assert_eq!(
        settled,
        QuestionSettlement::Answered(vec![vec!["Vermelho".to_owned()]])
    );
    assert_eq!(cell.get(), Some(&settled));
    assert!(wake.try_recv().is_ok(), "the blocked hook is woken");
    assert_eq!(broadcasts.len(), 1);
    assert!(broadcasts[0].contains("question-resolved"));
    assert!(broadcasts[0].contains("answered"));
    assert!(settle_question(&ctx, "q-1", None, QuestionSettlement::Cancelled, |_| true).is_err());
}

#[test]
fn a_stale_generation_never_settles_the_current_question() {
    let mut state = daemon_state(HashMap::new());
    let (_, cell, _wake_receiver) = admit_question(&mut state, "q-1", true);
    let ctx = question_ctx(state);

    assert!(settle_question(
        &ctx,
        "q-1",
        Some(999),
        QuestionSettlement::Cancelled,
        |_| true
    )
    .is_err());
    assert!(cell.get().is_none());
    assert!(ctx
        .state
        .lock()
        .expect("state")
        .pending_questions
        .contains_key("q-1"));
}

#[test]
fn a_passed_deadline_expires_the_card_and_leaves_the_question_to_the_terminal() {
    let mut state = daemon_state(HashMap::new());
    admit_question(&mut state, "q-codex", false);
    state
        .pending_questions
        .get_mut("q-codex")
        .expect("pending")
        .expires_at = Some(Instant::now() - Duration::from_secs(1));
    let ctx = question_ctx(state);
    let broadcasts: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let recorder = {
        let broadcasts = Arc::clone(&broadcasts);
        move |message: String| {
            broadcasts.lock().expect("broadcasts").push(message);
            true
        }
    };

    expire_questions(&ctx, Instant::now(), recorder);
    let broadcasts = broadcasts.lock().expect("broadcasts").clone();

    assert!(ctx
        .state
        .lock()
        .expect("state")
        .pending_questions
        .is_empty());
    assert_eq!(broadcasts.len(), 1);
    assert!(broadcasts[0].contains("expired"));
}

#[test]
fn a_question_without_a_deadline_is_never_expired_by_the_poller() {
    let mut state = daemon_state(HashMap::new());
    admit_question(&mut state, "q-1", true);
    let ctx = question_ctx(state);

    expire_questions(&ctx, Instant::now() + Duration::from_secs(3600), |_| true);

    assert!(ctx
        .state
        .lock()
        .expect("state")
        .pending_questions
        .contains_key("q-1"));
}

#[test]
fn the_notification_action_reads_back_the_session_and_whether_it_is_answerable() {
    let mut state = daemon_state(HashMap::new());
    admit_question(&mut state, "q-1", true);
    admit_question(&mut state, "q-codex", false);
    let ctx = question_ctx(state);

    assert_eq!(
        pending_question(&ctx, "q-1"),
        Some((HookId::new("claude", "question-session"), true))
    );
    assert_eq!(
        pending_question(&ctx, "q-codex"),
        Some((HookId::new("claude", "question-session"), false))
    );
    assert_eq!(pending_question(&ctx, "missing"), None);
}

#[test]
fn a_pending_question_type_is_reachable_from_the_daemon_state() {
    let (wake_sender, _receiver) = mpsc::channel();
    let pending = PendingQuestion {
        connection_id: Some(1),
        session_id: HookId::new("claude", "s"),
        question_generation: 0,
        cell: Arc::new(OnceLock::new()),
        wake_sender,
        answerable: true,
        expires_at: None,
    };
    assert!(pending.answerable);
}
