#[cfg(any(test, not(feature = "qa-harness")))]
use crate::terminal;
#[cfg(any(test, not(feature = "qa-harness")))]
use std::path::Path;
use std::path::PathBuf;

pub const AGENTS: [&str; 3] = ["claude", "codex", "opencode"];
#[cfg(any(test, not(feature = "qa-harness")))]
const SESSION_SCRIPT: &str = "cd \"$1\" && exec \"$2\" run -- \"$3\"";

pub fn known_agent(name: &str) -> Result<&'static str, String> {
    AGENTS
        .iter()
        .copied()
        .find(|agent| *agent == name)
        .ok_or_else(|| format!("unknown agent '{name}'"))
}

pub fn available(lookup: impl Fn(&str) -> Option<PathBuf>) -> Vec<String> {
    AGENTS
        .iter()
        .filter(|agent| lookup(agent).is_some())
        .map(|agent| (*agent).to_owned())
        .collect()
}

#[cfg(any(test, not(feature = "qa-harness")))]
pub fn session_argv(program: &Path, folder: &Path, bridge: &Path, agent: &str) -> Vec<String> {
    let folder = folder.to_string_lossy();
    let bridge = bridge.to_string_lossy();
    terminal::terminal_argv(program, SESSION_SCRIPT, &[&folder, &bridge, agent])
}

#[cfg(any(test, not(feature = "qa-harness")))]
#[cfg(not(target_os = "macos"))]
pub fn open(folder: &str, agent: &str) -> Result<(), String> {
    let agent = known_agent(agent)?;
    let folder = Path::new(folder);
    if !folder.is_absolute() || !folder.is_dir() {
        return Err(format!("'{}' is not a directory", folder.display()));
    }
    let program =
        terminal::pick().ok_or_else(|| "no terminal emulator found on PATH".to_owned())?;
    let bridge = crate::client::daemon_candidates()
        .into_iter()
        .find(|path| path.is_file())
        .or_else(|| terminal::on_path("open-islandd"))
        .ok_or("Falta o componente de entrada. Reinstale o Open Island.")?;
    terminal::spawn_detached(session_argv(&program, folder, &bridge, agent))
}

#[cfg(test)]
#[path = "launch_tests.rs"]
mod tests;

#[cfg(any(test, all(target_os = "macos", not(feature = "qa-harness"))))]
fn terminal_script(folder: &str, agent: &str) -> String {
    crate::launch_macos::applescript(folder, agent, false)
}
#[cfg(all(target_os = "macos", not(feature = "qa-harness")))]
pub fn open(folder: &str, agent: &str) -> Result<(), String> {
    open_macos(folder, agent, "terminal")
}
#[cfg(all(target_os = "macos", not(feature = "qa-harness")))]
pub fn open_macos(folder: &str, agent: &str, terminal: &str) -> Result<(), String> {
    crate::launch_macos::open(folder, agent, terminal)
}
