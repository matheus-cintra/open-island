use crate::notifications::lifecycle::{DaemonContext, SharedState};
use crate::server::wire::event_message;
use open_island_core::protocol::EventData;

/// Delivers `message` to every subscriber, dropping failed subscribers, and
/// reports whether at least one subscriber received it.
pub fn broadcast(state: &SharedState, message: String) -> bool {
    broadcast_except(state, message, None)
}

/// Legacy listeners and subscribed UIs count as action surfaces. Hook connections
/// can receive legacy events but do not count; diagnostic clients receive no events.
pub fn broadcast_except(state: &SharedState, message: String, except: Option<u64>) -> bool {
    let state_event = serde_json::from_str::<serde_json::Value>(&message)
        .ok()
        .and_then(|value| {
            value
                .get("event")
                .and_then(|value| value.as_str())
                .map(str::to_owned)
        })
        .is_some_and(|event| {
            !matches!(
                event.as_str(),
                "island-toggle" | "open-settings" | "question-focus"
            )
        });
    let subscribers = state
        .lock()
        .map(|mut state| {
            state.publication_revision = state.publication_revision.saturating_add(1);
            state
                .subscribers
                .iter()
                .filter(|subscriber| !subscriber.diagnostic_only && Some(subscriber.connection_id) != except)
                .map(|subscriber| {
                    let invalidation = subscriber.ui_epoch.as_ref().filter(|_| state_event).map(|epoch| {
                        serde_json::json!({"v":1,"event":"ui-state-invalidated","data":{"daemon_epoch":epoch,"publication_revision":state.publication_revision}}).to_string()
                    });
                    (subscriber.connection_id, subscriber.sender.clone(), invalidation, subscriber.receives_actions)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut delivered = false;
    let failed = subscribers
        .into_iter()
        .filter_map(|(connection_id, sender, invalidation, receives_actions)| {
            if sender
                .send(invalidation.unwrap_or_else(|| message.clone()))
                .is_ok()
            {
                delivered |= receives_actions;
                None
            } else {
                Some(connection_id)
            }
        })
        .collect::<Vec<_>>();
    if let Ok(mut state) = state.lock() {
        state
            .subscribers
            .retain(|subscriber| !failed.contains(&subscriber.connection_id));
    }
    delivered
}

/// Returns whether the message reached at least one island. The approval path needs that:
/// with nobody listening there is no surface to decide on, and holding the agent for the
/// full approval timeout only to deny it would be the island blocking the agent again.
pub fn make_broadcast(state: SharedState) -> impl Fn(String) -> bool + Clone {
    move |message| broadcast(&state, message)
}

/// The hook never counts its own connection as an available action surface.
pub fn make_hook_broadcast(
    state: SharedState,
    connection_id: u64,
) -> impl Fn(String) -> bool + Clone {
    move |message| broadcast_except(&state, message, Some(connection_id))
}

pub fn broadcast_sessions(ctx: &DaemonContext) {
    let Ok(sessions) = crate::discovery_cache::sessions(ctx) else {
        return;
    };
    broadcast(
        &ctx.state,
        event_message("sessions-updated", EventData::Sessions(sessions)),
    );
}
