use crate::message_delivery::DaemonEpoch;
use std::io;

pub fn generate() -> io::Result<DaemonEpoch> {
    let mut bytes = [0u8; 16];
    #[cfg(target_os = "linux")]
    {
        let mut offset = 0;
        while offset < bytes.len() {
            let count = unsafe {
                libc::getrandom(bytes[offset..].as_mut_ptr().cast(), bytes.len() - offset, 0)
            };
            if count < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if count == 0 {
                return Err(io::Error::other("epoch entropy unavailable"));
            }
            offset += count as usize;
        }
    }
    #[cfg(target_os = "macos")]
    if unsafe { libc::getentropy(bytes.as_mut_ptr().cast(), bytes.len()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(DaemonEpoch(
        bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
    ))
}
