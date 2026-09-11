//! Best-effort process queries shared by discovery and terminal resolvers.
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(target_os = "macos")]
pub use macos::*;

pub fn exists(pid: u32) -> bool {
    if pid <= 1 || pid > i32::MAX as u32 {
        return false;
    }
    // EPERM means the process exists but belongs to another security context.
    unsafe {
        libc::kill(pid as i32, 0) == 0
            || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
}

/// Skip CLI/helper processes, but never switch to another app if the owning GUI
/// application was found and macOS refused its activation.
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
            Some(false) => return Err(format!(
                "O macOS recusou ativar o aplicativo do processo {current}. Verifique se ele ainda está aberto."
            )),
            None => {}
        }
        let Some(next) = parent(current) else { break };
        current = next;
    }
    Err(format!("Não foi encontrado um aplicativo aberto para o processo {pid}. Abra a sessão novamente no terminal."))
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
