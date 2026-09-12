use crate::{
    session::Session,
    terminal::{classify, ProcessSnapshot},
};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentSpec {
    pub id: &'static str,
    pub display: &'static str,
    pub proc_names: &'static [&'static str],
}

pub const AGENTS: &[AgentSpec] = &[
    AgentSpec {
        id: "claude",
        display: "Claude",
        proc_names: &["claude"],
    },
    AgentSpec {
        id: "codex",
        display: "Codex",
        proc_names: &["codex"],
    },
    AgentSpec {
        id: "opencode",
        display: "OpenCode",
        proc_names: &["opencode"],
    },
    AgentSpec {
        id: "cursor",
        display: "Cursor",
        proc_names: &["cursor"],
    },
    AgentSpec {
        id: "gemini",
        display: "Gemini",
        proc_names: &["gemini"],
    },
    AgentSpec {
        id: "kimi",
        display: "Kimi",
        proc_names: &["kimi", "kimicode"],
    },
    AgentSpec {
        id: "qwen",
        display: "Qwen",
        proc_names: &["qwen", "qwen-code", "qwenwork"],
    },
    AgentSpec {
        id: "pi",
        display: "Pi",
        proc_names: &["pi", "ohmypi"],
    },
    AgentSpec {
        id: "amp",
        display: "Amp",
        proc_names: &["amp"],
    },
    AgentSpec {
        id: "droid",
        display: "Droid",
        proc_names: &["droid"],
    },
    AgentSpec {
        id: "trae",
        display: "Trae",
        proc_names: &["trae"],
    },
    AgentSpec {
        id: "deepseek",
        display: "DeepSeek",
        proc_names: &["deepseek"],
    },
];

pub fn agent_for_argv0(argv0: &str) -> Option<&'static AgentSpec> {
    let basename = Path::new(argv0).file_name()?.to_str()?;
    AGENTS.iter().find(|agent| {
        agent
            .proc_names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(basename))
    })
}

const HELPER_FLAGS: &[&[u8]] = &[b"--chrome-native-host"];

pub(crate) fn agent_for_cmdline(command: &[u8]) -> Option<String> {
    let mut args = command.split(|byte| *byte == 0);
    let agent = args
        .next()
        .filter(|arg| !arg.is_empty())
        .and_then(|argv0| std::str::from_utf8(argv0).ok())
        .and_then(|argv0| Path::new(argv0).file_name())
        .and_then(|name| name.to_str())
        .and_then(agent_for_argv0)?;
    if args.any(|arg| HELPER_FLAGS.contains(&arg)) {
        return None;
    }
    Some(agent.id.to_owned())
}

pub fn scan() -> Vec<Session> {
    let self_pid = std::process::id();
    let ancestors = ancestor_pids(self_pid);
    let snapshots = read_snapshots();
    classify_processes(&snapshots, self_pid, &ancestors)
}

pub fn terminal_for_session(session: &Session) -> Option<crate::terminal::TerminalInfo> {
    terminal_for_sessions(&[session]).remove(&session.pid)
}

/// Resolve terminal metadata from one process table snapshot. A reconciliation
/// can include hook-only sessions whose interpreter argv0 prevents `scan` from
/// recognizing them, so their environments are refreshed before classification.
pub(crate) fn terminal_for_sessions(
    sessions: &[&Session],
) -> HashMap<u32, crate::terminal::TerminalInfo> {
    terminal_for_sessions_with(sessions, read_snapshots, crate::process::environment)
}

fn terminal_for_sessions_with(
    sessions: &[&Session],
    read_snapshots: impl FnOnce() -> Vec<ProcessSnapshot>,
    read_environment: impl Fn(u32) -> Vec<u8>,
) -> HashMap<u32, crate::terminal::TerminalInfo> {
    if sessions.is_empty() {
        return HashMap::new();
    }
    let mut snapshots = read_snapshots();
    let requested: HashSet<u32> = sessions.iter().map(|session| session.pid).collect();
    for snapshot in &mut snapshots {
        // Hook-only agents can have an interpreter as argv0. Read their process
        // environment even when generic discovery did not classify them.
        if requested.contains(&snapshot.pid) {
            snapshot.env = parse_environment(&read_environment(snapshot.pid));
        }
    }
    terminal_for_sessions_from_snapshots(sessions, &snapshots)
}

fn terminal_for_sessions_from_snapshots(
    sessions: &[&Session],
    snapshots: &[ProcessSnapshot],
) -> HashMap<u32, crate::terminal::TerminalInfo> {
    let requested: HashSet<u32> = sessions.iter().map(|session| session.pid).collect();
    snapshots
        .iter()
        .filter(|snapshot| requested.contains(&snapshot.pid))
        .map(|snapshot| (snapshot.pid, classify(snapshot, snapshots)))
        .collect()
}

pub fn classify_processes(
    snapshots: &[ProcessSnapshot],
    self_pid: u32,
    ancestors: &HashSet<u32>,
) -> Vec<Session> {
    snapshots
        .iter()
        .filter_map(|snapshot| {
            let agent = snapshot.agent.as_deref()?;
            if snapshot.pid == self_pid || ancestors.contains(&snapshot.pid) {
                return None;
            }
            let terminal = classify(snapshot, snapshots);
            let mut session = Session::new(agent, &snapshot.cwd, snapshot.pid, &terminal.kind);
            session.raise_pid = Some(terminal.raise_pid);
            session.launcher = crate::terminal::launcher_of(snapshot, snapshots);
            match crate::send::capability(&terminal) {
                Ok(channel) => session.send_channel = Some(channel.to_owned()),
                Err(blocked) => session.send_blocked = Some(blocked.code().to_owned()),
            }
            Some(session)
        })
        .collect()
}

fn read_snapshots() -> Vec<ProcessSnapshot> {
    crate::process::pids()
        .into_iter()
        .filter_map(|pid| {
            let (ppid, comm) = crate::process::parent_and_comm(pid)?;
            let agent =
                crate::process::command(pid).and_then(|command| agent_for_cmdline(&command));
            let (cwd, env) = if agent.is_some() {
                (
                    crate::process::cwd(pid)?.to_string_lossy().into_owned(),
                    parse_environment(&crate::process::environment(pid)),
                )
            } else {
                (String::new(), HashMap::new())
            };
            Some(ProcessSnapshot {
                pid,
                ppid,
                comm,
                agent,
                cwd,
                env,
            })
        })
        .collect()
}

pub(crate) fn parse_environment(bytes: &[u8]) -> HashMap<String, String> {
    bytes
        .split(|byte| *byte == 0)
        .filter_map(|item| {
            let separator = item.iter().position(|byte| *byte == b'=')?;
            let (key, value) = item.split_at(separator);
            let value = value.get(1..)?;
            let key = std::str::from_utf8(key).ok()?;
            let value = std::str::from_utf8(value).ok()?;
            if key == "TERM_PROGRAM"
                || key == crate::input_bridge::ENV
                || key == "KITTY_WINDOW_ID"
                || key == "KITTY_LISTEN_ON"
                || key == "TMUX"
                || key == "TMUX_PANE"
                || key == "ZELLIJ"
                || key == "ZELLIJ_SESSION_NAME"
                || key == "ZELLIJ_PANE_ID"
                || key == "ALACRITTY_WINDOW_ID"
                || key == "GHOSTTY_RESOURCES_DIR"
                || key == "ZED_TERM"
                || key == "VSCODE_PID"
                || key == "VSCODE_INJECTION"
                || key == "TERM_PROGRAM_VERSION"
                || key.starts_with("WEZTERM_")
            {
                Some((key.to_owned(), value.to_owned()))
            } else {
                None
            }
        })
        .collect()
}

fn ancestor_pids(mut pid: u32) -> HashSet<u32> {
    let mut result = HashSet::new();
    for _ in 0..32 {
        let Some((ppid, _)) = crate::process::parent_and_comm(pid) else {
            break;
        };
        if ppid == 0 || !result.insert(ppid) {
            break;
        }
        pid = ppid;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(pid: u32, ppid: u32, comm: &str, agent: Option<&str>) -> ProcessSnapshot {
        ProcessSnapshot {
            pid,
            ppid,
            comm: comm.to_owned(),
            agent: agent.map(str::to_owned),
            cwd: "/home/user/project".to_owned(),
            env: HashMap::new(),
        }
    }

    #[test]
    fn synthetic_processes_detect_agents_and_terminals() {
        let claude = snapshot(30, 20, "claude", Some("claude"));
        let shell = snapshot(20, 10, "zsh", None);
        let kitty = snapshot(10, 1, "kitty", None);
        let codex = snapshot(40, 41, "codex", Some("codex"));
        let alacritty = snapshot(41, 1, "alacritty", None);
        let sessions = classify_processes(
            &[claude, shell, kitty, codex, alacritty],
            999,
            &HashSet::new(),
        );
        assert_eq!(
            sessions
                .iter()
                .map(|session| session.agent.as_str())
                .collect::<Vec<_>>(),
            vec!["claude", "codex"]
        );
        assert_eq!(sessions[0].terminal, "kitty");
        assert_eq!(sessions[1].terminal, "alacritty");
    }

    #[test]
    fn chrome_native_host_is_not_a_session() {
        assert_eq!(
            agent_for_cmdline(b"/home/user/.local/bin/claude\0--chrome-native-host\0"),
            None
        );
        assert_eq!(
            agent_for_cmdline(b"/home/user/.local/bin/claude\0"),
            Some("claude".to_owned())
        );
        assert_eq!(
            agent_for_cmdline(b"claude\0--agent\0atlas\0"),
            Some("claude".to_owned())
        );
    }

    #[test]
    fn parse_environment_keeps_host_keys_but_drops_unrelated_keys() {
        let bytes = b"ZELLIJ=0\0ZELLIJ_SESSION_NAME=main\0ZELLIJ_PANE_ID=4\0ALACRITTY_WINDOW_ID=5\0GHOSTTY_RESOURCES_DIR=/tmp/ghostty\0ZED_TERM=1\0VSCODE_PID=555\0VSCODE_INJECTION=1\0TERM_PROGRAM_VERSION=1.2\0SECRET_TOKEN=hidden\0";
        let env = parse_environment(bytes);
        for key in [
            "ZELLIJ",
            "ZELLIJ_SESSION_NAME",
            "ZELLIJ_PANE_ID",
            "ALACRITTY_WINDOW_ID",
            "GHOSTTY_RESOURCES_DIR",
            "ZED_TERM",
            "VSCODE_PID",
            "VSCODE_INJECTION",
            "TERM_PROGRAM_VERSION",
        ] {
            assert!(env.contains_key(key), "missing whitelisted key {key}");
        }
        assert!(!env.contains_key("SECRET_TOKEN"));
    }

    #[test]
    fn synthetic_snapshots_detect_all_requested_agents() {
        let processes = [
            snapshot(1, 0, "cursor", Some("cursor")),
            snapshot(2, 0, "gemini", Some("gemini")),
            snapshot(3, 0, "kimi", Some("kimi")),
            snapshot(4, 0, "qwen-code", Some("qwen")),
        ];
        let sessions = classify_processes(&processes, 999, &HashSet::new());
        assert_eq!(
            sessions
                .iter()
                .map(|session| session.agent.as_str())
                .collect::<Vec<_>>(),
            vec!["cursor", "gemini", "kimi", "qwen"]
        );
        assert_eq!(
            agent_for_argv0("/usr/bin/KIMICODE").map(|agent| agent.id),
            Some("kimi")
        );
    }

    #[test]
    fn terminal_metadata_for_multiple_sessions_uses_the_same_snapshot() {
        let claude = snapshot(30, 20, "python3", None);
        let shell = snapshot(20, 10, "zsh", None);
        let kitty = snapshot(10, 1, "kitty", None);
        let codex = snapshot(40, 41, "node", None);
        let alacritty = snapshot(41, 1, "alacritty", None);
        let claude_session = Session::new("claude", "/work", 30, "unknown");
        let codex_session = Session::new("codex", "/work", 40, "unknown");

        let scans = std::cell::Cell::new(0);
        let terminals = terminal_for_sessions_with(
            &[&claude_session, &codex_session],
            || {
                scans.set(scans.get() + 1);
                vec![claude, shell, kitty, codex, alacritty]
            },
            |_| Vec::new(),
        );

        assert_eq!(scans.get(), 1);
        assert_eq!(terminals.len(), 2);
        assert_eq!(
            terminals.get(&30).map(|info| info.kind.as_str()),
            Some("kitty")
        );
        assert_eq!(
            terminals.get(&40).map(|info| info.kind.as_str()),
            Some("alacritty")
        );
    }

    #[test]
    fn terminal_metadata_does_not_scan_when_no_sessions_need_it() {
        let scans = std::cell::Cell::new(0);
        let terminals = terminal_for_sessions_with(
            &[],
            || {
                scans.set(scans.get() + 1);
                Vec::new()
            },
            |_| Vec::new(),
        );

        assert!(terminals.is_empty());
        assert_eq!(scans.get(), 0);
    }
}
