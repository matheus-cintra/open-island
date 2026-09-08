use std::{env, path::PathBuf};

pub fn isolated_home() -> PathBuf {
    env::temp_dir().join(format!("open-island-test-home-{}", std::process::id()))
}
