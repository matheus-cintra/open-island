use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use minisign_verify::{PublicKey, Signature};

struct Environment(Vec<(&'static str, Option<std::ffi::OsString>)>);
impl Environment {
    fn set(values: &[(&'static str, PathBuf)]) -> Self {
        let previous = values
            .iter()
            .map(|(key, value)| {
                let previous = std::env::var_os(key);
                std::env::set_var(key, value);
                (*key, previous)
            })
            .collect();
        Self(previous)
    }
}
impl Drop for Environment {
    fn drop(&mut self) {
        for (key, previous) in &self.0 {
            match previous {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}
struct DaemonGuard;
impl Drop for DaemonGuard {
    fn drop(&mut self) {
        let pid = request("ping").ok().and_then(|v| v["pid"].as_i64());
        let _ = stop_daemon();
        // start_daemon owns a waiter thread. Let it reap the child before the test exits.
        if let Some(pid) = pid {
            for _ in 0..100 {
                if unsafe { libc::kill(pid as i32, 0) } != 0 {
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

/// Run alone with the freshly signed CI artifact; never changes the installed application.
#[test]
#[ignore = "requires OPEN_ISLAND_UPDATER_ARCHIVE from a native signed build"]
fn signed_bundle_rejects_tampering_and_restarts_the_real_daemon() {
    let archive_path = PathBuf::from(
        std::env::var_os("OPEN_ISLAND_UPDATER_ARCHIVE").expect("native artifact path"),
    );
    let bytes = fs::read(&archive_path).unwrap();
    let config: Value = serde_json::from_str(include_str!("../tauri.macos.conf.json")).unwrap();
    let key = String::from_utf8(
        STANDARD
            .decode(config["plugins"]["updater"]["pubkey"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    let key = PublicKey::decode(&key).unwrap();
    let signature_path = PathBuf::from(format!("{}.sig", archive_path.display()));
    let signature = String::from_utf8(
        STANDARD
            .decode(fs::read_to_string(signature_path).unwrap().trim())
            .unwrap(),
    )
    .unwrap();
    let signature = Signature::decode(&signature).unwrap();
    key.verify(&bytes, &signature, true)
        .expect("published signature matches embedded key");
    let mut tampered = bytes.clone();
    tampered[0] ^= 1;
    assert!(key.verify(&tampered, &signature, true).is_err());

    let root = tempfile::Builder::new()
        .prefix("oi-update-test-")
        .tempdir_in("/tmp")
        .unwrap();
    let config_path = root.path().join("config.json");
    fs::write(
        &config_path,
        r#"{"updates":{"check_enabled":false},"sound":{"enabled":false}}"#,
    )
    .unwrap();
    let _environment = Environment::set(&[
        ("HOME", root.path().into()),
        ("XDG_CONFIG_HOME", root.path().into()),
        ("XDG_DATA_HOME", root.path().into()),
        ("XDG_STATE_HOME", root.path().into()),
        ("OPEN_ISLAND_CONFIG", config_path),
        ("OPEN_ISLAND_SOCKET", root.path().join("test.sock")),
    ]);
    let _daemon = DaemonGuard;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes.as_slice()));
    archive.unpack(root.path()).unwrap();
    let installed = root.path().join("Open Island.app");
    let version = env!("CARGO_PKG_VERSION");
    validate(&installed, version).unwrap();
    start_daemon(&installed, version).unwrap();
    let old_pid = request("ping").unwrap()["pid"].clone();
    install_at(bytes, version.into(), installed.clone()).unwrap();
    let ping = request("ping").unwrap();
    assert_ne!(ping["pid"], old_pid, "daemon must actually restart");
    assert_eq!(ping["version"], version);
    validate(&installed, version).unwrap();
}
