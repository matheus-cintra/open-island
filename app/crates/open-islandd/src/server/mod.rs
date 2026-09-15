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
    io::{self, BufWriter, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
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
    open_island_core::paths::socket()
}

pub fn bind_socket(path: &Path) -> io::Result<UnixListener> {
    open_island_core::paths::prepare_socket(path)?;
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
    ctx.snapshots.cancel(connection_id);
    if let Ok(mut state) = ctx.state.lock() {
        state
            .subscribers
            .retain(|subscriber| subscriber.connection_id != connection_id);
    }
    lifecycle::disconnect_pending(ctx, connection_id, make_broadcast(Arc::clone(&ctx.state)));
}

pub fn client(
    stream: UnixStream,
    ctx: DaemonContext,
    connection_id: u64,
    admission: Arc<admission::Admission>,
    handshake: admission::Permit,
) {
    let mut handshake = Some(handshake);
    let mut managed = None;
    let Ok(writer_input) = stream.try_clone() else {
        return;
    };
    let Ok(shutdown) = stream.try_clone() else {
        return;
    };
    let sender = outbox::Outbox::with_metrics(shutdown, admission.outbox.clone());
    if let Ok(mut state) = ctx.state.lock() {
        state.subscribers.push(lifecycle::Subscriber {
            receives_actions: true,
            diagnostic_only: false,
            ui_epoch: None,
            connection_id,
            sender: sender.clone(),
        });
    }
    let output = sender.clone();
    let writer = thread::spawn(move || {
        let _ = writer_input.set_write_timeout(Some(Duration::from_secs(2)));
        let mut writer = BufWriter::new(writer_input);
        while let Some(message) = output.receive() {
            if writeln!(writer, "{message}")
                .and_then(|_| writer.flush())
                .is_err()
            {
                break;
            }
        }
        output.close();
    });
    let mut reader = frame::Frames::new(stream);
    let local = admission::Budget::default();
    while let Ok(Some(line)) = reader.next() {
        match serde_json::from_slice::<Request>(&line) {
            Ok(request) => {
                if request.method == "subscribe_ui" && managed.is_none() {
                    let Some(permit) = admission.managed.acquire(32) else {
                        let _ = sender.send(response(request.id, Err("server_busy".into())));
                        continue;
                    };
                    managed = Some(permit);
                    handshake.take();
                }
                let diagnostic = request.method == "ping"
                    && request
                        .params
                        .as_ref()
                        .and_then(|params| params.get("client_role"))
                        .and_then(Value::as_str)
                        == Some("diagnostic");
                if request.method == "hook_event" || diagnostic || request.method == "subscribe_ui"
                {
                    if let Ok(mut state) = ctx.state.lock() {
                        if let Some(subscriber) = state
                            .subscribers
                            .iter_mut()
                            .find(|s| s.connection_id == connection_id)
                        {
                            subscriber.receives_actions = request.method == "subscribe_ui";
                            subscriber.diagnostic_only = diagnostic;
                        }
                    }
                }
                let bulk = matches!(
                    request.method.as_str(),
                    "get_ui_state" | "get_ui_state_page"
                );
                let blocking = request.method == "hook_event";
                let global = if bulk {
                    admission.bulk.acquire(4)
                } else if blocking {
                    admission.blocking.acquire(64)
                } else {
                    admission.fast.acquire(64)
                };
                let local = if blocking || bulk {
                    None
                } else {
                    local.acquire(8)
                };
                let Some(global) = global.filter(|_| blocking || bulk || local.is_some()) else {
                    let _ = sender.send(response(request.id, Err("server_busy".into())));
                    continue;
                };
                let ctx = ctx.clone();
                let sender = sender.clone();
                thread::spawn(move || {
                    let (_global, _local) = (global, local);
                    let _ = sender.send(handle(ctx, connection_id, request));
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
    sender.close();
    disconnect(&ctx, connection_id);
    let _ = writer.join();
}

pub fn accept_loop(
    listener: &UnixListener,
    ctx: DaemonContext,
    flag: &ShutdownFlag,
    next_connection: &AtomicU64,
) {
    let admission = ctx.admission.clone();
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
                let Some(permit) = admission.connections.acquire(32) else {
                    continue;
                };
                let admission = admission.clone();
                thread::spawn(move || {
                    client(stream, ctx, connection_id, admission, permit);
                });
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_TICK);
            }
            Err(error) => eprintln!("open-islandd accept error: {error}"),
        }
    }
}

pub mod snapshot_stream;

pub mod admission;
mod frame;
pub mod outbox;

pub mod guarded_actions;

pub mod guarded_jump;

pub mod outbox_metrics;
