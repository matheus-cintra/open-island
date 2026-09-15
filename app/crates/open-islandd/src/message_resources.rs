use crate::message_runner::MessageRunner;
use std::{
    collections::HashSet,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResourceKey {
    Bridge(String),
    Tmux(String),
    Zellij(String),
    Wezterm(String, String),
    Kitty(String, String),
}
#[derive(Default)]
pub struct Resources {
    active: Mutex<HashSet<ResourceKey>>,
    changed: Condvar,
}
pub struct ResourceGuard {
    owner: Arc<Resources>,
    key: ResourceKey,
}
impl Resources {
    pub fn acquire(
        self: &Arc<Self>,
        key: ResourceKey,
        runner: &MessageRunner,
    ) -> Result<ResourceGuard, String> {
        let deadline = Instant::now() + runner.remaining()?.min(Duration::from_secs(2));
        let mut active = self.active.lock().map_err(|_| "channel_unavailable")?;
        loop {
            runner.remaining()?;
            if !active.contains(&key) {
                active.insert(key.clone());
                return Ok(ResourceGuard {
                    owner: self.clone(),
                    key,
                });
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("channel_busy".to_owned());
            }
            active = self
                .changed
                .wait_timeout(active, remaining.min(Duration::from_millis(50)))
                .map_err(|_| "channel_unavailable")?
                .0;
        }
    }
}
impl Drop for ResourceGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = self.owner.active.lock() {
            active.remove(&self.key);
        }
        self.owner.changed.notify_all();
    }
}

pub fn normalized(path: &std::path::Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}
pub fn tmux_key(
    socket: Option<&str>,
    environment: &std::collections::HashMap<String, String>,
) -> ResourceKey {
    let socket = socket.map(std::path::PathBuf::from).unwrap_or_else(|| {
        std::path::Path::new(
            environment
                .get("TMUX_TMPDIR")
                .map(String::as_str)
                .unwrap_or("/tmp"),
        )
        .join(format!("tmux-{}/default", unsafe { libc::geteuid() }))
    });
    ResourceKey::Tmux(normalized(&socket))
}
