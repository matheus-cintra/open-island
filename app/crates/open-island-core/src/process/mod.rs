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
