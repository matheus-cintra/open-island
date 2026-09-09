pub mod handle;
pub mod wire;

use crate::broadcast::make_broadcast;
use crate::notifications::lifecycle::{self, DaemonContext};
use crate::notifications::shutdown::ShutdownFlag;
use crate::server::handle::handle;
use crate::server::wire::response;
use open_island_core::protocol::Request;
use serde_json::Value;
use std::{
    env, fs,
    io::{self, BufRead, BufReader, BufWriter, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

/// The accept loop polls instead of blocking so a shutdown can break it, and every new
/// connection waits out one full tick. `shutdown::TICK` is 250 ms, which the island never
/// notices because it connects once, but a per-keypress `toggle` pays it every time.
pub const ACCEPT_TICK: Duration = Duration::from_millis(20);

pub fn socket_path() -> PathBuf {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--socket" {
            if let Some(path) = args.next() {
                return PathBuf::from(path);
            }
        }
    }
    if let Some(path) = env::var_os("OPEN_ISLAND_SOCKET") {
        return PathBuf::from(path);
    }
    PathBuf::from(env::var_os("XDG_RUNTIME_DIR").unwrap_or_else(|| "/tmp".into()))
        .join("open-island.sock")
}

pub fn bind_socket(path: &Path) -> io::Result<UnixListener> {
    match UnixListener::bind(path) {
        Ok(listener) => {
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
            Ok(listener)
        }
        Err(bind_error) => match UnixStream::connect(path) {
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "another daemon is running",
            )),
            Err(_) => {
                let _ = fs::remove_file(path);
                UnixListener::bind(path)
                    .and_then(|listener| {
                        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
                        Ok(listener)
                    })
                    .map_err(|error| {
                        io::Error::new(
                            error.kind(),
                            format!("{bind_error}; rebinding failed: {error}"),
                        )
                    })
            }
        },
    }
}

pub fn disconnect(ctx: &DaemonContext, connection_id: u64) {
    if let Ok(mut state) = ctx.state.lock() {
        state
            .subscribers
            .retain(|subscriber| subscriber.connection_id != connection_id);
    }
    lifecycle::disconnect_pending(ctx, connection_id, make_broadcast(Arc::clone(&ctx.state)));
}

pub fn client(stream: UnixStream, ctx: DaemonContext, connection_id: u64) {
    let (sender, receiver) = mpsc::channel::<String>();
    if let Ok(mut state) = ctx.state.lock() {
        state.subscribers.push(lifecycle::Subscriber {
            connection_id,
            sender: sender.clone(),
        });
    }
    let writer_ctx = ctx.clone();
    let writer_input = match stream.try_clone() {
        Ok(stream) => stream,
        Err(_) => return,
    };
    thread::spawn(move || {
        let mut writer = BufWriter::new(writer_input);
        for message in receiver {
            if writeln!(writer, "{message}")
                .and_then(|_| writer.flush())
                .is_err()
            {
                disconnect(&writer_ctx, connection_id);
                break;
            }
        }
    });
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        let read = match reader.read_line(&mut line) {
            Ok(read) => read,
            Err(_) => break,
        };
        if read == 0 {
            break;
        }
        let ctx_for_request = ctx.clone();
        let response_sender = sender.clone();
        match serde_json::from_str::<Request>(line.trim()) {
            Ok(request) => {
                thread::spawn(move || {
                    let _ = response_sender.send(handle(ctx_for_request, connection_id, request));
                });
            }
            Err(error) => {
                let _ = sender.send(response(
                    Value::Null,
                    Err(format!("invalid request: {error}")),
                ));
            }
        }
    }
    disconnect(&ctx, connection_id);
}

pub fn accept_loop(
    listener: &UnixListener,
    ctx: DaemonContext,
    flag: &ShutdownFlag,
    next_connection: &AtomicU64,
) {
    let _ = listener.set_nonblocking(true);
    loop {
        if flag.is_requested() {
            return;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let connection_id = next_connection.fetch_add(1, Ordering::Relaxed);
                let ctx = ctx.clone();
                thread::spawn(move || client(stream, ctx, connection_id));
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_TICK);
            }
            Err(error) => eprintln!("open-islandd accept error: {error}"),
        }
    }
}
