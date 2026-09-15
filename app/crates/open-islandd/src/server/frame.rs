use std::{
    io::{self, Read},
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};
const MAX_INPUT: usize = 1024 * 1024;
const FRAME_TIMEOUT: Duration = Duration::from_secs(5);
pub struct Frames {
    stream: UnixStream,
    pending: Vec<u8>,
    received: Instant,
    first: bool,
}
impl Frames {
    pub fn new(stream: UnixStream) -> Self {
        Self {
            stream,
            pending: Vec::new(),
            received: Instant::now(),
            first: true,
        }
    }
    pub fn next(&mut self) -> io::Result<Option<Vec<u8>>> {
        let mut deadline = if self.first || !self.pending.is_empty() {
            Some(self.received + FRAME_TIMEOUT)
        } else {
            None
        };
        let mut line = Vec::new();
        loop {
            if let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
                if line.len() + end > MAX_INPUT {
                    return Err(io::Error::other("request_too_large"));
                }
                line.extend(self.pending.drain(..=end));
                self.first = false;
                return Ok(Some(line));
            }
            if line.len() + self.pending.len() > MAX_INPUT {
                return Err(io::Error::other("request_too_large"));
            }
            line.append(&mut self.pending);
            let timeout = deadline.map(|at| at.saturating_duration_since(Instant::now()));
            if timeout.is_some_and(|d| d.is_zero()) {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "frame_timeout"));
            }
            self.stream.set_read_timeout(timeout)?;
            let mut buffer = [0; 8192];
            let count = self.stream.read(&mut buffer)?;
            if count == 0 {
                return if line.is_empty() {
                    Ok(None)
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "incomplete_frame",
                    ))
                };
            }
            self.received = Instant::now();
            deadline.get_or_insert(self.received + FRAME_TIMEOUT);
            self.pending.extend_from_slice(&buffer[..count]);
        }
    }
}
