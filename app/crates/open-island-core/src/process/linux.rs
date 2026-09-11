use std::{fs, path::PathBuf};

pub fn pids() -> Vec<u32> {
    fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
        .collect()
}
pub fn parent_and_comm(pid: u32) -> Option<(u32, String)> {
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
pub fn command(pid: u32) -> Option<Vec<u8>> {
    fs::read(format!("/proc/{pid}/cmdline")).ok()
}
pub fn environment(pid: u32) -> Vec<u8> {
    fs::read(format!("/proc/{pid}/environ")).unwrap_or_default()
}
pub fn cwd(pid: u32) -> Option<PathBuf> {
    fs::read_link(format!("/proc/{pid}/cwd")).ok()
}

pub fn stdin_device(pid: u32) -> Option<u64> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    let metadata = fs::metadata(format!("/proc/{pid}/fd/0")).ok()?;
    metadata
        .file_type()
        .is_char_device()
        .then(|| metadata.rdev())
}
