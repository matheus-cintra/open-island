use crate::{
    jump::{JumpStep, LocationResolver},
    runner::CommandRunner,
    terminal::TerminalInfo,
};
pub struct WeztermResolver;
impl LocationResolver for WeztermResolver {
    fn id(&self) -> &str {
        "wezterm"
    }
    fn can_resolve(&self, host: &TerminalInfo) -> bool {
        host.terminal
            .as_ref()
            .is_some_and(|terminal| terminal.kind == "wezterm")
    }
    fn resolve(&self, host: &TerminalInfo, _runner: &dyn CommandRunner) -> Option<Vec<JumpStep>> {
        let terminal = host.terminal.as_ref()?;
        let pane_id = terminal
            .window_id
            .clone()
            .or_else(|| host.env.get("WEZTERM_PANE").cloned());
        let mut steps = Vec::with_capacity(2);
        if let Some(pane_id) = pane_id {
            steps.push(JumpStep::WeztermActivatePane { pane_id });
        }
        steps.push(JumpStep::RaiseWindow { pid: terminal.pid });
        Some(steps)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::FakeRunner;
    use crate::terminal::TerminalLayer;
    use std::collections::HashMap;

    fn host(window_id: Option<&str>, env_pane: Option<&str>) -> TerminalInfo {
        let mut env = HashMap::new();
        if let Some(pane_id) = env_pane {
            env.insert("WEZTERM_PANE".into(), pane_id.into());
        }
        TerminalInfo {
            kind: "wezterm".into(),
            raise_pid: 999,
            agent_pid: 777,
            multiplexer: None,
            terminal: Some(TerminalLayer {
                kind: "wezterm".into(),
                pid: 4242,
                window_id: window_id.map(str::to_owned),
            }),
            editor: None,
            env,
        }
    }

    #[test]
    fn pane_id_from_terminal_emits_activate_then_gui_raise() {
        let runner = FakeRunner::new();
        assert_eq!(
            WeztermResolver.resolve(&host(Some("0"), None), &runner),
            Some(vec![
                JumpStep::WeztermActivatePane {
                    pane_id: "0".into()
                },
                JumpStep::RaiseWindow { pid: 4242 },
            ])
        );
    }

    #[test]
    fn pane_id_from_environment_emits_activate_then_gui_raise() {
        let runner = FakeRunner::new();
        assert_eq!(
            WeztermResolver.resolve(&host(None, Some("7")), &runner),
            Some(vec![
                JumpStep::WeztermActivatePane {
                    pane_id: "7".into()
                },
                JumpStep::RaiseWindow { pid: 4242 },
            ])
        );
    }

    #[test]
    fn missing_pane_id_still_raises_gui_window() {
        let runner = FakeRunner::new();
        assert_eq!(
            WeztermResolver.resolve(&host(None, None), &runner),
            Some(vec![JumpStep::RaiseWindow { pid: 4242 }])
        );
    }

    #[test]
    fn activate_failure_does_not_remove_raise_step_from_plan() {
        let runner = FakeRunner::new();
        runner.push_err("wezterm", "activate-pane failed");
        assert_eq!(
            WeztermResolver.resolve(&host(Some("0"), None), &runner),
            Some(vec![
                JumpStep::WeztermActivatePane {
                    pane_id: "0".into()
                },
                JumpStep::RaiseWindow { pid: 4242 },
            ])
        );
    }

    #[test]
    fn declines_kitty_and_ghostty_hosts() {
        let kitty = TerminalInfo {
            terminal: Some(TerminalLayer {
                kind: "kitty".into(),
                pid: 1,
                window_id: None,
            }),
            ..host(None, None)
        };
        let ghostty = TerminalInfo {
            terminal: Some(TerminalLayer {
                kind: "ghostty".into(),
                pid: 2,
                window_id: None,
            }),
            ..host(None, None)
        };
        assert!(!WeztermResolver.can_resolve(&kitty));
        assert!(!WeztermResolver.can_resolve(&ghostty));
    }
}
