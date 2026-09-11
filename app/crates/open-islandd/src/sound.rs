use open_island_core::config::{SoundConfig, SoundEvent};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TrySendError},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::config_handle::ConfigHandle;
use crate::notifications::shutdown::BoundedThread;

#[cfg(target_os = "linux")]
pub const PLAYER: &str = "pw-play";
#[cfg(target_os = "macos")]
pub const PLAYER: &str = "/usr/bin/afplay";
pub const MAX_CONCURRENT: usize = 8;
pub const QUEUE_DEPTH: usize = 8;
pub const REAP_TICK: Duration = Duration::from_millis(500);

#[cfg(target_os = "linux")]
#[zbus::proxy(
    default_service = "org.erikreider.swaync",
    default_path = "/org/erikreider/swaync/cc",
    interface = "org.erikreider.swaync.cc",
    gen_async = false
)]
trait ControlCenter {
    fn get_dnd(&self) -> zbus::Result<bool>;
}

pub fn requested_sound(sound: &SoundConfig, event: SoundEvent) -> Option<&PathBuf> {
    if !sound.enabled || sound.quiet {
        return None;
    }
    sound.events.path(event)
}

// Wraps past midnight when the end is before the start, which is the whole point of the
// setting: agents run overnight.
pub fn in_quiet_hours(sound: &SoundConfig, minute_of_day: u32) -> bool {
    if !sound.quiet_hours || sound.quiet_hours_start == sound.quiet_hours_end {
        return false;
    }
    if sound.quiet_hours_start < sound.quiet_hours_end {
        (sound.quiet_hours_start..sound.quiet_hours_end).contains(&minute_of_day)
    } else {
        minute_of_day >= sound.quiet_hours_start || minute_of_day < sound.quiet_hours_end
    }
}

pub fn minute_of_day_now() -> u32 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0);
    let local = secs as i64 + local_offset_seconds();
    (local.rem_euclid(86_400) / 60) as u32
}

fn local_offset_seconds() -> i64 {
    let output = Command::new("date").arg("+%z").output().ok();
    let text = output
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .unwrap_or_default();
    let text = text.trim();
    if text.len() != 5 {
        return 0;
    }
    let sign = if text.starts_with('-') { -1 } else { 1 };
    let hours: i64 = text[1..3].parse().unwrap_or(0);
    let minutes: i64 = text[3..5].parse().unwrap_or(0);
    sign * (hours * 3600 + minutes * 60)
}

pub fn admits(
    sound: &SoundConfig,
    do_not_disturb: bool,
    quiet_scene: bool,
    live: usize,
    minute_of_day: u32,
) -> bool {
    if quiet_scene {
        return false;
    }
    if sound.follow_dnd && do_not_disturb {
        return false;
    }
    if in_quiet_hours(sound, minute_of_day) {
        return false;
    }
    live < MAX_CONCURRENT
}

pub fn command(path: &Path, volume: f32) -> Command {
    let mut command = Command::new(PLAYER);
    command
        .arg(if cfg!(target_os = "macos") {
            "-v"
        } else {
            "--volume"
        })
        .arg(format!("{volume:.3}"))
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

#[derive(Clone)]
pub struct SoundPlayer {
    config: ConfigHandle,
    sender: Arc<Mutex<Option<SyncSender<PathBuf>>>>,
    quiet_scene: Arc<AtomicBool>,
}

impl SoundPlayer {
    pub fn silent() -> Self {
        Self {
            config: ConfigHandle::default(),
            sender: Arc::new(Mutex::new(None)),
            quiet_scene: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn set_quiet_scene(&self, active: bool) {
        self.quiet_scene.store(active, Ordering::Relaxed);
    }

    pub fn start(config: ConfigHandle) -> (Self, Option<BoundedThread>) {
        let (sender, receiver) = sync_channel(QUEUE_DEPTH);
        let thread_config = config.clone();
        let quiet_scene = Arc::new(AtomicBool::new(false));
        let thread_scene = Arc::clone(&quiet_scene);
        let thread = BoundedThread::spawn("open-island-sound", move || {
            run(&thread_config, receiver, &thread_scene);
        })
        .ok();
        let sender = if thread.is_some() { Some(sender) } else { None };
        (
            Self {
                config,
                sender: Arc::new(Mutex::new(sender)),
                quiet_scene,
            },
            thread,
        )
    }

    pub fn play(&self, event: SoundEvent) {
        let config = self.config.get();
        let Some(path) = requested_sound(&config.sound, event) else {
            return;
        };
        let guard = self
            .sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(sender) = guard.as_ref() else {
            return;
        };
        match sender.try_send(path.clone()) {
            Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
        }
    }

    pub fn shutdown(&self) {
        let mut guard = self
            .sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.take();
    }
}

fn run(config: &ConfigHandle, receiver: Receiver<PathBuf>, quiet_scene: &AtomicBool) {
    let mut live: Vec<Child> = Vec::new();
    let mut probe = DndProbe::new();
    loop {
        live.retain_mut(|child| matches!(child.try_wait(), Ok(None)));
        let received = if live.is_empty() {
            receiver.recv().map_err(|_| RecvTimeoutError::Disconnected)
        } else {
            receiver.recv_timeout(REAP_TICK)
        };
        let path = match received {
            Ok(path) => path,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        let config = config.get();
        let do_not_disturb = config.sound.follow_dnd && probe.do_not_disturb();
        if !admits(
            &config.sound,
            do_not_disturb,
            quiet_scene.load(Ordering::Relaxed),
            live.len(),
            minute_of_day_now(),
        ) {
            continue;
        }
        match command(&path, config.sound.volume).spawn() {
            Ok(child) => live.push(child),
            Err(error) => eprintln!("open-islandd: {PLAYER} {}: {error}", path.display()),
        }
    }
    for mut child in live {
        let _ = child.wait();
    }
}

#[cfg(target_os = "linux")]
#[derive(Default)]
pub struct DndProbe {
    proxy: Option<ControlCenterProxy<'static>>,
    unavailable: bool,
}

#[cfg(target_os = "linux")]
impl DndProbe {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn do_not_disturb(&mut self) -> bool {
        if self.unavailable {
            return false;
        }
        if self.proxy.is_none() {
            match connect() {
                Some(proxy) => self.proxy = Some(proxy),
                None => {
                    self.unavailable = true;
                    return false;
                }
            }
        }
        self.proxy
            .as_ref()
            .and_then(|proxy| proxy.get_dnd().ok())
            .unwrap_or(false)
    }
}

#[cfg(target_os = "linux")]
fn connect() -> Option<ControlCenterProxy<'static>> {
    let connection = zbus::blocking::Connection::session().ok()?;
    ControlCenterProxy::builder(&connection).build().ok()
}

#[cfg(test)]
#[path = "sound_tests.rs"]
mod tests;

#[cfg(target_os = "macos")]
pub struct DndProbe;
#[cfg(target_os = "macos")]
impl DndProbe {
    pub fn new() -> Self {
        Self
    }
    pub fn do_not_disturb(&mut self) -> bool {
        crate::native_state::focus()
    }
}
