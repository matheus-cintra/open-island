use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

pub trait ServiceControl: Send + Sync {
    fn stop_daemon_unit(&self) -> Result<(), String>;
    fn run_daemon(&self, arguments: &[&str]) -> Result<String, String>;
    fn detach_daemon(&self, arguments: &[&str]) -> Result<(), String>;
    fn run_update(&self, prompt: &str) -> Result<(), String>;
    fn launch(&self) -> Result<(), String>;
    fn register_shortcut(&self) -> Result<(), String>;
    fn autoconfigure(&self) -> Result<(), String>;
}

static SERVICE_CONTROL: OnceLock<Mutex<Option<Arc<dyn ServiceControl>>>> = OnceLock::new();

fn lock_control() -> MutexGuard<'static, Option<Arc<dyn ServiceControl>>> {
    SERVICE_CONTROL
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

pub struct ServiceControlGuard;

impl Drop for ServiceControlGuard {
    fn drop(&mut self) {
        *lock_control() = None;
    }
}

pub fn install(control: Arc<dyn ServiceControl>) -> Result<ServiceControlGuard, String> {
    let mut current = lock_control();
    if current.is_some() {
        return Err("QA service control is already installed".to_owned());
    }
    *current = Some(control);
    Ok(ServiceControlGuard)
}

fn with_control<T>(
    operation: impl FnOnce(&dyn ServiceControl) -> Result<T, String>,
) -> Result<T, String> {
    let current = lock_control();
    let control = current
        .as_ref()
        .ok_or_else(|| "QA service control is not installed".to_owned())?;
    operation(control.as_ref())
}

pub fn preflight() -> Result<(), String> {
    with_control(|_| Ok(()))
}

pub fn stop_daemon_unit() -> Result<(), String> {
    with_control(|control| control.stop_daemon_unit())
}

pub fn run_daemon(arguments: &[&str]) -> Result<String, String> {
    with_control(|control| control.run_daemon(arguments))
}

pub fn detach_daemon(arguments: &[&str]) -> Result<(), String> {
    with_control(|control| control.detach_daemon(arguments))
}

pub fn run_update(prompt: &str) -> Result<(), String> {
    with_control(|control| control.run_update(prompt))
}

pub fn deny_launch() -> Result<(), String> {
    with_control(|control| control.launch())
}

pub fn register_shortcut() -> Result<(), String> {
    with_control(|control| control.register_shortcut())
}

pub fn authorize_autoconfigure() -> Result<(), String> {
    with_control(|control| control.autoconfigure())
}
