//! Read-only QA of the production shell transport against an explicitly supplied socket.
#[allow(dead_code)]
#[path = "../src/daemon_transport/mod.rs"]
mod daemon_transport;
use daemon_transport::{
    refresh::{Phase, Refresh},
    Transport,
};
use std::{
    collections::HashSet,
    io::Write,
    path::PathBuf,
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};

fn run() -> Result<(), &'static str> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("expected_socket_and_epoch_count");
    }
    let path = PathBuf::from(&args[0]);
    let count: usize = args[1]
        .to_str()
        .and_then(|s| s.parse().ok())
        .filter(|n| matches!(n, 1 | 2))
        .ok_or("invalid_epoch_count")?;
    if !path.exists() {
        return Err("qa_socket_missing");
    }
    let control = Arc::new(Refresh::default());
    let events = control.clone();
    let transport = Arc::new(
        Transport::start(
            path,
            Arc::new(move |event, data| events.event(event, &data)),
            Box::new(|| {}),
        )
        .map_err(|_| "transport_start_failed")?,
    );
    let (updates, received) = mpsc::channel();
    let _worker = control
        .start(
            transport,
            Arc::new(move |cache| {
                let _ = updates.send(cache);
            }),
        )
        .map_err(|_| "refresh_start_failed")?;
    let mut epochs = HashSet::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    while epochs.len() < count {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or("connection_probe_timeout")?;
        let cache = received
            .recv_timeout(remaining)
            .map_err(|_| "connection_probe_timeout")?;
        if cache.phase == Phase::Incompatible {
            return Err("daemon_incompatible");
        }
        if cache.phase != Phase::Connected {
            continue;
        }
        let snapshot = cache.snapshot.ok_or("missing_snapshot")?;
        if snapshot.snapshot.discovering {
            continue;
        }
        if epochs.insert(snapshot.snapshot.daemon_epoch.0.clone()) {
            let cached = control.cache();
            if cached.snapshot.as_ref().map(|s| &s.snapshot.daemon_epoch)
                != Some(&snapshot.snapshot.daemon_epoch)
            {
                return Err("late_view_cache_mismatch");
            }
            println!(
                "{}",
                serde_json::json!({"connected_epochs":epochs.len(),"generation":cache.generation,"cached":true,"sessions":snapshot.snapshot.sessions.len()})
            );
            std::io::stdout()
                .flush()
                .map_err(|_| "probe_output_failed")?;
        }
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
