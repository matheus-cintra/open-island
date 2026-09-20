use crate::{
    jump::{JumpStep, LocationResolver},
    runner::CommandRunner,
    terminal::TerminalInfo,
};
use serde::Deserialize;
use std::path::Path;

pub struct KittyResolver;

#[derive(Deserialize)]
struct OsWindow {
    tabs: Vec<Tab>,
}
#[derive(Deserialize)]
struct Tab {
    windows: Vec<Window>,
}
#[derive(Deserialize)]
struct Window {
    id: u64,
    foreground_processes: Vec<Process>,
}
#[derive(Deserialize)]
struct Process {
    pid: u32,
}

fn reachable(socket: &str) -> bool {
    let path = socket.strip_prefix("unix:").unwrap_or(socket);
    path.starts_with('@') || Path::new(path).exists()
}

impl LocationResolver for KittyResolver {
    fn id(&self) -> &str {
        "kitty"
    }
    fn can_resolve(&self, host: &TerminalInfo) -> bool {
        host.terminal.as_ref().is_some_and(|t| t.kind == "kitty")
    }
    fn resolve(&self, host: &TerminalInfo, runner: &dyn CommandRunner) -> Option<Vec<JumpStep>> {
        let terminal = host.terminal.as_ref()?;
        let mut steps = Vec::new();
        // Without the instance socket `kitty @` falls back to the caller's
        // controlling terminal, which the daemon does not have.
        if let Some(socket) = host
            .env
            .get("KITTY_LISTEN_ON")
            .filter(|socket| reachable(socket.as_str()))
        {
            let window_id = terminal.window_id.clone().or_else(|| {
                let output = runner.run("kitty", &["@", "--to", socket, "ls"]).ok()?;
                if !output.status.success() {
                    return None;
                }
                let windows: Vec<OsWindow> = serde_json::from_slice(&output.stdout).ok()?;
                windows
                    .into_iter()
                    .flat_map(|os| os.tabs)
                    .flat_map(|tab| tab.windows)
                    .find(|window| {
                        window
                            .foreground_processes
                            .iter()
                            .any(|p| p.pid == host.agent_pid || p.pid == terminal.pid)
                    })
                    .map(|window| window.id.to_string())
            });
            if let Some(window_id) = window_id {
                steps.push(JumpStep::KittyFocusWindow {
                    window_id,
                    socket: socket.clone(),
                });
            }
        }
        steps.push(JumpStep::RaiseWindow { pid: terminal.pid });
        Some(steps)
    }
}
