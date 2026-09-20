#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(feature = "qa-harness")]
mod qa;

use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct ProcessBirthIdentity(pub u64);
impl ProcessBirthIdentity {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}
pub fn birth_identity(pid: u32) -> Option<ProcessBirthIdentity> {
    if pid <= 1 || pid > i32::MAX as u32 {
        return None;
    }
    #[cfg(feature = "qa-harness")]
    {
        qa::route(pid, None, |source| source.birth_identity(pid))
    }
    #[cfg(all(not(feature = "qa-harness"), target_os = "linux"))]
    {
        linux::birth_identity(pid)
    }
    #[cfg(all(not(feature = "qa-harness"), target_os = "macos"))]
    {
        macos::birth_identity(pid)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessStat {
    pub parent: u32,
    pub comm: String,
    pub birth: Option<ProcessBirthIdentity>,
}

pub fn stat_fields(pid: u32) -> Option<ProcessStat> {
    #[cfg(feature = "qa-harness")]
    {
        qa::route(pid, None, |source| source.stat_fields(pid))
    }
    #[cfg(all(not(feature = "qa-harness"), target_os = "linux"))]
    {
        linux::stat_fields(pid)
    }
    #[cfg(all(not(feature = "qa-harness"), target_os = "macos"))]
    {
        macos::stat_fields(pid)
    }
}

#[cfg(feature = "qa-harness")]
pub use qa::{
    install_process_source_for_qa, register_process_for_qa, ProcessSource, ProcessSourceGuard,
};

#[cfg(feature = "qa-harness")]
pub fn system_process_source_for_qa() -> std::sync::Arc<dyn ProcessSource> {
    #[cfg(target_os = "linux")]
    let source = linux::SystemProcessSource;
    #[cfg(target_os = "macos")]
    let source = macos::SystemProcessSource;
    std::sync::Arc::new(source)
}

pub fn pids() -> Vec<u32> {
    #[cfg(feature = "qa-harness")]
    {
        qa::pids()
    }
    #[cfg(not(feature = "qa-harness"))]
    platform_pids()
}

pub fn parent_and_comm(pid: u32) -> Option<(u32, String)> {
    #[cfg(feature = "qa-harness")]
    {
        qa::route(pid, None, |source| source.parent_and_comm(pid))
    }
    #[cfg(not(feature = "qa-harness"))]
    platform_parent_and_comm(pid)
}

pub fn command(pid: u32) -> Option<Vec<u8>> {
    #[cfg(feature = "qa-harness")]
    {
        qa::route(pid, None, |source| source.command(pid))
    }
    #[cfg(not(feature = "qa-harness"))]
    platform_command(pid)
}

pub fn environment(pid: u32) -> Vec<u8> {
    #[cfg(feature = "qa-harness")]
    {
        qa::route(pid, Vec::new(), |source| source.environment(pid))
    }
    #[cfg(not(feature = "qa-harness"))]
    platform_environment(pid)
}

pub fn cwd(pid: u32) -> Option<PathBuf> {
    #[cfg(feature = "qa-harness")]
    {
        qa::route(pid, None, |source| source.cwd(pid))
    }
    #[cfg(not(feature = "qa-harness"))]
    platform_cwd(pid)
}

pub fn stdin_device(pid: u32) -> Option<u64> {
    #[cfg(feature = "qa-harness")]
    {
        qa::route(pid, None, |source| source.stdin_device(pid))
    }
    #[cfg(not(feature = "qa-harness"))]
    platform_stdin_device(pid)
}

pub fn exists(pid: u32) -> bool {
    if pid <= 1 || pid > i32::MAX as u32 {
        return false;
    }
    #[cfg(feature = "qa-harness")]
    {
        qa::route(pid, false, |source| source.exists(pid))
    }
    #[cfg(not(feature = "qa-harness"))]
    system_exists(pid)
}

fn system_exists(pid: u32) -> bool {
    unsafe {
        libc::kill(pid as i32, 0) == 0
            || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
}

#[cfg(all(target_os = "linux", not(feature = "qa-harness")))]
use linux::{
    command as platform_command, cwd as platform_cwd, environment as platform_environment,
    parent_and_comm as platform_parent_and_comm, pids as platform_pids,
    stdin_device as platform_stdin_device,
};
#[cfg(target_os = "macos")]
pub use macos::{activate, displays_asleep, session_unavailable};
#[cfg(all(target_os = "macos", not(feature = "qa-harness")))]
use macos::{
    command as platform_command, cwd as platform_cwd, environment as platform_environment,
    parent_and_comm as platform_parent_and_comm, pids as platform_pids,
    stdin_device as platform_stdin_device,
};

#[cfg(any(test, target_os = "macos"))]
fn session_unavailable_from(on_console: Option<bool>, locked: Option<bool>) -> Option<bool> {
    match (on_console, locked) {
        (Some(false), _) | (_, Some(true)) => Some(true),
        (Some(true), Some(false)) => Some(false),
        _ => None,
    }
}

#[cfg(test)]
#[test]
fn session_lock_and_user_switch_are_distinct_from_unknown_session_state() {
    assert_eq!(session_unavailable_from(Some(true), Some(true)), Some(true));
    assert_eq!(
        session_unavailable_from(Some(false), Some(false)),
        Some(true)
    );
    assert_eq!(
        session_unavailable_from(Some(true), Some(false)),
        Some(false)
    );
    assert_eq!(session_unavailable_from(None, None), None);
    assert_eq!(session_unavailable_from(Some(true), None), None);
    assert_eq!(session_unavailable_from(None, Some(true)), Some(true));
}

#[cfg(any(target_os = "macos", test))]
fn activate_ancestor(
    pid: u32,
    mut activate: impl FnMut(u32) -> Option<bool>,
    mut parent: impl FnMut(u32) -> Option<u32>,
) -> Result<(), String> {
    let mut current = pid;
    let mut visited = std::collections::HashSet::new();
    for _ in 0..64 {
        if current <= 1 || current > i32::MAX as u32 || !visited.insert(current) {
            break;
        }
        match activate(current) {
            Some(true) => return Ok(()),
            Some(false) => {
                return Err(format!(
                    "O macOS recusou ativar o aplicativo do processo {current}. Confira se ele ainda está aberto."
                ))
            }
            None => {}
        }
        let Some(next) = parent(current) else { break };
        current = next;
    }
    Err(format!(
        "Nenhum aplicativo aberto para o processo {pid}. Abra a sessão de novo no terminal."
    ))
}

#[cfg(test)]
mod activation_tests {
    use super::activate_ancestor;

    #[test]
    fn helper_process_resolves_to_its_gui_ancestor() {
        let mut tried = Vec::new();
        let result = activate_ancestor(
            30,
            |pid| {
                tried.push(pid);
                (pid == 10).then_some(true)
            },
            |pid| match pid {
                30 => Some(20),
                20 => Some(10),
                _ => None,
            },
        );
        assert!(result.is_ok());
        assert_eq!(tried, [30, 20, 10]);
    }

    #[test]
    fn denied_gui_activation_does_not_activate_another_app() {
        let mut tried = Vec::new();
        let result = activate_ancestor(
            30,
            |pid| {
                tried.push(pid);
                Some(false)
            },
            |_| Some(10),
        );
        assert!(result.unwrap_err().contains("recusou"));
        assert_eq!(tried, [30]);
    }

    #[test]
    fn missing_process_cycle_and_launchd_stop_without_activating_unrelated_apps() {
        for parent in [None, Some(30), Some(1)] {
            let mut tried = Vec::new();
            assert!(activate_ancestor(
                30,
                |pid| {
                    tried.push(pid);
                    None
                },
                |_| parent
            )
            .is_err());
            assert_eq!(tried, [30]);
        }
    }
}
