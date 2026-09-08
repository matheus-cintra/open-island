use crate::{
    filters::{self, LauncherRule, SilenceRule, Subject},
    naming,
    protocol::{ApprovalDecision, HookEvent, HookEventKind, QuestionOutcome},
    session::{
        Attention, HookId, PermissionState, QuestionState, QueuedMessage, Session, Subagent, Task,
        TaskStatus,
    },
};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const IDLE_AFTER: Duration = Duration::from_secs(10 * 60);
pub const MAX_QUEUED_MESSAGES: usize = 32;
pub const MAX_PENDING_APPROVALS: usize = 32;
pub const MAX_SUBAGENTS: usize = 12;

const SUBAGENT_TOOLS: &[&str] = &["Agent", "Task"];
const PLAN_MODE: &str = "plan";
const TODO_TOOL: &str = "todowrite";
pub const MAX_TASKS: usize = 12;

fn todo_list(tool_name: Option<&str>, tool_input: Option<&Value>) -> Option<Vec<Task>> {
    if !tool_name?.eq_ignore_ascii_case(TODO_TOOL) {
        return None;
    }
    let items = tool_input?.get("todos")?.as_array()?;
    Some(
        items
            .iter()
            .take(MAX_TASKS)
            .filter_map(|item| {
                let content = item.get("content")?.as_str()?.trim();
                if content.is_empty() {
                    return None;
                }
                Some(Task {
                    content: content.to_owned(),
                    status: item
                        .get("status")
                        .and_then(Value::as_str)
                        .and_then(TaskStatus::parse)
                        .unwrap_or(TaskStatus::Pending),
                })
            })
            .collect(),
    )
}

#[derive(Clone, Debug)]
struct HookState {
    session: Session,
    last_seen: Instant,
    stopped_at: Option<Instant>,
    seen_at: Option<Instant>,
    reminded_at: Option<Instant>,
    waiting_since: Option<Instant>,
    waiting_reminded_at: Option<Instant>,
    branch_cwd: String,
    first_prompt: Option<String>,
    launcher: Option<String>,
    filtered: bool,
    prior_mode: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReminderScopes {
    pub needs_response: bool,
    pub completed_tasks: bool,
}

impl ReminderScopes {
    pub fn any(self) -> bool {
        self.needs_response || self.completed_tasks
    }
}

impl Default for ReminderScopes {
    fn default() -> Self {
        Self {
            needs_response: false,
            completed_tasks: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdleReminder {
    pub session_id: HookId,
    pub agent: String,
    pub label: String,
    pub waited: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingApproval {
    session_id: HookId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingQuestion {
    session_id: HookId,
}

enum QuestionIntent<'a> {
    Open(&'a str),
    Resolve(&'a str),
}

fn question_intent(event: &HookEvent) -> Option<QuestionIntent<'_>> {
    let question_id = event.question_id.as_deref()?;
    match event.event {
        HookEventKind::PostToolUse | HookEventKind::QuestionAnswered => {
            Some(QuestionIntent::Resolve(question_id))
        }
        HookEventKind::PreToolUse
        | HookEventKind::PermissionRequest
        | HookEventKind::QuestionAsked => Some(QuestionIntent::Open(question_id)),
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub struct SessionStore {
    hooks: HashMap<HookId, HookState>,
    pending_approvals: HashMap<String, PendingApproval>,
    approval_order: VecDeque<String>,
    pending_questions: HashMap<String, PendingQuestion>,
    question_order: VecDeque<String>,
    idle_after: Duration,
    cleanup_after: Duration,
    rules: Vec<SilenceRule>,
    launchers: Vec<LauncherRule>,
    prompts: VecDeque<Instant>,
    queues: HashMap<String, MessageQueue>,
    next_message_id: u64,
}

#[derive(Clone, Debug, Default)]
struct MessageQueue {
    messages: VecDeque<QueuedMessage>,
    armed: bool,
}

fn stopped(attention: Option<Attention>) -> bool {
    !matches!(
        attention,
        Some(Attention::Working) | Some(Attention::NeedsAttention)
    )
}

impl Default for SessionStore {
    fn default() -> Self {
        Self {
            hooks: HashMap::new(),
            pending_approvals: HashMap::new(),
            approval_order: VecDeque::new(),
            pending_questions: HashMap::new(),
            question_order: VecDeque::new(),
            idle_after: IDLE_AFTER,
            cleanup_after: Duration::ZERO,
            rules: Vec::new(),
            launchers: Vec::new(),
            prompts: VecDeque::new(),
            queues: HashMap::new(),
            next_message_id: 1,
        }
    }
}

impl SessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_idle_after(&mut self, idle_after: Duration) {
        self.idle_after = idle_after;
    }

    pub fn enqueue_message(
        &mut self,
        session_id: &str,
        text: String,
        now_ms: u64,
        attention: Option<Attention>,
    ) -> QueuedMessage {
        let message = QueuedMessage {
            id: self.next_message_id,
            text,
            queued_at_ms: now_ms,
        };
        self.next_message_id += 1;
        let queue = self.queues.entry(session_id.to_owned()).or_default();
        queue.messages.push_back(message.clone());
        while queue.messages.len() > MAX_QUEUED_MESSAGES {
            queue.messages.pop_front();
        }
        if stopped(attention) {
            queue.armed = true;
        }
        message
    }

    pub fn cancel_message(&mut self, session_id: &str, message_id: u64) -> bool {
        let Some(queue) = self.queues.get_mut(session_id) else {
            return false;
        };
        let before = queue.messages.len();
        queue.messages.retain(|message| message.id != message_id);
        let removed = queue.messages.len() != before;
        if queue.messages.is_empty() {
            self.queues.remove(session_id);
        }
        removed
    }

    pub fn take_due_messages(&mut self, sessions: &[Session]) -> Vec<(Session, QueuedMessage)> {
        let mut due = Vec::new();
        self.queues.retain(|session_id, queue| {
            let Some(session) = sessions.iter().find(|session| session.id == *session_id) else {
                return false;
            };
            if !stopped(session.attention) {
                queue.armed = true;
            } else if queue.armed {
                if let Some(message) = queue.messages.pop_front() {
                    queue.armed = false;
                    due.push((session.clone(), message));
                }
            }
            !queue.messages.is_empty()
        });
        due
    }

    pub fn prior_mode(&self, hook_id: &HookId) -> Option<String> {
        self.hooks.get(hook_id)?.prior_mode.clone()
    }

    // Fires on the upward crossing only. C5's lesson: a level-keyed signal turned one real
    // stop into 398 ticks.
    pub fn note_prompt(&mut self, now: Instant, window: Duration, threshold: u32) -> bool {
        if threshold < 2 || window.is_zero() {
            return false;
        }
        while self
            .prompts
            .front()
            .is_some_and(|at| now.duration_since(*at) > window)
        {
            self.prompts.pop_front();
        }
        let was_over = self.prompts.len() >= threshold as usize;
        self.prompts.push_back(now);
        !was_over && self.prompts.len() >= threshold as usize
    }

    pub fn set_cleanup_after(&mut self, cleanup_after: Duration) {
        self.cleanup_after = cleanup_after;
    }

    // Hidden, not removed: the process is still alive, so removing the hook would only
    // downgrade a rich row to a bare one, and any new event un-hides it anyway.
    fn stale(&self, hook_id: &HookId, state: &HookState, now: Instant) -> bool {
        if self.cleanup_after.is_zero() {
            return false;
        }
        let busy = self
            .pending_questions
            .values()
            .any(|pending| pending.session_id == *hook_id)
            || self
                .pending_approvals
                .values()
                .any(|pending| pending.session_id == *hook_id);
        !busy
            && state
                .stopped_at
                .is_some_and(|stopped_at| now.duration_since(stopped_at) >= self.cleanup_after)
    }

    pub fn set_filter_rules(&mut self, rules: Vec<SilenceRule>, launchers: Vec<LauncherRule>) {
        self.rules = rules;
        self.launchers = launchers;
        for state in self.hooks.values_mut() {
            state.filtered = !filters::admits(
                &self.rules,
                &self.launchers,
                Subject::new(
                    &state.session.cwd,
                    state.first_prompt.as_deref(),
                    state.launcher.as_deref(),
                ),
            );
        }
        let silenced = self
            .hooks
            .iter()
            .filter(|(_, state)| state.filtered)
            .map(|(hook_id, _)| hook_id.clone())
            .collect::<Vec<_>>();
        for hook_id in silenced {
            self.drop_pending(&hook_id);
        }
    }

    pub fn apply_hook_event(&mut self, event: HookEvent) {
        self.apply_hook_event_at(event, Instant::now());
    }

    pub fn apply_hook_event_at(&mut self, event: HookEvent, now: Instant) {
        self.apply_hook_event_at_wall(event, now, wall_clock_ms());
    }

    /// `now` drives the attention and idle logic and stays monotonic; `wall_ms` is the
    /// absolute stamp the island renders the elapsed badge from. Separate parameters so a
    /// test can pin the stamp without two transitions landing in the same millisecond.
    pub fn apply_hook_event_at_wall(&mut self, event: HookEvent, now: Instant, wall_ms: u64) {
        if event.event == HookEventKind::SessionEnd {
            self.remove_hook(&event.session_id);
            return;
        }

        let hook_id = event.session_id.clone();
        let approval_id = event.approval_id.clone();
        let prompt = event.prompt.clone();
        let opened_question = match question_intent(&event) {
            Some(QuestionIntent::Open(id)) => Some(id.to_owned()),
            _ => None,
        };
        let resolved_question = match question_intent(&event) {
            Some(QuestionIntent::Resolve(id)) => Some(id.to_owned()),
            _ => None,
        };
        {
            let state = self
                .hooks
                .entry(hook_id.clone())
                .or_insert_with(|| HookState {
                    session: initial_session(&event, &hook_id),
                    last_seen: now,
                    stopped_at: None,
                    seen_at: None,
                    reminded_at: None,
                    waiting_since: None,
                    waiting_reminded_at: None,
                    branch_cwd: String::new(),
                    first_prompt: None,
                    launcher: None,
                    filtered: false,
                    prior_mode: None,
                });

            let from_subagent = event.agent_id.is_some()
                && matches!(
                    event.event,
                    HookEventKind::PreToolUse
                        | HookEventKind::PostToolUse
                        | HookEventKind::SubagentStop
                );

            state.last_seen = now;
            if !matches!(event.event, HookEventKind::Status) && !from_subagent {
                state.stopped_at = None;
            }
            state.session.agent = event.agent.clone();
            if let Some(cwd) = event.cwd {
                if state.branch_cwd != cwd {
                    state.branch_cwd = cwd.clone();
                    state.session.branch = naming::branch_of(Path::new(&cwd));
                }
                state.session.cwd = cwd;
            }
            if let Some(pid) = event.pid {
                state.session.pid = pid;
            }
            if let Some(summary) = event
                .summary
                .or(event.prompt)
                .filter(|text| !naming::is_system_prompt(text))
            {
                state.session.summary = Some(naming::summarize(&summary));
            }
            if let Some(status) = event.status {
                state.session.status = Some(status);
            }
            if let Some(mode) = event.mode.or(event.permission_mode) {
                if mode != PLAN_MODE {
                    state.prior_mode = Some(mode.clone());
                }
                state.session.mode = Some(mode);
            }
            if let Some(model) = event.model {
                state.session.model = Some(model);
            }
            if let Some(effort) = event.effort {
                state.session.effort = Some(effort);
            }
            if let Some(prompt) = prompt.as_deref() {
                if state.first_prompt.is_none() {
                    state.first_prompt = Some(prompt.to_owned());
                }
                if state.session.name.is_none() {
                    state.session.name = naming::derive_name(prompt);
                }
            }

            if stamps_activity(&event.event) && !from_subagent {
                state.session.since_ms = Some(wall_ms);
            }

            match event.event {
                HookEventKind::SessionStart => {}
                HookEventKind::SessionEnd => return,
                HookEventKind::UserPromptSubmit => {
                    state.session.last_message = None;
                    state.session.last_message_body = None;
                    state.session.subagents = None;
                }
                HookEventKind::PreToolUse => {
                    if let Some(tasks) =
                        todo_list(event.tool_name.as_deref(), event.tool_input.as_ref())
                    {
                        state.session.tasks = Some(tasks);
                    }
                    if let Some(agent_id) = event.agent_id.as_deref() {
                        working_subagent(
                            &mut state.session.subagents,
                            agent_id,
                            event.tool_name.as_deref(),
                            tool_argument(event.tool_input.as_ref()),
                        );
                    } else {
                        state.session.current_tool = event.tool_name;
                    }
                }
                HookEventKind::PostToolUse => {
                    if let Some(tasks) =
                        todo_list(event.tool_name.as_deref(), event.tool_input.as_ref())
                    {
                        state.session.tasks = Some(tasks);
                    }
                    if let Some(agent_id) = event.agent_id.as_deref() {
                        working_subagent(&mut state.session.subagents, agent_id, None, None);
                    } else {
                        state.session.current_tool = None;
                        if let Some(spawned) = spawned_subagent(
                            event.tool_name.as_deref(),
                            event.tool_input.as_ref(),
                            event.tool_response.as_ref(),
                            wall_ms,
                        ) {
                            let known = state.session.subagents.get_or_insert_with(Vec::new);
                            if known.len() < MAX_SUBAGENTS
                                && !known.iter().any(|entry| entry.id == spawned.id)
                            {
                                known.push(spawned);
                            }
                        }
                    }
                }
                HookEventKind::SubagentStop => {
                    if let Some(agent_id) = event.agent_id.as_deref() {
                        finish_subagent(&mut state.session.subagents, agent_id);
                    }
                }
                HookEventKind::Stop => {
                    state.session.current_tool = None;
                    state.stopped_at = Some(now);
                    if let Some(answer) = event.last_message.as_deref() {
                        if let Some(spoken) = naming::spoken_line(answer) {
                            state.session.last_message = Some(spoken);
                        }
                        if let Some(body) = naming::transcript_body(answer) {
                            state.session.last_message_body = Some(body);
                        }
                    }
                }
                HookEventKind::Status => {}
                HookEventKind::QuestionAsked | HookEventKind::QuestionAnswered => {}
                HookEventKind::PermissionRequest => {
                    if opened_question.is_none() {
                        state.session.permission_state = Some(PermissionState::Pending);
                    }
                }
            }
            if opened_question.is_some() {
                state.session.question_state = Some(QuestionState::Pending);
            }
        }
        if let Some(state) = self.hooks.get_mut(&hook_id) {
            state.filtered = !filters::admits(
                &self.rules,
                &self.launchers,
                Subject::new(
                    &state.session.cwd,
                    state.first_prompt.as_deref(),
                    state.launcher.as_deref(),
                ),
            );
            if state.filtered {
                self.drop_pending(&hook_id);
                return;
            }
        }
        if let Some(approval_id) = approval_id {
            self.add_approval(approval_id, hook_id.clone(), now);
        }
        if let Some(question_id) = opened_question {
            self.add_question(question_id, hook_id, now);
        } else if let Some(question_id) = resolved_question {
            self.resolve_question(&question_id, QuestionOutcome::Answered);
        }
    }

    pub fn resolve_approval(&mut self, approval_id: &str, decision: ApprovalDecision) {
        let Some(pending) = self.pending_approvals.remove(approval_id) else {
            return;
        };
        self.approval_order.retain(|id| id != approval_id);
        if let Some(state) = self.hooks.get_mut(&pending.session_id) {
            state.session.permission_state = Some(match decision {
                ApprovalDecision::Allow | ApprovalDecision::AllowAlways => PermissionState::Allowed,
                ApprovalDecision::Deny => PermissionState::Denied,
            });
        }
        self.settle_waiting(&pending.session_id);
    }

    pub fn withdraw_approval(&mut self, approval_id: &str) {
        let Some(pending) = self.pending_approvals.remove(approval_id) else {
            return;
        };
        self.approval_order.retain(|id| id != approval_id);
        if let Some(state) = self.hooks.get_mut(&pending.session_id) {
            state.session.permission_state = Some(PermissionState::Unknown);
        }
        self.settle_waiting(&pending.session_id);
    }

    pub fn resolve_question(&mut self, question_id: &str, outcome: QuestionOutcome) {
        let Some(pending) = self.pending_questions.remove(question_id) else {
            return;
        };
        self.question_order.retain(|id| id != question_id);
        if let Some(state) = self.hooks.get_mut(&pending.session_id) {
            state.session.question_state = Some(match outcome {
                QuestionOutcome::Answered => QuestionState::Answered,
                QuestionOutcome::Cancelled | QuestionOutcome::Expired => QuestionState::Expired,
            });
        }
        self.settle_waiting(&pending.session_id);
    }

    pub fn session_label(&self, hook_id: &HookId) -> Option<String> {
        self.hooks.get(hook_id).map(|state| {
            state
                .session
                .name
                .clone()
                .unwrap_or_else(|| state.session.title.clone())
        })
    }

    pub fn mark_seen(&mut self, hook_id: &HookId, now: Instant) {
        if let Some(state) = self.hooks.get_mut(hook_id) {
            state.seen_at = Some(now);
        }
    }

    pub fn disconnect_hook(&mut self, hook_id: &HookId) {
        self.remove_hook(hook_id);
    }

    pub fn take_idle_reminders(
        &mut self,
        now: Instant,
        after: Duration,
        scopes: ReminderScopes,
    ) -> Vec<IdleReminder> {
        if after.is_zero() || !scopes.any() {
            return Vec::new();
        }
        let waiting: HashSet<HookId> = self
            .pending_questions
            .values()
            .map(|pending| pending.session_id.clone())
            .chain(
                self.pending_approvals
                    .values()
                    .map(|pending| pending.session_id.clone()),
            )
            .collect();
        let mut due = Vec::new();
        for (hook_id, state) in &mut self.hooks {
            if state.filtered {
                continue;
            }
            let waited = if waiting.contains(hook_id) {
                if !scopes.needs_response {
                    continue;
                }
                let Some(waiting_since) = state.waiting_since else {
                    continue;
                };
                if state
                    .waiting_reminded_at
                    .is_some_and(|reminded_at| reminded_at >= waiting_since)
                {
                    continue;
                }
                let waited = now.duration_since(waiting_since);
                if waited < after {
                    continue;
                }
                state.waiting_reminded_at = Some(now);
                waited
            } else {
                if !scopes.completed_tasks {
                    continue;
                }
                let Some(stopped_at) = state.stopped_at else {
                    continue;
                };
                if state.seen_at.is_some_and(|seen_at| seen_at >= stopped_at) {
                    continue;
                }
                if state
                    .reminded_at
                    .is_some_and(|reminded_at| reminded_at >= stopped_at)
                {
                    continue;
                }
                let waited = now.duration_since(stopped_at);
                if waited < after {
                    continue;
                }
                state.reminded_at = Some(now);
                waited
            };
            due.push(IdleReminder {
                session_id: hook_id.clone(),
                agent: state.session.agent.clone(),
                label: state
                    .session
                    .name
                    .clone()
                    .unwrap_or_else(|| state.session.title.clone()),
                waited,
            });
        }
        due.sort_by(|left, right| left.session_id.as_str().cmp(right.session_id.as_str()));
        due
    }

    pub fn snapshot(&mut self, processes: &[Session]) -> Vec<Session> {
        self.snapshot_at(processes, Instant::now())
    }

    pub fn snapshot_at(&mut self, processes: &[Session], now: Instant) -> Vec<Session> {
        let mut used = vec![false; processes.len()];
        let mut missing_hooks = Vec::new();
        let mut joined = Vec::new();
        let mut sessions = Vec::new();
        for (hook_id, state) in &self.hooks {
            let process_index = find_process(&used, processes, |process| {
                process.agent == state.session.agent && process.pid == state.session.pid
            })
            .or_else(|| {
                find_process(&used, processes, |process| {
                    process.agent == state.session.agent && process.cwd == state.session.cwd
                })
            });
            let Some(index) = process_index else {
                missing_hooks.push(hook_id.clone());
                continue;
            };
            used[index] = true;
            joined.push((hook_id.clone(), index));
        }
        let mut silenced = Vec::new();
        for (hook_id, index) in &joined {
            let launcher = processes[*index].launcher.clone();
            let Some(state) = self.hooks.get_mut(hook_id) else {
                continue;
            };
            if state.launcher == launcher {
                continue;
            }
            state.launcher = launcher;
            state.filtered = !filters::admits(
                &self.rules,
                &self.launchers,
                Subject::new(
                    &state.session.cwd,
                    state.first_prompt.as_deref(),
                    state.launcher.as_deref(),
                ),
            );
            if state.filtered {
                silenced.push(hook_id.clone());
            }
        }
        for hook_id in silenced {
            self.drop_pending(&hook_id);
        }
        for (hook_id, index) in &joined {
            let Some(state) = self.hooks.get(hook_id) else {
                continue;
            };
            if state.filtered || self.stale(hook_id, state, now) {
                continue;
            }
            let attention = self.attention_of(hook_id, state, now);
            sessions.push(merge_session(&state.session, &processes[*index], attention));
        }
        for hook_id in missing_hooks {
            self.remove_hook(&hook_id);
        }

        sessions.extend(
            processes
                .iter()
                .enumerate()
                .filter(|(index, _)| !used[*index])
                .map(|(_, process)| process)
                .filter(|process| {
                    filters::admits(
                        &self.rules,
                        &self.launchers,
                        Subject::new(&process.cwd, None, process.launcher.as_deref()),
                    )
                })
                .cloned(),
        );
        for session in &mut sessions {
            session.queued_messages = self
                .queues
                .get(&session.id)
                .filter(|queue| !queue.messages.is_empty())
                .map(|queue| queue.messages.iter().cloned().collect());
        }
        sessions.sort_by(|left, right| {
            left.attention
                .unwrap_or(Attention::Working)
                .cmp(&right.attention.unwrap_or(Attention::Working))
                .then_with(|| left.agent.cmp(&right.agent))
                .then_with(|| left.id.cmp(&right.id))
                .then_with(|| left.cwd.cmp(&right.cwd))
                .then_with(|| left.pid.cmp(&right.pid))
        });
        sessions
    }

    fn add_approval(&mut self, approval_id: String, session_id: HookId, now: Instant) {
        if self.pending_approvals.contains_key(&approval_id) {
            return;
        }
        self.mark_waiting(&session_id, now);
        self.pending_approvals
            .insert(approval_id.clone(), PendingApproval { session_id });
        self.approval_order.push_back(approval_id);
        while self.approval_order.len() > MAX_PENDING_APPROVALS {
            let Some(oldest) = self.approval_order.pop_front() else {
                break;
            };
            self.pending_approvals.remove(&oldest);
        }
    }

    fn add_question(&mut self, question_id: String, session_id: HookId, now: Instant) {
        if self.pending_questions.contains_key(&question_id) {
            return;
        }
        self.mark_waiting(&session_id, now);
        self.pending_questions
            .insert(question_id.clone(), PendingQuestion { session_id });
        self.question_order.push_back(question_id);
        while self.question_order.len() > MAX_PENDING_APPROVALS {
            let Some(oldest) = self.question_order.pop_front() else {
                break;
            };
            self.pending_questions.remove(&oldest);
        }
    }

    fn attention_of(&self, hook_id: &HookId, state: &HookState, now: Instant) -> Attention {
        let waiting = self
            .pending_questions
            .values()
            .any(|pending| pending.session_id == *hook_id)
            || self
                .pending_approvals
                .values()
                .any(|pending| pending.session_id == *hook_id);
        if waiting {
            return Attention::WaitingForInput;
        }
        let Some(stopped_at) = state.stopped_at else {
            return Attention::Working;
        };
        let seen = state.seen_at.is_some_and(|seen_at| seen_at >= stopped_at);
        if seen || now.duration_since(stopped_at) >= self.idle_after {
            Attention::Idle
        } else {
            Attention::NeedsAttention
        }
    }

    fn remove_hook(&mut self, hook_id: &HookId) {
        self.hooks.remove(hook_id);
        self.drop_pending(hook_id);
    }

    fn is_waiting(&self, hook_id: &HookId) -> bool {
        self.pending_questions
            .values()
            .any(|pending| pending.session_id == *hook_id)
            || self
                .pending_approvals
                .values()
                .any(|pending| pending.session_id == *hook_id)
    }

    fn mark_waiting(&mut self, hook_id: &HookId, now: Instant) {
        if let Some(state) = self.hooks.get_mut(hook_id) {
            if state.waiting_since.is_none() {
                state.waiting_since = Some(now);
            }
        }
    }

    fn settle_waiting(&mut self, hook_id: &HookId) {
        if self.is_waiting(hook_id) {
            return;
        }
        if let Some(state) = self.hooks.get_mut(hook_id) {
            state.waiting_since = None;
            state.waiting_reminded_at = None;
        }
    }

    fn drop_pending(&mut self, hook_id: &HookId) {
        let approvals = self
            .pending_approvals
            .iter()
            .filter(|(_, pending)| pending.session_id == *hook_id)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for approval_id in approvals {
            self.pending_approvals.remove(&approval_id);
            self.approval_order.retain(|id| id != &approval_id);
        }
        let questions = self
            .pending_questions
            .iter()
            .filter(|(_, pending)| pending.session_id == *hook_id)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for question_id in questions {
            self.pending_questions.remove(&question_id);
            self.question_order.retain(|id| id != &question_id);
        }
        self.settle_waiting(hook_id);
    }
}

/// The events that change what the activity line says, and therefore what the elapsed
/// badge is counting from. A `Status` event or a question does not restart the clock.
fn stamps_activity(kind: &HookEventKind) -> bool {
    matches!(
        kind,
        HookEventKind::SessionStart
            | HookEventKind::UserPromptSubmit
            | HookEventKind::PreToolUse
            | HookEventKind::PostToolUse
            | HookEventKind::Stop
    )
}

fn spawned_subagent(
    tool_name: Option<&str>,
    tool_input: Option<&serde_json::Value>,
    tool_response: Option<&serde_json::Value>,
    wall_ms: u64,
) -> Option<Subagent> {
    if !SUBAGENT_TOOLS.contains(&tool_name?) {
        return None;
    }
    let id = tool_response?.get("agentId")?.as_str()?;
    let field = |name: &str| {
        tool_input
            .and_then(|input| input.get(name))
            .and_then(|value| value.as_str())
    };
    Some(Subagent {
        id: id.to_owned(),
        kind: field("subagent_type").unwrap_or("agent").to_owned(),
        description: field("description").map(naming::summarize),
        tool: None,
        summary: None,
        since_ms: Some(wall_ms),
        done: false,
    })
}

fn subagent<'a>(
    subagents: &'a mut Option<Vec<Subagent>>,
    agent_id: &str,
) -> Option<&'a mut Subagent> {
    subagents
        .as_mut()?
        .iter_mut()
        .find(|entry| entry.id == agent_id)
}

fn tool_argument(tool_input: Option<&serde_json::Value>) -> Option<String> {
    let input = tool_input?;
    ["command", "file_path", "pattern", "path", "url"]
        .into_iter()
        .find_map(|key| input.get(key).and_then(|value| value.as_str()))
        .filter(|text| !text.is_empty())
        .map(naming::summarize)
}

fn working_subagent(
    subagents: &mut Option<Vec<Subagent>>,
    agent_id: &str,
    tool: Option<&str>,
    argument: Option<String>,
) {
    if let Some(entry) = subagent(subagents, agent_id) {
        entry.tool = tool.map(str::to_owned);
        entry.summary = argument;
    }
}

fn finish_subagent(subagents: &mut Option<Vec<Subagent>>, agent_id: &str) {
    if let Some(entry) = subagent(subagents, agent_id) {
        entry.tool = None;
        entry.summary = None;
        entry.done = true;
    }
}

fn wall_clock_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

fn initial_session(event: &HookEvent, hook_id: &HookId) -> Session {
    let mut session = Session::new(
        &event.agent,
        event.cwd.as_deref().unwrap_or(""),
        event.pid.unwrap_or(0),
        "",
    );
    session.id = hook_id.to_string();
    session.hook_id = Some(hook_id.clone());
    if let Some(agent_session_id) = event.agent_session_id.as_deref() {
        session = session.with_hook_id(agent_session_id);
    }
    session
}

fn merge_session(hook: &Session, process: &Session, attention: Attention) -> Session {
    let mut merged = process.clone();
    merged.id = hook.id.clone();
    merged.hook_id = hook.hook_id.clone();
    merged.status = hook.status.clone();
    merged.current_tool = hook.current_tool.clone();
    merged.summary = hook.summary.clone();
    merged.last_message = hook.last_message.clone();
    merged.last_message_body = hook.last_message_body.clone();
    merged.mode = hook.mode.clone();
    merged.subagents = hook.subagents.clone();
    merged.tasks = hook.tasks.clone();
    merged.permission_state = hook.permission_state.clone();
    merged.question_state = hook.question_state.clone();
    merged.attention = Some(attention);
    merged.name = hook.name.clone();
    merged.branch = hook.branch.clone();
    merged.model = hook.model.clone();
    merged.effort = hook.effort.clone();
    merged.since_ms = hook.since_ms;
    merged
}

fn find_process<F>(used: &[bool], processes: &[Session], predicate: F) -> Option<usize>
where
    F: Fn(&Session) -> bool,
{
    processes
        .iter()
        .enumerate()
        .find_map(|(index, process)| (!used[index] && predicate(process)).then_some(index))
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
