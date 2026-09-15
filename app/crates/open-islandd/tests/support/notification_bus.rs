use std::{
    io,
    os::unix::{net::UnixStream, process::CommandExt},
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub struct TestBus {
    child: Child,
    directory: tempfile::TempDir,
}

impl TestBus {
    pub fn start() -> io::Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("oi-bus-")
            .tempdir_in("/tmp")?;
        let address = format!("unix:path={}", directory.path().join("bus").display());
        let child = Command::new("dbus-daemon")
            .args([
                "--session",
                "--nofork",
                "--nopidfile",
                "--address",
                &address,
            ])
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()?;
        let mut bus = Self { child, directory };
        let deadline = Instant::now() + Duration::from_secs(2);
        while UnixStream::connect(bus.socket()).is_err() {
            if bus.child.try_wait()?.is_some() || Instant::now() >= deadline {
                return Err(io::Error::other("private bus did not become ready"));
            }
            thread::sleep(Duration::from_millis(10));
        }
        Ok(bus)
    }

    pub fn socket(&self) -> PathBuf {
        self.directory.path().join("bus")
    }

    pub fn address(&self) -> String {
        format!("unix:path={}", self.socket().display())
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for TestBus {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_millis(200);
        while matches!(self.child.try_wait(), Ok(None)) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.wait();
    }
}
