use crate::{
    compositor::{Compositor, MonitorInfo},
    geometry,
};
use std::{ffi::c_void, sync::Mutex};
use tauri::{Emitter, Manager};

#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq)]
struct Screen {
    id: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale: f64,
    safe_top: f64,
    notch_width: f64,
}
#[derive(Default)]
struct Snapshot {
    screens: Vec<Screen>,
    pointer: (i32, i32),
    pid: u32,
}
static SNAPSHOT: Mutex<Snapshot> = Mutex::new(Snapshot {
    screens: Vec::new(),
    pointer: (0, 0),
    pid: 0,
});
static SIZE: Mutex<(f64, f64)> = Mutex::new((232.0, 46.0));
extern "C" {
    fn oi_take_layout_invalidated() -> i32;
    fn oi_panel_init(handle: *mut c_void);
    fn oi_panel_keyboard(handle: *mut c_void, active: i32);
    fn oi_panel_frame(handle: *mut c_void, x: f64, top: f64, width: f64, height: f64);
    fn oi_screens(screens: *mut Screen, capacity: i32) -> i32;
    fn oi_pointer(x: *mut f64, y: *mut f64, pid: *mut u32);
}
fn selected() -> Option<Screen> {
    let name = geometry::selected_monitor();
    let snapshot = SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner());
    name.and_then(|id| {
        snapshot
            .screens
            .iter()
            .find(|screen| screen.id.to_string() == id)
            .copied()
    })
    .or_else(|| snapshot.screens.first().copied())
}
pub fn safe_top() -> f64 {
    selected().map(|s| s.safe_top).unwrap_or(0.0)
}
pub fn notch_width() -> f64 {
    selected().map(|s| s.notch_width).unwrap_or(0.0)
}
// All AppKit queries and mutations are made on the application thread.
fn refresh(window: &tauri::WebviewWindow) {
    let mut screens = [Screen::default(); 32];
    let count = unsafe { oi_screens(screens.as_mut_ptr(), screens.len() as i32) }.max(0) as usize;
    let (mut x, mut y, mut pid) = (0.0, 0.0, 0);
    unsafe {
        oi_pointer(&mut x, &mut y, &mut pid);
    }
    let changed = {
        let mut snapshot = SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner());
        let next = screens[..count.min(screens.len())].to_vec();
        let changed = snapshot.screens != next;
        *snapshot = Snapshot {
            screens: next,
            pointer: (x as i32, y as i32),
            pid,
        };
        changed
    };
    if changed || unsafe { oi_take_layout_invalidated() != 0 } {
        geometry::forget_monitor_box();
        let (width, height) = *SIZE.lock().unwrap_or_else(|e| e.into_inner());
        resize(window, width, height);
        let _ = window.emit("island-screen-changed", ());
    }
}
pub fn init(window: &tauri::WebviewWindow) -> Result<(), String> {
    unsafe {
        oi_panel_init(window.ns_window().map_err(|e| e.to_string())?);
    }
    refresh(window);
    let target = window.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let current = target.clone();
        if target
            .run_on_main_thread(move || refresh(&current))
            .is_err()
        {
            break;
        }
    });
    Ok(())
}
pub fn resize(window: &tauri::WebviewWindow, width: f64, height: f64) {
    *SIZE.lock().unwrap_or_else(|e| e.into_inner()) = (width, height);
    let Some(screen) = selected() else {
        return;
    };
    let frame = crate::screen_geometry::place(
        screen.x,
        screen.y,
        screen.width,
        screen.notch_width,
        screen.safe_top,
        width,
        height,
    );
    if let Ok(handle) = window.ns_window() {
        unsafe {
            oi_panel_frame(handle, frame.x, frame.y, frame.width, frame.height);
        }
        geometry::remember_rect(geometry::IslandRect {
            x: frame.x as i32,
            y: frame.y as i32,
            width: frame.width as i32,
            height: frame.height as i32,
        });
    }
}
pub fn keyboard(window: &tauri::WebviewWindow, active: bool) {
    if let Ok(handle) = window.ns_window() {
        unsafe {
            oi_panel_keyboard(handle, i32::from(active));
        }
    }
}
pub fn monitor(window: &tauri::WebviewWindow, _: Option<&str>) {
    let (width, height) = *SIZE.lock().unwrap_or_else(|e| e.into_inner());
    resize(window, width, height);
    let _ = window.emit("island-screen-changed", ());
}
pub fn monitor_report() -> Vec<String> {
    MacCompositor.monitor_names()
}
pub struct MacCompositor;
impl Compositor for MacCompositor {
    fn available(&self) -> bool {
        true
    }
    fn monitors(&self) -> Vec<MonitorInfo> {
        SNAPSHOT
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .screens
            .iter()
            .enumerate()
            .map(|(i, s)| MonitorInfo {
                name: s.id.to_string(),
                x: s.x as i32,
                y: s.y as i32,
                width: s.width as u32,
                physical_width_mm: 0,
                scale: s.scale,
                reserved_top: s.safe_top as u32,
                focused: i == 0,
            })
            .collect()
    }
    fn cursor_position(&self) -> Option<(i32, i32)> {
        Some(SNAPSHOT.lock().ok()?.pointer)
    }
    fn any_fullscreen(&self) -> Option<bool> {
        None
    }
    fn focused_pid(&self) -> Option<u32> {
        Some(SNAPSHOT.lock().ok()?.pid)
    }
    fn window_class(&self, _: u32) -> Option<String> {
        None
    }
    fn compact_height(&self, _: Option<&str>) -> Option<u32> {
        // AppKit reports the camera strip separately. There is no compositor
        // bar height to impose on the frontend's compact content.
        None
    }
    fn ui_scale(&self, _: Option<&str>) -> f64 {
        1.0
    }
}

pub fn setup_menu(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::{
        menu::{MenuBuilder, MenuItemBuilder},
        tray::TrayIconBuilder,
    };
    let settings = MenuItemBuilder::with_id("settings", "Ajustes…").build(app)?;
    let toggle = MenuItemBuilder::with_id("toggle", "Alternar ilha").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Sair").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&settings, &toggle, &quit])
        .build()?;
    TrayIconBuilder::new()
        .title("◒")
        .tooltip("Open Island (experimental)")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "settings" => {
                let _ = crate::commands::open_settings(app.clone());
            }
            "toggle" => {
                let _ = app.emit("island-toggle", ());
            }
            "quit" => {
                if let Err(error) = crate::commands::quit_app(app.clone()) {
                    eprintln!("{error}");
                }
            }
            _ => (),
        })
        .build(app)?;
    Ok(())
}
