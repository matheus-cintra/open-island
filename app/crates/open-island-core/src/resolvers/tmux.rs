use super::ancestry::{emulator_ancestor, proc_parent_and_comm};
use crate::{
    jump::{JumpStep, LocationResolver},
    runner::CommandRunner,
    terminal::{MultiplexerKind, TerminalInfo},
};

pub struct TmuxResolver;

fn tmux_args(socket: Option<&str>, command: &[&str]) -> Vec<String> {
    let mut args = Vec::with_capacity(command.len() + usize::from(socket.is_some()) * 2);
    if let Some(socket) = socket {
        args.extend(["-S".to_owned(), socket.to_owned()]);
    }
    args.extend(command.iter().map(|arg| (*arg).to_owned()));
    args
}

fn run_tmux(
    runner: &dyn CommandRunner,
    socket: Option<&str>,
    command: &[&str],
) -> Option<std::process::Output> {
    let args = tmux_args(socket, command);
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    runner.run("tmux", &refs).ok()
}

fn pane_from_output(output: &[u8], agent_pid: u32) -> Option<String> {
    String::from_utf8_lossy(output).lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let pane = fields.next()?;
        let pane_pid = fields.next()?.parse::<u32>().ok()?;
        if pane_pid == agent_pid {
            Some(pane.to_owned())
        } else {
            None
        }
    })
}

fn client_from_output(output: &[u8]) -> Option<(String, u32)> {
    String::from_utf8_lossy(output).lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let client = fields.next()?.to_owned();
        let pid = fields.next()?.parse().ok()?;
        Some((client, pid))
    })
}

fn resolve_with_ancestry(
    host: &TerminalInfo,
    runner: &dyn CommandRunner,
    parent_and_comm: &dyn Fn(u32) -> Option<(u32, String)>,
) -> Option<Vec<JumpStep>> {
    let multiplexer = host.multiplexer.as_ref()?;
    if multiplexer.kind != MultiplexerKind::Tmux {
        return None;
    }
    let socket = multiplexer.socket.as_deref();
    let pane = match multiplexer.pane_id.clone() {
        Some(pane) => pane,
        None => {
            let output = run_tmux(
                runner,
                socket,
                &[
                    "list-panes",
                    "-a",
                    "-F",
                    "#{pane_id} #{pane_pid} #{session_name}:#{window_index}",
                ],
            );
            let Some(output) = output.filter(|output| output.status.success()) else {
                return Some(vec![
                    JumpStep::TmuxPaneGone,
                    JumpStep::ActivateApp {
                        pid: host.raise_pid,
                    },
                ]);
            };
            let Some(pane) = pane_from_output(&output.stdout, host.agent_pid) else {
                return Some(vec![
                    JumpStep::TmuxPaneGone,
                    JumpStep::ActivateApp {
                        pid: host.raise_pid,
                    },
                ]);
            };
            pane
        }
    };
    let mut steps = vec![
        JumpStep::TmuxSelectPane {
            socket: multiplexer.socket.clone(),
            pane: pane.clone(),
        },
        JumpStep::TmuxSelectWindow {
            socket: multiplexer.socket.clone(),
            pane: pane.clone(),
        },
    ];
    let client_output = run_tmux(
        runner,
        socket,
        &[
            "list-clients",
            "-F",
            "#{client_name} #{client_pid} #{session_name}",
        ],
    );
    let Some(output) = client_output.filter(|output| output.status.success()) else {
        steps.extend([
            JumpStep::TmuxNoAttachedClient,
            JumpStep::ActivateApp {
                pid: host.raise_pid,
            },
        ]);
        return Some(steps);
    };
    let Some((client, client_pid)) = client_from_output(&output.stdout) else {
        steps.extend([
            JumpStep::TmuxNoAttachedClient,
            JumpStep::ActivateApp {
                pid: host.raise_pid,
            },
        ]);
        return Some(steps);
    };
    steps.push(JumpStep::TmuxSwitchClient {
        socket: multiplexer.socket.clone(),
        client,
        pane,
    });
    let raise = emulator_ancestor(client_pid, parent_and_comm).map_or(
        JumpStep::ActivateApp {
            pid: host.raise_pid,
        },
        |pid| JumpStep::RaiseWindow { pid },
    );
    steps.push(raise);
    Some(steps)
}

impl LocationResolver for TmuxResolver {
    fn id(&self) -> &str {
        "tmux"
    }
    fn can_resolve(&self, host: &TerminalInfo) -> bool {
        host.multiplexer
            .as_ref()
            .is_some_and(|mux| mux.kind == MultiplexerKind::Tmux)
    }
    fn resolve(&self, host: &TerminalInfo, runner: &dyn CommandRunner) -> Option<Vec<JumpStep>> {
        resolve_with_ancestry(host, runner, &proc_parent_and_comm)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::jump::JumpStep;
    use crate::runner::FakeRunner;
    use crate::terminal::{MultiplexerKind, MultiplexerLayer};
    use std::collections::HashMap;

    fn tmux_host(pane_id: Option<&str>) -> TerminalInfo {
        TerminalInfo {
            kind: "kitty".into(),
            raise_pid: 900,
            agent_pid: 700,
            multiplexer: Some(MultiplexerLayer {
                kind: MultiplexerKind::Tmux,
                pane_id: pane_id.map(str::to_owned),
                session_name: Some("qa".into()),
                socket: Some("/tmp/tmux.sock".into()),
                server_pid: Some(800),
            }),
            terminal: None,
            editor: None,
            env: HashMap::new(),
        }
    }

    fn selected_pane(pane: &str) -> Vec<JumpStep> {
        vec![
            JumpStep::TmuxSelectPane {
                socket: Some("/tmp/tmux.sock".into()),
                pane: pane.into(),
            },
            JumpStep::TmuxSelectWindow {
                socket: Some("/tmp/tmux.sock".into()),
                pane: pane.into(),
            },
        ]
    }
    #[test]
    fn resolves_tmux_hosts() {
        assert!(TmuxResolver.can_resolve(&tmux_host(Some("%3"))));
    }

    #[test]
    fn pane_gone_degrades_to_activate_app() {
        let host = tmux_host(None);
        let runner = FakeRunner::new();
        runner.push_ok("tmux", "");

        let steps = TmuxResolver.resolve(&host, &runner);

        assert_eq!(
            steps,
            Some(vec![
                JumpStep::TmuxPaneGone,
                JumpStep::ActivateApp { pid: 900 },
            ])
        );
    }

    #[test]
    fn pane_found_switches_client_and_raises_emulator() {
        let host = tmux_host(Some("%3"));
        let runner = FakeRunner::new();
        runner.push_ok("tmux", "/dev/pts/11 993508 qa\n");
        let ancestry = |pid| match pid {
            993508 => Some((993086, "tmux: client".into())),
            993086 => Some((1, "kitty".into())),
            _ => None,
        };

        let steps = resolve_with_ancestry(&host, &runner, &ancestry);

        let mut expected = selected_pane("%3");
        expected.extend([
            JumpStep::TmuxSwitchClient {
                socket: Some("/tmp/tmux.sock".into()),
                client: "/dev/pts/11".into(),
                pane: "%3".into(),
            },
            JumpStep::RaiseWindow { pid: 993086 },
        ]);
        assert_eq!(steps, Some(expected));
    }

    #[test]
    fn no_attached_client_degrades_to_activate_app() {
        let host = tmux_host(Some("%3"));
        let runner = FakeRunner::new();
        runner.push_ok("tmux", "");

        let steps = resolve_with_ancestry(&host, &runner, &|_| None);

        let mut expected = selected_pane("%3");
        expected.extend([
            JumpStep::TmuxNoAttachedClient,
            JumpStep::ActivateApp { pid: 900 },
        ]);
        assert_eq!(steps, Some(expected));
    }

    #[test]
    fn list_clients_failure_degrades_without_error() {
        let host = tmux_host(Some("%3"));
        let runner = FakeRunner::new();
        runner.push_status("tmux", 1, "", "server unavailable");

        let steps = resolve_with_ancestry(&host, &runner, &|_| None);

        let mut expected = selected_pane("%3");
        expected.extend([
            JumpStep::TmuxNoAttachedClient,
            JumpStep::ActivateApp { pid: 900 },
        ]);
        assert_eq!(steps, Some(expected));
    }

    #[test]
    fn declines_zellij_and_plain_kitty() {
        let mut host = tmux_host(Some("%3"));
        if let Some(mux) = host.multiplexer.as_mut() {
            mux.kind = MultiplexerKind::Zellij;
        }
        assert!(!TmuxResolver.can_resolve(&host));
        host.multiplexer = None;
        assert!(!TmuxResolver.can_resolve(&host));
    }
}
