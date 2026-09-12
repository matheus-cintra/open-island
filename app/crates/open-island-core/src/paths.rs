//! Paths must agree when launched by Finder, launchd, a shell or an agent hook.
use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

pub fn directory_from(
    kind: &str,
    override_dir: Option<OsString>,
    home: Option<OsString>,
    macos: bool,
) -> Option<PathBuf> {
    if let Some(base) = override_dir {
        return Some(PathBuf::from(base).join("open-island"));
    }
    let home = PathBuf::from(home?);
    if macos {
        Some(
            home.join("Library/Application Support/Open Island")
                .join(kind),
        )
    } else {
        Some(
            home.join(match kind {
                "config" => ".config",
                "data" => ".local/share",
                _ => ".local/state",
            })
            .join("open-island"),
        )
    }
}
fn directory(kind: &str, variable: &str) -> Option<PathBuf> {
    directory_from(
        kind,
        env::var_os(variable),
        env::var_os("HOME"),
        cfg!(target_os = "macos"),
    )
}
pub fn config_dir() -> Option<PathBuf> {
    directory("config", "XDG_CONFIG_HOME")
}
pub fn data_dir() -> Option<PathBuf> {
    directory("data", "XDG_DATA_HOME")
}
pub fn state_dir() -> Option<PathBuf> {
    directory("state", "XDG_STATE_HOME")
}
pub fn socket() -> PathBuf {
    if let Some(path) = env::var_os("OPEN_ISLAND_SOCKET") {
        return path.into();
    }
    if let Some(base) = env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(base).join("open-island.sock");
    }
    if cfg!(target_os = "macos") {
        private_runtime().join("island.sock")
    } else {
        PathBuf::from("/tmp/open-island.sock")
    }
}
pub fn private_runtime() -> PathBuf {
    PathBuf::from(format!("/tmp/open-island-{}", unsafe { libc::geteuid() }))
}
pub fn prepare_socket(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    let private = private_runtime();
    if path.parent() != Some(private.as_path()) {
        return Ok(());
    }
    match std::fs::DirBuilder::new().mode(0o700).create(&private) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error),
    }
    let metadata = std::fs::symlink_metadata(private)?;
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "diretório do socket não é privado",
        ));
    }
    Ok(())
}
pub fn executable(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let mut directories: Vec<PathBuf> = env::var_os("PATH")
        .map(|path| env::split_paths(&path).collect())
        .unwrap_or_default();
    if cfg!(target_os = "macos") {
        directories
            .extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"].map(PathBuf::from));
        if let Some(home) = env::var_os("HOME") {
            for suffix in [".local/bin", ".cargo/bin", ".bun/bin", ".npm-global/bin"] {
                directories.push(PathBuf::from(&home).join(suffix));
            }
        }
    }
    directories
        .into_iter()
        .map(|dir| dir.join(name))
        .find(|path| {
            path.metadata().is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
}
pub fn bundled_sounds() -> Option<PathBuf> {
    Some(
        env::current_exe()
            .ok()?
            .parent()?
            .join("../Resources/sounds"),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn platform_paths_and_overrides() {
        assert_eq!(
            directory_from("config", None, Some("/Users/a".into()), true).unwrap(),
            PathBuf::from("/Users/a/Library/Application Support/Open Island/config")
        );
        assert_eq!(
            directory_from(
                "state",
                Some("/isolated".into()),
                Some("/Users/a".into()),
                true
            )
            .unwrap(),
            PathBuf::from("/isolated/open-island")
        );
        assert_eq!(
            directory_from("config", None, Some("/home/a".into()), false).unwrap(),
            PathBuf::from("/home/a/.config/open-island")
        );
        assert!(private_runtime().as_os_str().len() < 70);
    }
}
