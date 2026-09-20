use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
const MAX_MODEL: u64 = 4 * 1024 * 1024 * 1024;
pub fn diagnostic_status(directory: Option<&Path>) -> &'static str {
    if directory.is_some_and(|directory| read(directory).is_ok()) {
        "configured"
    } else {
        "unavailable"
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema_version: u8,
    model_path: PathBuf,
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
pub fn validate(path: &Path) -> Result<PathBuf, &'static str> {
    let path = path.canonicalize().map_err(|_| "model_unavailable")?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(&path)
        .map_err(|_| "model_unavailable")?;
    let metadata = file.metadata().map_err(|_| "model_unavailable")?;
    if !metadata.is_file() || metadata.len() < 48 || metadata.len() > MAX_MODEL {
        return Err("invalid_model");
    }
    let mut header = [0; 8];
    file.read_exact(&mut header).map_err(|_| "invalid_model")?;
    if u32::from_le_bytes(header[..4].try_into().unwrap()) != 0x67676d6c {
        return Err("invalid_model");
    }
    let vocabulary = u32::from_le_bytes(header[4..].try_into().unwrap());
    if vocabulary == 51864 {
        return Err("model_language_unsupported");
    }
    if !(51865..=65536).contains(&vocabulary) {
        return Err("invalid_model");
    }
    Ok(path)
}
pub fn read(directory: &Path) -> Result<PathBuf, &'static str> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(directory.join("voice.json"))
        .map_err(|_| "model_unavailable")?;
    if !file
        .metadata()
        .is_ok_and(|m| m.is_file() && m.len() <= 16384)
    {
        return Err("model_unavailable");
    }
    let mut text = Vec::new();
    file.take(16385)
        .read_to_end(&mut text)
        .map_err(|_| "model_unavailable")?;
    if text.len() > 16384 {
        return Err("model_unavailable");
    }
    let config: Config = serde_json::from_slice(&text).map_err(|_| "model_unavailable")?;
    if config.schema_version != 1 || !config.model_path.is_absolute() {
        return Err("model_unavailable");
    }
    validate(&config.model_path).map_err(|_| "model_unavailable")
}
pub fn clear(directory: &Path) -> Result<(), &'static str> {
    match fs::remove_file(directory.join("voice.json")) {
        Ok(()) => File::open(directory)
            .and_then(|file| file.sync_all())
            .map_err(|_| "model_configuration_failed"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("model_configuration_failed"),
    }
}
pub fn save(directory: &Path, selected: &Path) -> Result<(), &'static str> {
    let model_path = validate(selected)?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)
        .map_err(|_| "model_configuration_failed")?;
    let nonce = open_island_core::epoch::generate().map_err(|_| "model_configuration_failed")?;
    let temporary = Temporary(directory.join(format!(".voice-{}.tmp", nonce.0)));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary.0)
        .map_err(|_| "model_configuration_failed")?;
    serde_json::to_writer(
        &mut file,
        &Config {
            schema_version: 1,
            model_path,
        },
    )
    .map_err(|_| "model_configuration_failed")?;
    file.flush()
        .and_then(|_| file.sync_all())
        .map_err(|_| "model_configuration_failed")?;
    fs::rename(&temporary.0, directory.join("voice.json"))
        .map_err(|_| "model_configuration_failed")?;
    File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|_| "model_configuration_failed")?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn model(path: &Path, vocabulary: u32) {
        let mut bytes = vec![0u8; 48];
        bytes[..4].copy_from_slice(&0x67676d6cu32.to_le_bytes());
        bytes[4..8].copy_from_slice(&vocabulary.to_le_bytes());
        fs::write(path, bytes).unwrap();
    }
    #[test]
    fn configuration_is_private_atomic_and_does_not_store_audio_or_text() {
        let dir = tempfile::tempdir().unwrap();
        let selected = dir.path().canonicalize().unwrap().join("selected.bin");
        model(&selected, 51865);
        let state = dir.path().join("state");
        save(&state, &selected).unwrap();
        assert_eq!(read(&state).unwrap(), selected);
        assert_eq!(diagnostic_status(Some(&state)), "configured");
        assert_eq!(diagnostic_status(None), "unavailable");
        let path = state.join("voice.json");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let value: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 2);
        model(&selected, 51864);
        assert_eq!(save(&state, &selected), Err("model_language_unsupported"));
        assert_eq!(fs::read_dir(state).unwrap().count(), 1);
    }
    #[test]
    fn removal_only_deletes_local_configuration_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let selected = dir.path().join("selected.bin");
        model(&selected, 51865);
        let state = dir.path().join("state");
        save(&state, &selected).unwrap();
        clear(&state).unwrap();
        assert_eq!(read(&state), Err("model_unavailable"));
        assert_eq!(diagnostic_status(Some(&state)), "unavailable");
        assert!(selected.is_file());
        clear(&state).unwrap();
        fs::create_dir(state.join("voice.json")).unwrap();
        assert_eq!(clear(&state), Err("model_configuration_failed"));
        assert!(state.join("voice.json").is_dir());
    }
    #[test]
    fn invalid_deleted_oversized_and_nonregular_models_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(validate(dir.path()), Err("invalid_model"));
        let selected = dir.path().join("model.bin");
        model(&selected, 51865);
        OpenOptions::new()
            .write(true)
            .open(&selected)
            .unwrap()
            .set_len(MAX_MODEL + 1)
            .unwrap();
        assert_eq!(validate(&selected), Err("invalid_model"));
        fs::remove_file(&selected).unwrap();
        assert_eq!(validate(&selected), Err("model_unavailable"));
        fs::write(dir.path().join("voice.json"), b"{bad").unwrap();
        assert_eq!(read(dir.path()), Err("model_unavailable"));
    }
}
