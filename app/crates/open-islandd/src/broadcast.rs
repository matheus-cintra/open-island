use crate::notifications::lifecycle::{DaemonContext, SharedState};
use crate::server::wire::event_message;
use open_island_core::{discovery, protocol::EventData};

/// Delivers `message` to every subscriber, dropping failed subscribers, and
/// reports whether at least one subscriber received it.
pub fn broadcast(state: &SharedState, message: String) -> bool {
    broadcast_except(state, message, None)
}

/// Every connection is a subscriber, the caller's own included, so a request that reports
/// whether anyone is listening has to leave itself out of the count.
pub fn broadcast_except(state: &SharedState, message: String, except: Option<u64>) -> bool {
    let subscribers = state
        .lock()
        .map(|state| {
            state
                .subscribers
                .iter()
                .filter(|subscriber| Some(subscriber.connection_id) != except)
                .map(|subscriber| (subscriber.connection_id, subscriber.sender.clone()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut delivered = false;
    let failed = subscribers
        .into_iter()
        .filter_map(|(connection_id, sender)| {
            if sender.send(message.clone()).is_ok() {
                delivered = true;
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

/// The broadcast the hook path uses. It leaves the hook's own connection out of the count,
/// because every connection is a subscriber: without the exclusion an approval would always
/// look as if an island had seen it, even when the only listener was the agent asking.
pub fn make_hook_broadcast(
    state: SharedState,
    connection_id: u64,
) -> impl Fn(String) -> bool + Clone {
    move |message| broadcast_except(&state, message, Some(connection_id))
}

pub fn broadcast_sessions(ctx: &DaemonContext) {
    let processes = discovery::scan();
    let Ok(sessions) = ctx
        .state
        .lock()
        .map(|mut state| state.store.snapshot(&processes))
    else {
        return;
    };
    broadcast(
        &ctx.state,
        event_message("sessions-updated", EventData::Sessions(sessions)),
    );
}
