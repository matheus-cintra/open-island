// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod appicon;
mod client;
mod compositor;
mod geometry;
mod launch;
mod layershell;
mod settings;
mod terminal;
mod update;

use client::DaemonClient;
use compositor::Compositor;
use geometry::{
    compact_size, forget_monitor_box, island_rect, pointer_is_inside, position_island,
    remember_origin, selected_monitor, SELECTED_MONITOR,
};
use gtk::prelude::{FileChooserExt, NativeDialogExt};
use open_island_core::{protocol::ApprovalDecision, session::Session};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use tauri::{Emitter, Listener, Manager, State};

const POINTER_TICK: Duration = Duration::from_millis(100);
const REVEAL_REMAP: Duration = Duration::from_millis(80);

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

fn watch_pointer(window: tauri::WebviewWindow, compositor: impl Compositor) {
    if !compositor.available() {
        return;
    }
    let mut last: Option<bool> = None;
    let mut last_fullscreen: Option<bool> = None;
    let mut last_focus: Option<Option<u32>> = None;
    loop {
        std::thread::sleep(POINTER_TICK);
        let focus = compositor.focused_pid();
        if last_focus != Some(focus) {
            last_focus = Some(focus);
            if window
                .emit("island-focus", FocusState { pid: focus })
                .is_err()
            {
                return;
            }
        }
        if let Some(fullscreen) = compositor.any_fullscreen() {
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
        let Some((x, y)) = compositor.cursor_position() else {
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
fn island_keyboard(window: tauri::WebviewWindow, active: bool) -> Result<(), String> {
    let target = window.clone();
    window
        .run_on_main_thread(move || {
            if let Ok(gtk_window) = target.gtk_window() {
                layershell::set_keyboard(&gtk_window, active);
            }
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn pick_session_folder(
    window: tauri::WebviewWindow,
    agent: String,
    title: String,
    accept: String,
    cancel: String,
) -> Result<(), String> {
    let agent = launch::known_agent(&agent)?;
    let target = window.clone();
    window
        .run_on_main_thread(move || {
            let dialog = gtk::FileChooserNative::new(
                Some(&title),
                None::<&gtk::Window>,
                gtk::FileChooserAction::SelectFolder,
                Some(&accept),
                Some(&cancel),
            );
            if let Some(home) = std::env::var_os("HOME") {
                dialog.set_current_folder(home);
            }
            let holder = Rc::new(RefCell::new(Some(dialog.clone())));
            dialog.connect_response(move |dialog, response| {
                let path = (response == gtk::ResponseType::Accept)
                    .then(|| dialog.filename())
                    .flatten()
                    .map(|folder| folder.to_string_lossy().into_owned());
                let _ = target.emit("session-folder", json!({ "agent": agent, "path": path }));
                holder.borrow_mut().take();
            });
            dialog.show();
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn agents_available() -> Vec<String> {
    launch::available(terminal::on_path)
}

#[tauri::command]
fn open_session(agent: String, folder: String) -> Result<(), String> {
    launch::open(&folder, &agent)
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
fn check_update(client: State<'_, DaemonClient>) -> Result<Value, String> {
    client.check_update()
}

#[tauri::command]
fn run_update(prompt: String) -> Result<(), String> {
    update::run(&prompt)
}

#[tauri::command]
fn send_message(
    id: String,
    text: String,
    client: State<'_, DaemonClient>,
) -> Result<Value, String> {
    client.send_message(&id, &text)
}

#[tauri::command]
fn cancel_message(
    id: String,
    message_id: u64,
    client: State<'_, DaemonClient>,
) -> Result<(), String> {
    client.cancel_message(&id, message_id)
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
    let compositor = compositor::current();
    IslandMetrics {
        scale: compositor.ui_scale(monitor.as_deref()),
        compact_height: compositor.compact_height(monitor.as_deref()),
    }
}

#[tauri::command]
fn list_monitors() -> Vec<String> {
    compositor::current().monitor_names()
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
        let monitor = compositor::current().monitor_named(Some(name))?;
        Some((name.to_owned(), monitor.x, monitor.y))
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
            island_keyboard,
            pick_session_folder,
            agents_available,
            open_session,
            get_config,
            get_usage,
            get_update,
            check_update,
            run_update,
            send_message,
            cancel_message,
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
                    watch_pointer(pointer_window, compositor::current());
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
