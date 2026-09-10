// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod appicon;
mod client;
mod commands;
mod compositor;
mod geometry;
mod launch;
mod layershell;
mod pointer;
mod settings;
mod terminal;
mod update;

use client::DaemonClient;
use commands::reveal_settings;
use geometry::{compact_size, remember_origin};
use pointer::watch_pointer;
use tauri::{Listener, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::list_sessions,
            commands::jump,
            commands::resolve_approval,
            commands::answer_question,
            commands::set_island_size,
            commands::island_keyboard,
            commands::pick_session_folder,
            commands::agents_available,
            commands::open_session,
            commands::get_config,
            commands::get_usage,
            commands::get_update,
            commands::check_update,
            commands::run_update,
            commands::send_message,
            commands::cancel_message,
            commands::save_config,
            commands::sound_theme_files,
            commands::play_sound,
            commands::user_sound_dir,
            commands::integration_status,
            commands::set_integration,
            commands::open_settings,
            commands::terminal_icon,
            commands::list_monitors,
            commands::set_island_monitor,
            commands::monitor_report,
            commands::island_metrics,
            commands::app_version,
            commands::remove_auto_configuration,
            commands::quit_app
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
