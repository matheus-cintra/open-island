use std::{io, os::unix::net::UnixStream, path::Path, time::Duration};
pub fn connect(path: &Path) -> io::Result<UnixStream> {
    open_island_core::unix_socket::connect(path, Duration::from_millis(200))
}
