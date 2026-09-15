use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, OnceLock},
};

use super::ProcessBirthIdentity;

pub trait ProcessSource: Send + Sync {
    fn pids(&self) -> Vec<(u32, ProcessBirthIdentity)>;
    fn birth_identity(&self, pid: u32) -> Option<ProcessBirthIdentity>;
    fn parent_and_comm(&self, pid: u32) -> Option<(u32, String)>;
    fn command(&self, pid: u32) -> Option<Vec<u8>>;
    fn environment(&self, pid: u32) -> Vec<u8>;
    fn cwd(&self, pid: u32) -> Option<PathBuf>;
    fn stdin_device(&self, pid: u32) -> Option<u64>;
    fn exists(&self, pid: u32) -> bool;
}

struct ProcessRegistry {
    source: Arc<dyn ProcessSource>,
    entries: BTreeMap<u32, ProcessBirthIdentity>,
}

static PROCESS_REGISTRY: OnceLock<Mutex<Option<ProcessRegistry>>> = OnceLock::new();

fn lock_registry() -> MutexGuard<'static, Option<ProcessRegistry>> {
    PROCESS_REGISTRY
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

pub struct ProcessSourceGuard;

impl Drop for ProcessSourceGuard {
    fn drop(&mut self) {
        *lock_registry() = None;
    }
}

pub fn install_process_source_for_qa(
    source: Arc<dyn ProcessSource>,
) -> Result<ProcessSourceGuard, String> {
    let mut current = lock_registry();
    if current.is_some() {
        return Err("QA process source is already installed".to_owned());
    }
    *current = Some(ProcessRegistry {
        source,
        entries: BTreeMap::new(),
    });
    Ok(ProcessSourceGuard)
}

pub fn register_process_for_qa(pid: u32, identity: ProcessBirthIdentity) -> Result<(), String> {
    let mut current = lock_registry();
    let registry = current
        .as_mut()
        .ok_or_else(|| "QA process source is not installed".to_owned())?;
    registry.entries.insert(pid, identity);
    Ok(())
}

pub(super) fn pids() -> Vec<u32> {
    let current = lock_registry();
    match current.as_ref() {
        Some(registry) => registry
            .entries
            .iter()
            .filter_map(|(pid, identity)| {
                (registry.source.birth_identity(*pid).as_ref() == Some(identity)).then_some(*pid)
            })
            .collect(),
        None => Vec::new(),
    }
}

pub(super) fn route<T>(pid: u32, denied: T, operation: impl FnOnce(&dyn ProcessSource) -> T) -> T {
    let current = lock_registry();
    match current.as_ref() {
        Some(registry)
            if registry.entries.get(&pid).is_some_and(|identity| {
                registry.source.birth_identity(pid).as_ref() == Some(identity)
            }) =>
        {
            operation(registry.source.as_ref())
        }
        Some(_) => denied,
        None => denied,
    }
}
