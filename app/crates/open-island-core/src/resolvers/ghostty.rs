use crate::{
    jump::{JumpStep, LocationResolver},
    runner::CommandRunner,
    terminal::TerminalInfo,
};
use serde::Deserialize;

pub struct GhosttyResolver;

#[derive(Deserialize)]
struct HyprlandClient {
    class: String,
    title: String,
    pid: u32,
    address: String,
}

impl LocationResolver for GhosttyResolver {
    fn id(&self) -> &str {
        "ghostty"
    }
    fn can_resolve(&self, host: &TerminalInfo) -> bool {
        host.terminal
            .as_ref()
            .is_some_and(|terminal| terminal.kind == "ghostty")
    }

    fn resolve(&self, host: &TerminalInfo, runner: &dyn CommandRunner) -> Option<Vec<JumpStep>> {
        let fallback = || {
            Some(vec![JumpStep::ActivateApp {
                pid: host.raise_pid,
            }])
        };
        let Some(terminal) = host.terminal.as_ref() else {
            return fallback();
        };
        let output = match runner.run("hyprctl", &["clients", "-j"]) {
            Ok(output) if output.status.success() => output,
            _ => return fallback(),
        };
        let clients: Vec<HyprlandClient> = match serde_json::from_slice(&output.stdout) {
            Ok(clients) => clients,
            Err(_) => return fallback(),
        };
        let candidates = clients
            .into_iter()
            .filter(|client| {
                let class = client.class.to_ascii_lowercase();
                (class == "com.mitchellh.ghostty" || class == "ghostty")
                    && client.pid == terminal.pid
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return fallback();
        }
        let selected = match candidates.as_slice() {
            [selected] => selected,
            candidates if candidates.len() > 1 => {
                let Some(title) = host.env.get("GHOSTTY_TITLE") else {
                    return fallback();
                };
                let mut matching = candidates.iter().filter(|client| client.title == *title);
                let Some(selected) = matching.next() else {
                    return fallback();
                };
                if matching.next().is_some() {
                    return fallback();
                }
                selected
            }
            _ => return fallback(),
        };
        let address = if selected.address.starts_with("0x") {
            selected.address.clone()
        } else {
            format!("0x{}", selected.address)
        };
        Some(vec![
            JumpStep::FocusWindowAddress { address },
            JumpStep::RaiseWindow { pid: selected.pid },
        ])
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::jump::JumpStep;
    use crate::runner::FakeRunner;
    use crate::terminal::TerminalLayer;
    use std::collections::HashMap;

    fn host(kind: &str, pid: u32, title: Option<&str>) -> TerminalInfo {
        let mut env = HashMap::new();
        if let Some(title) = title {
            env.insert("GHOSTTY_TITLE".into(), title.into());
        }
        TerminalInfo {
            kind: kind.into(),
            raise_pid: 900,
            agent_pid: 42,
            multiplexer: None,
            terminal: Some(TerminalLayer {
                kind: kind.into(),
                pid,
                window_id: None,
            }),
            editor: None,
            env,
        }
    }

    fn runner(json: &str) -> FakeRunner {
        let runner = FakeRunner::new();
        runner.push_ok("hyprctl", json);
        runner
    }

    #[test]
    fn unique_match_focus_then_raise_normalizes_address() {
        let host = host("ghostty", 123, None);
        let runner = runner(
            r#"[{"class":"com.mitchellh.ghostty","title":"shell","pid":123,"address":"abc"}]"#,
        );
        assert_eq!(
            GhosttyResolver.resolve(&host, &runner),
            Some(vec![
                JumpStep::FocusWindowAddress {
                    address: "0xabc".into()
                },
                JumpStep::RaiseWindow { pid: 123 },
            ])
        );
    }

    #[test]
    fn ambiguous_match_uses_host_pid_tie_break() {
        let host = host("ghostty", 222, None);
        let runner = runner(
            r#"[
            {"class":"com.mitchellh.ghostty","title":"same","pid":111,"address":"111"},
            {"class":"GHOSTTY","title":"same","pid":222,"address":"222"}
        ]"#,
        );
        assert_eq!(
            GhosttyResolver.resolve(&host, &runner),
            Some(vec![
                JumpStep::FocusWindowAddress {
                    address: "0x222".into()
                },
                JumpStep::RaiseWindow { pid: 222 },
            ])
        );
    }

    #[test]
    fn ambiguous_same_pid_without_title_degrades() {
        let host = host("ghostty", 222, None);
        let runner = runner(
            r#"[
            {"class":"com.mitchellh.ghostty","title":"one","pid":222,"address":"1"},
            {"class":"com.mitchellh.ghostty","title":"two","pid":222,"address":"2"}
        ]"#,
        );
        assert_eq!(
            GhosttyResolver.resolve(&host, &runner),
            Some(vec![JumpStep::ActivateApp { pid: 900 }])
        );
    }

    #[test]
    fn no_ghostty_window_degrades() {
        let host = host("ghostty", 222, None);
        let runner = runner(r#"[{"class":"kitty","title":"shell","pid":222,"address":"1"}]"#);
        assert_eq!(
            GhosttyResolver.resolve(&host, &runner),
            Some(vec![JumpStep::ActivateApp { pid: 900 }])
        );
    }

    #[test]
    fn command_failure_and_malformed_json_degrade_without_error() {
        let host = host("ghostty", 222, None);
        let failed = FakeRunner::new();
        failed.push_status("hyprctl", 1, "", "failed");
        assert_eq!(
            GhosttyResolver.resolve(&host, &failed),
            Some(vec![JumpStep::ActivateApp { pid: 900 }])
        );
        let malformed = runner("not json");
        assert_eq!(
            GhosttyResolver.resolve(&host, &malformed),
            Some(vec![JumpStep::ActivateApp { pid: 900 }])
        );
    }

    #[test]
    fn can_resolve_only_ghostty_terminal_hosts() {
        let kitty = host("kitty", 222, None);
        let wezterm = host("wezterm", 222, None);
        assert!(!GhosttyResolver.can_resolve(&kitty));
        assert!(!GhosttyResolver.can_resolve(&wezterm));
    }
}
