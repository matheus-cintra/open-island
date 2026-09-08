use std::{
    env, fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

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
    let staging = directory.join(format!(
        ".config-{}.tmp",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos())
    ));
    fs::write(&staging, r#"{"updates": {"check_enabled": false}}"#).expect("isolated config");
    fs::rename(&staging, &path).expect("isolated config");
    path
}
