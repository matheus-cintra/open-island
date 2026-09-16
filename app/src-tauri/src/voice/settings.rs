use super::{controller::Controller, model};
use serde::Serialize;
#[cfg(not(feature = "qa-harness"))]
use tauri::{Emitter, Manager};
#[cfg(not(feature = "qa-harness"))]
use tauri_plugin_dialog::DialogExt;
use tauri::{State, WebviewWindow};

#[derive(Serialize)]
pub struct ModelStatus {
    configured: bool,
    error: Option<&'static str>,
}
fn settings_window(label: &str) -> Result<(), &'static str> {
    if matches!(label, "main" | "settings") {
        Ok(())
    } else {
        Err("voice_window_forbidden")
    }
}
fn status(directory: &std::path::Path) -> ModelStatus {
    match model::read(directory) {
        Ok(_) => ModelStatus {
            configured: true,
            error: None,
        },
        Err(error) => ModelStatus {
            configured: false,
            error: Some(error),
        },
    }
}

#[tauri::command]
pub async fn voice_model_status(window: WebviewWindow) -> Result<ModelStatus, String> {
    settings_window(window.label())?;
    let directory = open_island_core::paths::state_dir().ok_or("model_unavailable")?;
    tauri::async_runtime::spawn_blocking(move || status(&directory))
        .await
        .map_err(|_| "model_unavailable".into())
}

#[tauri::command]
pub async fn voice_select_model(
    window: WebviewWindow,
    voice: State<'_, Controller>,
) -> Result<Option<ModelStatus>, String> {
    settings_window(window.label())?;
    #[cfg(feature = "qa-harness")]
    {
        let _ = voice;
        Err("qa_requires_adapter".into())
    }
    #[cfg(not(feature = "qa-harness"))]
    {
        let selection = voice.begin_model_selection()?;
        let directory = open_island_core::paths::state_dir().ok_or("model_unavailable")?;
        tauri::async_runtime::spawn_blocking(move || {
            let _selection = selection;
            let path = window
                .dialog()
                .file()
                .set_parent(&window)
                .set_title("Selecionar modelo GGML multilíngue")
                .add_filter("Modelo GGML", &["bin"])
                .blocking_pick_file();
            let Some(path) = path else {
                return Ok(None);
            };
            let path = path.into_path().map_err(|_| "invalid_model".to_owned())?;
            model::save(&directory, &path).map_err(str::to_owned)?;
            let status = status(&directory);
            let _ = window.emit_to("main", "voice-model-status", &status);
            Ok(Some(status))
        })
        .await
        .map_err(|_| "model_configuration_failed".to_owned())?
    }
}

#[tauri::command]
pub async fn voice_clear_model(window: WebviewWindow) -> Result<ModelStatus, String> {
    settings_window(window.label())?;
    #[cfg(feature = "qa-harness")]
    {
        Err("qa_requires_adapter".into())
    }
    #[cfg(not(feature = "qa-harness"))]
    {
        let directory = open_island_core::paths::state_dir().ok_or("model_unavailable")?;
        let app = window.app_handle().clone();
        tauri::async_runtime::spawn_blocking(move || {
            let voice = app.state::<Controller>();
            let _selection = voice.begin_model_removal()?;
            model::clear(&directory)?;
            let status = status(&directory);
            let _ = window.emit_to("main", "voice-model-status", &status);
            Ok(status)
        })
        .await
        .map_err(|_| "model_configuration_failed".to_owned())?
    }
}

#[tauri::command]
pub fn voice_open_microphone_settings(window: WebviewWindow) -> Result<(), String> {
    settings_window(window.label())?;
    #[cfg(feature = "qa-harness")]
    {
        Err("qa_requires_adapter".into())
    }
    #[cfg(all(not(feature = "qa-harness"), target_os = "macos"))]
    {
        use tauri_plugin_opener::OpenerExt;
        window
            .opener()
            .open_url(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
                None::<&str>,
            )
            .map_err(|_| "microphone_settings_unavailable".into())
    }
    #[cfg(all(not(feature = "qa-harness"), not(target_os = "macos")))]
    {
        Err("microphone_settings_unavailable".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_status_exposes_no_file_path_and_unknown_windows_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let value = serde_json::to_value(status(directory.path())).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"configured":false,"error":"model_unavailable"})
        );
        assert!(settings_window("settings").is_ok());
        assert!(settings_window("main").is_ok());
        assert_eq!(settings_window("preview"), Err("voice_window_forbidden"));
    }
}
