use super::Transport;
use open_island_core::{message_delivery::DaemonEpoch, snapshot_page, ui_state::UiSnapshot};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, Serialize)]
pub struct SyncedSnapshot {
    pub generation: u64,
    pub snapshot: UiSnapshot,
}
#[derive(Deserialize)]
struct Begin {
    snapshot_id: u64,
    daemon_epoch: DaemonEpoch,
    publication_revision: u64,
}
impl Transport {
    #[cfg(test)]
    pub fn ui_snapshot(&self) -> Result<SyncedSnapshot, String> {
        self.ui_snapshot_until(|| false)
    }
    pub fn ui_snapshot_until(
        &self,
        cancelled: impl Fn() -> bool,
    ) -> Result<SyncedSnapshot, String> {
        if cancelled() {
            return Err("snapshot_cancelled".into());
        }
        let generation = self.generation()?;
        let begin: Begin = serde_json::from_value(self.request("get_ui_state", json!({}))?)
            .map_err(|_| "invalid_snapshot_begin")?;
        let result = (|| {
            let snapshot: UiSnapshot = snapshot_page::decode(begin.snapshot_id, |expected| {
                if cancelled() || self.generation()? != generation {
                    return Err("stale_connection".into());
                }
                serde_json::from_value(self.request(
                    "get_ui_state_page",
                    json!({"snapshot_id":begin.snapshot_id,"expected_page":expected}),
                )?)
                .map_err(|_| "invalid_snapshot_page".into())
            })?;
            if cancelled() || self.generation()? != generation {
                return Err("stale_connection".into());
            }
            if snapshot.schema_version != 1
                || snapshot.daemon_epoch != begin.daemon_epoch
                || snapshot.publication_revision != begin.publication_revision
            {
                return Err("invalid_snapshot_identity".into());
            }
            Ok(SyncedSnapshot {
                generation,
                snapshot,
            })
        })();
        if result.is_err() && self.generation() == Ok(generation) {
            let _ = self.request("cancel_ui_state", json!({}));
        }
        result
    }
    fn generation(&self) -> Result<u64, String> {
        let state = self
            .shared
            .state
            .lock()
            .map_err(|_| "transport_unavailable")?;
        state.writer.as_ref().ok_or("daemon_unavailable")?;
        Ok(state.generation)
    }
}
