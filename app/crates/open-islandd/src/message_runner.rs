use open_island_core::runner::CommandRunner;
use std::{
    io::{self, Read},
    os::{fd::AsRawFd, unix::process::CommandExt},
    process::{Child, Command, Output, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub struct MessageRunner {
    pub started: Instant,
    pub cancelled: Arc<AtomicBool>,
    pub effects: AtomicBool,
}
impl MessageRunner {
    pub fn new(started: Instant, cancelled: Arc<AtomicBool>) -> Self {
        Self {
            started,
            cancelled,
            effects: AtomicBool::new(false),
        }
    }
    pub fn remaining(&self) -> Result<Duration, String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err("delivery_cancelled".to_owned());
        }
        Duration::from_secs(8)
            .checked_sub(self.started.elapsed())
            .filter(|time| !time.is_zero())
            .ok_or_else(|| "delivery_timeout".to_owned())
    }
    pub fn run_effect(&self, program: &str, args: &[&str]) -> Result<Output, String> {
        self.execute(program, args, true, false)
    }
    pub fn run_cleanup(&self, program: &str, args: &[&str]) -> Result<Output, String> {
        self.execute(program, args, false, true)
    }
    fn stage_remaining(&self, cleanup: bool) -> Result<Duration, String> {
        if !cleanup {
            return self.remaining();
        }
        Duration::from_millis(9500)
            .checked_sub(self.started.elapsed())
            .filter(|time| !time.is_zero())
            .ok_or_else(|| "cleanup_timeout".to_owned())
    }
    fn execute(
        &self,
        program: &str,
        args: &[&str],
        effect: bool,
        cleanup: bool,
    ) -> Result<Output, String> {
        self.stage_remaining(cleanup)?;
        let child = Command::new(
            open_island_core::paths::executable(program).unwrap_or_else(|| program.into()),
        )
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|_| "channel_spawn_failed".to_owned())?;
        if effect {
            self.effects.store(true, Ordering::Release);
        }
        let mut group = ProcessGroup { child };
        let mut stdout = group.child.stdout.take().ok_or("channel_pipe_failed")?;
        let mut stderr = group.child.stderr.take().ok_or("channel_pipe_failed")?;
        nonblocking(stdout.as_raw_fd())?;
        nonblocking(stderr.as_raw_fd())?;
        let (mut out, mut err) = (Vec::new(), Vec::new());
        loop {
            drain(&mut stdout, &mut out);
            drain(&mut stderr, &mut err);
            if let Some(status) = group.child.try_wait().map_err(|_| "channel_wait_failed")? {
                drain(&mut stdout, &mut out);
                drain(&mut stderr, &mut err);
                return Ok(Output {
                    status,
                    stdout: out,
                    stderr: err,
                });
            }
            if let Err(error) = self.stage_remaining(cleanup) {
                group.signal(libc::SIGTERM);
                let kill_at = if cleanup {
                    self.started + Duration::from_millis(9750)
                } else {
                    (self.started + Duration::from_secs(9))
                        .min(Instant::now() + Duration::from_secs(1))
                };
                while Instant::now() < kill_at {
                    drain(&mut stdout, &mut out);
                    drain(&mut stderr, &mut err);
                    if group
                        .child
                        .try_wait()
                        .map_err(|_| "channel_wait_failed")?
                        .is_some()
                    {
                        return Err(error);
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                group.signal(libc::SIGKILL);
                let _ = group.child.wait();
                return Err(error);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
impl CommandRunner for MessageRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<Output, String> {
        self.execute(program, args, false, false)
    }
}
struct ProcessGroup {
    child: Child,
}
impl ProcessGroup {
    fn signal(&self, signal: i32) {
        unsafe {
            libc::kill(-(self.child.id() as i32), signal);
        }
    }
}
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        self.signal(libc::SIGKILL);
        let _ = self.child.wait();
    }
}
fn nonblocking(fd: i32) -> Result<(), String> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err("channel_pipe_failed".to_owned());
    }
    Ok(())
}
fn drain(reader: &mut impl Read, output: &mut Vec<u8>) {
    let mut bytes = [0; 8192];
    for _ in 0..8 {
        match reader.read(&mut bytes) {
            Ok(0) => break,
            Ok(count) => {
                let keep = count.min(65536usize.saturating_sub(output.len()));
                output.extend_from_slice(&bytes[..keep]);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
}
