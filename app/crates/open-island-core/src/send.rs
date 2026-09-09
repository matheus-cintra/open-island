use crate::{jump::JumpStep, runner::CommandRunner, terminal::TerminalInfo};
use std::path::{Path, PathBuf};

const TMUX_BUFFER: &str = "open-island";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Channel {
    Tmux {
        socket: Option<String>,
        pane: String,
    },
    Zellij {
        session: String,
        pane_id: String,
    },
    Wezterm {
        socket: String,
        pane_id: String,
    },
    Kitty {
        socket: String,
        window_id: String,
    },
}

impl Channel {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Tmux { .. } => "tmux",
            Self::Zellij { .. } => "zellij",
            Self::Wezterm { .. } => "wezterm",
            Self::Kitty { .. } => "kitty",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocked {
    HostUnsupported,
    KittyRemoteControlOff,
    WeztermSocketMissing,
    PaneGone,
}

impl Blocked {
    pub fn code(&self) -> &'static str {
        match self {
            Self::HostUnsupported => "host_unsupported",
            Self::KittyRemoteControlOff => "kitty_remote_control_off",
            Self::WeztermSocketMissing => "wezterm_socket_missing",
            Self::PaneGone => "pane_gone",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendStep {
    pub program: String,
    pub args: Vec<String>,
}

pub fn capability(host: &TerminalInfo) -> Result<&'static str, Blocked> {
    if let Some(multiplexer) = host.multiplexer.as_ref() {
        if multiplexer.pane_id.is_some() {
            return Ok(multiplexer.kind.as_str());
        }
    }
    match host
        .terminal
        .as_ref()
        .map(|terminal| terminal.kind.as_str())
    {
        Some("wezterm") => {
            if host.env.contains_key("WEZTERM_UNIX_SOCKET") || host.env.contains_key("WEZTERM_PANE")
            {
                Ok("wezterm")
            } else {
                Err(Blocked::WeztermSocketMissing)
            }
        }
        Some("kitty") => {
            if host.env.contains_key("KITTY_LISTEN_ON") {
                Ok("kitty")
            } else {
                Err(Blocked::KittyRemoteControlOff)
            }
        }
        _ => Err(Blocked::HostUnsupported),
    }
}

pub fn channel_for(
    host: &TerminalInfo,
    steps: &[JumpStep],
    runtime_dir: Option<&Path>,
    socket_exists: &dyn Fn(&Path) -> bool,
) -> Result<Channel, Blocked> {
    for step in steps {
        match step {
            JumpStep::TmuxPaneGone => return Err(Blocked::PaneGone),
            JumpStep::TmuxSelectPane { socket, pane }
            | JumpStep::TmuxSelectWindow { socket, pane }
            | JumpStep::TmuxSwitchClient { socket, pane, .. } => {
                return Ok(Channel::Tmux {
                    socket: socket.clone(),
                    pane: pane.clone(),
                })
            }
            JumpStep::ZellijFocusPane { session, pane_id } => {
                return Ok(Channel::Zellij {
                    session: session.clone(),
                    pane_id: pane_id.clone(),
                })
            }
            JumpStep::WeztermActivatePane { pane_id } => {
                let socket = wezterm_socket(host, runtime_dir, socket_exists)
                    .ok_or(Blocked::WeztermSocketMissing)?;
                return Ok(Channel::Wezterm {
                    socket,
                    pane_id: pane_id.clone(),
                });
            }
            JumpStep::KittyFocusWindow { window_id } => {
                let socket = host
                    .env
                    .get("KITTY_LISTEN_ON")
                    .cloned()
                    .ok_or(Blocked::KittyRemoteControlOff)?;
                return Ok(Channel::Kitty {
                    socket,
                    window_id: window_id.clone(),
                });
            }
            _ => {}
        }
    }
    Err(
        match host
            .terminal
            .as_ref()
            .map(|terminal| terminal.kind.as_str())
        {
            Some("kitty") => Blocked::KittyRemoteControlOff,
            Some("wezterm") => Blocked::WeztermSocketMissing,
            _ => Blocked::HostUnsupported,
        },
    )
}

fn wezterm_socket(
    host: &TerminalInfo,
    runtime_dir: Option<&Path>,
    socket_exists: &dyn Fn(&Path) -> bool,
) -> Option<String> {
    if let Some(socket) = host.env.get("WEZTERM_UNIX_SOCKET") {
        return Some(socket.clone());
    }
    let pid = host.terminal.as_ref()?.pid;
    let candidate: PathBuf = runtime_dir?.join("wezterm").join(format!("gui-sock-{pid}"));
    socket_exists(&candidate).then(|| candidate.to_string_lossy().into_owned())
}

pub fn normalize(text: &str) -> String {
    let unified = text.replace("\r\n", "\n").replace('\r', "\n");
    unified.trim_end_matches('\n').to_owned()
}

pub fn plan(channel: &Channel, text: &str) -> Vec<SendStep> {
    let text = normalize(text);
    let multi = text.contains('\n');
    match channel {
        Channel::Tmux { socket, pane } => {
            let tmux = |args: &[&str]| {
                let mut owned = Vec::new();
                if let Some(socket) = socket {
                    owned.extend(["-S".to_owned(), socket.clone()]);
                }
                owned.extend(args.iter().map(|arg| (*arg).to_owned()));
                step("tmux", owned)
            };
            let mut steps = if multi {
                vec![
                    tmux(&["set-buffer", "-b", TMUX_BUFFER, &text]),
                    tmux(&["paste-buffer", "-p", "-d", "-b", TMUX_BUFFER, "-t", pane]),
                ]
            } else {
                vec![tmux(&["send-keys", "-t", pane, "-l", &text])]
            };
            steps.push(tmux(&["send-keys", "-t", pane, "Enter"]));
            steps
        }
        Channel::Zellij { session, pane_id } => vec![
            step(
                "zellij",
                strings(&["--session", session, "action", "focus-pane-id", pane_id]),
            ),
            step(
                "zellij",
                strings(&["--session", session, "action", "write-chars", &text]),
            ),
            step(
                "zellij",
                strings(&["--session", session, "action", "write", "13"]),
            ),
        ],
        Channel::Wezterm { socket, pane_id } => {
            let wezterm = |args: &[&str]| {
                let mut owned = vec![
                    format!("WEZTERM_UNIX_SOCKET={socket}"),
                    "wezterm".to_owned(),
                ];
                owned.extend(args.iter().map(|arg| (*arg).to_owned()));
                step("env", owned)
            };
            let mut send = vec!["cli", "send-text", "--pane-id", pane_id];
            if !multi {
                send.push("--no-paste");
            }
            send.push(&text);
            vec![
                wezterm(&send),
                wezterm(&["cli", "send-text", "--pane-id", pane_id, "--no-paste", "\r"]),
            ]
        }
        Channel::Kitty { socket, window_id } => {
            let target = format!("id:{window_id}");
            let escaped = text.replace('\\', "\\\\");
            let mut send = vec!["@", "--to", socket, "send-text", "--match", &target];
            if multi {
                send.push("--bracketed-paste=enable");
            }
            send.push(&escaped);
            vec![
                step("kitty", strings(&send)),
                step(
                    "kitty",
                    strings(&["@", "--to", socket, "send-text", "--match", &target, "\\r"]),
                ),
            ]
        }
    }
}

pub fn execute(steps: &[SendStep], runner: &dyn CommandRunner) -> Result<(), String> {
    for current in steps {
        let args: Vec<&str> = current.args.iter().map(String::as_str).collect();
        let output = runner.run(&current.program, &args)?;
        if !output.status.success() {
            return Err(format!(
                "{} failed: {}",
                current.program,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }
    Ok(())
}

fn step(program: &str, args: Vec<String>) -> SendStep {
    SendStep {
        program: program.to_owned(),
        args,
    }
}

fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_owned()).collect()
}

#[cfg(test)]
#[path = "send_tests.rs"]
mod tests;
