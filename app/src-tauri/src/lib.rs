// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod appicon;
mod client;
mod commands;
mod compositor;
mod geometry;
mod launch;
#[cfg(target_os = "linux")]
mod layershell;
mod platform;
mod pointer;
#[cfg(any(test, target_os = "macos"))]
mod screen_geometry;
mod settings;
mod shortcut;
mod terminal;
mod update;

use client::DaemonClient;
use commands::reveal_settings;
use geometry::compact_size;
#[cfg(target_os = "linux")]
use geometry::remember_origin;
use pointer::watch_pointer;
use tauri::{Listener, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if args.iter().any(|arg| arg == "--settings") {
                let _ = commands::open_settings(app.clone());
            }
        }))
        .plugin(shortcut::plugin());
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::platform_capabilities,
            shortcut::get_shortcut,
            shortcut::set_shortcut,
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
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                platform::setup_menu(app.handle())?;
                shortcut::restore(app.handle());
            }
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
                if let Err(error) = platform::init(&window) {
                    eprintln!("open-island: {error}");
                }
                platform::resize(&window, width, height);
                #[cfg(target_os = "linux")]
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
