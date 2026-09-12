use crate::{
    compositor::{self, Compositor},
    layershell,
};
pub fn init(window: &tauri::WebviewWindow) -> Result<(), String> {
    layershell::init(&window.gtk_window().map_err(|e| e.to_string())?)
}
pub fn resize(window: &tauri::WebviewWindow, width: f64, height: f64) {
    if let Ok(gtk) = window.gtk_window() {
        layershell::resize(&gtk, width as i32, height as i32);
    }
}
pub fn keyboard(window: &tauri::WebviewWindow, active: bool) {
    if let Ok(gtk) = window.gtk_window() {
        layershell::set_keyboard(&gtk, active);
    }
}
pub fn monitor(window: &tauri::WebviewWindow, name: Option<&str>) {
    let rect = name.and_then(|name| compositor::current().monitor_named(Some(name)));
    if let Ok(gtk) = window.gtk_window() {
        layershell::set_monitor(
            &gtk,
            rect.as_ref().map(|rect| layershell::MonitorTarget {
                name: &rect.name,
                x: rect.x,
                y: rect.y,
            }),
        );
    }
}
pub fn monitor_report() -> Vec<String> {
    layershell::monitor_report()
}
pub fn safe_top() -> f64 {
    0.0
}
pub fn notch_width() -> f64 {
    0.0
}
