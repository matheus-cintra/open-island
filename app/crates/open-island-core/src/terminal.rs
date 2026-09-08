use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MultiplexerKind {
    Tmux,
    Zellij,
}

impl MultiplexerKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tmux => "tmux",
            Self::Zellij => "zellij",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiplexerLayer {
    pub kind: MultiplexerKind,
    pub pane_id: Option<String>,
    pub session_name: Option<String>,
    pub socket: Option<String>,
    pub server_pid: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalLayer {
    pub kind: String,
    pub pid: u32,
    pub window_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorKind {
    Zed,
    VsCode,
    Cursor,
    Windsurf,
    Codium,
}

impl EditorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Zed => "zed",
            Self::VsCode => "code",
            Self::Cursor => "cursor",
            Self::Windsurf => "windsurf",
            Self::Codium => "codium",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorLayer {
    pub kind: EditorKind,
    pub pid: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalInfo {
    /// The outermost emulator kind, or the editor kind when no emulator exists.
    pub kind: String,
    /// The process that `focus::focus_pid` must target; never a multiplexer server.
    pub raise_pid: u32,
    pub agent_pid: u32,
    pub multiplexer: Option<MultiplexerLayer>,
    pub terminal: Option<TerminalLayer>,
    pub editor: Option<EditorLayer>,
    pub env: HashMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessSnapshot {
    pub pid: u32,
    pub ppid: u32,
    pub comm: String,
    pub agent: Option<String>,
    pub cwd: String,
    pub env: HashMap<String, String>,
}

/// A comm is not always the kind: wezterm's GUI process is `wezterm-gui`. Every
/// comm-derived kind must route through here or no resolver would claim the host.
pub(crate) fn kind_for_comm(comm: &str) -> Option<&'static str> {
    match comm {
        "kitty" => Some("kitty"),
        "alacritty" => Some("alacritty"),
        "wezterm-gui" | "wezterm" => Some("wezterm"),
        "ghostty" => Some("ghostty"),
        _ => None,
    }
}

const SHELL_COMMS: [&str; 8] = ["bash", "zsh", "fish", "sh", "dash", "ksh", "tcsh", "csh"];

pub fn launcher_of(snapshot: &ProcessSnapshot, processes: &[ProcessSnapshot]) -> Option<String> {
    let by_pid: HashMap<u32, &ProcessSnapshot> = processes.iter().map(|p| (p.pid, p)).collect();
    let mut current = snapshot;
    for _ in 0..16 {
        let parent = by_pid.get(&current.ppid)?;
        if parent.pid == current.pid || parent.pid <= 1 {
            return None;
        }
        let comm = parent.comm.as_str();
        if !SHELL_COMMS.contains(&comm) && comm != snapshot.comm {
            return Some(comm.to_owned());
        }
        current = parent;
    }
    None
}

fn editor_kind_for_comm(comm: &str) -> Option<EditorKind> {
    match comm {
        "zed-editor" | "zed" => Some(EditorKind::Zed),
        "code-oss" | "code" => Some(EditorKind::VsCode),
        "cursor" => Some(EditorKind::Cursor),
        "windsurf" => Some(EditorKind::Windsurf),
        "codium" | "vscodium" => Some(EditorKind::Codium),
        _ => None,
    }
}

/// Every terminal that sets `TERM_PROGRAM` rewrites it, so it identifies the
/// INNERMOST host. The per-emulator variables leak instead: a wezterm, ghostty,
/// alacritty, tmux or zellij started from kitty still carries `KITTY_WINDOW_ID`,
/// so reading those first would label every nested host as kitty.
fn env_terminal_kind(env: &HashMap<String, String>) -> Option<&'static str> {
    if let Some(program) = env.get("TERM_PROGRAM") {
        match program.to_ascii_lowercase().as_str() {
            "wezterm" => return Some("wezterm"),
            "ghostty" => return Some("ghostty"),
            _ => {}
        }
    }
    if env.contains_key("KITTY_WINDOW_ID") || env.contains_key("KITTY_LISTEN_ON") {
        Some("kitty")
    } else if env.contains_key("ALACRITTY_WINDOW_ID") {
        Some("alacritty")
    } else if env.contains_key("WEZTERM_PANE") || env.contains_key("WEZTERM_UNIX_SOCKET") {
        Some("wezterm")
    } else if env.contains_key("GHOSTTY_RESOURCES_DIR") {
        Some("ghostty")
    } else {
        None
    }
}

fn window_id_for(kind: &str, env: &HashMap<String, String>) -> Option<String> {
    match kind {
        "kitty" => env.get("KITTY_WINDOW_ID").cloned(),
        "alacritty" => env.get("ALACRITTY_WINDOW_ID").cloned(),
        "wezterm" => env.get("WEZTERM_PANE").cloned(),
        _ => None,
    }
}

pub fn classify(snapshot: &ProcessSnapshot, processes: &[ProcessSnapshot]) -> TerminalInfo {
    let by_pid: HashMap<u32, &ProcessSnapshot> = processes.iter().map(|p| (p.pid, p)).collect();
    let mut current = snapshot;
    let env = snapshot.env.clone();
    let multiplexer_kind = if env.contains_key("TMUX_PANE") || env.contains_key("TMUX") {
        Some(MultiplexerKind::Tmux)
    } else if env.contains_key("ZELLIJ")
        || env.contains_key("ZELLIJ_PANE_ID")
        || env.contains_key("ZELLIJ_SESSION_NAME")
    {
        Some(MultiplexerKind::Zellij)
    } else {
        None
    };
    let env_kind = env_terminal_kind(&env);
    let mut terminal_pid = None;
    let mut walk_kind = None;
    let mut server_pid = None;
    let mut editor_pid = None;
    let mut editor_kind = None;
    for _ in 0..8 {
        if terminal_pid.is_none() {
            if let Some(kind) = kind_for_comm(&current.comm) {
                walk_kind = Some(kind);
                terminal_pid = Some(current.pid);
            }
        }
        if server_pid.is_none()
            && (current.comm == "tmux"
                || current.comm == "tmux: server"
                || current.comm == "zellij")
        {
            server_pid = Some(current.pid);
        }
        if editor_pid.is_none() {
            if let Some(kind) = editor_kind_for_comm(&current.comm) {
                editor_kind = Some(kind);
                editor_pid = Some(current.pid);
            }
        }
        let Some(parent) = by_pid.get(&current.ppid) else {
            break;
        };
        current = parent;
    }
    let editor = if env.contains_key("ZED_TERM") || editor_kind == Some(EditorKind::Zed) {
        Some(EditorLayer {
            kind: EditorKind::Zed,
            pid: editor_pid.unwrap_or(snapshot.pid),
        })
    } else if env.get("TERM_PROGRAM").map(String::as_str) == Some("vscode") || editor_kind.is_some()
    {
        let kind = editor_kind.unwrap_or(EditorKind::VsCode);
        let pid = editor_pid
            .or_else(|| env.get("VSCODE_PID").and_then(|value| value.parse().ok()))
            .unwrap_or(snapshot.pid);
        Some(EditorLayer { kind, pid })
    } else {
        None
    };
    let terminal = terminal_pid.map(|pid| {
        let kind = walk_kind.or(env_kind).unwrap_or("unknown");
        TerminalLayer {
            kind: kind.to_owned(),
            pid,
            window_id: window_id_for(kind, &env),
        }
    });
    let multiplexer = multiplexer_kind.map(|kind| MultiplexerLayer {
        kind,
        pane_id: match kind {
            MultiplexerKind::Tmux => env.get("TMUX_PANE").cloned(),
            MultiplexerKind::Zellij => env.get("ZELLIJ_PANE_ID").cloned(),
        },
        session_name: env.get("ZELLIJ_SESSION_NAME").cloned(),
        socket: env
            .get("TMUX")
            .and_then(|value| value.split(',').next())
            .map(str::to_owned),
        server_pid,
    });
    let kind = terminal
        .as_ref()
        .map(|layer| layer.kind.clone())
        .or_else(|| editor.as_ref().map(|layer| layer.kind.as_str().to_owned()))
        .unwrap_or_else(|| "unknown".to_owned());
    let raise_pid = terminal
        .as_ref()
        .map(|layer| layer.pid)
        .or_else(|| editor.as_ref().map(|layer| layer.pid))
        .unwrap_or(snapshot.pid);
    TerminalInfo {
        kind,
        raise_pid,
        agent_pid: snapshot.pid,
        multiplexer,
        terminal,
        editor,
        env,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: u32, ppid: u32, comm: &str, agent: Option<&str>) -> ProcessSnapshot {
        ProcessSnapshot {
            pid,
            ppid,
            comm: comm.to_owned(),
            agent: agent.map(str::to_owned),
            cwd: "/tmp/project".to_owned(),
            env: HashMap::new(),
        }
    }

    #[test]
    fn parent_chain_resolves_shell_to_kitty() {
        let agent = process(30, 20, "claude", Some("claude"));
        let shell = process(20, 10, "zsh", None);
        let kitty = process(10, 1, "kitty", None);
        let processes = vec![agent.clone(), shell, kitty];
        let info = classify(&agent, &processes);
        assert_eq!(info.kind, "kitty");
        assert_eq!(info.raise_pid, 10);
    }

    #[test]
    fn ppid_walk_normalises_wezterm_gui_comm_to_the_wezterm_kind() {
        let agent = process(30, 20, "claude", Some("claude"));
        let shell = process(20, 10, "zsh", None);
        let wezterm = process(10, 1, "wezterm-gui", None);
        let processes = vec![agent.clone(), shell, wezterm];
        let info = classify(&agent, &processes);
        assert_eq!(info.kind, "wezterm");
        assert_eq!(
            info.terminal.as_ref().map(|layer| layer.kind.as_str()),
            Some("wezterm")
        );
        assert_eq!(info.raise_pid, 10);
    }

    fn with_env(mut snapshot: ProcessSnapshot, entries: &[(&str, &str)]) -> ProcessSnapshot {
        snapshot.env = entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect();
        snapshot
    }

    #[test]
    fn plain_kitty_uses_env_window_id() {
        let agent = with_env(
            process(30, 20, "claude", Some("claude")),
            &[("KITTY_WINDOW_ID", "1")],
        );
        let processes = vec![
            agent.clone(),
            process(20, 10, "zsh", None),
            process(10, 1, "kitty", None),
        ];
        let info = classify(&agent, &processes);
        assert_eq!(info.kind, "kitty");
        assert_eq!(info.raise_pid, 10);
        assert!(info.multiplexer.is_none());
        assert_eq!(
            info.terminal
                .as_ref()
                .and_then(|layer| layer.window_id.as_deref()),
            Some("1")
        );
    }

    #[test]
    fn tmux_inside_kitty_keeps_outer_terminal() {
        let agent = with_env(
            process(30, 20, "claude", Some("claude")),
            &[
                ("TMUX_PANE", "%3"),
                ("TMUX", "/tmp/tmux-1000/default,1234,0"),
                ("KITTY_WINDOW_ID", "1"),
            ],
        );
        let processes = vec![
            agent.clone(),
            process(20, 40, "zsh", None),
            process(40, 10, "tmux: server", None),
            process(10, 1, "kitty", None),
        ];
        let info = classify(&agent, &processes);
        assert_eq!(info.kind, "kitty");
        assert_eq!(info.raise_pid, 10);
        assert_eq!(
            info.multiplexer.as_ref().map(|layer| layer.kind),
            Some(MultiplexerKind::Tmux)
        );
        assert_eq!(
            info.multiplexer
                .as_ref()
                .and_then(|layer| layer.pane_id.as_deref()),
            Some("%3")
        );
        assert_eq!(
            info.multiplexer
                .as_ref()
                .and_then(|layer| layer.socket.as_deref()),
            Some("/tmp/tmux-1000/default")
        );
    }

    #[test]
    fn zellij_inside_wezterm_is_layered() {
        let agent = with_env(
            process(30, 20, "claude", Some("claude")),
            &[
                ("ZELLIJ", "0"),
                ("ZELLIJ_SESSION_NAME", "main"),
                ("ZELLIJ_PANE_ID", "4"),
                ("WEZTERM_PANE", "2"),
                ("WEZTERM_UNIX_SOCKET", "/run/user/1000/wezterm/gui-sock-1"),
            ],
        );
        let processes = vec![
            agent.clone(),
            process(20, 10, "zsh", None),
            process(10, 1, "wezterm-gui", None),
        ];
        let info = classify(&agent, &processes);
        assert_eq!(info.kind, "wezterm");
        assert_eq!(info.terminal.as_ref().map(|layer| layer.pid), Some(10));
        assert_eq!(
            info.multiplexer.as_ref().map(|layer| layer.kind),
            Some(MultiplexerKind::Zellij)
        );
    }

    #[test]
    fn vscode_integrated_terminal_detects_cursor_editor() {
        let agent = with_env(
            process(30, 20, "claude", Some("claude")),
            &[("TERM_PROGRAM", "vscode"), ("VSCODE_PID", "555")],
        );
        let processes = vec![
            agent.clone(),
            process(20, 50, "bash", None),
            process(50, 1, "cursor", None),
        ];
        let info = classify(&agent, &processes);
        assert_eq!(info.kind, "cursor");
        assert_eq!(info.raise_pid, 50);
        assert_eq!(
            info.editor,
            Some(EditorLayer {
                kind: EditorKind::Cursor,
                pid: 50
            })
        );
        assert!(info.terminal.is_none());
    }

    #[test]
    fn bare_unknown_falls_back_to_agent() {
        let agent = process(30, 20, "claude", Some("claude"));
        let processes = vec![agent.clone(), process(20, 1, "init", None)];
        let info = classify(&agent, &processes);
        assert_eq!(info.kind, "unknown");
        assert_eq!(info.raise_pid, 30);
        assert!(info.multiplexer.is_none() && info.terminal.is_none() && info.editor.is_none());
    }

    #[test]
    fn multiplexer_without_reachable_emulator_degrades_safely() {
        let agent = with_env(
            process(30, 20, "claude", Some("claude")),
            &[("TMUX_PANE", "%3")],
        );
        let processes = vec![agent.clone(), process(20, 1, "tmux: server", None)];
        let info = classify(&agent, &processes);
        assert_eq!(info.kind, "unknown");
        assert_eq!(info.raise_pid, 30);
        assert!(info.terminal.is_none());
        assert!(info.multiplexer.is_some());
    }

    #[test]
    fn leaked_kitty_env_does_not_hijack_a_wezterm_host() {
        let agent = with_env(
            process(30, 20, "claude", Some("claude")),
            &[
                ("KITTY_WINDOW_ID", "1"),
                ("KITTY_LISTEN_ON", "unix:/tmp/kitty-88507"),
                ("TERM_PROGRAM", "WezTerm"),
                ("WEZTERM_PANE", "0"),
                ("WEZTERM_UNIX_SOCKET", "/run/user/1000/wezterm/gui-sock-10"),
            ],
        );
        let processes = vec![
            agent.clone(),
            process(20, 10, "bash", None),
            process(10, 1, "wezterm-gui", None),
        ];
        let info = classify(&agent, &processes);
        assert_eq!(info.kind, "wezterm");
        assert_eq!(info.raise_pid, 10);
        assert_eq!(
            info.terminal
                .as_ref()
                .and_then(|layer| layer.window_id.as_deref()),
            Some("0")
        );
    }

    #[test]
    fn leaked_kitty_env_does_not_hijack_an_alacritty_host() {
        let agent = with_env(
            process(30, 20, "claude", Some("claude")),
            &[
                ("KITTY_WINDOW_ID", "1"),
                ("KITTY_LISTEN_ON", "unix:/tmp/kitty-88507"),
                ("ALACRITTY_WINDOW_ID", "94071139484688"),
            ],
        );
        let processes = vec![
            agent.clone(),
            process(20, 10, "bash", None),
            process(10, 1, "alacritty", None),
        ];
        let info = classify(&agent, &processes);
        assert_eq!(info.kind, "alacritty");
        assert_eq!(info.raise_pid, 10);
        assert_eq!(
            info.terminal
                .as_ref()
                .and_then(|layer| layer.window_id.as_deref()),
            Some("94071139484688")
        );
    }

    #[test]
    fn zed_editor_comm_is_recognised_as_the_zed_editor() {
        let agent = process(30, 20, "claude", Some("claude"));
        let processes = vec![
            agent.clone(),
            process(20, 10, "bash", None),
            process(10, 1, "zed-editor", None),
        ];
        let info = classify(&agent, &processes);
        assert_eq!(info.kind, "zed");
        assert_eq!(info.raise_pid, 10);
        assert_eq!(
            info.editor.as_ref().map(|layer| layer.kind),
            Some(EditorKind::Zed)
        );
    }

    #[test]
    fn code_oss_comm_is_recognised_as_the_vscode_family() {
        let agent = with_env(
            process(30, 20, "claude", Some("claude")),
            &[("TERM_PROGRAM", "vscode"), ("VSCODE_PID", "555")],
        );
        let processes = vec![
            agent.clone(),
            process(20, 10, "bash", None),
            process(10, 1, "code-oss", None),
        ];
        let info = classify(&agent, &processes);
        assert_eq!(info.kind, "code");
        assert_eq!(info.raise_pid, 10);
        assert_eq!(
            info.editor.as_ref().map(|layer| layer.kind),
            Some(EditorKind::VsCode)
        );
    }
}
