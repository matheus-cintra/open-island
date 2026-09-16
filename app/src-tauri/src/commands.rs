use crate::client::DaemonClient;
use crate::compositor::{self, Compositor};
use crate::geometry::{forget_monitor_box, position_island, selected_monitor, SELECTED_MONITOR};
#[cfg(not(feature = "qa-harness"))]
use crate::update;
use crate::{appicon, launch, platform, settings, terminal};
use open_island_core::protocol::ApprovalDecision;
use open_island_core::session::Session;
use serde_json::{json, Value};
use std::time::Duration;
use tauri::{Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

pub const REVEAL_REMAP: Duration = Duration::from_millis(80);

#[tauri::command]
pub fn jump_v2(
    client: State<'_, DaemonClient>,
    id: String,
    identity: open_island_core::message_delivery::DeliveryIdentity,
) -> Result<(), String> {
    client.jump_v2(&id, identity)
}

#[tauri::command]
pub fn get_daemon_ui_state(
    client: State<'_, DaemonClient>,
) -> crate::daemon_transport::refresh::UiCache {
    client.get_daemon_ui_state()
}

#[tauri::command]
pub fn resolve_approval_v2(
    client: State<'_, DaemonClient>,
    approval_id: String,
    pending_generation: u64,
    decision: ApprovalDecision,
    identity: open_island_core::message_delivery::DeliveryIdentity,
) -> Result<(), String> {
    client.resolve_approval_v2(&approval_id, pending_generation, decision, identity)
}
#[tauri::command]
pub fn answer_question_v2(
    client: State<'_, DaemonClient>,
    question_id: String,
    pending_generation: u64,
    answers: Vec<Vec<String>>,
    identity: open_island_core::message_delivery::DeliveryIdentity,
) -> Result<(), String> {
    client.answer_question_v2(&question_id, pending_generation, answers, identity)
}

#[tauri::command]
pub fn cancel_message_v2(
    client: State<'_, DaemonClient>,
    id: String,
    message_id: u64,
    identity: open_island_core::message_delivery::DeliveryIdentity,
) -> Result<(), String> {
    client.cancel_message_v2(&id, message_id, identity)
}

#[tauri::command]
pub fn get_message_recovery(
    client: State<'_, DaemonClient>,
) -> Result<Vec<crate::message_recovery::RecoveryRecord>, String> {
    client.message_recovery()
}
#[tauri::command]
pub fn discard_message_recovery(
    client: State<'_, DaemonClient>,
    id: open_island_core::message_delivery::ClientSubmissionId,
) -> Result<bool, String> {
    client.discard_message_recovery(id)
}
#[tauri::command]
pub fn send_message_v2(
    client: State<'_, DaemonClient>,
    id: String,
    text: String,
    identity: open_island_core::message_delivery::DeliveryIdentity,
) -> Result<Value, String> {
    client.send_message_v2(&id, &text, identity)
}

#[tauri::command]
pub fn get_ui_state(
    client: State<'_, DaemonClient>,
) -> Result<crate::daemon_transport::sync::SyncedSnapshot, String> {
    client.get_ui_state()
}

#[tauri::command]
pub fn list_sessions(client: State<'_, DaemonClient>) -> Result<Vec<Session>, String> {
    client.list_sessions()
}

#[tauri::command]
pub fn jump(id: String, client: State<'_, DaemonClient>) -> Result<(), String> {
    client.jump(&id)
}

#[tauri::command]
pub fn resolve_approval(
    approval_id: String,
    decision: ApprovalDecision,
    client: State<'_, DaemonClient>,
) -> Result<(), String> {
    client.resolve_approval(&approval_id, decision)
}

#[tauri::command]
pub fn answer_question(
    question_id: String,
    answers: Vec<Vec<String>>,
    client: State<'_, DaemonClient>,
) -> Result<(), String> {
    client.answer_question(&question_id, answers)
}

#[tauri::command]
pub fn set_island_size(
    window: tauri::WebviewWindow,
    width: f64,
    height: f64,
) -> Result<(), String> {
    position_island(&window, width, height)
}

#[tauri::command]
pub fn island_keyboard(window: tauri::WebviewWindow, active: bool) -> Result<(), String> {
    let target = window.clone();
    window
        .run_on_main_thread(move || {
            platform::keyboard(&target, active);
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn pick_session_folder(
    window: tauri::WebviewWindow,
    agent: String,
    title: String,
    accept: String,
    cancel: String,
) -> Result<(), String> {
    let agent = launch::known_agent(&agent)?;
    let _ = (accept, cancel);
    let target = window.clone();
    window
        .dialog()
        .file()
        .set_title(title)
        .pick_folder(move |folder| {
            let path = folder
                .and_then(|path| path.into_path().ok())
                .map(|path| path.to_string_lossy().into_owned());
            let _ = target.emit("session-folder", json!({"agent": agent, "path": path}));
        });
    Ok(())
}

#[tauri::command]
pub fn agents_available() -> Vec<String> {
    launch::available(terminal::on_path)
}

#[cfg(not(feature = "qa-harness"))]
#[tauri::command]
pub fn open_session(
    agent: String,
    folder: String,
    client: State<'_, DaemonClient>,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let payload = client.get_config()?;
        let config =
            open_island_core::config::Config::from_json_str(&payload["config"].to_string());
        launch::open_macos(&folder, &agent, &config.integrations.macos_terminal)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = client;
        launch::open(&folder, &agent)
    }
}

#[cfg(feature = "qa-harness")]
#[tauri::command]
pub fn open_session(
    agent: String,
    folder: String,
    client: State<'_, DaemonClient>,
) -> Result<(), String> {
    let _ = (agent, folder, client);
    crate::qa::service_control::deny_launch()
}

#[tauri::command]
pub fn get_config(client: State<'_, DaemonClient>) -> Result<Value, String> {
    client.get_config()
}

#[tauri::command]
pub fn get_usage(client: State<'_, DaemonClient>) -> Result<Value, String> {
    client.get_usage()
}

#[tauri::command]
pub fn get_update(client: State<'_, DaemonClient>) -> Result<Value, String> {
    client.get_update()
}

#[tauri::command]
pub fn check_update(client: State<'_, DaemonClient>) -> Result<Value, String> {
    client.check_update()
}

#[tauri::command]
pub async fn run_update(app: tauri::AppHandle, prompt: String) -> Result<(), String> {
    #[cfg(feature = "qa-harness")]
    {
        let _ = app;
        crate::qa::service_control::run_update(&prompt)
    }
    #[cfg(not(feature = "qa-harness"))]
    #[cfg(target_os = "macos")]
    {
        let _ = prompt;
        crate::update_macos::run(app).await
    }
    #[cfg(not(feature = "qa-harness"))]
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        update::run(&prompt)
    }
}

#[tauri::command]
pub fn send_message(
    id: String,
    text: String,
    client: State<'_, DaemonClient>,
) -> Result<Value, String> {
    client.send_message(&id, &text)
}

#[tauri::command]
pub fn cancel_message(
    id: String,
    message_id: u64,
    client: State<'_, DaemonClient>,
) -> Result<(), String> {
    client.cancel_message(&id, message_id)
}

#[tauri::command]
pub fn save_config(config: Value) -> Result<(), String> {
    settings::save(config)
}

#[tauri::command]
pub fn user_sound_dir() -> String {
    settings::user_sound_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[tauri::command]
pub fn play_sound(path: String, client: State<'_, DaemonClient>) -> Result<(), String> {
    client.play_sound(&path)
}

#[tauri::command]
pub fn sound_theme_files() -> Vec<String> {
    settings::theme_sounds()
}

#[tauri::command]
pub fn integration_status() -> Result<Value, String> {
    settings::integration_status()
}

#[tauri::command]
pub fn set_integration(name: String, enabled: bool) -> Result<Value, String> {
    settings::set_integration(&name, enabled)
}

#[tauri::command]
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[tauri::command]
pub fn remove_auto_configuration() -> Result<Vec<String>, String> {
    settings::remove_auto_configuration()
}

#[tauri::command]
pub fn quit_app(app: tauri::AppHandle) -> Result<(), String> {
    settings::stop_daemon_unit()?;
    app.exit(0);
    Ok(())
}

#[derive(Clone, serde::Serialize)]
pub struct IslandMetrics {
    scale: f64,
    compact_height: Option<u32>,
    safe_top: f64,
    notch_width: f64,
}

#[tauri::command]
pub fn terminal_icon(pid: u32) -> Option<String> {
    let session = Session::new("", "", pid, "");
    let terminal = open_island_core::discovery::terminal_for_session(&session)?;
    appicon::for_pid(terminal.raise_pid)
}

#[tauri::command]
pub fn island_metrics() -> IslandMetrics {
    let monitor = selected_monitor();
    let compositor = compositor::current();
    IslandMetrics {
        safe_top: platform::safe_top(),
        notch_width: platform::notch_width(),
        scale: compositor.ui_scale(monitor.as_deref()),
        compact_height: compositor.compact_height(monitor.as_deref()),
    }
}

#[tauri::command]
pub fn list_monitors() -> Vec<String> {
    compositor::current().monitor_names()
}

#[tauri::command]
pub fn set_island_monitor(app: tauri::AppHandle, name: Option<String>) -> Result<(), String> {
    let wanted = name.filter(|value| !value.is_empty());
    {
        let mut guard = SELECTED_MONITOR
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *guard == wanted {
            return Ok(());
        }
        *guard = wanted.clone();
    }
    forget_monitor_box();
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "no island window".to_owned())?;
    let target = window.clone();
    window
        .run_on_main_thread(move || platform::monitor(&target, wanted.as_deref()))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn monitor_report() -> Vec<String> {
    platform::monitor_report()
}

// Hyprland drops xdg-activation unless misc:focus_on_activate is on, so set_focus alone
// never moves a window off another workspace; unmapping makes it be placed again.
pub fn reveal_settings(window: &tauri::WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    window
        .set_title("Ajustes do Open Island")
        .map_err(|error| error.to_string())?;
    #[cfg(target_os = "linux")]
    if window.is_visible().unwrap_or(false) {
        window.hide().map_err(|error| error.to_string())?;
        std::thread::sleep(REVEAL_REMAP);
    }
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())?;
    window
        .emit("settings-revealed", ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn open_settings(app: tauri::AppHandle, pane: Option<String>) -> Result<(), String> {
    let window = app
        .get_webview_window("settings")
        .ok_or_else(|| "no settings window".to_owned())?;
    reveal_settings(&window)?;
    if let Some(pane) = pane {
        window
            .emit_to("settings", "open-pane", pane)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn platform_capabilities() -> Value {
    json!({"os": std::env::consts::OS, "experimental": cfg!(target_os = "macos"),
        "hyprland": cfg!(target_os = "linux"), "automatic_dnd": true,
        "screen_off": true, "fullscreen_detection": true,
        "global_shortcut": cfg!(target_os = "macos"), "manual_update": false})
}

#[tauri::command]
pub fn macos_focus_status() -> Value {
    #[cfg(target_os = "macos")]
    {
        json!(platform::focus_status())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Value::Null
    }
}
#[tauri::command]
pub fn request_focus_permission(window: tauri::WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        platform::request_focus_permission(&window)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window;
        Err("Disponível somente no macOS.".into())
    }
}

#[tauri::command]
pub async fn diagnostic_report() -> Result<open_island_core::diagnostics::Report, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut report = open_island_core::diagnostics::collect(
            &open_island_core::paths::socket(),
            Some(env!("CARGO_PKG_VERSION")),
        );
        report.model =
            crate::voice::model::diagnostic_status(open_island_core::paths::state_dir().as_deref())
                .into();
        report
    })
    .await
    .map_err(|_| "diagnostic_unavailable".into())
}
