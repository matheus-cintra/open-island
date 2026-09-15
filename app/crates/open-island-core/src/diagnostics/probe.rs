use std::{
    io::{ErrorKind, Read},
    os::{fd::AsRawFd, unix::process::CommandExt},
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Owned {
    child: Child,
    /// Keep the process group armed while output may still be held by a child.
    ///
    /// A successful child exit alone is not enough to disarm this: a shell can
    /// leave a background process holding the probe's stdout open. Once the
    /// child has exited and EOF was observed, no descendant can still own that
    /// inherited descriptor, so dropping the group avoids targeting a reused
    /// process-group id after a normal completion.
    kill_group: bool,
}

impl Drop for Owned {
    fn drop(&mut self) {
        if self.kill_group {
            unsafe {
                libc::kill(-(self.child.id() as i32), libc::SIGKILL);
            }
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}
pub fn run(
    program: &Path,
    args: &[&str],
    total_deadline: Instant,
) -> Result<(bool, String), &'static str> {
    let deadline = total_deadline.min(Instant::now() + Duration::from_secs(2));
    if Instant::now() >= deadline {
        return Err("timeout");
    }
    let mut child = Owned {
        child: Command::new(program)
            .args(args)
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "unavailable")?,
        kill_group: true,
    };
    let mut output = child.child.stdout.take().ok_or("unavailable")?;
    let fd = output.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err("unavailable");
    }
    let mut bytes = Vec::new();
    loop {
        if Instant::now() >= deadline {
            return Err("timeout");
        }
        let mut eof = false;
        for _ in 0..8 {
            let mut buffer = [0; 4096];
            match output.read(&mut buffer) {
                Ok(0) => {
                    eof = true;
                    break;
                }
                Ok(n) => {
                    if bytes.len() + n > 32768 {
                        return Err("output_limit");
                    }
                    bytes.extend_from_slice(&buffer[..n]);
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(_) => return Err("unavailable"),
            }
        }
        if let Some(status) = child.child.try_wait().map_err(|_| "unavailable")? {
            if eof {
                child.kill_group = false;
                let output = String::from_utf8(bytes).map_err(|_| "invalid_output")?;
                return Ok((status.success(), output));
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
}
