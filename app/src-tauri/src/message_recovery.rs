use open_island_core::message_delivery::{
    ClientSubmissionId, DaemonEpoch, DeliveryIdentity, DeliveryState, MessageDelivery,
    SessionInstanceId,
};
use serde::Serialize;
use std::collections::BTreeMap;

const MAX_RECORDS: usize = 256;
const MAX_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
pub struct RecoveryRecord {
    pub client_submission_id: ClientSubmissionId,
    pub origin_epoch: DaemonEpoch,
    pub session_instance_id: SessionInstanceId,
    pub text: String,
    pub last_state: DeliveryState,
    pub message_id: Option<u64>,
    pub previous_state: Option<DeliveryState>,
    pub local: bool,
    pub previous_epoch: bool,
}
pub struct MessageRecovery {
    prefix: String,
    next: u64,
    records: BTreeMap<String, RecoveryRecord>,
}
impl MessageRecovery {
    pub fn new() -> std::io::Result<Self> {
        Ok(Self {
            prefix: open_island_core::epoch::generate()?.0,
            next: 1,
            records: BTreeMap::new(),
        })
    }
    pub fn reserve(
        &mut self,
        identity: &DeliveryIdentity,
        text: String,
    ) -> Result<ClientSubmissionId, String> {
        open_island_core::input_bridge::validate_text(&text)?;
        if text.trim().is_empty() {
            return Err("empty_message".into());
        }
        if self.records.len() >= MAX_RECORDS
            || self
                .records
                .values()
                .map(|r| r.text.len())
                .sum::<usize>()
                .saturating_add(text.len())
                > MAX_BYTES
        {
            return Err("recovery_full".into());
        }
        let next = self.next.checked_add(1).ok_or("submission_id_exhausted")?;
        let id = ClientSubmissionId(format!("{}-{}", self.prefix, self.next));
        self.next = next;
        self.records.insert(
            id.0.clone(),
            RecoveryRecord {
                client_submission_id: id.clone(),
                origin_epoch: identity.daemon_epoch.clone(),
                session_instance_id: identity.session_instance_id.clone(),
                text,
                last_state: DeliveryState::Queued,
                message_id: None,
                previous_state: None,
                local: false,
                previous_epoch: false,
            },
        );
        Ok(id)
    }
    pub fn admitted(&mut self, id: &ClientSubmissionId, epoch: &DaemonEpoch, message_id: u64) {
        if let Some(record) = self.records.get_mut(&id.0) {
            if &record.origin_epoch == epoch && !record.local {
                record.message_id = Some(message_id);
            }
        }
    }
    pub fn failed(&mut self, id: &ClientSubmissionId, uncertain: bool) {
        if let Some(record) = self.records.get_mut(&id.0) {
            if !record.local {
                record.last_state = if uncertain {
                    DeliveryState::Unconfirmed
                } else {
                    DeliveryState::Failed
                };
            }
        }
    }
    pub fn reconcile(&mut self, epoch: &DaemonEpoch, deliveries: &[MessageDelivery]) {
        self.records.retain(|_, record| {
            if &record.origin_epoch != epoch {
                record.previous_epoch = true;
                if !record.local {
                    record.previous_state = Some(record.last_state);
                    record.last_state = DeliveryState::Unconfirmed;
                    record.local = true;
                }
                return true;
            }
            let current = deliveries.iter().find(|delivery| {
                delivery.client_submission_id.as_ref() == Some(&record.client_submission_id)
                    && delivery.identity.as_ref().is_some_and(|identity| {
                        identity.daemon_epoch == record.origin_epoch
                            && identity.session_instance_id == record.session_instance_id
                    })
            });
            if let Some(delivery) = current {
                if delivery.state == DeliveryState::Delivered {
                    return false;
                }
                record.last_state = delivery.state;
                record.message_id = Some(delivery.message_id);
                record.local = false;
                record.previous_state = None;
            } else if record.message_id.is_some() && !record.local {
                record.previous_state = Some(record.last_state);
                record.last_state = DeliveryState::Unconfirmed;
                record.local = true;
            }
            true
        });
    }
    pub fn discard(&mut self, id: &ClientSubmissionId) -> bool {
        self.records.remove(&id.0).is_some()
    }
    pub fn confirmed_cancel(&mut self, identity: &DeliveryIdentity, message_id: u64) {
        self.records.retain(|_, record| {
            record.origin_epoch != identity.daemon_epoch
                || record.session_instance_id != identity.session_instance_id
                || record.message_id != Some(message_id)
        });
    }
    pub fn snapshot(&self) -> Vec<RecoveryRecord> {
        self.records.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn identity(epoch: &str) -> DeliveryIdentity {
        DeliveryIdentity {
            daemon_epoch: DaemonEpoch(epoch.into()),
            session_instance_id: SessionInstanceId("session-instance".into()),
        }
    }
    fn delivery(id: ClientSubmissionId, state: DeliveryState) -> MessageDelivery {
        MessageDelivery {
            message_id: 1,
            session_id: "session".into(),
            text: "original".into(),
            queued_at_ms: 0,
            state,
            error_code: None,
            attempt_id: None,
            identity: Some(identity("old")),
            client_submission_id: Some(id),
        }
    }
    #[test]
    fn restart_queued_sending_and_before_response_preserves_original_without_rebinding() {
        for state in [
            None,
            Some(DeliveryState::Queued),
            Some(DeliveryState::Sending),
        ] {
            let mut recovery = MessageRecovery::new().unwrap();
            let id = recovery
                .reserve(&identity("old"), "original".into())
                .unwrap();
            if let Some(state) = state {
                recovery.reconcile(&DaemonEpoch("old".into()), &[delivery(id.clone(), state)]);
            }
            recovery.reconcile(&DaemonEpoch("new".into()), &[]);
            recovery.admitted(&id, &DaemonEpoch("old".into()), 99);
            let mut recycled = delivery(id.clone(), DeliveryState::Delivered);
            recycled.identity = Some(identity("new"));
            recovery.reconcile(&DaemonEpoch("new".into()), &[recycled]);
            let record = recovery.snapshot().pop().unwrap();
            assert_eq!(record.text, "original");
            assert_eq!(record.last_state, DeliveryState::Unconfirmed);
            assert_eq!(
                record.previous_state,
                Some(state.unwrap_or(DeliveryState::Queued))
            );
            assert!(record.local);
            assert_ne!(record.message_id, Some(99));
            assert!(recovery.discard(&id));
        }
    }
    #[test]
    fn webview_reload_reads_same_shell_ledger_and_only_matching_delivery_releases_slot() {
        let mut shell = MessageRecovery::new().unwrap();
        let id = shell.reserve(&identity("old"), "original".into()).unwrap();
        let first_webview = shell.snapshot();
        drop(first_webview);
        assert_eq!(shell.snapshot()[0].text, "original");
        let mut other = delivery(id.clone(), DeliveryState::Delivered);
        other.identity.as_mut().unwrap().session_instance_id =
            SessionInstanceId("different".into());
        shell.reconcile(&DaemonEpoch("old".into()), &[other]);
        assert_eq!(shell.snapshot().len(), 1);
        shell.reconcile(
            &DaemonEpoch("old".into()),
            &[delivery(id, DeliveryState::Delivered)],
        );
        assert!(shell.snapshot().is_empty());
    }
    #[test]
    fn missing_admitted_record_stays_visible_and_a_late_matching_confirmation_releases_it() {
        let mut ledger = MessageRecovery::new().unwrap();
        let epoch = DaemonEpoch("old".into());
        let id = ledger.reserve(&identity("old"), "original".into()).unwrap();
        ledger.reconcile(&epoch, &[delivery(id.clone(), DeliveryState::Queued)]);
        ledger.reconcile(&epoch, &[]);
        let record = &ledger.snapshot()[0];
        assert!(record.local);
        assert!(!record.previous_epoch);
        assert_eq!(record.last_state, DeliveryState::Unconfirmed);
        assert_eq!(record.previous_state, Some(DeliveryState::Queued));
        assert_eq!(record.text, "original");
        ledger.reconcile(&epoch, &[delivery(id, DeliveryState::Delivered)]);
        assert!(ledger.snapshot().is_empty());
    }
    #[test]
    fn confirmed_discard_releases_missing_record_only_with_exact_origin() {
        let mut ledger = MessageRecovery::new().unwrap();
        let id = ledger.reserve(&identity("old"), "original".into()).unwrap();
        ledger.reconcile(
            &DaemonEpoch("old".into()),
            &[delivery(id, DeliveryState::Queued)],
        );
        ledger.reconcile(&DaemonEpoch("old".into()), &[]);
        assert!(ledger.snapshot()[0].local);
        ledger.confirmed_cancel(&identity("new"), 1);
        ledger.confirmed_cancel(&identity("old"), 2);
        assert_eq!(ledger.snapshot().len(), 1);
        ledger.confirmed_cancel(&identity("old"), 1);
        assert!(ledger.snapshot().is_empty());
    }
    #[test]
    fn recovery_capacity_refuses_before_submission_and_discard_releases_capacity() {
        for large in [false, true] {
            let mut ledger = MessageRecovery::new().unwrap();
            let count = if large { 64 } else { 256 };
            let text = if large { "x".repeat(65536) } else { "x".into() };
            let mut ids = Vec::new();
            for _ in 0..count {
                ids.push(ledger.reserve(&identity("old"), text.clone()).unwrap());
            }
            let before = ledger.snapshot();
            assert_eq!(
                ledger.reserve(&identity("old"), "x".into()).unwrap_err(),
                "recovery_full"
            );
            assert_eq!(ledger.snapshot().len(), before.len());
            assert_eq!(ledger.snapshot()[0].text, before[0].text);
            ledger.discard(&ids[0]);
            assert!(ledger.reserve(&identity("old"), "x".into()).is_ok());
        }
    }
}
