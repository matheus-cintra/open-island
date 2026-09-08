use super::ancestry::{emulator_ancestor, pids_with_comm_and_cmdline, proc_parent_and_comm};
use crate::{
    jump::{JumpStep, LocationResolver},
    runner::CommandRunner,
    terminal::{MultiplexerKind, TerminalInfo},
};

pub struct ZellijResolver;
impl LocationResolver for ZellijResolver {
    fn id(&self) -> &str {
        "zellij"
    }
    fn can_resolve(&self, host: &TerminalInfo) -> bool {
        host.multiplexer
            .as_ref()
            .is_some_and(|layer| layer.kind == MultiplexerKind::Zellij)
    }
    fn resolve(&self, host: &TerminalInfo, _: &dyn CommandRunner) -> Option<Vec<JumpStep>> {
        resolve_with_ancestry(host, &proc_parent_and_comm, &pids_with_comm_and_cmdline)
    }
}

fn resolve_with_ancestry(
    host: &TerminalInfo,
    parent_and_comm: &dyn Fn(u32) -> Option<(u32, String)>,
    candidates: &dyn Fn(&str, &str) -> Vec<u32>,
) -> Option<Vec<JumpStep>> {
    let multiplexer = host.multiplexer.as_ref()?;
    if multiplexer.kind != MultiplexerKind::Zellij {
        return None;
    }
    let (Some(session), Some(pane_id)) = (
        multiplexer.session_name.as_ref(),
        multiplexer.pane_id.as_ref(),
    ) else {
        return Some(vec![JumpStep::ActivateApp {
            pid: host.raise_pid,
        }]);
    };
    let raise_step = host.terminal.as_ref().map_or_else(
        || {
            candidates("zellij", session)
                .into_iter()
                .find_map(|pid| emulator_ancestor(pid, parent_and_comm))
                .map_or(
                    JumpStep::ActivateApp {
                        pid: host.raise_pid,
                    },
                    |pid| JumpStep::RaiseWindow { pid },
                )
        },
        |terminal| JumpStep::RaiseWindow { pid: terminal.pid },
    );
    Some(vec![
        JumpStep::ZellijFocusPane {
            session: session.clone(),
            pane_id: pane_id.clone(),
        },
        raise_step,
    ])
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        runner::FakeRunner,
        terminal::{MultiplexerLayer, TerminalLayer},
    };
    use std::collections::HashMap;

    fn host(
        pane_id: Option<&str>,
        session_name: Option<&str>,
        terminal: Option<TerminalLayer>,
    ) -> TerminalInfo {
        TerminalInfo {
            kind: "zellij".into(),
            raise_pid: 42,
            agent_pid: 100,
            multiplexer: Some(MultiplexerLayer {
                kind: MultiplexerKind::Zellij,
                pane_id: pane_id.map(str::to_owned),
                session_name: session_name.map(str::to_owned),
                socket: None,
                server_pid: Some(7),
            }),
            terminal,
            editor: None,
            env: HashMap::new(),
        }
    }

    fn scripted_client(pid: u32, emulator: u32) -> impl Fn(u32) -> Option<(u32, String)> {
        move |current| match current {
            value if value == pid => Some((emulator, "zellij".into())),
            value if value == emulator => Some((1, "kitty".into())),
            _ => None,
        }
    }

    #[test]
    fn focuses_pane_then_raises_emulator_window() {
        let host = host(
            Some("1"),
            Some("phasebqa"),
            Some(TerminalLayer {
                kind: "kitty".into(),
                pid: 900,
                window_id: Some("1".into()),
            }),
        );
        let runner = FakeRunner::new();
        assert_eq!(
            ZellijResolver.resolve(&host, &runner),
            Some(vec![
                JumpStep::ZellijFocusPane {
                    session: "phasebqa".into(),
                    pane_id: "1".into(),
                },
                JumpStep::RaiseWindow { pid: 900 },
            ])
        );
    }

    #[test]
    fn missing_pane_id_degrades_without_running_zellij() {
        let runner = FakeRunner::new();
        assert_eq!(
            ZellijResolver.resolve(&host(None, Some("phasebqa"), None), &runner),
            Some(vec![JumpStep::ActivateApp { pid: 42 }])
        );
        assert!(runner.calls().is_empty());
    }

    #[test]
    fn missing_session_degrades_without_running_zellij() {
        let runner = FakeRunner::new();
        assert_eq!(
            ZellijResolver.resolve(&host(Some("1"), None, None), &runner),
            Some(vec![JumpStep::ActivateApp { pid: 42 }])
        );
        assert!(runner.calls().is_empty());
    }

    #[test]
    fn missing_emulator_activates_host_after_focusing_pane() {
        let ancestry = |_: u32| None;
        let candidates = |_: &str, _: &str| Vec::new();
        assert_eq!(
            resolve_with_ancestry(
                &host(Some("1"), Some("phasebqa"), None),
                &ancestry,
                &candidates,
            ),
            Some(vec![
                JumpStep::ZellijFocusPane {
                    session: "phasebqa".into(),
                    pane_id: "1".into(),
                },
                JumpStep::ActivateApp { pid: 42 },
            ])
        );
    }

    #[test]
    fn daemonized_client_raises_its_kitty_window_not_agent() {
        let host = host(Some("1"), Some("phasebqa"), None);
        let chain = scripted_client(500, 900);
        let candidates = |comm: &str, session: &str| {
            assert_eq!(comm, "zellij");
            assert_eq!(session, "phasebqa");
            vec![500]
        };

        assert_eq!(
            resolve_with_ancestry(&host, &chain, &candidates),
            Some(vec![
                JumpStep::ZellijFocusPane {
                    session: "phasebqa".into(),
                    pane_id: "1".into(),
                },
                JumpStep::RaiseWindow { pid: 900 },
            ])
        );
        assert_ne!(900, host.raise_pid);
        assert_ne!(900, host.agent_pid);
    }

    #[test]
    fn server_only_candidate_degrades_to_activate_app() {
        let host = host(Some("1"), Some("phasebqa"), None);
        let chain = |pid| match pid {
            700 => Some((1, "systemd".into())),
            _ => None,
        };
        let candidates = |_: &str, _: &str| vec![700];

        assert_eq!(
            resolve_with_ancestry(&host, &chain, &candidates),
            Some(vec![
                JumpStep::ZellijFocusPane {
                    session: "phasebqa".into(),
                    pane_id: "1".into(),
                },
                JumpStep::ActivateApp { pid: 42 },
            ])
        );
    }

    #[test]
    fn client_candidate_is_selected_when_server_is_first() {
        let host = host(Some("1"), Some("phasebqa"), None);
        let chain = |pid| match pid {
            700 => Some((1, "systemd".into())),
            500 => Some((900, "zellij".into())),
            900 => Some((1, "kitty".into())),
            _ => None,
        };
        let candidates = |_: &str, _: &str| vec![700, 500];

        let plan = resolve_with_ancestry(&host, &chain, &candidates);
        assert_eq!(
            plan.as_ref().and_then(|steps| steps.last()),
            Some(&JumpStep::RaiseWindow { pid: 900 })
        );
    }

    #[test]
    fn resolves_only_zellij_hosts() {
        let tmux = TerminalInfo {
            kind: "tmux".into(),
            multiplexer: Some(MultiplexerLayer {
                kind: MultiplexerKind::Tmux,
                pane_id: Some("%1".into()),
                session_name: Some("main".into()),
                socket: None,
                server_pid: Some(7),
            }),
            ..host(None, None, None)
        };
        let kitty = TerminalInfo {
            kind: "kitty".into(),
            multiplexer: None,
            ..host(None, None, None)
        };
        assert!(!ZellijResolver.can_resolve(&tmux));
        assert!(!ZellijResolver.can_resolve(&kitty));
    }
}
