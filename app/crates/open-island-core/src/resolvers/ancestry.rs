use std::fs;

use crate::terminal::kind_for_comm;

pub(crate) fn proc_parent_and_comm(pid: u32) -> Option<(u32, String)> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let comm_start = stat.find('(')?;
    let comm_end = stat.rfind(')')?;
    let fields = stat
        .get(comm_end + 2..)?
        .split_whitespace()
        .collect::<Vec<_>>();
    let ppid = fields.get(1)?.parse().ok()?;
    Some((ppid, stat.get(comm_start + 1..comm_end)?.to_owned()))
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
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| {
            let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
                return false;
            };
            let Some(comm_start) = stat.find('(') else {
                return false;
            };
            let Some(comm_end) = stat.rfind(')') else {
                return false;
            };
            if stat.get(comm_start + 1..comm_end) != Some(comm) {
                return false;
            }
            fs::read(format!("/proc/{pid}/cmdline"))
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .is_some_and(|cmdline| cmdline.replace('\0', " ").contains(needle))
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
