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

#[cfg(not(target_os = "macos"))]
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

/// Shell quoting and AppleScript string quoting are separate boundaries.
#[cfg(any(test, target_os = "macos"))]
fn terminal_script(folder: &str, agent: &str) -> String {
    let quote = |text: &str| format!("'{}'", text.replace('\'', "'\"'\"'"));
    let command = format!("cd {} && {}", quote(folder), quote(agent));
    let literal = command
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    format!("tell application \"Terminal\"\nactivate\ndo script \"{literal}\"\nend tell")
}
#[cfg(target_os = "macos")]
pub fn open(folder: &str, agent: &str) -> Result<(), String> {
    let agent = known_agent(agent)?;
    let path = Path::new(folder);
    if !path.is_absolute() || !path.is_dir() {
        return Err("Selecione uma pasta válida.".into());
    }
    let executable = terminal::on_path(agent)
        .ok_or_else(|| format!("Instale {agent} para abrir uma sessão."))?;
    let output = std::process::Command::new("/usr/bin/osascript")
        .args([
            "-e",
            &terminal_script(folder, &executable.to_string_lossy()),
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!("Não foi possível abrir o Terminal. Em Ajustes do Sistema → Privacidade e Segurança → Automação, permita que Open Island controle Terminal. {}", String::from_utf8_lossy(&output.stderr).trim()))
    }
}
