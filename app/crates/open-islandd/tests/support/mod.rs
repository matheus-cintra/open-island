use std::{
    env, fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

// Timestamps can coincide between concurrent tests on macOS. Pair this counter
// with the process ID for fixture names that cannot collide within a test run.
pub fn unique_id() -> u64 {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

pub fn isolated_home() -> PathBuf {
    env::temp_dir().join(format!("open-island-test-home-{}", std::process::id()))
}

pub fn config_without_release_check() -> PathBuf {
    let directory = isolated_home().join("open-island");
    let path = directory.join("config.json");
    if path.is_file() {
        return path;
    }
    fs::create_dir_all(&directory).expect("isolated config dir");
    let staging = directory.join(format!(".config-{}.tmp", unique_id()));
    fs::write(&staging, r#"{"updates": {"check_enabled": false}}"#).expect("isolated config");
    fs::rename(&staging, &path).expect("isolated config");
    path
}
