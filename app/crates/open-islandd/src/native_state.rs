//! Short-lived state reported by the macOS GUI, which owns the Focus permission.
use serde::Deserialize;
use std::sync::Mutex;
use std::time::{Duration, Instant};
const TTL: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub focus: Option<bool>,
}
#[derive(Default)]
struct State {
    report: Report,
    received: Option<Instant>,
}
impl State {
    fn focus(&self, now: Instant) -> bool {
        self.received
            .is_some_and(|received| now.saturating_duration_since(received) < TTL)
            && self.report.focus == Some(true)
    }
}
static STATE: Mutex<State> = Mutex::new(State {
    report: Report { focus: None },
    received: None,
});

pub fn report(report: Report) {
    let mut state = STATE.lock().unwrap_or_else(|poison| poison.into_inner());
    *state = State {
        report,
        received: Some(Instant::now()),
    };
}
pub fn focus() -> bool {
    STATE
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .focus(Instant::now())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disconnected_or_restarted_gui_cannot_leave_focus_stuck_on() {
        let now = Instant::now();
        let state = State {
            report: Report { focus: Some(true) },
            received: Some(now),
        };
        assert!(state.focus(now + Duration::from_secs(2)));
        assert!(!state.focus(now + TTL));
        assert!(!State::default().focus(now));
    }
    #[test]
    fn unknown_or_denied_permission_is_not_a_silenced_state() {
        let now = Instant::now();
        for focus in [None, Some(false)] {
            assert!(!State {
                report: Report { focus },
                received: Some(now)
            }
            .focus(now));
        }
    }
}
