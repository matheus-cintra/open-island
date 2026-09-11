//! Own a PTY for one agent, forwarding the user's terminal and island input.
use open_island_core::input_bridge::{self as wire, Request, Response};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::{
    ffi::OsString,
    io::{self, Write},
    os::unix::{io::AsRawFd, net::UnixListener},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

struct ChildGuard(Box<dyn portable_pty::Child + Send + Sync>, i32);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_some() {
            return;
        }
        let _ = self.0.kill();
        let flags = unsafe { libc::fcntl(self.1, libc::F_GETFL) };
        let nonblocking = flags >= 0
            && unsafe { libc::fcntl(self.1, libc::F_SETFL, flags | libc::O_NONBLOCK) } == 0;
        let deadline = Instant::now() + Duration::from_millis(300);
        while Instant::now() < deadline {
            if nonblocking {
                // On Darwin, pending output can delay even process exit. The
                // normal loop drains it; cancellation must do so as well.
                let mut bytes = [0u8; 8192];
                unsafe {
                    libc::read(self.1, bytes.as_mut_ptr().cast(), bytes.len());
                }
            }
            if self.0.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if let Some(pid) = self.0.process_id() {
            unsafe {
                libc::kill(pid as i32, libc::SIGKILL);
            }
        }
        unsafe {
            libc::tcflush(self.1, libc::TCIOFLUSH);
        }
        let _ = self.0.wait();
    }
}

struct TerminalGuard(Option<libc::termios>);
impl TerminalGuard {
    fn raw() -> io::Result<Self> {
        unsafe {
            if libc::isatty(0) != 1 {
                return Ok(Self(None));
            }
            let mut original = std::mem::zeroed();
            if libc::tcgetattr(0, &mut original) != 0 {
                return Err(io::Error::last_os_error());
            }
            let mut raw = original;
            libc::cfmakeraw(&mut raw);
            if libc::tcsetattr(0, libc::TCSANOW, &raw) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(Some(original)))
        }
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if let Some(original) = &self.0 {
            unsafe {
                libc::tcsetattr(0, libc::TCSANOW, original);
            }
        }
    }
}

fn size() -> PtySize {
    let mut value: libc::winsize = unsafe { std::mem::zeroed() };
    unsafe {
        libc::ioctl(0, libc::TIOCGWINSZ, &mut value);
    }
    PtySize {
        rows: if value.ws_row == 0 { 24 } else { value.ws_row },
        cols: if value.ws_col == 0 { 80 } else { value.ws_col },
        pixel_width: value.ws_xpixel,
        pixel_height: value.ws_ypixel,
    }
}

#[derive(Default)]
struct PasteMode {
    tail: Vec<u8>,
    enabled: bool,
}
impl PasteMode {
    fn observe(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.tail.push(byte);
            if self.tail.ends_with(b"\x1b[?2004h") {
                self.enabled = true;
            }
            if self.tail.ends_with(b"\x1b[?2004l") || self.tail.ends_with(b"\x1bc") {
                self.enabled = false;
            }
            if self.tail.len() > 8 {
                self.tail.remove(0);
            }
        }
    }
}

fn descendant(mut pid: u32, root: u32) -> bool {
    for _ in 0..64 {
        if pid == root {
            return true;
        }
        if pid <= 1 {
            return false;
        }
        let Some((parent, _)) = open_island_core::process::parent_and_comm(pid) else {
            return false;
        };
        if parent == pid {
            return false;
        }
        pid = parent;
    }
    false
}

fn write_pty(fd: i32, bytes: &[u8]) -> io::Result<()> {
    let mut rest = bytes;
    let deadline = Instant::now() + Duration::from_secs(2);
    while !rest.is_empty() {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "A sessão não recebeu toda a mensagem; confira antes de reenviar.",
            ));
        }
        let count = unsafe { libc::write(fd, rest.as_ptr().cast(), rest.len()) };
        if count > 0 {
            rest = &rest[count as usize..];
            continue;
        }
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::Interrupted {
            continue;
        }
        if error.kind() != io::ErrorKind::WouldBlock {
            return Err(error);
        }
        let mut poll = libc::pollfd {
            fd,
            events: libc::POLLOUT,
            revents: 0,
        };
        unsafe {
            libc::poll(&mut poll, 1, 20);
        }
    }
    Ok(())
}

fn deliver(request: Request, root: u32, fd: i32, device: u64, paste: bool) -> Result<(), String> {
    wire::validate_text(&request.text)?;
    if request.pid <= 1 || request.pid > i32::MAX as u32 || !descendant(request.pid, root) {
        return Err("A sessão não pertence a esta conexão de entrada.".into());
    }
    let foreground = unsafe { libc::tcgetpgrp(fd) };
    let target = unsafe { libc::getpgid(request.pid as i32) };
    if foreground <= 1 || target != foreground {
        return Err("O agente não está em primeiro plano neste terminal.".into());
    }
    if open_island_core::process::stdin_device(request.pid) != Some(device) {
        return Err("O processo selecionado não lê a entrada deste terminal.".into());
    }
    if !paste && request.text.contains(['\n', '\t']) {
        return Err("O agente ainda não habilitou colagem de texto. Aguarde o prompt antes de enviar várias linhas.".into());
    }
    let bytes = if paste {
        format!("\x1b[200~{}\x1b[201~", request.text)
    } else {
        request.text
    };
    write_pty(fd, bytes.as_bytes()).map_err(|e| e.to_string())?;
    if paste {
        std::thread::sleep(Duration::from_millis(50));
    }
    // Recheck after paste so an exiting agent cannot hand Enter to another process group.
    if unsafe { libc::tcgetpgrp(fd) } != target
        || !open_island_core::process::exists(request.pid)
        || open_island_core::process::stdin_device(request.pid) != Some(device)
    {
        return Err("A sessão encerrou durante a colagem. O Enter não foi enviado.".into());
    }
    write_pty(fd, b"\r").map_err(|e| e.to_string())
}

pub fn run(args: Vec<OsString>) -> Result<i32, String> {
    if args.is_empty() {
        return Err("Uso: open-islandd run -- <agente> [argumentos]".into());
    }
    run_inner(args).map_err(|e| e.to_string())
}

fn run_inner(args: Vec<OsString>) -> Result<i32, Box<dyn std::error::Error>> {
    let private = open_island_core::paths::private_runtime();
    open_island_core::paths::prepare_socket(&private.join("input.sock"))?;
    let directory = tempfile::Builder::new()
        .prefix("input-")
        .tempdir_in(private)?;
    let path = directory.path().join("s");
    let listener = UnixListener::bind(&path)?;
    listener.set_nonblocking(true)?;
    let pair = native_pty_system().openpty(size())?;
    let fd = pair.master.as_raw_fd().ok_or("PTY indisponível")?;
    use std::os::unix::fs::MetadataExt;
    let device = std::fs::metadata(
        pair.master
            .tty_name()
            .ok_or("Dispositivo da PTY indisponível")?,
    )?
    .rdev();
    let mut command = CommandBuilder::from_argv(args);
    command.cwd(std::env::current_dir()?);
    command.env(wire::ENV, &path);
    let mut child = ChildGuard(pair.slave.spawn_command(command)?, fd);
    let root = child.0.process_id().ok_or("PID indisponível")?;
    drop(pair.slave);
    let _terminal = TerminalGuard::raw()?;
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags < 0 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
            return Err(io::Error::last_os_error().into());
        }
    }
    let stopping = Arc::new(AtomicBool::new(false));
    let resizing = Arc::new(AtomicBool::new(false));
    struct Signals(Vec<signal_hook::SigId>);
    impl Drop for Signals {
        fn drop(&mut self) {
            for id in self.0.drain(..) {
                signal_hook::low_level::unregister(id);
            }
        }
    }
    let mut signals = Signals(Vec::new());
    for signal in [libc::SIGTERM, libc::SIGHUP, libc::SIGINT] {
        signals
            .0
            .push(signal_hook::flag::register(signal, stopping.clone())?);
    }
    signals.0.push(signal_hook::flag::register(
        libc::SIGWINCH,
        resizing.clone(),
    )?);
    let mut paste = PasteMode::default();
    let mut stdin = true;
    let mut buffer = [0u8; 8192];
    loop {
        if stopping.load(Ordering::Relaxed) {
            return Ok(128 + libc::SIGTERM);
        }
        if resizing.swap(false, Ordering::Relaxed) {
            pair.master.resize(size())?;
        }
        let mut fds = [
            libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: if stdin { 0 } else { -1 },
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: listener.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let result = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as _, 50) };
        if result < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            return Err(io::Error::last_os_error().into());
        }
        if fds[0].revents & (libc::POLLIN | libc::POLLHUP) != 0 {
            let count = unsafe { libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
            if count > 0 {
                let bytes = &buffer[..count as usize];
                paste.observe(bytes);
                io::stdout().write_all(bytes)?;
                io::stdout().flush()?;
            }
        }
        if let Some(status) = child.0.try_wait()? {
            // Exit can race the last output chunk; drain bytes already available.
            loop {
                let count = unsafe { libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
                if count <= 0 {
                    break;
                }
                io::stdout().write_all(&buffer[..count as usize])?;
            }
            io::stdout().flush()?;
            return Ok(status.exit_code() as i32);
        }
        if fds[1].revents & (libc::POLLIN | libc::POLLHUP) != 0 {
            let count = unsafe { libc::read(0, buffer.as_mut_ptr().cast(), buffer.len()) };
            if count > 0 {
                write_pty(fd, &buffer[..count as usize])?;
            } else if count == 0 {
                stdin = false;
            }
        }
        if fds[2].revents & libc::POLLIN != 0 {
            if let Ok((mut connection, _)) = listener.accept() {
                connection.set_read_timeout(Some(Duration::from_millis(500)))?;
                connection.set_write_timeout(Some(Duration::from_millis(500)))?;
                let error = match wire::read_frame::<Request>(&connection) {
                    Ok(request) => deliver(request, root, fd, device, paste.enabled).err(),
                    Err(error) => Some(error.to_string()),
                };
                let _ = wire::write_frame(&mut connection, &Response { error });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paste_mode_handles_split_sequences_and_reset() {
        let mut mode = PasteMode::default();
        mode.observe(b"hello\x1b[?20");
        assert!(!mode.enabled);
        mode.observe(b"04htext");
        assert!(mode.enabled);
        mode.observe(b"\x1b[?2004l");
        assert!(!mode.enabled);
        mode.observe(b"\x1b[?2004h\x1bc");
        assert!(!mode.enabled);
    }
}
