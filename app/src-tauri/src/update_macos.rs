use crate::update_bundle::BundleTransaction;
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

static INSTALLING: AtomicBool = AtomicBool::new(false);
struct Installation;
impl Drop for Installation {
    fn drop(&mut self) {
        INSTALLING.store(false, Ordering::Release);
    }
}

fn request(method: &str) -> Result<Value, String> {
    let mut socket =
        UnixStream::connect(open_island_core::paths::socket()).map_err(|e| e.to_string())?;
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    socket
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    writeln!(
        socket,
        "{}",
        json!({"v":1,"id":1,"method":method,"params":{}})
    )
    .map_err(|e| e.to_string())?;
    // A new connection can receive a sessions snapshot before its response.
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut reader = BufReader::new(socket);
    while Instant::now() < deadline {
        let mut line = String::new();
        let read = reader
            .by_ref()
            .take(1024 * 1024)
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        let value: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        if value["id"] == 1 {
            return if value["ok"] == true {
                Ok(value["data"].clone())
            } else {
                Err(value["error"].to_string())
            };
        }
    }
    Err("O daemon não respondeu à atualização.".into())
}

fn stop_daemon() -> Result<(), String> {
    let ping = request("ping")?;
    let pid = ping["pid"]
        .as_i64()
        .filter(|id| *id > 1 && *id <= i32::MAX as i64)
        .ok_or("PID do daemon inválido.")? as i32;
    if ping["daemon"] != "open-islandd" {
        return Err("Socket não pertence ao daemon esperado.".into());
    }
    if unsafe { libc::kill(pid, libc::SIGTERM) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        if request("ping").ok().and_then(|v| v["pid"].as_i64()) != Some(pid as i64) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err("O daemon não encerrou a tempo. A atualização foi cancelada.".into())
}

fn start_daemon(bundle: &Path, version: &str) -> Result<(), String> {
    struct Starting(Option<std::process::Child>);
    impl Drop for Starting {
        fn drop(&mut self) {
            if let Some(child) = self.0.as_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
    let mut starting = Starting(None);
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if request("ping").is_ok_and(|v| v["daemon"] == "open-islandd" && v["version"] == version) {
            if let Some(mut child) = starting.0.take() {
                thread::spawn(move || {
                    let _ = child.wait();
                });
            }
            return Ok(());
        }
        if let Some(child) = starting.0.as_mut() {
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                starting.0 = None;
            }
        }
        if starting.0.is_none() {
            // A former daemon or launchd can briefly hold the socket lock during restart.
            starting.0 = Some(
                Command::new(bundle.join("Contents/MacOS/open-islandd"))
                    .arg("--socket")
                    .arg(open_island_core::paths::socket())
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(|e| e.to_string())?,
            );
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err("A nova versão do daemon não respondeu.".into())
}

fn output(command: &mut Command) -> Result<String, String> {
    let result = command
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(String::from_utf8_lossy(&result.stderr).trim().into());
    }
    Ok(String::from_utf8_lossy(&result.stdout).trim().into())
}

fn validate(bundle: &Path, version: &str) -> Result<(), String> {
    let plist = bundle.join("Contents/Info.plist");
    for (key, expected) in [
        ("CFBundleIdentifier", "app.open-island"),
        ("CFBundleExecutable", "open-island"),
        ("LSMinimumSystemVersion", "12.0"),
        ("CFBundleShortVersionString", version),
    ] {
        if output(
            Command::new("/usr/libexec/PlistBuddy")
                .args(["-c", &format!("Print :{key}")])
                .arg(&plist),
        )? != expected
        {
            return Err(format!("Pacote incompatível: {key}."));
        }
    }
    output(
        Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(bundle),
    )?;
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    };
    for name in ["open-island", "open-islandd"] {
        let binary = bundle.join("Contents/MacOS").join(name);
        if output(Command::new("/usr/bin/lipo").arg("-archs").arg(&binary))? != arch {
            return Err(format!("Arquitetura incompatível em {name}."));
        }
    }
    for name in [
        "device-added",
        "complete",
        "message",
        "dialog-warning",
        "suspend-error",
    ] {
        if fs::metadata(bundle.join(format!("Contents/Resources/sounds/{name}.wav")))
            .map_err(|e| e.to_string())?
            .len()
            == 0
        {
            return Err(format!("Som ausente: {name}."));
        }
    }
    Ok(())
}

fn install(bytes: Vec<u8>, version: String, installed: PathBuf) -> Result<(), String> {
    if std::env::var_os("OPEN_ISLANDD").is_some() {
        return Err(
            "Remova o override OPEN_ISLANDD para atualizar o daemon junto com o aplicativo.".into(),
        );
    }
    install_at(bytes, version, installed)
}

fn install_at(bytes: Vec<u8>, version: String, installed: PathBuf) -> Result<(), String> {
    let parent = installed.parent().ok_or("Local do aplicativo inválido.")?;
    let staging = tempfile::Builder::new().prefix(".open-island-update-").tempdir_in(parent)
        .map_err(|e| format!("Sem acesso para atualizar este aplicativo. Mova-o para ~/Applications ou instale pelo DMG. {e}"))?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes.as_slice()));
    archive.unpack(staging.path()).map_err(|e| e.to_string())?;
    let entries: Vec<PathBuf> = fs::read_dir(staging.path())
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    if entries.len() != 1 || entries[0].extension().is_none_or(|e| e != "app") {
        return Err("O pacote deve conter somente o aplicativo Open Island.".into());
    }
    let candidate = &entries[0];
    validate(candidate, &version)?;
    let sessions = request("list_sessions")?;
    if sessions.as_array().is_some_and(|sessions| {
        sessions.iter().any(|session| {
            session["permission_state"] == "pending" || session["question_state"] == "pending"
        })
    }) {
        return Err("Responda às aprovações e perguntas pendentes antes de atualizar.".into());
    }
    let previous_version = request("ping")?["version"]
        .as_str()
        .ok_or("Versão do daemon inválida.")?
        .to_owned();
    let mut transaction =
        BundleTransaction::begin(&installed, staging, candidate).map_err(|e| format!("A troca segura do pacote falhou: {e}. O aplicativo anterior foi preservado; instale pelo DMG."))?;
    let result = stop_daemon().and_then(|_| start_daemon(&installed, &version));
    if let Err(error) = result {
        // Restore the bundle before restoring its daemon; never report a partial update as success.
        let _ = stop_daemon();
        transaction
            .rollback()
            .map_err(|rollback| format!("{error}; falha ao restaurar: {rollback}"))?;
        start_daemon(&installed, &previous_version).map_err(|restore| {
            format!("{error}; pacote restaurado, mas o daemon falhou: {restore}")
        })?;
        return Err(error);
    }
    transaction.commit();
    Ok(())
}

pub async fn run(app: tauri::AppHandle) -> Result<(), String> {
    if INSTALLING.swap(true, Ordering::AcqRel) {
        return Err("Já existe uma atualização em andamento.".into());
    }
    let _installation = Installation;
    // Tauri caches this path before startup and rejects symlinked macOS paths.
    // Use the same location for replacement and relaunch, even if the bundle moves later.
    let executable = tauri::process::current_binary(&app.env())
        .map_err(|e| format!("Não foi possível determinar um local seguro para reiniciar: {e}"))?;
    let installed = executable
        .ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .ok_or("Instale Open Island em Aplicativos antes de atualizar.")?
        .to_path_buf();
    let _ = app.emit("update-progress", "Verificando atualização…");
    let updater = app
        .updater_builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| format!("Não foi possível consultar o pacote assinado: {e}"))?
        .ok_or("Nenhuma atualização assinada disponível para este Mac.")?;
    let _ = app.emit("update-progress", "Baixando e verificando assinatura…");
    // download() verifies the signature with the embedded public key before returning bytes.
    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|e| format!("Download ou assinatura inválida: {e}"))?;
    let _ = app.emit("update-progress", "Instalando e reiniciando o daemon…");
    tauri::async_runtime::spawn_blocking(move || install(bytes, update.version, installed))
        .await
        .map_err(|e| e.to_string())??;
    app.restart();
}

#[cfg(test)]
#[path = "update_macos_tests.rs"]
mod tests;
