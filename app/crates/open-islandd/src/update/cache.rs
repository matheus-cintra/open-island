use open_island_core::config::write_atomic;
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub const TTL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct CachedCheck {
    pub tag: String,
    pub checked_at_ms: u64,
}

pub fn path() -> Option<PathBuf> {
    path_from(std::env::var_os("XDG_STATE_HOME"), std::env::var_os("HOME"))
}

fn path_from(xdg_state_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let base = xdg_state_home
        .map(PathBuf::from)
        .or_else(|| home.map(|home| PathBuf::from(home).join(".local/state")))?;
    Some(base.join("open-island").join("update.json"))
}

pub fn load(path: &Path, now_ms: u64) -> Option<CachedCheck> {
    let text = fs::read_to_string(path).ok()?;
    let check: CachedCheck = serde_json::from_str(&text).ok()?;
    let age = Duration::from_millis(now_ms.saturating_sub(check.checked_at_ms));
    (age < TTL).then_some(check)
}

pub fn save(path: &Path, check: &CachedCheck) {
    let Ok(text) = serde_json::to_string(check) else {
        return;
    };
    let _ = write_atomic(path, &text);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "open-island-update-cache-{}-{name}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("temp dir");
            Self(path)
        }

        fn file(&self) -> PathBuf {
            self.0.join("update.json")
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const TTL_MS: u64 = 24 * 60 * 60 * 1000;

    fn check(checked_at_ms: u64) -> CachedCheck {
        CachedCheck {
            tag: "v0.2.0".to_owned(),
            checked_at_ms,
        }
    }

    #[test]
    fn a_saved_check_round_trips_through_load() {
        let dir = TempDir::new("round-trip");
        save(&dir.file(), &check(1_000));
        assert!(dir.file().is_file());
        assert_eq!(load(&dir.file(), 2_000), Some(check(1_000)));
    }

    #[test]
    fn a_check_older_than_the_ttl_loads_as_absent() {
        let dir = TempDir::new("ttl");
        save(&dir.file(), &check(1_000_000));
        assert_eq!(
            load(&dir.file(), 1_000_000 + TTL_MS - 1_000),
            Some(check(1_000_000))
        );
        assert_eq!(load(&dir.file(), 1_000_000 + TTL_MS + 1_000), None);
        assert_eq!(load(&dir.file(), 1_000_000 + TTL_MS), None);
    }

    #[test]
    fn a_check_from_the_future_still_counts_as_fresh() {
        let dir = TempDir::new("future");
        save(&dir.file(), &check(5_000));
        assert_eq!(load(&dir.file(), 1_000), Some(check(5_000)));
    }

    #[test]
    fn a_missing_corrupt_or_truncated_file_loads_as_absent() {
        let dir = TempDir::new("corrupt");
        assert_eq!(load(&dir.file(), 1), None);
        fs::write(dir.file(), "{ not json").expect("write");
        assert_eq!(load(&dir.file(), 1), None);
        fs::write(dir.file(), r#"{"tag": "v0.2.0", "checked_at_ms": 1"#).expect("write");
        assert_eq!(load(&dir.file(), 1), None);
        fs::write(dir.file(), r#"{"tag": "v0.2.0"}"#).expect("write");
        assert_eq!(load(&dir.file(), 1), None);
        fs::write(dir.file(), "").expect("write");
        assert_eq!(load(&dir.file(), 1), None);
    }

    #[test]
    fn the_path_honours_xdg_state_home_and_falls_back_to_the_home_state_dir() {
        assert_eq!(
            path_from(Some("/state".into()), Some("/home/user".into())),
            Some(PathBuf::from("/state/open-island/update.json"))
        );
        assert_eq!(
            path_from(None, Some("/home/user".into())),
            Some(PathBuf::from(
                "/home/user/.local/state/open-island/update.json"
            ))
        );
        assert_eq!(path_from(None, None), None);
    }

    #[test]
    fn the_temp_dir_is_gone_after_the_test_that_made_it() {
        let file = {
            let dir = TempDir::new("cleanup");
            save(&dir.file(), &check(1));
            assert!(dir.file().is_file());
            dir.file()
        };
        assert!(!file.exists());
        assert!(!file.parent().expect("parent").exists());
    }
}
