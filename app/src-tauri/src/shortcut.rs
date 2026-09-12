use serde_json::{json, Value};
#[cfg(target_os = "macos")]
use tauri::Emitter;
#[cfg(target_os = "macos")]
use tauri_plugin_global_shortcut::GlobalShortcutExt;
#[cfg(target_os = "macos")]
static STATUS: std::sync::Mutex<(String, Option<String>)> =
    std::sync::Mutex::new((String::new(), None));

#[cfg(target_os = "macos")]
pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _, event| {
            if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                let _ = app.emit("island-toggle", ());
            }
        })
        .build()
}
#[cfg(target_os = "macos")]
fn path() -> Option<std::path::PathBuf> {
    open_island_core::paths::config_dir().map(|dir| dir.join("shortcut.json"))
}
#[cfg(target_os = "macos")]
pub fn restore(app: &tauri::AppHandle) {
    let combo = path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value["shortcut"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "Command+Shift+I".into());
    let error = if combo.is_empty() {
        None
    } else {
        app.global_shortcut()
            .register(combo.as_str())
            .err()
            .map(|error| format!("Atalho indisponível ou em conflito: {error}"))
    };
    *STATUS.lock().unwrap_or_else(|e| e.into_inner()) = (combo, error);
}
#[tauri::command]
pub fn get_shortcut() -> Value {
    #[cfg(target_os = "macos")]
    {
        let status = STATUS.lock().unwrap_or_else(|e| e.into_inner());
        json!({"shortcut": status.0, "error": status.1})
    }
    #[cfg(not(target_os = "macos"))]
    {
        json!({"shortcut": "", "error": null})
    }
}
#[tauri::command]
pub fn set_shortcut(app: tauri::AppHandle, shortcut: String) -> Result<Value, String> {
    #[cfg(target_os = "macos")]
    {
        let mut status = STATUS.lock().map_err(|e| e.to_string())?;
        let wanted = shortcut.trim();
        if status.0 == wanted && status.1.is_none() {
            return Ok(json!({"shortcut": status.0, "error": null}));
        }
        if !wanted.is_empty() {
            app.global_shortcut()
                .register(wanted)
                .map_err(|error| format!("Atalho indisponível ou em conflito: {error}"))?;
        }
        let saved = path()
            .ok_or_else(|| "Diretório de configurações indisponível".to_owned())
            .and_then(|path| {
                open_island_core::config::write_atomic(
                    &path,
                    &json!({"shortcut": wanted}).to_string(),
                )
            });
        if let Err(error) = saved {
            if !wanted.is_empty() {
                let _ = app.global_shortcut().unregister(wanted);
            }
            return Err(error);
        }
        if !status.0.is_empty() && status.1.is_none() {
            let _ = app.global_shortcut().unregister(status.0.as_str());
        }
        *status = (wanted.to_owned(), None);
        Ok(json!({"shortcut": wanted, "error": null}))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, shortcut);
        Err("Configure o atalho na integração Hyprland.".into())
    }
}
