use std::{fs, path::PathBuf};

pub(super) fn pids() -> Vec<u32> {
    fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
        .collect()
}
pub(super) fn parent_and_comm(pid: u32) -> Option<(u32, String)> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let close = text.rfind(')')?;
    let comm = text.get(text.find('(')? + 1..close)?.to_owned();
    Some((
        text.get(close + 2..)?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()?,
        comm,
    ))
}
pub(super) fn command(pid: u32) -> Option<Vec<u8>> {
    fs::read(format!("/proc/{pid}/cmdline")).ok()
}
pub(super) fn environment(pid: u32) -> Vec<u8> {
    fs::read(format!("/proc/{pid}/environ")).unwrap_or_default()
}
pub(super) fn cwd(pid: u32) -> Option<PathBuf> {
    fs::read_link(format!("/proc/{pid}/cwd")).ok()
}

pub(super) fn stdin_device(pid: u32) -> Option<u64> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    let metadata = fs::metadata(format!("/proc/{pid}/fd/0")).ok()?;
    metadata
        .file_type()
        .is_char_device()
        .then(|| metadata.rdev())
}

#[cfg(feature = "qa-harness")]
pub(super) struct SystemProcessSource;

#[cfg(feature = "qa-harness")]
impl super::ProcessSource for SystemProcessSource {
    fn pids(&self) -> Vec<(u32, super::ProcessBirthIdentity)> {
        pids()
            .into_iter()
            .filter_map(|pid| birth_identity(pid).map(|identity| (pid, identity)))
            .collect()
    }

    fn birth_identity(&self, pid: u32) -> Option<super::ProcessBirthIdentity> {
        birth_identity(pid)
    }

    fn parent_and_comm(&self, pid: u32) -> Option<(u32, String)> {
        parent_and_comm(pid)
    }

    fn command(&self, pid: u32) -> Option<Vec<u8>> {
        command(pid)
    }

    fn environment(&self, pid: u32) -> Vec<u8> {
        environment(pid)
    }

    fn cwd(&self, pid: u32) -> Option<PathBuf> {
        cwd(pid)
    }

    fn stdin_device(&self, pid: u32) -> Option<u64> {
        stdin_device(pid)
    }

    fn exists(&self, pid: u32) -> bool {
        super::system_exists(pid)
    }
}

pub(super) fn birth_identity(pid: u32) -> Option<super::ProcessBirthIdentity> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let close = text.rfind(')')?;
    text.get(close + 2..)?
        .split_whitespace()
        .nth(19)?
        .parse()
        .ok()
        .map(super::ProcessBirthIdentity::new)
}
