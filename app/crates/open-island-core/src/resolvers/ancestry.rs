use crate::terminal::kind_for_comm;

pub(crate) fn proc_parent_and_comm(pid: u32) -> Option<(u32, String)> {
    crate::process::parent_and_comm(pid)
}

pub(crate) fn emulator_ancestor(
    pid: u32,
    parent_and_comm: &dyn Fn(u32) -> Option<(u32, String)>,
) -> Option<u32> {
    let mut current = pid;
    for _ in 0..64 {
        let (ppid, comm) = parent_and_comm(current)?;
        if kind_for_comm(&comm).is_some() {
            return Some(current);
        }
        if ppid == current || ppid == 0 {
            return None;
        }
        current = ppid;
    }
    None
}

pub(crate) fn pids_with_comm_and_cmdline(comm: &str, needle: &str) -> Vec<u32> {
    crate::process::pids()
        .into_iter()
        .filter(|pid| {
            crate::process::parent_and_comm(*pid).is_some_and(|(_, name)| name == comm)
                && crate::process::command(*pid).is_some_and(|bytes| {
                    String::from_utf8_lossy(&bytes)
                        .replace('\0', " ")
                        .contains(needle)
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::emulator_ancestor;

    #[test]
    fn emulator_ancestor_follows_scripted_chain() {
        let chain = |pid| match pid {
            30 => Some((20, "bash".into())),
            20 => Some((10, "zellij".into())),
            10 => Some((1, "kitty".into())),
            _ => None,
        };
        assert_eq!(emulator_ancestor(30, &chain), Some(10));
    }
}
