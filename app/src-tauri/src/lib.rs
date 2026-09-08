// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod appicon;
mod client;
mod hypr;
mod layershell;
mod settings;
mod update;

use client::DaemonClient;
use open_island_core::{protocol::ApprovalDecision, session::Session};
use serde_json::Value;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{Emitter, Listener, Manager, State};

const POINTER_TICK: Duration = Duration::from_millis(100);
const REVEAL_REMAP: Duration = Duration::from_millis(80);
const MONITOR_CACHE_TTL: Duration = Duration::from_millis(1000);
const COMPACT_WIDTH: f64 = 232.0;
const COMPACT_HEIGHT: f64 = 46.0;

#[derive(Clone, Copy)]
struct IslandRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

#[derive(Clone, serde::Serialize)]
struct PointerState {
    inside: bool,
}

#[derive(Clone, serde::Serialize)]
struct FullscreenState {
    fullscreen: bool,
}

#[derive(Clone, serde::Serialize)]
struct FocusState {
    pid: Option<u32>,
}

static ISLAND_RECT: Mutex<IslandRect> = Mutex::new(IslandRect {
    x: 0,
    y: 0,
    width: 0,
    height: 0,
});

#[derive(Clone, Copy)]
struct MonitorBox {
    x: i32,
    y: i32,
    width: u32,
}

static MONITOR_BOX: Mutex<Option<(MonitorBox, Instant)>> = Mutex::new(None);
static SELECTED_MONITOR: Mutex<Option<String>> = Mutex::new(None);

fn selected_monitor() -> Option<String> {
    SELECTED_MONITOR
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

fn forget_monitor_box() {
    *MONITOR_BOX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

fn remember_rect(rect: IslandRect) {
    let mut guard = ISLAND_RECT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = rect;
}

fn island_rect() -> IslandRect {
    *ISLAND_RECT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn pointer_is_inside(rect: IslandRect, x: i32, y: i32) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

fn watch_pointer(window: tauri::WebviewWindow) {
    let Some(socket) = hypr::socket_path() else {
        return;
    };
    let mut last: Option<bool> = None;
    let mut last_fullscreen: Option<bool> = None;
    let mut last_focus: Option<Option<u32>> = None;
    loop {
        std::thread::sleep(POINTER_TICK);
        let focus = hypr::focused_pid();
        if last_focus != Some(focus) {
            last_focus = Some(focus);
            if window
                .emit("island-focus", FocusState { pid: focus })
                .is_err()
            {
                return;
            }
        }
        if let Some(fullscreen) = hypr::any_fullscreen() {
            if last_fullscreen != Some(fullscreen) {
                last_fullscreen = Some(fullscreen);
                if window
                    .emit("island-fullscreen", FullscreenState { fullscreen })
                    .is_err()
                {
                    return;
                }
            }
        }
        let rect = island_rect();
        if rect.width == 0 || rect.height == 0 {
            continue;
        }
        let Some((x, y)) = hypr::cursor_position(&socket) else {
            continue;
        };
        let inside = pointer_is_inside(rect, x, y);
        if last == Some(inside) {
            continue;
        }
        last = Some(inside);
        if window
            .emit("island-pointer", PointerState { inside })
            .is_err()
        {
            return;
        }
    }
}

#[tauri::command]
fn list_sessions(client: State<'_, DaemonClient>) -> Result<Vec<Session>, String> {
    client.list_sessions()
}

#[tauri::command]
fn jump(id: String, client: State<'_, DaemonClient>) -> Result<(), String> {
    client.jump(&id)
}

#[tauri::command]
fn resolve_approval(
    approval_id: String,
    decision: ApprovalDecision,
    client: State<'_, DaemonClient>,
) -> Result<(), String> {
    client.resolve_approval(&approval_id, decision)
}

#[tauri::command]
fn answer_question(
    question_id: String,
    answers: Vec<Vec<String>>,
    client: State<'_, DaemonClient>,
) -> Result<(), String> {
    client.answer_question(&question_id, answers)
}

#[tauri::command]
fn set_island_size(window: tauri::WebviewWindow, width: f64, height: f64) -> Result<(), String> {
    position_island(&window, width, height)
}

#[tauri::command]
fn get_config(client: State<'_, DaemonClient>) -> Result<Value, String> {
    client.get_config()
}

#[tauri::command]
fn get_usage(client: State<'_, DaemonClient>) -> Result<Value, String> {
    client.get_usage()
}

#[tauri::command]
fn get_update(client: State<'_, DaemonClient>) -> Result<Value, String> {
    client.get_update()
}

#[tauri::command]
fn run_update(prompt: String) -> Result<(), String> {
    update::run(&prompt)
}

#[tauri::command]
fn save_config(config: Value) -> Result<(), String> {
    settings::save(config)
}

#[tauri::command]
fn user_sound_dir() -> String {
    settings::user_sound_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[tauri::command]
fn play_sound(path: String, client: State<'_, DaemonClient>) -> Result<(), String> {
    client.play_sound(&path)
}

#[tauri::command]
fn sound_theme_files() -> Vec<String> {
    settings::theme_sounds()
}

#[tauri::command]
fn integration_status() -> Result<Value, String> {
    settings::integration_status()
}

#[tauri::command]
fn set_integration(name: String, enabled: bool) -> Result<Value, String> {
    settings::set_integration(&name, enabled)
}

#[tauri::command]
fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[tauri::command]
fn remove_auto_configuration() -> Result<Vec<String>, String> {
    settings::remove_auto_configuration()
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) -> Result<(), String> {
    settings::stop_daemon_unit()?;
    app.exit(0);
    Ok(())
}

#[derive(Clone, serde::Serialize)]
struct IslandMetrics {
    scale: f64,
    compact_height: Option<u32>,
}

#[tauri::command]
fn terminal_icon(pid: u32) -> Option<String> {
    let session = Session::new("", "", pid, "");
    let terminal = open_island_core::discovery::terminal_for_session(&session)?;
    appicon::for_pid(terminal.raise_pid)
}

#[tauri::command]
fn island_metrics() -> IslandMetrics {
    let monitor = selected_monitor();
    IslandMetrics {
        scale: hypr::ui_scale(monitor.as_deref()),
        compact_height: hypr::compact_height(monitor.as_deref()),
    }
}

#[tauri::command]
fn list_monitors() -> Vec<String> {
    hypr::monitor_names()
}

#[tauri::command]
fn set_island_monitor(app: tauri::AppHandle, name: Option<String>) -> Result<(), String> {
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
    let rect = wanted.as_deref().and_then(|name| {
        let monitor = hypr::monitor_named(Some(name))?;
        Some((
            name.to_owned(),
            monitor.get("x")?.as_i64()? as i32,
            monitor.get("y")?.as_i64()? as i32,
        ))
    });
    let target = window.clone();
    window
        .run_on_main_thread(move || {
            if let Ok(gtk_window) = target.gtk_window() {
                let target = rect.as_ref().map(|(name, x, y)| layershell::MonitorTarget {
                    name,
                    x: *x,
                    y: *y,
                });
                layershell::set_monitor(&gtk_window, target);
            }
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn monitor_report() -> Vec<String> {
    layershell::monitor_report()
}

// Hyprland drops xdg-activation unless misc:focus_on_activate is on, so set_focus alone
// never moves a window off another workspace; unmapping makes it be placed again.
fn reveal_settings(window: &tauri::WebviewWindow) -> Result<(), String> {
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
fn open_settings(app: tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("settings")
        .ok_or_else(|| "no settings window".to_owned())?;
    reveal_settings(&window)
}

fn position_island(window: &tauri::WebviewWindow, width: f64, height: f64) -> Result<(), String> {
    remember_origin(window, width, height);
    let target = window.clone();
    let (width, height) = (width as i32, height as i32);
    window
        .run_on_main_thread(move || {
            if let Ok(gtk_window) = target.gtk_window() {
                layershell::resize(&gtk_window, width, height);
            }
        })
        .map_err(|error| error.to_string())
}

fn remember_origin(window: &tauri::WebviewWindow, width: f64, height: f64) {
    if let Ok(Some((x, y))) = centered_origin(window, width) {
        remember_rect(IslandRect {
            x,
            y,
            width: width as i32,
            height: height as i32,
        });
    }
}

fn compact_size() -> (f64, f64) {
    let monitor = selected_monitor();
    let scale = hypr::ui_scale(monitor.as_deref());
    let height = hypr::compact_height(monitor.as_deref())
        .map(f64::from)
        .unwrap_or((COMPACT_HEIGHT * scale).round());
    ((COMPACT_WIDTH * scale).round(), height)
}

fn monitor_box(window: &tauri::WebviewWindow) -> Result<Option<MonitorBox>, String> {
    let mut guard = MONITOR_BOX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((rect, stamp)) = *guard {
        if stamp.elapsed() < MONITOR_CACHE_TTL {
            return Ok(Some(rect));
        }
    }
    let rect = match hypr::monitor_named(selected_monitor().as_deref()).and_then(|monitor| {
        Some(MonitorBox {
            x: monitor.get("x")?.as_i64()? as i32,
            y: monitor.get("y")?.as_i64()? as i32,
            width: monitor.get("width")?.as_u64()? as u32,
        })
    }) {
        Some(rect) => Some(rect),
        None => window
            .available_monitors()
            .map_err(|e| e.to_string())?
            .first()
            .map(|monitor| MonitorBox {
                x: 0,
                y: 0,
                width: monitor.size().width,
            }),
    };
    *guard = rect.map(|value| (value, Instant::now()));
    Ok(rect)
}

fn centered_origin(
    window: &tauri::WebviewWindow,
    width: f64,
) -> Result<Option<(i32, i32)>, String> {
    Ok(monitor_box(window)?.map(|rect| (rect.x + (rect.width as i32 - width as i32) / 2, rect.y)))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            list_sessions,
            jump,
            resolve_approval,
            answer_question,
            set_island_size,
            get_config,
            get_usage,
            get_update,
            run_update,
            save_config,
            sound_theme_files,
            play_sound,
            user_sound_dir,
            integration_status,
            set_integration,
            open_settings,
            terminal_icon,
            list_monitors,
            set_island_monitor,
            monitor_report,
            island_metrics,
            app_version,
            remove_auto_configuration,
            quit_app
        ])
        .setup(|app| {
            let client =
                DaemonClient::start(app.handle().clone()).map_err(std::io::Error::other)?;
            app.manage(client);
            if let Some(settings) = app.get_webview_window("settings") {
                let opener = app.handle().clone();
                settings.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        if let Some(window) = opener.get_webview_window("settings") {
                            let _ = window.hide();
                        }
                    }
                });
                if std::env::args().any(|argument| argument == "--settings") {
                    let _ = reveal_settings(&settings);
                }
            }
            let settings_app = app.handle().clone();
            app.listen_any("open-settings", move |_| {
                if let Some(window) = settings_app.get_webview_window("settings") {
                    let _ = reveal_settings(&window);
                }
            });
            if let Some(window) = app.get_webview_window("main") {
                let (width, height) = compact_size();
                match window.gtk_window() {
                    Ok(gtk_window) => {
                        if let Err(error) = layershell::init(&gtk_window) {
                            eprintln!("open-island: island is not a layer surface: {error}");
                        }
                        layershell::resize(&gtk_window, width as i32, height as i32);
                    }
                    Err(error) => eprintln!("open-island: no gtk window: {error}"),
                }
                remember_origin(&window, width, height);
                window.show()?;
                let pointer_window = window.clone();
                std::thread::spawn(move || {
                    watch_pointer(pointer_window);
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
