use open_island_core::{
    config::Config,
    process::{
        install_process_source_for_qa, register_process_for_qa, system_process_source_for_qa,
        ProcessSourceGuard,
    },
};
use std::{env, io, path::Path};
pub mod helpers;

pub struct RuntimeGuard {
    helpers: Vec<helpers::Helper>,
    _process_source: ProcessSourceGuard,
}

impl RuntimeGuard {
    pub fn create_sessions(
        &mut self,
        store: &mut open_island_core::store::SessionStore,
    ) -> io::Result<()> {
        self.helpers = helpers::spawn(store)?;
        Ok(())
    }
}

pub fn activate(socket: &Path, config: &Config) -> io::Result<RuntimeGuard> {
    if env::var_os("OPEN_ISLAND_QA") != Some("1".into()) {
        return Err(io::Error::other("OPEN_ISLAND_QA=1 is required"));
    }
    if config.usage.show_limits || config.updates.check_enabled {
        return Err(io::Error::other(
            "QA must disable usage and update network probes",
        ));
    }
    if config.integrations.auto_configure {
        return Err(io::Error::other(
            "QA config must disable integrations.auto_configure",
        ));
    }
    validate_environment(socket)?;
    let source = system_process_source_for_qa();
    let pid = std::process::id();
    let identity = source
        .birth_identity(pid)
        .ok_or_else(|| io::Error::other("cannot read QA daemon birth identity"))?;
    let guard = install_process_source_for_qa(source).map_err(io::Error::other)?;
    register_process_for_qa(pid, identity).map_err(io::Error::other)?;
    Ok(RuntimeGuard {
        helpers: Vec::new(),
        _process_source: guard,
    })
}

fn required_private_directory(variable: &str) -> io::Result<std::path::PathBuf> {
    let path = env::var_os(variable)
        .map(std::path::PathBuf::from)
        .ok_or_else(|| io::Error::other(format!("{variable} is required")))?;
    if !path.is_absolute() {
        return Err(io::Error::other(format!("{variable} must be absolute")));
    }
    if std::fs::canonicalize(&path)? != path {
        return Err(io::Error::other(
            "QA paths must be canonical, without symlinks",
        ));
    }
    let metadata = std::fs::metadata(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
            return Err(io::Error::other(format!("{variable} must be private")));
        }
    }
    Ok(path)
}

pub fn validate_environment(socket: &Path) -> io::Result<()> {
    if env::var_os("OPEN_ISLAND_QA") != Some("1".into()) {
        return Err(io::Error::other("OPEN_ISLAND_QA=1 is required"));
    }
    let home = required_private_directory("HOME")?;
    for variable in ["XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME"] {
        let directory = required_private_directory(variable)?;
        if !directory.starts_with(&home) {
            return Err(io::Error::other(format!(
                "{variable} must be inside the private QA HOME"
            )));
        }
    }
    let configured_socket = env::var_os("OPEN_ISLAND_SOCKET")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| io::Error::other("OPEN_ISLAND_SOCKET is required"))?;
    if configured_socket != socket || !socket.starts_with(&home) {
        return Err(io::Error::other(
            "OPEN_ISLAND_SOCKET must be inside the private QA HOME",
        ));
    }
    if env::var_os("OPEN_ISLAND_QA_CREDENTIALS_BLOCKED") != Some("1".into()) {
        return Err(io::Error::other(
            "QA credential blocking marker is required",
        ));
    }
    let parent = socket
        .parent()
        .ok_or_else(|| io::Error::other("socket parent missing"))?;
    let canonical = std::fs::canonicalize(parent)?;
    if canonical != parent || !canonical.starts_with(&home) {
        return Err(io::Error::other(
            "QA socket parent must be canonical and private",
        ));
    }
    if let Some(bus) = env::var_os("DBUS_SESSION_BUS_ADDRESS") {
        let bus = bus.to_string_lossy();
        let prefix = format!("unix:path={}/", home.display());
        if !bus.starts_with(&prefix) || bus.contains("..") || bus.contains(';') {
            return Err(io::Error::other("QA cannot use the inherited session bus"));
        }
    }
    Ok(())
}
