use crate::{focus, runner::CommandRunner, session::Session, terminal::TerminalInfo};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JumpStep {
    TmuxSelectPane {
        socket: Option<String>,
        pane: String,
    },
    TmuxSelectWindow {
        socket: Option<String>,
        pane: String,
    },
    TmuxSwitchClient {
        socket: Option<String>,
        client: String,
        pane: String,
    },
    TmuxNoAttachedClient,
    TmuxPaneGone,
    ZellijFocusPane {
        session: String,
        pane_id: String,
    },
    WeztermActivatePane {
        pane_id: String,
    },
    KittyFocusWindow {
        window_id: String,
    },
    FocusWindowAddress {
        address: String,
    },
    RaiseWindow {
        pid: u32,
    },
    ActivateApp {
        pid: u32,
    },
}

pub trait LocationResolver {
    fn id(&self) -> &str;
    fn can_resolve(&self, host: &TerminalInfo) -> bool;
    fn resolve(&self, host: &TerminalInfo, runner: &dyn CommandRunner) -> Option<Vec<JumpStep>>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JumpPlan {
    pub steps: Vec<JumpStep>,
    pub resolver: Option<String>,
    pub tried: Vec<String>,
}

pub struct JumpPlanner {
    resolvers: Vec<Box<dyn LocationResolver>>,
}

impl JumpPlanner {
    pub fn new(resolvers: Vec<Box<dyn LocationResolver>>) -> Self {
        Self { resolvers }
    }

    pub fn plan(&self, host: &TerminalInfo, runner: &dyn CommandRunner) -> JumpPlan {
        let mut tried = Vec::new();
        for resolver in &self.resolvers {
            if resolver.can_resolve(host) {
                tried.push(resolver.id().to_owned());
                if let Some(steps) = resolver.resolve(host, runner) {
                    if !steps.is_empty() {
                        return JumpPlan {
                            steps,
                            resolver: Some(resolver.id().to_owned()),
                            tried,
                        };
                    }
                }
            }
        }
        JumpPlan {
            steps: vec![JumpStep::ActivateApp {
                pid: host.raise_pid,
            }],
            resolver: None,
            tried,
        }
    }
}

pub struct JumpExecutor<'a> {
    runner: &'a dyn CommandRunner,
}

impl<'a> JumpExecutor<'a> {
    pub fn new(runner: &'a dyn CommandRunner) -> Self {
        Self { runner }
    }

    pub fn execute(&self, steps: &[JumpStep]) -> Result<(), String> {
        let mut inner_error = None;
        for step in steps {
            let result = match step {
                JumpStep::TmuxNoAttachedClient | JumpStep::TmuxPaneGone => Ok(()),
                JumpStep::TmuxSelectPane { socket, pane } => {
                    self.tmux(socket, &["select-pane", "-t", pane])
                }
                JumpStep::TmuxSelectWindow { socket, pane } => {
                    self.tmux(socket, &["select-window", "-t", pane])
                }
                JumpStep::TmuxSwitchClient {
                    socket,
                    client,
                    pane,
                } => self.tmux(socket, &["switch-client", "-c", client, "-t", pane]),
                JumpStep::ZellijFocusPane { session, pane_id } => self.run(
                    "zellij",
                    &["--session", session, "action", "focus-pane-id", pane_id],
                ),
                JumpStep::WeztermActivatePane { pane_id } => {
                    self.run("wezterm", &["cli", "activate-pane", "--pane-id", pane_id])
                }
                JumpStep::KittyFocusWindow { window_id } => self.run(
                    "kitty",
                    &["@", "focus-window", "--match", &format!("id:{window_id}")],
                ),
                JumpStep::FocusWindowAddress { address } => self.run(
                    "hyprctl",
                    &["dispatch", "focuswindow", &format!("address:{address}")],
                ),
                JumpStep::RaiseWindow { pid } | JumpStep::ActivateApp { pid } => {
                    focus::focus_pid(*pid)
                }
            };
            if let Err(error) = result {
                if matches!(
                    step,
                    JumpStep::RaiseWindow { .. } | JumpStep::ActivateApp { .. }
                ) {
                    return Err(error);
                }
                inner_error = Some(error);
            }
        }
        if steps.iter().any(|step| {
            matches!(
                step,
                JumpStep::RaiseWindow { .. } | JumpStep::ActivateApp { .. }
            )
        }) {
            Ok(())
        } else {
            inner_error.map_or(Ok(()), Err)
        }
    }

    fn tmux(&self, socket: &Option<String>, args: &[&str]) -> Result<(), String> {
        let mut owned = Vec::new();
        if let Some(socket) = socket {
            owned.extend(["-S".to_owned(), socket.clone()]);
        }
        owned.extend(args.iter().map(|arg| (*arg).to_owned()));
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
        self.run("tmux", &refs)
    }

    fn run(&self, program: &str, args: &[&str]) -> Result<(), String> {
        let output = self.runner.run(program, args)?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "{program} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }
}

pub fn jump(session: &Session) -> Result<(), String> {
    let info = crate::discovery::terminal_for_session(session)
        .ok_or_else(|| format!("session '{}' is no longer running", session.id))?;
    let runner = crate::runner::SystemRunner;
    let plan = JumpPlanner::new(crate::resolvers::default_resolvers()).plan(&info, &runner);
    JumpExecutor::new(&runner).execute(&plan.steps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{runner::FakeRunner, terminal::TerminalLayer};
    use std::collections::HashMap;

    fn host(kind: &str, pid: u32, window_id: Option<&str>) -> TerminalInfo {
        TerminalInfo {
            kind: kind.to_owned(),
            raise_pid: pid,
            agent_pid: 42,
            multiplexer: None,
            terminal: Some(TerminalLayer {
                kind: kind.to_owned(),
                pid,
                window_id: window_id.map(str::to_owned),
            }),
            editor: None,
            env: HashMap::new(),
        }
    }

    #[test]
    fn kitty_plan_has_focus_then_raise_without_spawning() {
        let runner = FakeRunner::new();
        let plan = JumpPlanner::new(crate::resolvers::default_resolvers())
            .plan(&host("kitty", 77, Some("9")), &runner);
        assert_eq!(
            plan.steps,
            vec![
                JumpStep::KittyFocusWindow {
                    window_id: "9".into()
                },
                JumpStep::RaiseWindow { pid: 77 }
            ]
        );
        assert_eq!(runner.calls(), Vec::<(String, Vec<String>)>::new());
    }

    #[test]
    fn kitty_plan_discovers_window_id() {
        let runner = FakeRunner::new();
        runner.push_ok(
            "kitty",
            r#"[{"tabs":[{"windows":[{"id":12,"foreground_processes":[{"pid":42}]}]}]}]"#,
        );
        let plan = JumpPlanner::new(vec![Box::new(crate::resolvers::kitty::KittyResolver)])
            .plan(&host("kitty", 77, None), &runner);
        assert_eq!(
            plan.steps,
            vec![
                JumpStep::KittyFocusWindow {
                    window_id: "12".into()
                },
                JumpStep::RaiseWindow { pid: 77 }
            ]
        );
        assert_eq!(runner.calls().len(), 1);
    }

    #[test]
    fn alacritty_plan_is_raise_only() {
        let plan = JumpPlanner::new(crate::resolvers::default_resolvers())
            .plan(&host("alacritty", 88, None), &FakeRunner::new());
        assert_eq!(plan.steps, vec![JumpStep::RaiseWindow { pid: 88 }]);
    }

    #[test]
    fn no_claim_degrades_and_records_tried_resolvers() {
        let runner = FakeRunner::new();
        let plan = JumpPlanner::new(vec![
            Box::new(crate::resolvers::tmux::TmuxResolver),
            Box::new(crate::resolvers::kitty::KittyResolver),
        ])
        .plan(&host("unknown", 91, None), &runner);
        assert_eq!(plan.steps, vec![JumpStep::ActivateApp { pid: 91 }]);
        assert_eq!(plan.resolver, None);
        assert!(plan.tried.is_empty());
    }

    #[test]
    fn planner_records_claiming_resolver_before_resolution() {
        let plan = JumpPlanner::new(crate::resolvers::default_resolvers())
            .plan(&host("kitty", 77, Some("9")), &FakeRunner::new());
        assert_eq!(plan.tried, vec!["kitty"]);
    }

    #[test]
    fn executor_continues_after_inner_failure() {
        let runner = FakeRunner::new();
        runner.push_status("kitty", 1, "", "failed");
        runner.push_ok("hyprctl", "");
        let result = JumpExecutor::new(&runner).execute(&[
            JumpStep::KittyFocusWindow {
                window_id: "9".into(),
            },
            JumpStep::FocusWindowAddress {
                address: "0x1".into(),
            },
        ]);
        assert!(result.is_err());
        assert_eq!(runner.calls().len(), 2);
    }
}
