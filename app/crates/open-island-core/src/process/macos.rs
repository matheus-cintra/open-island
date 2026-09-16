use std::{ffi::CStr, path::PathBuf};
extern "C" {
    fn oi_process_birth(pid: i32, birth: *mut u64) -> i32;
    fn oi_pids(out: *mut i32, bytes: i32) -> i32;
    fn oi_process(pid: i32, parent: *mut u32, name: *mut i8, capacity: i32) -> i32;
    fn oi_cwd(pid: i32, out: *mut i8, capacity: i32) -> i32;
    fn oi_procargs_length(pid: i32) -> usize;
    fn oi_procargs(pid: i32, out: *mut i8, length: *mut usize) -> i32;
    fn oi_stdin_device(pid: i32, device: *mut u64) -> i32;
    fn oi_displays_asleep() -> i32;
    fn oi_session_state(on_console: *mut i32, locked: *mut i32);
    fn oi_activate(pid: i32) -> i32;
}
pub(super) fn pids() -> Vec<u32> {
    let count = unsafe { oi_pids(std::ptr::null_mut(), 0) }.max(0) as usize;
    let mut values = vec![0i32; count + 1024];
    let read = unsafe { oi_pids(values.as_mut_ptr(), (values.len() * 4) as i32) }.max(0) as usize;
    values
        .into_iter()
        .take(read)
        .filter(|pid| *pid > 0)
        .map(|pid| pid as u32)
        .collect()
}
pub(super) fn parent_and_comm(pid: u32) -> Option<(u32, String)> {
    let mut parent = 0;
    let mut name = [0i8; 1024];
    (unsafe {
        oi_process(
            pid as i32,
            &mut parent,
            name.as_mut_ptr(),
            name.len() as i32,
        )
    } != 0)
        .then(|| {
            (
                parent,
                unsafe { CStr::from_ptr(name.as_ptr()) }
                    .to_string_lossy()
                    .into_owned(),
            )
        })
}
pub(super) fn cwd(pid: u32) -> Option<PathBuf> {
    let mut path = [0i8; 4096];
    (unsafe { oi_cwd(pid as i32, path.as_mut_ptr(), path.len() as i32) } != 0).then(|| {
        PathBuf::from(
            unsafe { CStr::from_ptr(path.as_ptr()) }
                .to_string_lossy()
                .as_ref(),
        )
    })
}
fn args(pid: u32) -> Option<(Vec<u8>, Vec<u8>)> {
    const MAX_PROCARGS_BYTES: usize = 1024 * 1024;
    let mut length = unsafe { oi_procargs_length(pid as i32) };
    if length == 0 || length > MAX_PROCARGS_BYTES {
        return None;
    }
    let mut bytes = vec![0; length];
    if unsafe { oi_procargs(pid as i32, bytes.as_mut_ptr().cast(), &mut length) } == 0 {
        return None;
    }
    bytes.truncate(length);
    super::super::process_args::parse(&bytes)
}
pub(super) fn command(pid: u32) -> Option<Vec<u8>> {
    args(pid).map(|(command, _)| command)
}
pub(super) fn environment(pid: u32) -> Vec<u8> {
    args(pid).map(|(_, env)| env).unwrap_or_default()
}
pub(super) fn stdin_device(pid: u32) -> Option<u64> {
    let mut device = 0;
    (unsafe { oi_stdin_device(pid as i32, &mut device) } != 0).then_some(device)
}
pub fn activate(pid: u32) -> Result<(), String> {
    super::activate_ancestor(
        pid,
        |pid| match unsafe { oi_activate(pid as i32) } {
            -1 => None,
            result => Some(result == 1),
        },
        |pid| parent_and_comm(pid).map(|(parent, _)| parent),
    )
}

pub fn displays_asleep() -> Option<bool> {
    match unsafe { oi_displays_asleep() } {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

pub fn session_unavailable() -> Option<bool> {
    let (mut console, mut locked) = (-1, -1);
    unsafe {
        oi_session_state(&mut console, &mut locked);
    }
    let boolean = |value| match value {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    };
    super::session_unavailable_from(boolean(console), boolean(locked))
}

#[cfg(feature = "qa-harness")]
pub(super) struct SystemProcessSource;

#[cfg(feature = "qa-harness")]
impl super::ProcessSource for SystemProcessSource {
    fn pids(&self) -> Vec<(u32, super::ProcessBirthIdentity)> {
        pids()
            .into_iter()
            .filter_map(|pid| self.birth_identity(pid).map(|birth| (pid, birth)))
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
    let mut birth = 0;
    (unsafe { oi_process_birth(pid as i32, &mut birth) } != 0)
        .then_some(super::ProcessBirthIdentity::new(birth))
}

pub(super) fn stat_fields(pid: u32) -> Option<super::ProcessStat> {
    let (parent, comm) = parent_and_comm(pid)?;
    let birth = (pid > 1 && pid <= i32::MAX as u32)
        .then(|| birth_identity(pid))
        .flatten();
    Some(super::ProcessStat { parent, comm, birth })
}
