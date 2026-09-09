use crate::terminal;
use std::path::{Path, PathBuf};

pub const AGENTS: [&str; 3] = ["claude", "codex", "opencode"];
const SESSION_SCRIPT: &str = "cd \"$1\" && exec \"$2\"";

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

pub fn session_argv(program: &Path, folder: &Path, agent: &str) -> Vec<String> {
    let folder = folder.to_string_lossy();
    terminal::terminal_argv(program, SESSION_SCRIPT, &[&folder, agent])
}

pub fn open(folder: &str, agent: &str) -> Result<(), String> {
    let agent = known_agent(agent)?;
    let folder = Path::new(folder);
    if !folder.is_absolute() || !folder.is_dir() {
        return Err(format!("'{}' is not a directory", folder.display()));
    }
    let program =
        terminal::pick().ok_or_else(|| "no terminal emulator found on PATH".to_owned())?;
    terminal::spawn_detached(session_argv(&program, folder, agent))
}

#[cfg(test)]
#[path = "launch_tests.rs"]
mod tests;
