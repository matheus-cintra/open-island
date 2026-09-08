use open_island_core::config::Config;
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct ConfigHandle {
    inner: Arc<RwLock<Arc<Config>>>,
}

impl ConfigHandle {
    pub fn new(config: Config) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Arc::new(config))),
        }
    }

    pub fn get(&self) -> Arc<Config> {
        Arc::clone(
            &self
                .inner
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        )
    }

    pub fn set(&self, config: Config) {
        let mut guard = self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = Arc::new(config);
    }
}

impl Default for ConfigHandle {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

#[cfg(test)]
#[path = "config_handle_tests.rs"]
mod tests;
