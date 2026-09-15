use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

pub const MAX_LIVE_RECORDS: usize = 256;
pub const MAX_LIVE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_SESSION_RECORDS: usize = 32;
const RESULT_TTL_MS: u64 = 5 * 60 * 1000;
pub const ORPHAN_TTL_MS: u64 = 10 * 60 * 1000;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DaemonEpoch(pub String);
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionInstanceId(pub String);
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClientSubmissionId(pub String);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PendingGeneration(pub u64);
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryIdentity {
    pub daemon_epoch: DaemonEpoch,
    pub session_instance_id: SessionInstanceId,
}
impl DeliveryIdentity {
    pub fn for_process(
        session: &crate::session::Session,
        birth: crate::process::ProcessBirthIdentity,
        epoch: &DaemonEpoch,
    ) -> Self {
        Self {
            daemon_epoch: epoch.clone(),
            session_instance_id: SessionInstanceId(format!(
                "{:?}:{}:{}:{}",
                (&session.agent, &session.hook_id),
                session.pid,
                birth.0,
                session.hook_generation
            )),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    Queued,
    Sending,
    Delivered,
    Failed,
    Unconfirmed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageDelivery {
    pub message_id: u64,
    pub session_id: String,
    pub text: String,
    pub queued_at_ms: u64,
    pub state: DeliveryState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<DeliveryIdentity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_submission_id: Option<ClientSubmissionId>,
}
#[derive(Clone, Debug)]
struct Record {
    delivery: MessageDelivery,
    settled_at: Option<u64>,
    orphaned_at: Option<u64>,
}
#[derive(Clone, Debug)]
pub struct DeliveryStore {
    records: VecDeque<Record>,
    armed: HashMap<String, bool>,
    next_message: u64,
    next_attempt: u64,
}
impl Default for DeliveryStore {
    fn default() -> Self {
        Self {
            records: VecDeque::new(),
            armed: HashMap::new(),
            next_message: 1,
            next_attempt: 1,
        }
    }
}
impl DeliveryStore {
    pub fn admit(
        &mut self,
        session_id: &str,
        text: String,
        now: u64,
        stopped: bool,
        identity: Option<DeliveryIdentity>,
        submission: Option<ClientSubmissionId>,
    ) -> Result<MessageDelivery, &'static str> {
        if let Some(submission) = &submission {
            if let Some(existing) = self
                .records
                .iter()
                .find(|r| r.delivery.client_submission_id.as_ref() == Some(submission))
            {
                if existing.delivery.session_id != session_id
                    || existing.delivery.identity != identity
                {
                    return Err("submission_conflict");
                }
                return Ok(existing.delivery.clone());
            }
        }
        let live = self
            .records
            .iter()
            .filter(|record| record.delivery.state != DeliveryState::Delivered)
            .collect::<Vec<_>>();
        if live.len() >= MAX_LIVE_RECORDS
            || live
                .iter()
                .filter(|record| record.delivery.session_id == session_id)
                .count()
                >= MAX_SESSION_RECORDS
            || live
                .iter()
                .map(|r| r.delivery.text.len())
                .sum::<usize>()
                .saturating_add(text.len())
                > MAX_LIVE_BYTES
        {
            return Err("queue_full");
        }
        let next = self
            .next_message
            .checked_add(1)
            .ok_or("message_id_exhausted")?;
        let delivery = MessageDelivery {
            message_id: self.next_message,
            session_id: session_id.to_owned(),
            text,
            queued_at_ms: now,
            state: DeliveryState::Queued,
            error_code: None,
            attempt_id: None,
            identity,
            client_submission_id: submission,
        };
        self.next_message = next;
        self.records.push_back(Record {
            delivery: delivery.clone(),
            settled_at: None,
            orphaned_at: None,
        });
        if stopped {
            self.armed.insert(session_id.to_owned(), true);
        }
        Ok(delivery)
    }
    pub fn orphan_session(&mut self, session_id: &str, now: u64) {
        for record in &mut self.records {
            if record.delivery.session_id == session_id {
                record.orphaned_at.get_or_insert(now);
            }
        }
        self.armed.remove(session_id);
    }
    pub fn observe(&mut self, sessions: &[(&str, bool)], now: u64) {
        let active = sessions.iter().map(|(id, _)| *id).collect::<HashSet<_>>();
        for (id, stopped) in sessions {
            if !stopped {
                self.armed.insert((*id).to_owned(), true);
            }
        }
        for record in &mut self.records {
            if !active.contains(record.delivery.session_id.as_str()) {
                record.orphaned_at.get_or_insert(now);
            }
        }
        self.prune(now);
    }
    pub fn reserve(&mut self, session: &str) -> Result<Option<MessageDelivery>, &'static str> {
        if !self.armed.get(session).copied().unwrap_or(false)
            || self.records.iter().any(|r| {
                r.delivery.session_id == session && r.delivery.state == DeliveryState::Sending
            })
        {
            return Ok(None);
        }
        let Some(record) = self.records.iter_mut().find(|r| {
            r.delivery.session_id == session
                && r.delivery.state == DeliveryState::Queued
                && r.orphaned_at.is_none()
        }) else {
            return Ok(None);
        };
        let next = self
            .next_attempt
            .checked_add(1)
            .ok_or("attempt_id_exhausted")?;
        record.delivery.attempt_id = Some(self.next_attempt);
        record.delivery.state = DeliveryState::Sending;
        self.next_attempt = next;
        self.armed.insert(session.to_owned(), false);
        Ok(Some(record.delivery.clone()))
    }
    pub fn rollback(&mut self, id: u64, attempt: u64) -> bool {
        let Some(record) = self.records.iter_mut().find(|r| {
            r.delivery.message_id == id
                && r.delivery.attempt_id == Some(attempt)
                && r.delivery.state == DeliveryState::Sending
        }) else {
            return false;
        };
        record.delivery.state = DeliveryState::Queued;
        record.delivery.attempt_id = None;
        self.armed.insert(record.delivery.session_id.clone(), true);
        true
    }
    pub fn settle(
        &mut self,
        id: u64,
        attempt: u64,
        outcome: DeliveryState,
        error: Option<String>,
        now: u64,
    ) -> bool {
        if !matches!(
            outcome,
            DeliveryState::Delivered | DeliveryState::Failed | DeliveryState::Unconfirmed
        ) {
            return false;
        }
        let Some(record) = self.records.iter_mut().find(|r| {
            r.delivery.message_id == id
                && r.delivery.attempt_id == Some(attempt)
                && r.delivery.state == DeliveryState::Sending
        }) else {
            return false;
        };
        record.delivery.state = outcome;
        record.delivery.error_code = error;
        record.settled_at = Some(now);
        if outcome == DeliveryState::Delivered {
            record.delivery.text.clear();
        }
        self.prune(now);
        true
    }
    pub fn cancel_guarded(
        &mut self,
        session: &str,
        id: u64,
        identity: &DeliveryIdentity,
    ) -> Result<(), &'static str> {
        let record = self
            .records
            .iter()
            .find(|r| r.delivery.message_id == id)
            .ok_or("message_not_found")?;
        if record.delivery.session_id != session
            || record.delivery.identity.as_ref() != Some(identity)
        {
            return Err("stale_session");
        }
        self.cancel(session, id).map(|_| ())
    }
    pub fn cancel(&mut self, session: &str, id: u64) -> Result<bool, &'static str> {
        let Some(index) = self
            .records
            .iter()
            .position(|r| r.delivery.session_id == session && r.delivery.message_id == id)
        else {
            return Ok(false);
        };
        if self.records[index].delivery.state == DeliveryState::Sending {
            return Err("delivery_in_progress");
        }
        if self.records[index].delivery.state == DeliveryState::Delivered {
            return Err("already_delivered");
        }
        self.records.remove(index);
        self.clean_armed();
        Ok(true)
    }
    pub fn snapshot(&self) -> Vec<MessageDelivery> {
        self.records.iter().map(|r| r.delivery.clone()).collect()
    }
    pub fn queued(&self, session: &str) -> Vec<crate::session::QueuedMessage> {
        self.records
            .iter()
            .filter(|r| {
                r.delivery.session_id == session
                    && r.delivery.state == DeliveryState::Queued
                    && r.orphaned_at.is_none()
            })
            .map(|r| crate::session::QueuedMessage {
                id: r.delivery.message_id,
                text: r.delivery.text.clone(),
                queued_at_ms: r.delivery.queued_at_ms,
            })
            .collect()
    }
    fn prune(&mut self, now: u64) {
        self.records.retain(|r| {
            !r.orphaned_at
                .is_some_and(|at| now.saturating_sub(at) >= ORPHAN_TTL_MS)
                && !(r.delivery.state == DeliveryState::Delivered
                    && r.settled_at
                        .is_some_and(|at| now.saturating_sub(at) >= RESULT_TTL_MS))
        });
        let mut count = 0;
        let mut sessions = HashMap::<String, usize>::new();
        let mut keep = HashSet::new();
        let mut delivered = self
            .records
            .iter()
            .filter(|r| r.delivery.state == DeliveryState::Delivered)
            .collect::<Vec<_>>();
        delivered.sort_by_key(|r| (r.settled_at, r.delivery.attempt_id));
        for record in delivered.into_iter().rev() {
            let session = sessions
                .entry(record.delivery.session_id.clone())
                .or_default();
            if count < 256 && *session < 64 {
                keep.insert(record.delivery.message_id);
                count += 1;
                *session += 1;
            }
        }
        self.records.retain(|r| {
            r.delivery.state != DeliveryState::Delivered || keep.contains(&r.delivery.message_id)
        });
        self.clean_armed();
    }
    fn clean_armed(&mut self) {
        self.armed.retain(|id, _| {
            self.records
                .iter()
                .any(|r| &r.delivery.session_id == id && r.orphaned_at.is_none())
        });
    }
}
