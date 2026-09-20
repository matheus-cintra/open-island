use crate::{
    session::Session,
    terminal::{classify, ProcessSnapshot},
};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    time::Instant,
};

/// One immutable process-table observation. Publication never probes the OS.
#[derive(Clone)]
pub struct Observation {
    pub complete: bool,
    pub started: Instant,
    pub sessions: Vec<Session>,
    pub births: HashMap<u32, crate::process::ProcessBirthIdentity>,
    pub alive: HashSet<u32>,
    pub hosts: HashMap<u32, crate::terminal::TerminalInfo>,
    pub branches: HashMap<String, Option<String>>,
}
impl Observation {
    pub fn empty() -> Self {
        Self {
            complete: false,
            started: Instant::now(),
            sessions: Vec::new(),
            births: HashMap::new(),
            alive: HashSet::new(),
            hosts: HashMap::new(),
            branches: HashMap::new(),
        }
    }
}

pub fn observe(hooks: &[Session]) -> Result<Observation, &'static str> {
    let mut observation = Observation::empty();
    let pids = crate::process::pids();
    #[cfg(not(feature = "qa-harness"))]
    if pids.is_empty() {
        return Err("discovery_unavailable");
    }
    observation.alive = pids.iter().copied().collect();
    let requested: HashSet<_> = hooks.iter().map(|session| session.pid).collect();
    let snapshots: Vec<_> = pids
        .into_iter()
        .filter_map(|pid| {
            let stat = crate::process::stat_fields(pid)?;
            let snapshot = read_snapshot(pid, &stat, requested.contains(&pid))?;
            if stat.birth != crate::process::birth_identity(pid) {
                return None;
            }
            if let Some(birth) = stat.birth {
                observation.births.insert(pid, birth);
            }
            Some(snapshot)
        })
        .collect();
    let self_pid = std::process::id();
    observation.sessions = classify_processes(&snapshots, self_pid, &ancestor_pids(self_pid));
    let targets: HashSet<_> = requested
        .into_iter()
        .chain(observation.sessions.iter().map(|s| s.pid))
        .collect();
    observation.hosts = snapshots
        .iter()
        .filter(|s| targets.contains(&s.pid))
        .map(|s| (s.pid, classify(s, &snapshots)))
        .collect();
    // A hook can arrive in the same tick in which the process table is changing.  In that
    // window the full scan may omit the requested PID even though its short ancestry is still
    // readable.  Recover only those missing hook hosts here so the cached projection does not
    // publish a transient `send_channel: null` and then leave the row unresolved until another
    // event arrives.
    for hook in hooks {
        if !observation.hosts.contains_key(&hook.pid) {
            if let Some(host) = terminal_for_delivery(hook, || true) {
                observation.hosts.insert(hook.pid, host);
            }
        }
    }
    for session in hooks.iter().chain(&observation.sessions) {
        observation
            .branches
            .entry(session.cwd.clone())
            .or_insert_with(|| crate::naming::branch_of(Path::new(&session.cwd)));
    }
    observation.complete = true;
    Ok(observation)
}

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

const SERVICE_SUBCOMMANDS: &[(&str, &[&[u8]])] =
    &[("codex", &[b"app-server", b"mcp", b"mcp-server", b"proto"])];

pub(crate) fn agent_for_cmdline(command: &[u8]) -> Option<String> {
    let mut args = command
        .split(|byte| *byte == 0)
        .filter(|arg| !arg.is_empty());
    let agent = args
        .next()
        .and_then(|argv0| std::str::from_utf8(argv0).ok())
        .and_then(|argv0| Path::new(argv0).file_name())
        .and_then(|name| name.to_str())
        .and_then(agent_for_argv0)?;
    let rest: Vec<&[u8]> = args.collect();
    if rest.iter().any(|arg| HELPER_FLAGS.contains(arg)) {
        return None;
    }
    if let Some(subcommand) = rest.iter().find(|arg| !arg.starts_with(b"-")) {
        if SERVICE_SUBCOMMANDS
            .iter()
            .any(|(id, names)| *id == agent.id && names.contains(subcommand))
        {
            return None;
        }
    }
    Some(agent.id.to_owned())
}

pub fn agent_pid_of_ancestor(agent: &str, from: u32) -> Option<u32> {
    agent_pid_of_ancestor_with(
        agent,
        from,
        |pid| crate::process::parent_and_comm(pid).map(|(parent, _)| parent),
        crate::process::command,
    )
}

fn agent_pid_of_ancestor_with(
    agent: &str,
    from: u32,
    parent_of: impl Fn(u32) -> Option<u32>,
    command_of: impl Fn(u32) -> Option<Vec<u8>>,
) -> Option<u32> {
    let mut pid = from;
    let mut visited = HashSet::new();
    for _ in 0..32 {
        let parent = parent_of(pid)?;
        if parent <= 1 || !visited.insert(parent) {
            return None;
        }
        let matched = command_of(parent)
            .and_then(|command| agent_for_cmdline(&command))
            .is_some_and(|id| id == agent);
        if matched {
            return Some(parent);
        }
        pid = parent;
    }
    None
}

pub fn scan() -> Vec<Session> {
    let self_pid = std::process::id();
    let ancestors = ancestor_pids(self_pid);
    let snapshots = read_snapshots();
    classify_processes(&snapshots, self_pid, &ancestors)
}

pub fn terminal_for_session(session: &Session) -> Option<crate::terminal::TerminalInfo> {
    terminal_for_delivery(session, || true)
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
            let stat = crate::process::stat_fields(pid)?;
            read_snapshot(pid, &stat, false)
        })
        .collect()
}
fn read_snapshot(
    pid: u32,
    stat: &crate::process::ProcessStat,
    requested: bool,
) -> Option<ProcessSnapshot> {
    let agent = crate::process::command(pid).and_then(|command| agent_for_cmdline(&command));
    let (cwd, env) = if agent.is_some() || requested {
        (
            crate::process::cwd(pid)?.to_string_lossy().into_owned(),
            parse_environment(&crate::process::environment(pid)),
        )
    } else {
        (String::new(), HashMap::new())
    };
    Some(ProcessSnapshot {
        pid,
        ppid: stat.parent,
        comm: stat.comm.clone(),
        agent,
        cwd,
        env,
    })
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

pub fn terminal_for_delivery(
    session: &Session,
    mut within_deadline: impl FnMut() -> bool,
) -> Option<crate::terminal::TerminalInfo> {
    let mut snapshots = Vec::new();
    let mut visited = HashSet::new();
    let mut pid = session.pid;
    for _ in 0..64 {
        if pid <= 1 || !visited.insert(pid) || !within_deadline() {
            break;
        }
        let Some((ppid, comm)) = crate::process::parent_and_comm(pid) else {
            break;
        };
        let selected = pid == session.pid;
        snapshots.push(ProcessSnapshot {
            pid,
            ppid,
            comm,
            agent: if selected {
                Some(session.agent.clone())
            } else {
                None
            },
            cwd: if selected {
                session.cwd.clone()
            } else {
                String::new()
            },
            env: if selected {
                parse_environment(&crate::process::environment(pid))
            } else {
                HashMap::new()
            },
        });
        pid = ppid;
    }
    if !within_deadline() {
        return None;
    }
    snapshots
        .first()
        .filter(|first| first.pid == session.pid)
        .map(|first| classify(first, &snapshots))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmdline(parts: &[&str]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for part in parts {
            bytes.extend_from_slice(part.as_bytes());
            bytes.push(0);
        }
        bytes
    }

    #[test]
    fn a_codex_service_subcommand_is_not_a_session() {
        assert_eq!(
            agent_for_cmdline(&cmdline(&["/opt/codex/bin/codex", "--yolo"])),
            Some("codex".to_owned())
        );
        assert_eq!(
            agent_for_cmdline(&cmdline(&["/opt/codex/bin/codex", "exec", "oi"])),
            Some("codex".to_owned())
        );
        assert_eq!(
            agent_for_cmdline(&cmdline(&[
                "/opt/codex/bin/codex",
                "app-server",
                "daemon",
                "pid-update-loop"
            ])),
            None
        );
        assert_eq!(
            agent_for_cmdline(&cmdline(&[
                "/opt/codex/bin/codex",
                "app-server",
                "--remote-control",
                "--list"
            ])),
            None
        );
        assert_eq!(
            agent_for_cmdline(&cmdline(&["/usr/bin/claude", "app-server"])),
            Some("claude".to_owned())
        );
    }

    #[test]
    fn a_hook_finds_the_agent_process_that_spawned_it() {
        let parents = HashMap::from([(500u32, 400u32), (400, 300), (300, 1)]);
        let commands = HashMap::from([
            (400u32, cmdline(&["/usr/bin/claude", "--resume"])),
            (300, cmdline(&["/usr/bin/kitty"])),
        ]);
        let parent_of = |pid: u32| parents.get(&pid).copied();
        let command_of = |pid: u32| commands.get(&pid).cloned();
        assert_eq!(
            agent_pid_of_ancestor_with("claude", 500, parent_of, command_of),
            Some(400)
        );
        assert_eq!(
            agent_pid_of_ancestor_with("codex", 500, parent_of, command_of),
            None
        );
    }

    #[test]
    fn a_real_process_chain_resolves_the_agent_pid() {
        use std::process::{Command, Stdio};
        let mut agent = Command::new("/bin/bash")
            .arg("-c")
            .arg("exec -a claude /bin/bash -c '/bin/sleep 30; true'")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn fake agent");
        let agent_pid = agent.id();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let leaf = loop {
            let child = crate::process::pids().into_iter().find(|pid| {
                crate::process::parent_and_comm(*pid).is_some_and(|(parent, _)| parent == agent_pid)
            });
            if let Some(child) = child {
                break child;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the fake agent never spawned a child"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        let found = agent_pid_of_ancestor("claude", leaf);
        unsafe { libc::kill(leaf as i32, libc::SIGKILL) };
        let _ = agent.kill();
        let _ = agent.wait();
        let gone = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while crate::process::exists(leaf) && std::time::Instant::now() < gone {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!crate::process::exists(leaf), "the fake agent leaked a child");
        assert_eq!(found, Some(agent_pid));
    }

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
